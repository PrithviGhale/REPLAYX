"""Windowed silent smoke check of the unchanged normal FFmpeg NVENC engine."""
from pathlib import Path
import argparse,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--name',default='ffmpeg-regression');a=ap.parse_args()
 if not a.name.replace('-','').isalnum():raise ValueError('Invalid output name')
 out=ROOT/'test-output/native-smoothness'/a.name;out.mkdir();stop=out/'source.stop'
 src=subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-motion-source.exe'),'--seconds','25','--fps','60','--log',str(out/'source.jsonl'),'--stop-file',str(stop)],creationflags=subprocess.CREATE_NO_WINDOW)
 try:
  time.sleep(1)
  with (out/'cli.log').open('w') as so,(out/'cli-error.log').open('w') as se:
   r=subprocess.run([str(ROOT/'src-tauri/target/release/replayx-cli.exe'),'capture-test','6','silent','fps=60','width=1920','height=1080','codec=h264','preset=balanced','scale_policy=exact','mode=stretch','capture_api=wgc','encoder=h264_nvenc'],cwd=out,stdout=so,stderr=se,timeout=35,creationflags=subprocess.CREATE_NO_WINDOW)
  assert r.returncode==0,(out/'cli-error.log').read_text()[-1500:]
 finally:
  stop.touch()
  try:src.wait(timeout=4)
  except subprocess.TimeoutExpired:src.terminate();src.wait(timeout=3)
 report=list((out/'test-output').glob('capture-test-*/report.json'));assert len(report)==1
 d=json.loads(report[0].read_text());assert d['status']['recording_engine']=='ffmpeg' and d['status']['encoder']=='h264_nvenc',d['status']
 lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'));ff=ROOT/'tools'/lock['runtime_dir']/'ffmpeg.exe'
 for clip in [d['first'],d['concurrent']]:
  subprocess.run([str(ff),'-v','error','-xerror','-threads','2','-i',clip,'-map','0','-f','null','-'],capture_output=True,check=True,timeout=30)
 assert (d['probe']['width'],d['probe']['height'],d['probe']['audio_tracks'])==(1920,1080,0)
 (out/'smoke.json').write_text(json.dumps({'status':'PASS','actual_backend':d['status']['recording_engine'],'encoder':d['status']['encoder'],'first':d['first'],'concurrent':d['concurrent'],'probe':d['probe'],'note':'Actual silent WGC/NVENC regression; not a new FFmpeg smoothness/performance certification.'},indent=2))
 print('PASS: normal FFmpeg WGC/NVENC, two concurrent replay clips, full decoding, 1080p60, no audio devices.')
if __name__=='__main__':main()
