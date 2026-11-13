"""Phase-continuity analysis of a captured known stereo tone (L=1000 Hz, R=1500 Hz).

Usage: python -I scripts/bench/audio-tone.py FILE [wav16|f32]
Generate the matching stimulus with:
  ffmpeg -f lavfi -i sine=frequency=1000:sample_rate=48000:duration=40 -f lavfi -i sine=frequency=1500:sample_rate=48000:duration=40 -filter_complex "[0][1]join=inputs=2:channel_layout=stereo,volume=0.8" -c:a pcm_s16le tone.wav
A single inserted or dropped frame shifts the phase by 360*f/48000 degrees
(7.5 deg at 1 kHz, 11.25 deg at 1.5 kHz), so phase steps between consecutive
10 ms blocks expose sample-level discontinuities that level meters miss.
Also reports level, clipping, channel identity/crosstalk and residual (THD+N).
"""
import array, cmath, json, math, sys

path, fmt = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else "wav16")
raw = open(path, "rb").read()
if fmt == "wav16":
    raw = raw[raw.find(b"data") + 8:]
    a = array.array("h"); a.frombytes(raw[: len(raw) // 2 * 2]); x = [v / 32768 for v in a]
else:
    a = array.array("f"); a.frombytes(raw[: len(raw) // 4 * 4]); x = list(a)
RATE, B = 48000, 480
L, R = x[0::2], x[1::2]

def demod(ch, f, s):
    w = 2 * math.pi * f / RATE
    return sum(ch[s + i] * cmath.exp(-1j * w * (s + i)) for i in range(B)) * 2 / B

def analyse(ch, f, other_f):
    blocks = [(s, demod(ch, f, s)) for s in range(0, len(ch) - B, B)]
    active = [(s, z) for s, z in blocks if abs(z) > 0.01]
    if len(active) < 50:
        return {"freq": f, "active_blocks": len(active), "note": "tone not present"}
    amp = sorted(abs(z) for _, z in active)[len(active) // 2]
    # Remove slow clock-drift slope with a running median of phase differences.
    diffs = []
    for (s0, z0), (s1, z1) in zip(active, active[1:]):
        if s1 - s0 == B:
            diffs.append((s1, math.degrees(cmath.phase(z1 / z0))))
    med = sorted(d for _, d in diffs)[len(diffs) // 2]
    steps = [(round(s / RATE, 3), round(d - med, 2)) for s, d in diffs if abs(d - med) > 2.0]
    # Residual after removing the fitted tone (per block): THD+N estimate.
    w = 2 * math.pi * f / RATE
    res = sig = 0.0
    for s, z in active[5:-5]:
        for i in range(B):
            n = s + i
            fit = (z * cmath.exp(1j * w * n)).real
            res += (ch[n] - fit) ** 2; sig += fit ** 2
    cross = sorted(abs(demod(ch, other_f, s)) for s, _ in active[5:200:10])
    return {
        "freq": f, "active_blocks": len(active), "amplitude": round(amp, 5),
        "phase_steps_over_2deg": len(steps), "first_steps_s_deg": steps[:12],
        "drift_deg_per_block": round(med, 4),
        "thd_n_db": round(10 * math.log10(res / sig), 1) if sig else None,
        "crosstalk_from_other_channel_db": round(20 * math.log10(max(cross[len(cross)//2], 1e-9) / amp), 1),
    }

peak = max(abs(v) for v in x)
print(json.dumps({
    "file": path, "seconds": round(len(L) / RATE, 3), "peak": round(peak, 5),
    "clipped": sum(1 for v in x if abs(v) >= 0.999),
    "left_1000hz": analyse(L, 1000, 1500), "right_1500hz": analyse(R, 1500, 1000),
}, indent=1))
