"""Actual GPU encode/decode audit against an independent timestamp oracle.
Generated D3D textures only; no desktop/audio capture. Choose an available vendor.
"""
import argparse,bisect,json,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--vendor',default='10de');ap.add_argument('--seconds',type=int,default=3);ap.add_argument('--release-delay-ms',type=int,default=0);ap.add_argument('--case',action='append');a=ap.parse_args()
 lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'));bins=ROOT/'tools'/lock['runtime_dir'];proto=ROOT/'src-tauri/target/release/replayx-native-proto.exe'
 def run(cmd):return subprocess.run(list(map(str,cmd)),capture_output=True,check=True,timeout=30).stdout
 cases=['exact60','regular80','rate120','rate144','rate240','rate250','jitter','gap','starve50','slow30','static60']
 if a.case:
  assert all(c in cases for c in a.case);cases=a.case
 base=ROOT/'test-output/native-smoothness';base.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='grid-',dir=base) as tmp:
  for case in cases:
   clip=Path(tmp)/(case+'.h264');r=json.loads(run([proto,'--synthetic',case,clip,'--seconds',a.seconds,'--vendor',a.vendor,'--pacing','clocked','--trace-schedule','--test-release-delay-ms',a.release_delay_ms]))
   assert r['source_schedule_complete'],'Source schedule was truncated; choose a shorter test'
   times=r['source_times_hns'];origin=times[0];end=r['source_end_hns'];targets=range(0,((end-origin)*60+10_000_000-1)//10_000_000)
   expected=[bisect.bisect_right(times,origin+n*10_000_000//60)-1 for n in targets]
   assert r['shown_captures']==expected,(case,'Wrong source schedule')
   raw=run([bins/'ffmpeg.exe','-v','error','-xerror','-threads','2','-i',clip,'-vf','crop=16:16:952:532,extractplanes=y','-f','rawvideo','-'])
   levels=[sum(raw[i:i+256])/256 for i in range(0,len(raw),256)];wanted=[126 if case=='static60' else 16+(i*7)%220 for i in expected]
   assert len(levels)==len(expected)==r['frames_out']==r['encoded'],(case,'Frame loss')
   assert all(abs(x-y)<=3 for x,y in zip(levels,wanted)),(case,'Wrong decoded content')
   frames=json.loads(run([bins/'ffprobe.exe','-v','error','-select_streams','v:0','-show_frames','-show_entries','frame=key_frame,pict_type','-of','json',clip]))['frames']
   assert [i for i,f in enumerate(frames) if f['key_frame']]==list(range(0,len(frames),120)),(case,'Wrong GOP')
   assert not any(f['pict_type']=='B' for f in frames)
   assert r['live_input_samples']==0 and r['output_timestamps_regular']
   if a.release_delay_ms:assert r['surface_reuse_waits']>0,(case,'Backpressure injection did not exercise surface waits')
   print(f"{a.vendor} {case}: PASS {len(expected)} decoded pictures; zero wrong IDs/grid choices; live leases 0",flush=True)
if __name__=='__main__':main()
