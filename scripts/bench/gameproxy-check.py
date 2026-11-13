"""Frame-by-frame smoothness of a recording of replayx-gameproxy.

The proxy stamps every rendered frame with its render time (24 bits, 0.25 ms
units). This reads the stamp from every decoded frame of a recording and
reports how far the game's clock advanced between consecutive video frames:
ideal is one output interval (16.67 ms at 60 fps, 8.33 ms at 120 fps).
  repeated   frame shows the same game frame as the previous one (0 ms)
  freeze     longest run of repeated frames, in ms
  late/jump  game time advanced by more than 1.5 intervals (a stall then catch-up)
Standard library + the pinned FFmpeg runtime only.
usage: python -I scripts/bench/gameproxy-check.py CLIP [--skip-seconds 1]
"""
import collections
import json
import os
import statistics
import subprocess
import sys

clip = sys.argv[1]
skip = float(sys.argv[sys.argv.index("--skip-seconds") + 1]) if "--skip-seconds" in sys.argv else 1.0
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
lock = json.loads(open(os.path.join(root, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig").read())
bindir = os.path.join(root, "tools", lock["runtime_dir"])
ffmpeg, ffprobe = os.path.join(bindir, "ffmpeg.exe"), os.path.join(bindir, "ffprobe.exe")
probe = json.loads(subprocess.check_output([ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries",
                                            "stream=width,height,avg_frame_rate,r_frame_rate", "-of", "json", clip]))
st = probe["streams"][0]
num, den = st["avg_frame_rate"].split("/")
fps = float(num) / float(den)
W, H = 320, 180
proc = subprocess.Popen([ffmpeg, "-v", "error", "-i", clip, "-map", "0:v:0", "-vf",
                         f"scale={W}:{H}:flags=area,format=gray", "-fps_mode", "passthrough",
                         "-f", "rawvideo", "-"], stdout=subprocess.PIPE)
codes = []
while True:
    raw = proc.stdout.read(W * H)
    if len(raw) < W * H:
        break
    v = 0
    for i in range(24):
        x0, y0 = 10 + i * 12, 9
        if sum(raw[(y0 + dy) * W + x0 + dx] for dy in range(4) for dx in range(4)) / 16 > 128:
            v |= 1 << i
    codes.append(v)
proc.wait()
first = int(skip * fps)
codes = codes[first:]
ms = [c * 0.25 for c in codes]
deltas = [b - a for a, b in zip(ms, ms[1:])]
# Ignore frames that are not the proxy (desktop before/after it): negative or absurd jumps.
valid = [d for d in deltas if 0 <= d < 1000]
ideal = 1000 / fps
runs, run = [], 0
for d in valid:
    if d == 0:
        run += 1
    else:
        if run:
            runs.append(run)
        run = 0
if run:
    runs.append(run)


def pct(p):
    s = sorted(valid)
    return round(s[min(len(s) - 1, int(p / 100 * len(s)))], 2) if s else None


report = {
    "clip": os.path.basename(clip),
    "size": f'{st["width"]}x{st["height"]}',
    "fps": round(fps, 3),
    "frames": len(valid) + 1,
    "ideal_ms": round(ideal, 3),
    "repeated_pct": round(100 * sum(1 for d in valid if d == 0) / max(1, len(valid)), 2),
    "unique_pct": round(100 * sum(1 for d in valid if d > 0) / max(1, len(valid)), 2),
    "longest_freeze_ms": round(max(runs, default=0) * ideal, 1),
    "jumps_over_1_5x_pct": round(100 * sum(1 for d in valid if d > 1.5 * ideal) / max(1, len(valid)), 2),
    "advance_ms_p50_p95_p99_max": [pct(50), pct(95), pct(99), round(max(valid), 2) if valid else None],
    "advance_ms_stdev": round(statistics.pstdev(valid), 2) if len(valid) > 1 else None,
    "advance_histogram_ms": dict(sorted(collections.Counter(round(d) for d in valid).items())),
}
print(json.dumps(report, indent=1))
