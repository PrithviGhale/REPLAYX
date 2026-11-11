"""Hardware ROI/codec regressions on our silent fullscreen fixture only.
No display-mode changes, real game, microphone or user settings. ROI tests are
not proof of physical 4:3 fullscreen/NVIDIA scaling behavior.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve(); out.relative_to(ROOT/'test-output')
    out.mkdir(parents=True, exist_ok=False)
    rel = ROOT/'src-tauri/target/release'
    lock = json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
    ff = ROOT/'tools'/lock['runtime_dir']/'ffmpeg.exe'
    cases = [('h264',None,'smooth'), ('hevc',None,'smooth'), ('av1',None,'smooth'),
             ('h264','1280x960','smooth'), ('h264','1440x1080','smooth'), ('h264','1280x960','exact')]
    fixture = subprocess.Popen([str(rel/'replayx-gameproxy.exe'), '--seconds','115','--iters','700',
        '--detail','1','--log',str(out/'fixture.json')])
    results = []
    try:
        time.sleep(3)
        for index,(codec,region,policy) in enumerate(cases):
            case = out/f'{index}-{codec}-{region or "full"}-{policy}'; case.mkdir()
            cmd = [str(rel/'replayx-cli.exe'),'capture-test','8','silent','fps=60','height=1080','width=1920',
                   'mode=stretch',f'codec={codec}','encoder=h264_nvenc',f'scale_policy={policy}']
            if region: cmd.append(f'region={region}')
            with (case/'capture.log').open('w') as log:
                recorder = subprocess.Popen(cmd,cwd=case,stdout=log,stderr=log)
                deadline = time.monotonic()+60
                try:
                    while recorder.poll() is None:
                        if fixture.poll() is not None: raise RuntimeError('Fixture closed; aborting uncovered capture')
                        if time.monotonic()>deadline: raise TimeoutError('Capture deadline')
                        time.sleep(.1)
                finally:
                    if recorder.poll() is None: recorder.terminate(); recorder.wait(timeout=15)
            reports = list((case/'test-output').glob('capture-test-*/report.json'))
            result = dict(codec=codec,region=region,policy=policy,exit=recorder.returncode)
            if recorder.returncode == 0 and len(reports)==1:
                report = json.loads(reports[0].read_text(encoding='utf-8'))
                result.update(actual_codec=report['probe']['video_codec'],width=report['probe']['width'],
                    height=report['probe']['height'],save_ms=report.get('first_ready_ms'),
                    actual_encoder=report['status']['encoder'],scaling=report['status']['scaling'])
                result['decodes']=[]
                for key in ['first','concurrent']:
                    if report.get(key):
                        d=subprocess.run([str(ff),'-v','error','-i',report[key],'-map','0','-f','null','-'],capture_output=True,timeout=45)
                        result['decodes'].append(dict(exit=d.returncode,clean=not d.stderr.strip()))
                subprocess.run([sys.executable,str(ROOT/'scripts/frame-pacing.py'),report['first'],
                    '--out',str(case/'pacing.json')],stdout=subprocess.DEVNULL,check=True,timeout=45)
            results.append(result)
            (out/'results.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
            print(json.dumps(result),flush=True)
    finally:
        fixture.wait(timeout=130)
    if any(r['exit'] or r.get('actual_codec')!=r['codec'] or not r.get('actual_encoder','').endswith('_nvenc') or len(r.get('decodes',[]))!=2
           or any(d['exit'] or not d['clean'] for d in r.get('decodes',[])) for r in results):
        raise SystemExit(1)

if __name__=='__main__': main()
