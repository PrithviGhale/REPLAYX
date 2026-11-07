"""Measure A/V synchronization in a REPLAYX clip that recorded the flash/beep
stimulus (white flash + 1 kHz beep at the same instant once per second).

Make and play the stimulus (outside REPLAYX), then record it:
  ffmpeg -f lavfi -i "color=black:s=480x480:r=60,drawbox=c=white:t=fill:enable='lt(mod(t,1),0.05)'" \
         -f lavfi -i "aevalsrc='0.5*sin(2*PI*1000*t)*lt(mod(t,1),0.05)':s=48000" \
         -t 60 -c:v libopenh264 -c:a pcm_s16le stimulus.mkv
  ffplay -loop 0 -x 480 -y 480 -alwaysontop stimulus.mkv

Usage: python -I scripts/av-sync.py FFMPEG CLIP [audio_track]
Prints per-event offsets (audio onset minus flash onset, ms) and drift.
On the stimulus file itself the detector reads a constant -2.0 ms (its bias).
"""
import array, json, math, subprocess, sys

import os
ffmpeg, clip = os.path.abspath(sys.argv[1]), sys.argv[2]
ffprobe = os.path.join(os.path.dirname(ffmpeg), "ffprobe" + os.path.splitext(ffmpeg)[1])
track = sys.argv[3] if len(sys.argv) > 3 else "0"

# Video: mean luma per frame with timestamps (tiny grayscale frames).
probe = subprocess.run([ffprobe, "-v", "error", "-select_streams", "v",
                        "-show_entries", "frame=pts_time", "-of", "csv=p=0", clip],
                       capture_output=True, text=True, check=True).stdout
times = [float(l.split(",")[0]) for l in probe.split() if l and l[0].isdigit()]
raw = subprocess.run([ffmpeg, "-v", "error", "-i", clip, "-map", "0:v", "-vf", "scale=64:36,format=gray",
                      "-fps_mode", "passthrough", "-f", "rawvideo", "-"], capture_output=True, check=True).stdout
size = 64 * 36
luma = [sum(raw[i:i + size]) / size for i in range(0, len(raw) - size + 1, size)]
n = min(len(luma), len(times))
base = sorted(luma[:n])[n // 2]
flashes = [times[i] for i in range(1, n) if luma[i] - base > 8 and luma[i - 1] - base <= 8]

# Audio: 1 kHz energy envelope in 1 ms hops.
pcm = subprocess.run([ffmpeg, "-v", "error", "-i", clip, "-map", f"0:a:{track}", "-ac", "1", "-ar", "48000",
                      "-f", "f32le", "-"], capture_output=True, check=True).stdout
a = array.array("f"); a.frombytes(pcm)
start = float(subprocess.run([ffprobe, "-v", "error", "-select_streams", f"a:{track}",
                              "-show_entries", "stream=start_time", "-of", "csv=p=0", clip],
                             capture_output=True, text=True).stdout.strip() or 0)
hop, win = 48, 240
w = 2 * math.pi * 1000 / 48000
env = []
for s in range(0, len(a) - win, hop):
    re = sum(a[s + i] * math.cos(w * i) for i in range(win))
    im = sum(a[s + i] * math.sin(w * i) for i in range(win))
    env.append(math.hypot(re, im) * 2 / win)
peak = max(env) if env else 0
beeps = []
for i in range(1, len(env)):
    if env[i] > peak * 0.5 and env[i - 1] <= peak * 0.5 and (not beeps or i * hop / 48000 + start - beeps[-1] > 0.5):
        # Window centre offset: envelope reaches half height ~half a window after onset.
        beeps.append(start + (i * hop + win / 2) / 48000 - win / 2 / 48000)

pairs = []
for f in flashes:
    near = min(beeps, key=lambda b: abs(b - f), default=None)
    if near is not None and abs(near - f) < 0.4:
        pairs.append((round(f, 3), round((near - f) * 1000, 1)))
offsets = [o for _, o in pairs]
result = {"flashes": len(flashes), "beeps": len(beeps), "paired": len(pairs)}
if offsets:
    k = max(1, len(offsets) // 5)
    first, last = offsets[:k], offsets[-k:]
    result.update({
        "offset_ms_mean": round(sum(offsets) / len(offsets), 1),
        "offset_ms_min": min(offsets), "offset_ms_max": max(offsets),
        "first_fifth_mean_ms": round(sum(first) / len(first), 1),
        "last_fifth_mean_ms": round(sum(last) / len(last), 1),
        "span_s": round(pairs[-1][0] - pairs[0][0], 1),
        "sample": pairs[:3] + pairs[-3:],
    })
print(json.dumps(result, indent=1))
