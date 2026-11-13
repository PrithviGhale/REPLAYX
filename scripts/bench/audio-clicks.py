"""Objective click/discontinuity analysis of interleaved stereo PCM.

Usage: python -I scripts/bench/audio-clicks.py FILE [f32|wav16] [rate]
(f32 = raw interleaved stereo f32le, e.g. `ffmpeg -i clip.mkv -map 0:a:0 -f f32le out.f32`)
Reports peak/RMS/clipping, zero-frame insertions inside signal, and
second-difference outliers ("clicks") relative to local signal activity.
"""
import array, json, math, struct, sys

path = sys.argv[1]
fmt = sys.argv[2] if len(sys.argv) > 2 else "f32"
rate = int(sys.argv[3]) if len(sys.argv) > 3 else 48000

raw = open(path, "rb").read()
if fmt == "wav16":
    i = raw.find(b"data")
    raw = raw[i + 8:]
    a = array.array("h"); a.frombytes(raw[: len(raw) // 2 * 2])
    x = [v / 32768.0 for v in a]
else:
    a = array.array("f"); a.frombytes(raw[: len(raw) // 4 * 4])
    x = list(a)
L, R = x[0::2], x[1::2]
n = min(len(L), len(R))
peak = max(max(abs(v) for v in L), max(abs(v) for v in R)) if n else 0
rms = math.sqrt(sum(v * v for v in x) / max(1, len(x)))
clip = sum(1 for v in x if abs(v) >= 0.999)

# Zero-frame insertions: frames where both channels are exactly 0 while the
# surrounding 4 frames on each side carry clearly nonzero signal.
zero_runs = 0
k = 4
ii = k
while ii < n - k:
    if L[ii] == 0.0 and R[ii] == 0.0:
        j = ii
        while j < n and L[j] == 0.0 and R[j] == 0.0:
            j += 1
        before = max(abs(L[ii - 1]), abs(R[ii - 1]))
        after = max(abs(L[j]), abs(R[j])) if j < n else 0
        if before > 0.02 and after > 0.02 and (j - ii) < 4800:
            zero_runs += 1
        ii = j
    else:
        ii += 1

# Click detector: |2nd difference| vs. local RMS of 2nd difference (window 256)
def clicks(ch):
    d = [ch[i] - 2 * ch[i - 1] + ch[i - 2] for i in range(2, len(ch))]
    W = 256
    out = []
    for s in range(0, len(d) - W, W):
        blk = d[s : s + W]
        e = math.sqrt(sum(v * v for v in blk) / W)
        # Neighbouring-block energy acts as the reference so a click does not
        # mask itself.
        ref_blocks = []
        for o in (-2 * W, -W, W, 2 * W):
            t = s + o
            if 0 <= t and t + W <= len(d):
                rb = d[t : t + W]
                ref_blocks.append(math.sqrt(sum(v * v for v in rb) / W))
        if not ref_blocks:
            continue
        ref = sorted(ref_blocks)[len(ref_blocks) // 2]
        m = max(abs(v) for v in blk)
        if m > 0.02 and m > 8 * max(ref, 1e-4) and m > 4 * e:
            out.append((s + 2) / rate)
    return out

cl = clicks(L)
cr = clicks(R)
dur = n / rate
merged = sorted(set(round(t, 3) for t in cl + cr))
active = sum(1 for s in range(0, n - 4800, 4800)
             if math.sqrt(sum(v * v for v in L[s:s + 4800]) / 4800) > 0.005) / 10.0
print(json.dumps({
    "file": path, "seconds": round(dur, 3), "active_seconds": active,
    "peak": round(peak, 5), "rms": round(rms, 5), "clipped_samples": clip,
    "zero_frame_insertions_in_signal": zero_runs,
    "click_events": len(merged),
    "clicks_per_active_second": round(len(merged) / active, 2) if active else None,
    "first_clicks_s": merged[:15],
}, indent=1))
