"""Generated-media V1 checks: no screen/audio-device capture or audible output."""
import array
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

ROOT=Path(__file__).parents[1]
sys.path.insert(0,str(ROOT/'scripts/bench'))
from native_v1_support import ReplayStore, audio_filter, mux_arguments
sys.path.pop(0)

class ClockContract(unittest.TestCase):
    def test_each_source_uses_its_own_clock_origin(self):
        self.assertIn('start_sample=4800',audio_filter(1,9_000_000,10_000_000))
        self.assertIn('adelay=2400S',audio_filter(2,10_500_000,10_000_000))
        self.assertIn('start_sample=0',audio_filter(2,10_500_000,10_000_000))
    def test_mux_copies_video_and_assigns_explicit_cfr_timestamps(self):
        command=mux_arguments([],0)
        self.assertEqual(command[command.index('-c:v')+1],'copy')
        self.assertIn('setts=time_base=1/60000:prescale=1:pts=N*1000:dts=N*1000:duration=1000',command)
        self.assertNotIn('h264_qsv',command)
    def test_snapshot_rejects_paths_and_pins_before_retention(self):
        with tempfile.TemporaryDirectory(dir=ROOT/'test-output') as tmp:
            root=Path(tmp); store=ReplayStore(root,'not-run',2,100,False)
            try:
                for index in range(3): (root/f'segment-{index:08}.mkv').write_bytes(b'fixture')
                (root/'segments.csv').write_text(''.join(f'segment-{i:08}.mkv,{i*2},{i*2+2}\n' for i in range(3)))
                store.refresh(); self.assertEqual(len(store.ring),3)
                with self.assertRaises(ValueError): store.save(2,'../wrong.mp4')
                store.prune(); self.assertEqual(len(store.ring),1)
                self.assertTrue((root/'segment-00000002.mkv').exists())
            finally: store.close()

