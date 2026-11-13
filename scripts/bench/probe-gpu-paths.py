"""Controlled desktop hardware path probes; no microphone or test sounds.
The project's own fullscreen fixture covers the desktop while frames are acquired.
Does not change display settings, drivers, FFmpeg or installed application files.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    out = a.out.resolve(); out.relative_to(ROOT/'test-output')
    out.mkdir(parents=True, exist_ok=False)
    lock = json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
    ff = ROOT/'tools'/lock['runtime_dir']/'ffmpeg.exe'
    proxy = ROOT/'src-tauri/target/release/replayx-gameproxy.exe'
    subprocess.run([str(proxy), '--texture-probe', '1', '--log', str(out/'textures.json')], check=True, timeout=15)
    fixture = subprocess.Popen([str(proxy), '--seconds', '75', '--detail', '1', '--iters', '500', '--log', str(out/'fixture.json')])
    results = []
    try:
        time.sleep(3)
        for name, graph in {
            'direct': 'null',
            'd3d11-nv12': 'scale_d3d11=width=1920:height=1080:format=nv12',
            'd3d11-resize': 'scale_d3d11=width=1440:height=1080:format=nv12',
            'cuda-interop': 'hwmap=derive_device=cuda:mode=read,scale_cuda=w=1920:h=1080',
        }.items():
            began = time.perf_counter()
            try:
                r = subprocess.run([str(ff), '-hide_banner', '-nostdin', '-v', 'verbose',
                    '-f', 'lavfi', '-i', 'ddagrab=framerate=60:draw_mouse=0', '-vf', graph,
                    '-frames:v', '60', '-an', '-c:v', 'h264_nvenc', '-preset', 'p5',
                    '-rc-lookahead', '20', '-f', 'null', '-'], capture_output=True, timeout=15)
                (out/f'{name}.log').write_bytes(r.stderr)
                results.append(dict(path=name, exit=r.returncode, wall_seconds=time.perf_counter()-began))
            except subprocess.TimeoutExpired:
                results.append(dict(path=name, exit=None, timeout=True))
    finally:
        fixture.wait(timeout=90)
    (out/'results.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
    print(json.dumps(results, indent=2))

if __name__ == '__main__':
    main()
