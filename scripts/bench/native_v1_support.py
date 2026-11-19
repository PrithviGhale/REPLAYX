"""Bounded container/replay helpers for the opt-in native V1 harness."""
import csv
import ctypes
from ctypes import wintypes
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import uuid

class Job:
    """A Windows kill-on-close job; descendants cannot outlive the harness."""
    def __init__(self):
        kernel=ctypes.WinDLL('kernel32',use_last_error=True); self.kernel=kernel
        class Basic(ctypes.Structure):
            _fields_=[('process_time',ctypes.c_int64),('job_time',ctypes.c_int64),('flags',wintypes.DWORD),
                      ('minimum',ctypes.c_size_t),('maximum',ctypes.c_size_t),('active',wintypes.DWORD),
                      ('affinity',ctypes.c_size_t),('priority',wintypes.DWORD),('scheduling',wintypes.DWORD)]
        class Io(ctypes.Structure):
            _fields_=[(name,ctypes.c_uint64) for name in ('read_ops','write_ops','other_ops','read_bytes','write_bytes','other_bytes')]
        class Limits(ctypes.Structure):
            _fields_=[('basic',Basic),('io',Io),('process_memory',ctypes.c_size_t),('job_memory',ctypes.c_size_t),
                      ('peak_process',ctypes.c_size_t),('peak_job',ctypes.c_size_t)]
        kernel.CreateJobObjectW.restype=wintypes.HANDLE
        kernel.SetInformationJobObject.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_void_p,wintypes.DWORD]
        kernel.AssignProcessToJobObject.argtypes=[wintypes.HANDLE,wintypes.HANDLE]
        kernel.CloseHandle.argtypes=[wintypes.HANDLE]
        self.handle=kernel.CreateJobObjectW(None,None)
        limits=Limits(); limits.basic.flags=0x2000
        if not self.handle or not kernel.SetInformationJobObject(self.handle,9,ctypes.byref(limits),ctypes.sizeof(limits)):
            self.close(); raise ctypes.WinError(ctypes.get_last_error())
    def attach(self,process):
        if not self.kernel.AssignProcessToJobObject(self.handle,int(process._handle)):
            process.kill(); raise ctypes.WinError(ctypes.get_last_error())
    def close(self):
        if getattr(self,'handle',None): self.kernel.CloseHandle(self.handle); self.handle=None

def atomic_json(path, value):
    path=Path(path); temporary=path.with_name(path.name+'.tmp')
    temporary.write_text(json.dumps(value,indent=2),encoding='utf-8')
    for attempt in range(20):
        try: os.replace(temporary,path); return
        except PermissionError:
            if attempt==19: raise
            time.sleep(.01)

def audio_filter(index, audio_origin, video_origin):
    # All arithmetic stays in samples until converting AAC; independently trim each device.
    delta=round((audio_origin-video_origin)*48000/10000000)
    return (f'[{index}:a]atrim=start_sample={max(0,-delta)},asetpts=PTS-STARTPTS,'
            f'adelay={max(0,delta)}S:all=1[a{index}]')

def mux_arguments(audio, origin, synthetic=False):
    args=['-hide_banner','-v','warning','-nostdin','-n','-probesize','32','-analyzeduration','0','-fpsprobesize','0','-use_wallclock_as_timestamps','1','-r','60','-f','h264','-i','pipe:0']
    filters=[]
    for index,feed in enumerate(audio,1):
        args+=['-thread_queue_size','256','-probesize','32','-analyzeduration','0']
        if synthetic:
            # Silent-to-the-user marker, only encoded into a test file.
            frequency=440 if index==1 else 880
            args+=['-f','lavfi','-i',fr'aevalsrc=if(between(mod(t\,2)\,1\,1.02)\,0.5*sin(2*PI*{frequency}*t)\,0):s=48000']
        else:
            args+=['-f','f32le','-ar','48000','-ac','2','-i',feed['url']]
        filters.append(audio_filter(index,feed['origin_qpc_hns'],origin))
    if filters: args+=['-filter_complex',';'.join(filters)]
    args+=['-map','0:v:0','-c:v','copy','-bsf:v','setts=time_base=1/60000:prescale=1:pts=N*1000:dts=N*1000:duration=1000']
    for index,feed in enumerate(audio,1):
        args+=['-map',f'[a{index}]',f'-metadata:s:a:{index-1}',f"title={feed['name']}"]
    if audio: args+=['-c:a','aac','-b:a','192k','-shortest','-shortest_buf_duration','1']
    args+=['-max_muxing_queue_size','512','-f','segment','-segment_time','2','-segment_format','matroska',
           '-segment_list','segments.csv','-segment_list_type','csv','-reset_timestamps','1','segment-%08d.mkv']
    return args