@unittest.skipUnless(os.environ.get('REPLAYX_NATIVE_TEST')=='1','Opt-in generated native GPU encoding')
class GeneratedV1(unittest.TestCase):
    def test_mux_replay_concurrency_pacing_and_synthetic_audio_alignment(self):
        lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
        runtime=ROOT/'tools'/lock['runtime_dir']; ffmpeg=runtime/'ffmpeg.exe'; probe=runtime/'ffprobe.exe'
        with tempfile.TemporaryDirectory(prefix='native-v1-',dir=ROOT/'test-output') as tmp:
            session=Path(tmp)/'session'
            process=subprocess.Popen([sys.executable,str(ROOT/'scripts/bench/native-v1.py'),'--synthetic','--synthetic-audio',
                '--seconds','6','--session',str(session),'--replay-seconds','2'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                until=time.monotonic()+8
                while not (session/'segments.csv').exists() or not (session/'segments.csv').read_text().strip():
                    self.assertIsNone(process.poll(),'V1 exited before its first segment')
                    self.assertLess(time.monotonic(),until); time.sleep(.03)
                for index in range(2):
                    (session/'requests'/f'{index}.json').write_text(json.dumps(dict(seconds=2,filename=f'replay-{index}.mp4')))
                stdout,stderr=process.communicate(timeout=25)
                self.assertEqual(process.returncode,0,(stdout+stderr).decode(errors='replace'))
                self.assertEqual(json.loads((session/'status.json').read_text())['state'],'complete')
                native=json.loads((session/'native.json').read_text())
                for name in ('replay-0.mp4','replay-1.mp4','recording.mp4'):
                    clip=session/'clips'/name
                    status=json.loads((session/'clips'/(name+'.json')).read_text())
                    self.assertEqual(status['state'],'ready'); self.assertFalse(status['video_reencoded'])
                    data=json.loads(subprocess.run([str(probe),'-v','error','-show_streams','-show_packets','-of','json',str(clip)],capture_output=True,timeout=15,check=True).stdout)
                    streams=data['streams']; self.assertEqual([s['codec_name'] for s in streams],['h264','aac','aac'])
                    video=[p for p in data['packets'] if p['stream_index']==0]
                    times=[float(p['pts_time']) for p in video]
                    self.assertTrue(all(abs(b-a-1/60)<.0011 for a,b in zip(times,times[1:])),f'{name}: discontinuous video timestamps')
                    self.assertEqual(video[0]['flags'][0],'K')
                    subprocess.run([str(ffmpeg),'-v','error','-xerror','-i',str(clip),'-map','0','-f','null','-'],capture_output=True,timeout=15,check=True)
                    if name=='recording.mp4':
                        self.assertEqual(len(video),native['frames_out'])
                        raw_video=subprocess.run([str(ffmpeg),'-v','error','-i',str(clip),'-vf','crop=16:16:952:532,extractplanes=y','-fps_mode','passthrough','-f','rawvideo','-'],capture_output=True,timeout=15,check=True).stdout
                        levels=[sum(raw_video[i:i+256])/256 for i in range(0,len(raw_video),256)]
                        self.assertEqual(len(levels),len(native['expected_luma']))
                        self.assertTrue(all(abs(a-b)<=3 for a,b in zip(levels,native['expected_luma'])))
                        # Both independent generated pulse tracks peak at the same 1-second video clock.
                        peaks=[]
                        for track in range(2):
                            raw=subprocess.run([str(ffmpeg),'-v','error','-i',str(clip),'-map',f'0:a:{track}','-ac','1','-ar','48000','-f','f32le','-'],capture_output=True,timeout=15,check=True).stdout
                            samples=array.array('f'); samples.frombytes(raw)
                            active=[i for i,v in enumerate(samples) if abs(v)>.05]
                            onsets=[active[0]]+[b for a,b in zip(active,active[1:]) if b-a>4800]
                            self.assertEqual(len(onsets),3)
                            moments=[i/48000 for i in onsets]; peaks.append(moments)
                            for actual,expected in zip(moments,(1,3,5)):
                                self.assertLess(abs(actual-(times[0]+expected)),.005)
                        self.assertTrue(all(abs(a-b)<.005 for a,b in zip(peaks[0],peaks[1])))
                self.assertLessEqual(len(list(session.glob('segment-*.mkv'))),2)
                self.assertNotIn('Timestamps are unset',(session/'mux.log').read_text())
            finally:
                if process.poll() is None: process.kill(); process.communicate(timeout=5)

    def test_stop_preserves_completed_segments_and_supports_offline_recovery(self):
        with tempfile.TemporaryDirectory(prefix='native-v1-stop-',dir=ROOT/'test-output') as tmp:
            session=Path(tmp)/'session'
            process=subprocess.Popen([sys.executable,str(ROOT/'scripts/bench/native-v1.py'),'--synthetic','--seconds','8','--session',str(session)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                until=time.monotonic()+7
                while not (session/'recovery.json').exists():
                    self.assertIsNone(process.poll()); self.assertLess(time.monotonic(),until); time.sleep(.03)
                (session/'stop').touch()
                process.communicate(timeout=12)
                self.assertNotEqual(process.returncode,0)
                status=json.loads((session/'status.json').read_text())
                self.assertEqual(status['state'],'fallback_required'); self.assertFalse(status['healthy'])
                self.assertTrue(status['fallback_safe']); self.assertFalse(status['automatic_fallback'])
                result=subprocess.run([sys.executable,str(ROOT/'scripts/bench/native-v1-recover.py'),'--session',str(session)],capture_output=True,timeout=30)
                self.assertEqual(result.returncode,0,result.stderr.decode(errors='replace'))
                self.assertTrue((session/'clips/recovered.mp4').exists())
            finally:
                if process.poll() is None:process.kill();process.communicate(timeout=5)

    def test_crashed_controller_job_stops_children_and_recovers_closed_segments(self):
        import psutil
        with tempfile.TemporaryDirectory(prefix='native-v1-crash-',dir=ROOT/'test-output') as tmp:
            session=Path(tmp)/'session'
            process=subprocess.Popen([sys.executable,str(ROOT/'scripts/bench/native-v1.py'),'--synthetic','--seconds','8','--session',str(session)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                until=time.monotonic()+7
                while not (session/'recovery.json').exists():
                    self.assertIsNone(process.poll()); self.assertLess(time.monotonic(),until); time.sleep(.03)
                active=subprocess.run([sys.executable,str(ROOT/'scripts/bench/native-v1-recover.py'),'--session',str(session)],capture_output=True,timeout=5)
                self.assertNotEqual(active.returncode,0)
                children=psutil.Process(process.pid).children(recursive=True)
                process.kill(); process.communicate(timeout=5)
                deadline=time.monotonic()+3
                while any(p.is_running() for p in children) and time.monotonic()<deadline:time.sleep(.02)
                self.assertFalse(any(p.is_running() for p in children),'Worker survived controller crash')
                result=subprocess.run([sys.executable,str(ROOT/'scripts/bench/native-v1-recover.py'),'--session',str(session)],capture_output=True,timeout=30)
                self.assertEqual(result.returncode,0,result.stderr.decode(errors='replace'))
                self.assertTrue((session/'clips/recovered.mp4').exists())
            finally:
                if process.poll() is None:process.kill();process.communicate(timeout=5)
