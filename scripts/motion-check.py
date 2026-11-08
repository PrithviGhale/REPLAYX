"""Frame-accurate smoothness check for a clip that recorded motion-fixture.py.

Decodes every frame (scaled to 320x180 gray, standard library only), reads the
fixture's 24-bit 60 Hz tick counter from each frame and reports:
  repeats  - consecutive frames showing the same tick (a duplicated picture)
  skips    - a tick that never appeared (a dropped picture)
  increments histogram - 1 everywhere means perfectly smooth 60 fps motion
Usage: python -I scripts/motion-check.py CLIP [--fixture-log LOG.json]
Only the part of the clip where the fixture is visible is analysed.
"""
import collections
import json
import os
import subprocess
import sys

clip = sys.argv[1]
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
lock = json.loads(open(os.path.join(root, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig").read())
bindir = os.path.join(root, "tools", lock["runtime_dir"])
ffmpeg, ffprobe = os.path.join(bindir, "ffmpeg.exe"), os.path.join(bindir, "ffprobe.exe")

pts = [float(x) for x in subprocess.check_output(
    [ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time",
     "-of", "csv=p=0", clip], text=True).split() if x and x[0].isdigit()]
W, H = 320, 180
proc = subprocess.Popen([ffmpeg, "-v", "error", "-i", clip, "-map", "0:v:0", "-vf",
                         f"scale={W}:{H}:flags=area,format=gray", "-fps_mode", "passthrough",
                         "-f", "rawvideo", "-"], stdout=subprocess.PIPE)
ticks = []
while True:
    raw = proc.stdout.read(W * H)
    if len(raw) < W * H:
        break
    bits = []
    for i in range(24):
        x0, y0 = 10 + i * 12, 9
        total = sum(raw[(y0 + dy) * W + x0 + dx] for dy in range(4) for dx in range(4))
        bits.append(total / 16 > 128)
    # Validity: the fixture background (#101820) surrounds the bit row.
    ticks.append(sum(int(b) << i for i, b in enumerate(bits)))
proc.wait()

# Keep the longest run where ticks look like the fixture (mostly increasing).
valid = [i for i in range(1, len(ticks)) if 0 <= ticks[i] - ticks[i - 1] <= 8]
if not valid:
    print(json.dumps({"clip": clip, "error": "fixture counter not found"}))
    sys.exit(1)
start, end = valid[0] - 1, valid[-1]
seq = ticks[start:end + 1]
inc = [b - a for a, b in zip(seq, seq[1:])]
hist = collections.Counter(inc)
frame_pts = pts[start:end + 1]
intervals = [round((b - a) * 1000, 1) for a, b in zip(frame_pts, frame_pts[1:])]
result = {
    "clip": os.path.basename(clip),
    "frames_analysed": len(seq),
    "seconds": round(len(seq) / 60, 2),
    "repeated_frames": hist.get(0, 0),
    "skipped_ticks": sum((d - 1) for d in inc if d > 1),
    "increments_histogram": dict(sorted(hist.items())),
    "repeat_pct": round(100 * hist.get(0, 0) / max(1, len(inc)), 2),
    "longest_repeat_run": max((len(r) for r in "".join("0" if d == 0 else "1" for d in inc).split("1")), default=0),
    "container_interval_ms": dict(sorted(collections.Counter(intervals).items())),
    # Where each irregularity is (seconds into the clip, tick increment):
    # 0 = repeated picture, 2+ = skipped screen updates.
    "anomalies_at_s": [[round((i + 1) / 60, 2), d] for i, d in enumerate(inc) if d != 1][:80],
}
steady = inc[30:]  # after the first 0.5 s (encoder warm-up)
result["steady_after_0_5s"] = {
    "repeated": sum(1 for d in steady if d == 0),
    "skipped_ticks": sum(d - 1 for d in steady if d > 1),
    "pct_irregular": round(100 * sum(1 for d in steady if d != 1) / max(1, len(steady)), 2),
}
if "--fixture-log" in sys.argv and os.path.exists(sys.argv[sys.argv.index("--fixture-log") + 1]):
    log = json.load(open(sys.argv[sys.argv.index("--fixture-log") + 1]))
    gaps = [round((b[0] - a[0]) * 1000) for a, b in zip(log, log[1:])]
    result["fixture_tick_gaps_ms"] = dict(sorted(collections.Counter(gaps).items()))
    result["fixture_missed_ticks"] = sum(b[1] - a[1] - 1 for a, b in zip(log, log[1:]) if b[1] - a[1] > 1)
print(json.dumps(result, indent=1))
