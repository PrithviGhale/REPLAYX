"""Approved unattended WINDOWED desktop frame-stage capture. No audio/game/settings.
Output is always a fresh directory in replayx-perf/test-output/native-smoothness.
"""
import argparse,json,subprocess,sys,time
from pathlib import Path

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--name',required=True);ap.add_argument('--seconds',type=int,default=8);ap.add_argument('--interval-us',type=int,default=0);ap.add_argument('--pacing',choices=['legacy','clocked'],default='clocked');ap.add_argument('--source',choices=['tk','d3d'],default='d3d');ap.add_argument('--fps',type=int,default=60);ap.add_argument('--static',action='store_true');ap.add_argument('--no-trace',action='store_true')
 a=ap.parse_args();root=Path(__file__).resolve().parents[2]
 if root.name.lower()!='replayx-perf' or not a.name.replace('-','').replace('_','').isalnum() or not 5<=a.seconds<=60:raise ValueError('Invalid isolated test parameters')
 out=root/'test-output/native-smoothness'/a.name;out.mkdir()
 fixture_log=out/'fixture.json'
 err=(out/'fixture.log').open('w')
 source_cmd=([sys.executable,str(root/'scripts/bench/motion-fixture.py'),'--windowed','--seconds',str(a.seconds+12),'--log',str(fixture_log)] if a.source=='tk' else [str(root/'src-tauri/target/release/replayx-motion-source.exe'),'--seconds',str(a.seconds+12),'--fps',str(a.fps),'--log',str(out/'source.jsonl'),'--static','1' if a.static else '0','--stop-file',str(fixture_log.with_suffix('.stop'))])
 fixture=subprocess.Popen(source_cmd,creationflags=subprocess.CREATE_NO_WINDOW,stdout=subprocess.DEVNULL,stderr=err)
 try:
  deadline=time.monotonic()+6
  if a.source=='tk':
   while not fixture_log.with_suffix('.clock.json').exists() and time.monotonic()<deadline:time.sleep(.02)
   g=json.loads(fixture_log.with_suffix('.geometry.json').read_text())
  else:
   while time.monotonic()<deadline:
    try:
     line=(out/'source.jsonl').read_text().splitlines()[0];g=json.loads(line);break
    except (OSError,IndexError,json.JSONDecodeError):time.sleep(.02)
   else:raise RuntimeError('D3D source did not initialize')
   fixture_log.with_suffix('.geometry.json').write_text(json.dumps(g))
  sx=g['width']/320;sy=g['height']/180
  roi=[round(g['left']+10*sx),round(g['top']+9*sy),round(12*sx),round(4*sx),round(4*sy)]
  if any(roi[i]%2 for i in [0,1,2,4]):raise ValueError(f'Stripe must be even-aligned: {roi}')
  with (out/'native.json').open('w') as so,(out/'native.log').open('w') as se:
   cmd=[str(root/'src-tauri/target/release/replayx-native-proto.exe'),'--record','--desktop','--seconds',str(a.seconds),'--display','0','--out',str(out/'capture.ts'),'--stream-ts','--pacing',a.pacing,'--interval-us',str(a.interval_us),]
   if not a.no_trace:cmd+=['--trace-file',str(out/'trace.jsonl'),'--trace-roi',','.join(map(str,roi))]
   (out/'command.json').write_text(json.dumps(cmd,indent=2))
   r=subprocess.run(cmd,stdout=so,stderr=se,timeout=a.seconds+25,creationflags=subprocess.CREATE_NO_WINDOW)
  print('Native stage capture:',r.returncode,out,flush=True)
  if r.returncode:raise RuntimeError((out/'native.log').read_text()[-3000:])
 finally:
  fixture_log.with_suffix('.stop').touch()

  try:fixture.wait(timeout=4)
  except subprocess.TimeoutExpired:fixture.terminate();fixture.wait(timeout=3)
  err.close()
 return out
if __name__=='__main__':main()
