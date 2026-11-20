"""Experimental native V1 session harness. No application settings or defaults change.
Live mode records only an already-running supported foreground game via the native guard.
Use --synthetic for terminal-only generated textures/audio (nothing is played).
Replay requests: write {"seconds": 6, "filename": "moment.mp4"} into SESSION/requests/name.json.
"""
import argparse
import json
import os
import psutil
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time
from native_v1_support import Job, ReplayStore, atomic_json, mux_arguments

ROOT=Path(__file__).resolve().parents[2]
BUILD=ROOT/'src-tauri/target/release'

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--session',type=Path,required=True,help='Fresh folder under this worktree/test-output')
    parser.add_argument('--seconds',type=int,default=30)
    parser.add_argument('--pid',type=int)
    parser.add_argument('--sources',type=Path,help='Explicit AudioSource JSON array, system/microphone only; omitted means silent')
    parser.add_argument('--synthetic',action='store_true')
    parser.add_argument('--synthetic-audio',action='store_true')
    parser.add_argument('--replay-only',action='store_true')
    parser.add_argument('--replay-seconds',type=int,default=60)
    parser.add_argument('--buffer-mib',type=int,default=256)
    args=parser.parse_args()
    root=args.session.resolve()
    if not root.is_relative_to((ROOT/'test-output').resolve()) or root==(ROOT/'test-output').resolve(): parser.error('Session must be a fresh child of this worktree/test-output')
    if not 2<=args.seconds<=600 or not 2<=args.replay_seconds<=1800 or not 16<=args.buffer_mib<=4096: parser.error('Duration/buffer outside experimental bounds')
    if not args.synthetic and not args.pid: parser.error('Live mode requires the explicitly selected supported foreground-game PID')
    if args.synthetic and args.sources: parser.error('Synthetic mode never opens real audio devices')
    if args.synthetic_audio and not args.synthetic: parser.error('Synthetic audio requires synthetic mode')
    lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig')); ffmpeg=ROOT/'tools'/lock['runtime_dir']/'ffmpeg.exe'
    required=[BUILD/'replayx-native-proto.exe',ffmpeg]
    if args.sources: required.append(BUILD/'replayx-native-audio.exe')
    if any(not p.exists() for p in required): parser.error('Build the native prototype/audio bridge and prepare the existing runtime first')
    root.mkdir(parents=True); sources=[]
    if args.sources:
        sources=json.loads(args.sources.read_text(encoding='utf-8-sig'))
        if not isinstance(sources,list) or not 1<=len(sources)<=2 or any(s.get('kind') not in ('system','microphone') or not s.get('enabled',True) for s in sources): parser.error('Explicit one/two enabled system or microphone sources required')
        atomic_json(root/'sources.json',sources)
    store=ReplayStore(root,ffmpeg,args.replay_seconds,args.buffer_mib*1024*1024,not args.replay_only)
    handles=[]; processes=[]; job=Job(); relay=None; connection=None; listener=None; failure=None; audio=None; native=None; mux=None
    started=time.monotonic(); status=dict(state='starting',healthy=False,experimental=True,production_defaults_changed=False,controller_pid=os.getpid(),controller_created=psutil.Process().create_time())
    audio_stop=root/'audio.stop'; stop=root/'stop'; relay_errors=[]
    def launch(command,name,**options):
        log=(root/(name+'.log')).open('wb'); handles.append(log)
        process=subprocess.Popen(list(map(str,command)),cwd=root,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW,**options)
        processes.append(process); job.attach(process); return process
    def await_json(path,timeout):
        until=time.monotonic()+timeout
        while time.monotonic()<until:
            try: return json.loads(path.read_text())
            except (FileNotFoundError,ValueError,PermissionError): pass
            if any(p.poll() not in (None,0) for p in processes): raise RuntimeError('Worker stopped during initialization; inspect session logs')
            time.sleep(.02)
        raise TimeoutError(f'Timed out waiting for {path.name}')
    try:
        atomic_json(root/'status.json',status)
        feeds=[]
        if args.sources:
            audio=launch([BUILD/'replayx-native-audio.exe','--sources',root/'sources.json','--seconds',str(args.seconds+30),
                          '--stop-file',audio_stop,'--ready',root/'audio-ready.json','--health',root/'audio-health.json'],'audio',stdout=subprocess.DEVNULL)
            feeds=await_json(root/'audio-ready.json',25)['feeds']
        listener=socket.socket(); listener.bind(('127.0.0.1',0)); listener.listen(1); listener.settimeout(5)
        command=[BUILD/'replayx-native-proto.exe','--seconds',str(args.seconds),'--stream-address',f'127.0.0.1:{listener.getsockname()[1]}',
                 '--origin-file',root/'origin.json','--stop-file',stop]
        if args.synthetic: command+=['--synthetic','regular80',root/'unused.h264','--paced','1']
        else: command+=['--record','--pid',str(args.pid),'--out',root/'unused.h264','--mode','monitor']
        report=(root/'native.json').open('wb'); handles.append(report)
        native=launch(command,'native',stdout=report)
        connection,_=listener.accept(); connection.settimeout(3); listener.close(); listener=None
        origin=await_json(root/'origin.json',5)['origin_qpc_hns']
        if args.synthetic_audio:
            feeds=[dict(name='Synthetic system',origin_qpc_hns=0),dict(name='Synthetic microphone',origin_qpc_hns=0)]
        mux=launch([ffmpeg,*mux_arguments(feeds,origin,args.synthetic_audio)],'mux',stdin=subprocess.PIPE,stdout=subprocess.DEVNULL)
        def forward():
            try:
                while True:
                    block=connection.recv(65536)
                    if not block: break
                    mux.stdin.write(block)
            except Exception as error: relay_errors.append(str(error))
            finally:
                try: mux.stdin.close()
                except OSError: pass
        relay=threading.Thread(target=forward,daemon=True); relay.start()
        deadline=time.monotonic()+args.seconds+15; last_segments=time.monotonic(); last_count=0; last_status=0
        while time.monotonic()<deadline:
            store.refresh(); store.requests(); store.prune()
            if len(store.seen)!=last_count: last_count=len(store.seen); last_segments=time.monotonic()
            if shutil.disk_usage(root).free<512*1024*1024: raise RuntimeError('Less than 512 MiB free; recording stopped to preserve completed files')
            if audio:
                health=await_json(root/'audio-health.json',1)
                if not health.get('healthy') or audio.poll() is not None: raise RuntimeError(f'Audio failed: {health}')
            if native.poll() is not None:
                if native.returncode: raise RuntimeError('Native capture/encoder failed; completed segments preserved')
                break
            if mux.poll() is not None:
                if mux.returncode==0 and not relay.is_alive():
                    native.wait(timeout=3)
                    if native.returncode==0: break
                raise RuntimeError('Muxer stopped while recording')
            if time.monotonic()-last_segments>8: raise RuntimeError('No completed video segment for eight seconds; recording is unhealthy')
            status.update(state='recording',healthy=bool(store.ring),closed_segments=len(store.seen),ring_bytes=sum(s['bytes'] for s in store.ring))
            if time.monotonic()-last_status>=1:
                atomic_json(root/'status.json',status); last_status=time.monotonic()
            time.sleep(.1)
        else: raise TimeoutError('Native session exceeded its deadline')
        relay.join(timeout=3)
        if relay.is_alive() or relay_errors: raise RuntimeError(f'Video transport failed: {relay_errors}')
        mux.wait(timeout=8)
        if mux.returncode: raise RuntimeError('Muxer could not finalize segments')
        report.flush(); native_report=json.loads((root/'native.json').read_text())
        if (not args.synthetic and not native_report.get('success')) or native_report['frames_out']!=native_report['encoded'] or native_report['live_input_samples']!=0 or not native_report.get('output_timestamps_regular'):
            raise RuntimeError('Native completion report failed integrity checks')
        store.refresh(); store.requests(); store.prune()
        if store.record: store.save(1800,'recording.mp4',store.record)
    except (Exception,KeyboardInterrupt) as error:
        failure=str(error) or 'Cancelled by user'
    finally:
        stop.touch(exist_ok=True); audio_stop.touch(exist_ok=True)
        if connection:
            try: connection.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            connection.close()
        if listener: listener.close()
        # Grace first; the job enforces a final bound, including driver/pipe shutdown stalls.
        until=time.monotonic()+5
        while any(p.poll() is None for p in processes) and time.monotonic()<until: time.sleep(.05)
        job.close()
        for process in processes:
            try: process.wait(timeout=2)
            except subprocess.TimeoutExpired: failure=failure or 'A worker survived job termination'
        store.close()
        for handle in handles: handle.close()
        if store.errors: failure=failure or '; '.join(store.errors)
        if failure:
            # Never silently label a stopped/restarted stream as continuous recording.
            status.update(state='fallback_required',healthy=False,error=failure,
                          fallback_safe=all(p.poll() is not None for p in processes),automatic_fallback=False)
        else: status.update(state='complete',healthy=False,error=None)
        status.update(elapsed_seconds=time.monotonic()-started,closed_segments=len(store.seen))
        atomic_json(root/'status.json',status)
    print(json.dumps(status,indent=2))
    if failure: raise SystemExit(1)
if __name__=='__main__': main()