class ReplayStore:
    def __init__(self, root, ffmpeg, seconds=60, budget=256*1024*1024, recording=True):
        self.root=Path(root); self.ffmpeg=ffmpeg; self.seconds=seconds; self.budget=budget; self.recording=recording
        self.ring=[]; self.seen=set(); self.record=[]; self.futures=[]; self.errors=[]
        self.pool=ThreadPoolExecutor(max_workers=2)
        for name in ('pins','recording','clips','requests'): (self.root/name).mkdir(exist_ok=True)
    def refresh(self):
        listing=self.root/'segments.csv'
        if not listing.exists(): return
        # Only newline-terminated rows represent closed segments. Never pin the active writer.
        text=listing.read_text(encoding='utf-8')
        if not text.endswith('\n'): text=text.rsplit('\n',1)[0]+'\n' if '\n' in text else ''
        changed=False
        for row in csv.reader(text.splitlines()):
            if len(row)!=3 or row[0] in self.seen: continue
            path=(self.root/row[0]).resolve()
            if path.parent!=self.root.resolve() or path.suffix!='.mkv': raise ValueError('Unexpected segment path')
            segment=dict(path=path,start=float(row[1]),end=float(row[2]),bytes=path.stat().st_size)
            if segment['end']<=segment['start']: raise ValueError('Invalid segment interval')
            self.seen.add(row[0]); self.ring.append(segment); changed=True
            if self.recording:
                pinned=self.root/'recording'/path.name; os.link(path,pinned); self.record.append(dict(segment,path=pinned))
        if changed:
            entries=self.record if self.recording else self.ring
            atomic_json(self.root/'recovery.json',dict(recording=self.recording,segments=[dict(s,path=str(s['path'])) for s in entries]))
    def prune(self):
        changed=False
        while len(self.ring)>1 and (self.ring[-1]['end']-self.ring[0]['end']>=self.seconds or sum(s['bytes'] for s in self.ring)>self.budget):
            self.ring.pop(0)['path'].unlink(); changed=True
        if not self.recording and changed:
            atomic_json(self.root/'recovery.json',dict(recording=False,segments=[dict(s,path=str(s['path'])) for s in self.ring]))
        if self.ring and sum(s['bytes'] for s in self.ring)>self.budget: raise RuntimeError('Single replay segment exceeds storage budget')
    def save(self, seconds, filename, segments=None):
        if Path(filename).name!=filename or not filename.endswith('.mp4'): raise ValueError('Clip must be a simple MP4 filename')
        if not 0<seconds<=1800: raise ValueError('Replay length out of range')
        self.futures=[future for future in self.futures if not future.done()]
        if len(self.futures)>=4: raise RuntimeError('Replay save queue is full')
        if (self.root/'clips'/filename).exists() or (self.root/'clips'/(filename+'.json')).exists(): raise FileExistsError(filename)
        selected=list(segments or [s for s in self.ring if s['end']>self.ring[-1]['end']-seconds]) if self.ring else list(segments or [])
        if not selected: raise ValueError('No complete segments yet')
        directory=self.root/'pins'/uuid.uuid4().hex; directory.mkdir()
        try:
            for index,segment in enumerate(selected): os.link(segment['path'],directory/f'{index:08}.mkv')
        except Exception:
            shutil.rmtree(directory); raise
        status=self.root/'clips'/(filename+'.json'); requested=time.monotonic()
        atomic_json(status,dict(state='saving',video_reencoded=False,segments=len(selected)))
        def finish():
            try:
                listing=directory/'concat.txt'
                first_pts=[]
                for index in range(len(selected)):
                    result=subprocess.run([str(Path(self.ffmpeg).with_name('ffprobe.exe')),'-v','error','-select_streams','v:0',
                        '-read_intervals','%+#1','-show_entries','packet=pts_time','-of','csv=p=0',str(directory/f'{index:08}.mkv')],
                        capture_output=True,timeout=3)
                    if result.returncode: raise RuntimeError('Could not read pinned segment video clock')
                    first_pts.append(float(result.stdout.decode().strip().splitlines()[0]))
                lines=[]
                for index,segment in enumerate(selected):
                    lines.append(f"file '{index:08}.mkv'\n")
                    # The CSV includes initial AAC priming; concat normalizes the file start. Remove that offset, not audio tails.
                    duration=segment['end']-segment['start']-first_pts[index]
                    lines.append(f'duration {duration:.9f}\n')
                listing.write_text(''.join(lines))
                stage=directory/'clip.mp4'
                job=Job()
                try:
                    process=subprocess.Popen([str(self.ffmpeg),'-v','error','-nostdin','-n','-f','concat','-safe','1','-i',str(listing),
                                              '-map','0','-c','copy','-movflags','+faststart',str(stage)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                                              creationflags=subprocess.CREATE_NO_WINDOW)
                    job.attach(process)
                    try: _,error=process.communicate(timeout=15)
                    except subprocess.TimeoutExpired:
                        process.kill(); process.communicate(timeout=2); raise
                    if process.returncode: raise RuntimeError(error.decode(errors='replace')[-1500:])
                finally: job.close()
                output=self.root/'clips'/filename; os.link(stage,output)
                atomic_json(status,dict(state='ready',video_reencoded=False,path=str(output),bytes=output.stat().st_size,
                    playable_seconds=time.monotonic()-requested,segments=len(selected)))
                return output
            except Exception as error:
                self.errors=(self.errors+[str(error)])[-16:]; atomic_json(status,dict(state='failed',error=str(error))); raise
            finally: shutil.rmtree(directory)
        future=self.pool.submit(finish); self.futures.append(future); return future
    def requests(self):
        for path in (self.root/'requests').glob('*.json'):
            claimed=path.with_suffix('.processing')
            try:
                path.rename(claimed); data=json.loads(claimed.read_text()); self.save(float(data['seconds']),data['filename'])
                claimed.rename(path.with_suffix('.accepted'))
            except Exception as error:
                atomic_json(path.with_suffix('.failed'),dict(error=str(error)))
                claimed.unlink(missing_ok=True)
    def close(self):
        self.pool.shutdown(wait=True,cancel_futures=False)
