"""Real Desktop Duplication experiments, run only on an authorized free desktop.
No audio. Paths are generated beneath test-output; never modifies user clips.
"""
import argparse
import json
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--seconds', type=int, default=20)
parser.add_argument('--mode', choices=['old', 'fps', 'round'], required=True)
parser.add_argument('--encoder', choices=['qsv', 'hybrid', 'cpu'], default='qsv')
parser.add_argument('--name', required=True)
args = parser.parse_args()
runtime = ROOT/'tools'/json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))['runtime_dir']
out = ROOT/'test-output'/'pacing'/args.name
out.mkdir(parents=True, exist_ok=False)
g = json.loads(subprocess.check_output([str(ROOT/'src-tauri/target/release/replayx-cli.exe'), 'displays']))[0]
filters = {
    'old': "setpts='if(isnan(PREV_OUTPTS),round(PTS*TB*60)/(TB*60),max(PREV_OUTPTS+1/(TB*60),round(PTS*TB*60)/(TB*60)))'",
    'fps': 'fps=fps=60:start_time=0:round=near',
    'round': 'setpts=round(PTS*TB*60)/(TB*60)',
}
scalers = {
    'qsv': 'hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12,setsar=1',
    'hybrid': 'hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12,hwdownload,format=nv12,setsar=1',
    'cpu': 'hwdownload,format=bgra,scale=1920:1080:flags=bilinear,format=yuv420p,setsar=1',
}
cmd = [str(runtime/'ffmpeg.exe'), '-hide_banner', '-nostdin', '-v', 'verbose', '-stats_period', '0.5',
       '-progress', str(out/'progress.txt'), '-f', 'lavfi', '-i',
       f"ddagrab=framerate=60:draw_mouse=0:video_size={g['active_width']}x{g['active_height']}",
       '-vf', filters[args.mode]+','+scalers[args.encoder], '-c:v', 'h264_qsv' if args.encoder=='qsv' else 'h264_nvenc',
       '-b:v', '12M', '-g', '120', '-bf', '0', '-fps_mode', 'cfr', '-t', str(args.seconds), '-n', str(out/'capture.mkv')]
begin = time.perf_counter()
with (out/'ffmpeg.log').open('w') as log:
    try:
        code = subprocess.run(cmd, stdout=log, stderr=log, timeout=args.seconds+20).returncode
    except subprocess.TimeoutExpired:
        code = -1
(out/'run.json').write_text(json.dumps(dict(command=cmd, geometry=g, exit=code, wall_seconds=time.perf_counter()-begin), indent=2))
print(out, code)
