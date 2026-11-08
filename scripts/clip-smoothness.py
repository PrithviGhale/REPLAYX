"""Smoothness of any recording (real gameplay, no test pattern needed).

Decodes every frame at 160x90 grey and measures how much it differs from the
previous one. In moving gameplay a frame that is (almost) identical to the
previous one is a repeated picture: the viewer sees a hitch. Reports the
repeat rate, how repeats cluster (bursts = visible stutter), the longest
freeze, and whether container timestamps are evenly spaced.
Read-only; standard library + the pinned FFmpeg runtime.
usage: python -I scripts/clip-smoothness.py CLIP [--threshold 0.35]
"""
import collections
import json
import os
import subprocess
import sys

clip = sys.argv[1]
threshold = float(sys.argv[sys.argv.index("--threshold") + 1]) if "--threshold" in sys.argv else 0.35
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
lock = json.loads(open(os.path.join(root, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig").read())
bindir = os.path.join(root, "tools", lock["runtime_dir"])
ffmpeg, ffprobe = os.path.join(bindir, "ffmpeg.exe"), os.path.join(bindir, "ffprobe.exe")
info = json.loads(subprocess.check_output([ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries",
                                           "stream=codec_name,width,height,avg_frame_rate,r_frame_rate:format=duration:format_tags",
                                           "-of", "json", clip]))
pts = [float(x) for x in subprocess.check_output(
    [ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time", "-of", "csv=p=0", clip],
    text=True).split() if x and x[0].isdigit()]
W, H = 160, 90
proc = subprocess.Popen([ffmpeg, "-v", "error", "-i", clip, "-map", "0:v:0", "-vf", f"scale={W}:{H}:flags=area,format=gray",
                         "-fps_mode", "passthrough", "-f", "rawvideo", "-"], stdout=subprocess.PIPE)
diffs, prev = [], None
while True:
    raw = proc.stdout.read(W * H)
    if len(raw) < W * H:
        break
    if prev is not None:
        diffs.append(sum(abs(a - b) for a, b in zip(raw, prev)) / (W * H))
    prev = raw
proc.wait()
fps_s = info["streams"][0]["avg_frame_rate"].split("/")
fps = float(fps_s[0]) / float(fps_s[1])
rep = [d < threshold for d in diffs]
runs, run = [], 0
for r in rep:
    if r:
        run += 1
    elif run:
        runs.append(run)
        run = 0
if run:
    runs.append(run)
# Per-second unique-frame counts: how "FPS" fluctuates as the viewer sees it.
per_sec = collections.Counter()
for i, r in enumerate(rep):
    if not r:
        per_sec[int((i + 1) / fps)] += 1
secs = [per_sec.get(s, 0) for s in range(int(len(rep) / fps))]
intervals = collections.Counter(round((b - a) * 1000, 1) for a, b in zip(pts, pts[1:]))
moving = sorted(diffs)[len(diffs) // 2] if diffs else 0
print(json.dumps({
    "clip": os.path.basename(clip),
    "stream": info["streams"][0],
    "frames": len(diffs) + 1,
    "median_frame_change": round(moving, 2),
    "repeated_pct": round(100 * sum(rep) / max(1, len(rep)), 2),
    "repeat_runs_histogram": dict(sorted(collections.Counter(runs).items())),
    "longest_freeze_ms": round(max(runs, default=0) * 1000 / fps, 1),
    "unique_frames_per_second_min_median_max": [min(secs, default=0), sorted(secs)[len(secs) // 2] if secs else 0, max(secs, default=0)],
    "unique_frames_per_second": secs[:60],
    "timestamp_intervals_ms": dict(sorted(intervals.items())[:12]),
}, indent=1))
