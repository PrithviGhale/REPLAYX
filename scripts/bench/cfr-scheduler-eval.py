"""Offline evaluation of 60 FPS CFR frame-selection schedulers on real WGC traces.

Reads saved native-engine traces (trace.jsonl with capture_dequeued and
captured_content events, source.jsonl with the test source's frame rate). Each
capture has its compositor timestamp and the source frame number decoded from
its pixels. Nothing is captured; no devices are opened.

Every scheduler picks, for output slot n (PTS n/60 s), the newest capture whose
time is <= origin + n/60 s + delta. They differ only in how delta is chosen:

- grid0       delta = 0 (the current Grid: origin at the first capture)
- sweep       every fixed delta in [0, 1/60 s) at 0.1 ms steps: best, median, worst
- oracle      delta re-chosen every 2 s from the new-picture arrival phases of the
              previous 2 s (needs per-capture change detection; upper bound)
- tsonly      the same estimator fed with every capture timestamp
              (what timestamps alone can see; redundant captures included)
- probe       delta chosen by measuring how many distinct pictures each candidate
              phase shows. Uses only "is output n identical to output n-1",
              which the encoder reports for free (an identical P-frame is a
              near-empty packet; checked separately against the MP4s).

Metrics:
- lost        captured source frames that never reach the output
- repeats     output slots that repeat the previous slot's picture
- uneven      holds (slots a picture stays on screen) outside
              {floor(60/src), ceil(60/src)}, i.e. visible judder
- av_ms       picture capture time minus its PTS (mean, min, max)

usage: py -3 scripts/bench/cfr-scheduler-eval.py [trace folders...] [--json out.json]
"""
import argparse
import json
import math
import statistics
import sys
from pathlib import Path

FAST = "--fast" in sys.argv
HNS = 10_000_000
FPS = 60
T = HNS / FPS
ROOT = Path(__file__).resolve().parents[2]
DEFAULT = ROOT / "test-output" / "native-smoothness"


def offset(slot):
    return slot * HNS // FPS


def load(folder):
    events = [json.loads(l) for l in (folder / "trace.jsonl").read_text(encoding="utf-8-sig").splitlines() if l.strip()]
    content = {e["capture"]: e["content_id"] for e in events if e["stage"] == "captured_content"}
    caps = sorted((e for e in events if e["stage"] == "capture_dequeued"), key=lambda e: e["capture"])
    if not caps or len(content) != len(caps):
        return None
    if not (folder / "source.jsonl").exists():
        return None
    src = []
    for l in (folder / "source.jsonl").read_text(encoding="utf-8-sig").splitlines():
        try:
            src.append(json.loads(l))
        except ValueError:
            pass  # a line cut off when the source process was stopped
    start = next(s for s in src if s["stage"] == "source_start")
    times = [c["capture_hns"] for c in caps]
    ids = [content[c["capture"]] for c in caps]
    return {"name": folder.name, "times": times, "ids": ids, "src_fps": start["fps"], "static": start.get("static", False)}


class Selector:
    """Newest capture <= deadline(n); deadline supplied per slot by a policy."""

    def __init__(self, times, ids):
        self.times, self.ids = times, ids
        self.origin = times[0]

    def run(self, policy):
        times, ids, origin = self.times, self.ids, self.origin
        out, i, n = [], 0, 0
        last_deadline=None
        while True:
            deadline = origin + offset(n) + policy.delta(n)
            if last_deadline is not None:deadline=max(deadline,last_deadline+1)
            if deadline > times[-1]:
                break
            while i + 1 < len(times) and times[i + 1] <= deadline:
                i += 1
            # Perfect source IDs are only offline change hints, never a readback
            # in production. Mirror the bounded wait through redundant captures.
            if isinstance(policy,LexLock) and out and ids[i]==ids[out[-1][1]]:
                intervals=[b-a for a,b in zip(policy.times,policy.times[1:]) if b>a]
                ratio=sorted(intervals)[len(intervals)//2]/T if len(intervals)>=7 else 0
                nominal=1 if 0.75<=ratio<=1.25 else 2 if 1.75<=ratio<=2.25 else None
                held=0
                for _,previous in reversed(out):
                    if ids[previous]!=ids[i]:break
                    held+=1
                picture_start=i
                while picture_start and ids[picture_start-1]==ids[i]:picture_start-=1
                while nominal is not None and held>=nominal and i+1<len(times) and ids[i]==ids[out[-1][1]]:
                    candidate=times[i+1]
                    changed=ids[i+1]!=ids[i]
                    if candidate>origin+offset(n)+int(T)*5//4:break
                    if changed and candidate-deadline>=deadline-times[picture_start]:break
                    deadline=candidate
                    while i+1<len(times) and times[i+1]<=deadline:i+=1
            policy.prev_deadline=deadline
            last_deadline=deadline
            if out and i < out[-1][1]:
                raise AssertionError("selection went backwards")
            out.append((n, i))
            policy.observe(n, i, out)
            n += 1
        return out


class Fixed:
    def __init__(self, d):
        self.d = d

    def delta(self, n):
        return self.d

    def observe(self, n, i, out):
        pass


def best_phase(phases):
    """Midpoint of the largest empty arc on the circle [0, T)."""
    if not phases:
        return None
    p = sorted(x % T for x in phases)
    gaps = [(p[(k + 1) % len(p)] - p[k]) % T or T for k in range(len(p))]
    k = max(range(len(p)), key=lambda j: gaps[j])
    return (p[k] + gaps[k] / 2) % T


class ArcTracker:
    """Every `window` slots, place the deadline in the middle of the largest
    empty arc of the arrival phases seen in the previous window."""

    def __init__(self, sel, arrivals, window=120, start=0.0):
        self.sel, self.arrivals, self.window = sel, sorted(arrivals), window
        self.d = start
        self.k = 0

    def delta(self, n):
        if n and n % self.window == 0:
            lo = self.sel.origin + offset(n - self.window)
            hi = self.sel.origin + offset(n)
            while self.k < len(self.arrivals) and self.arrivals[self.k] < lo:
                self.k += 1
            seen = [t - self.sel.origin for t in self.arrivals[self.k:] if t < hi]
            b = best_phase(seen)
            if b is not None:
                self.d = b
        return self.d

    def observe(self, n, i, out):
        pass


class PhaseLock:
    """Keeps the selection deadline in the widest gap between new-picture arrivals.

    - Keeps the arrival phases (mod 1/60 s) of the last `keep` arrivals it
      has seen; only arrivals at or before the previous slot's deadline (causal).
    - Best gap: the longest arc holding at most `outliers` of them (one late
      compositor frame must not move the deadline).
    - Holds while that arc is shorter than `min_arc` (no structure: a source
      faster than 60 Hz, 50 Hz, a static picture).
    - Otherwise re-centres when the deadline is within `margin` of either
      edge of the best gap, or outside it. A move inside an empty gap crosses
      no arrival, so it adds no repeat or skip.
    - delta stays in [-T/4, 5T/4): leaving it wraps by one period, which is
      the one repeat (or skip) a slower (faster) source needs anyway.
    """

    def __init__(self, sel, arrivals, keep=60, outliers=2, min_arc=0.25, margin=0.2, start=None):
        self.sel = sel
        self.arrivals = sorted(arrivals)
        self.keep, self.outliers = keep, outliers
        self.min_arc, self.margin = min_arc * T, margin * T
        self.k = 0
        self.phases = []
        self.d = T / 2 if start is None else start
        self.moves = 0
        self.wraps = 0
        self.prev_deadline = None

    def best_arc(self):
        p = sorted(self.phases)
        m = len(p)
        j = self.outliers + 1
        if m <= j:
            return None
        best = None
        for i in range(m):
            a = p[i]
            b = p[(i + j) % m] + (T if i + j >= m else 0)
            if best is None or b - a > best[1] - best[0]:
                best = (a, b)
        return best

    def delta(self, n):
        if self.prev_deadline is not None:
            while self.k < len(self.arrivals) and self.arrivals[self.k] <= self.prev_deadline:
                self.phases.append((self.arrivals[self.k] - self.sel.origin) % T)
                self.k += 1
            del self.phases[:-self.keep]
            arc = self.best_arc()
            if arc and arc[1] - arc[0] >= self.min_arc:
                a, b = arc
                # The representative of the arc closest to the current delta.
                shift = round((self.d - (a + b) / 2) / T) * T
                a, b = a + shift, b + shift
                edge = min(self.d - a, b - self.d)
                if edge < self.margin:
                    self.d = (a + b) / 2
                    self.moves += 1
                    if self.d >= 1.25 * T:
                        self.d -= T
                        self.wraps += 1
                    elif self.d < -0.25 * T:
                        self.d += T
                        self.wraps += 1
        self.prev_deadline = self.sel.origin + offset(n) + self.d
        return self.d

    def observe(self, n, i, out):
        pass


class DensityLock:
    """Keeps the selection deadline where new pictures do not arrive.

    - Arrival phases (mod 1/60 s) of the last `keep` new pictures seen at or
      before the previous slot's deadline (causal), in `bins` bins.
    - cost(phase) = sum over arrivals of a triangle of half-width `width`:
      1 at the arrival, 0 beyond `width`. Late frames, which only ever come
      later than their cluster, add cost where they actually land.
    - Structure: the cheapest phase costs at most `structure` x the mean cost.
      Without it (a source faster than 60 Hz, a 50 Hz source, a static
      picture) the deadline holds.
    - Moves only when the current phase costs more than the cheapest by
      `hysteresis` arrivals, to the middle of the cheapest run of bins.
      In an empty gap nothing is crossed, so the move adds no repeat or skip.
    - delta stays in [-T/4, 5T/4): leaving it wraps by one period, which is
      the one repeat (or skip) a slower (faster) source needs anyway.
    """

    def __init__(self, sel, arrivals, keep=60, bins=128, width=0.25, structure=0.2, hysteresis=0.5, start=None):
        self.sel = sel
        self.arrivals = sorted(arrivals)
        self.keep, self.bins = keep, bins
        self.width = max(1, round(width * bins))
        self.structure, self.hysteresis = structure, hysteresis
        self.k = 0
        self.ring = []
        self.d = T / 2 if start is None else start
        self.moves = self.wraps = 0
        self.prev_deadline = None
        self.dirty = False
        kernel = [max(0.0, 1 - abs(j) / (self.width + 1)) for j in range(-self.width, self.width + 1)]
        self.kernel = kernel

    def cost(self):
        hist = [0] * self.bins
        for b in self.ring:
            hist[b] += 1
        c = [0.0] * self.bins
        for b, n in enumerate(hist):
            if n:
                for j, w in enumerate(self.kernel):
                    c[(b + j - self.width) % self.bins] += n * w
        return c

    def delta(self, n):
        if self.prev_deadline is not None:
            while self.k < len(self.arrivals) and self.arrivals[self.k] <= self.prev_deadline:
                ph = (self.arrivals[self.k] - self.sel.origin) % T
                self.ring.append(int(ph * self.bins / T) % self.bins)
                self.k += 1
                self.dirty = True
            del self.ring[:-self.keep]
            if self.dirty and len(self.ring) >= 8:
                self.dirty = False
                c = self.cost()
                lo = min(c)
                mean = sum(c) / len(c)
                cur = int((self.d % T) * self.bins / T) % self.bins
                if lo <= self.structure * mean and c[cur] > lo + self.hysteresis:
                    # middle of the cheapest run (circular), the run nearest the current phase
                    best = None
                    for b in range(self.bins):
                        if c[b] <= lo + 1e-9 and c[(b - 1) % self.bins] > lo + 1e-9:
                            e = b
                            while c[(e + 1) % self.bins] <= lo + 1e-9 and (e + 1) % self.bins != b:
                                e += 1
                            run = e - b + 1
                            mid = (b + (run - 1) / 2 + 0.5) * T / self.bins
                            if best is None or run > best[0]:
                                best = (run, mid)
                    if best is None:  # every bin is minimal
                        best = (self.bins, self.d % T)
                    target = best[1]
                    shift = round((self.d - target) / T) * T
                    self.d = target + shift
                    self.moves += 1
                    if self.d >= 1.25 * T:
                        self.d -= T
                        self.wraps += 1
                    elif self.d < -0.25 * T:
                        self.d += T
                        self.wraps += 1
        self.prev_deadline = self.sel.origin + offset(n) + self.d
        return self.d

    def observe(self, n, i, out):
        pass


class ReplayLock(DensityLock):
    """DensityLock plus the exact cost of each phase on recent history:
    cost(phase) = frames that phase would have lost on the kept arrivals (two
    new pictures inside one slot) + `density_weight` x the density cost per
    arrival. Losses dominate; the density term keeps a margin and even holds."""

    def __init__(self, sel, arrivals, density_weight=0.5, **kw):
        super().__init__(sel, arrivals, **kw)
        self.density_weight = density_weight
        self.times = []

    def cost(self):
        dens = super().cost()
        top = max(dens) or 1.0
        c = []
        for b in range(self.bins):
            phi = (b + 0.5) * T / self.bins
            slots = {math.ceil((t - phi) / T) for t in self.times}
            lost = len(self.times) - len(slots)
            c.append(lost + self.density_weight * dens[b] / top)
        return c

    def delta(self, n):
        if self.prev_deadline is not None:
            k = self.k
            while k < len(self.arrivals) and self.arrivals[k] <= self.prev_deadline:
                self.times.append(self.arrivals[k] - self.sel.origin)
                k += 1
            del self.times[:-self.keep]
        return super().delta(n)


class LexLock(DensityLock):
    """First fewest lost frames, then the widest margin.

    For every phase bin: lost(phase) = how many of the kept new-picture
    arrivals that deadline phase would have lost on its own (two arrivals
    inside one slot), density(phase) as in DensityLock.
    - If the current phase would have lost at least max(`lost_hysteresis`,
      best / `lost_relative`) more frames than the best phase, move to the best-loss phases' lowest-density
      bin (middle of that run).
    - Otherwise behave as DensityLock restricted to phases that lose no more
      than the current one (structure test, density hysteresis).
    """

    def __init__(self, sel, arrivals, lost_hysteresis=1, lost_relative=4.0, **kw):
        super().__init__(sel, arrivals, **kw)
        self.lost_hysteresis = lost_hysteresis
        self.lost_relative = lost_relative
        self.times = []

    def lost(self):
        out = []
        for b in range(self.bins):
            phi = (b + 0.5) * T / self.bins
            slots = {math.ceil((t - phi) / T) for t in self.times}
            out.append(len(self.times) - len(slots))
        return out

    def delta(self, n):
        if self.prev_deadline is not None:
            while self.k < len(self.arrivals) and self.arrivals[self.k] <= self.prev_deadline:
                a = self.arrivals[self.k]
                self.ring.append(int(((a - self.sel.origin) % T) * self.bins / T) % self.bins)
                self.times.append(a - self.sel.origin)
                self.k += 1
                self.dirty = True
            del self.ring[:-self.keep]
            del self.times[:-self.keep]
            if self.dirty and len(self.ring) >= 8:
                self.dirty = False
                dens = self.cost()
                lost = self.lost()
                cur = int((self.d % T) * self.bins / T) % self.bins
                best_lost = min(lost)
                threshold = max(float(self.lost_hysteresis), best_lost / self.lost_relative)
                if lost[cur] >= best_lost + threshold:
                    allowed = [lost[b] == best_lost for b in range(self.bins)]
                    self._move(dens, allowed)
                else:
                    allowed = [lost[b] <= lost[cur] for b in range(self.bins)]
                    lo = min(dens[b] for b in range(self.bins) if allowed[b])
                    mean = sum(dens) / len(dens)
                    if lo <= self.structure * mean and dens[cur] > lo + self.hysteresis:
                        self._move(dens, allowed)
        self.prev_deadline = self.sel.origin + offset(n) + self.d
        return self.d

    def _move(self, dens, allowed):
        INF = float("inf")
        c = [dens[b] if allowed[b] else INF for b in range(self.bins)]
        lo = min(c)
        best = None
        for b in range(self.bins):
            if c[b] <= lo + 1e-9 and c[(b - 1) % self.bins] > lo + 1e-9:
                e = b
                while c[(e + 1) % self.bins] <= lo + 1e-9 and (e + 1) % self.bins != b:
                    e += 1
                run = e - b + 1
                mid = (b + (run - 1) / 2 + 0.5) * T / self.bins
                if best is None or run > best[0]:
                    best = (run, mid)
        target = best[1] if best else self.d % T
        shift = round((self.d - target) / T) * T
        self.d = target + shift
        self.moves += 1
        if self.d >= 1.25 * T:
            self.d -= T
            self.wraps += 1
        elif self.d < -0.25 * T:
            self.d += T
            self.wraps += 1


def metrics(trace, out, deltas=None):
    ids, times = trace["ids"], trace["times"]
    shown = [ids[i] for _, i in out]
    first_out = shown[0]
    last_out = shown[-1]
    captured = {x for x in ids if first_out <= x <= last_out}
    lost = len(captured - set(shown))
    repeats = sum(1 for a, b in zip(shown, shown[1:]) if a == b)
    holds, run = [], 1
    for a, b in zip(shown, shown[1:]):
        if a == b:
            run += 1
        else:
            holds.append(run)
            run = 1
    r = FPS / trace["src_fps"]
    ok = {math.floor(r), math.ceil(r)}
    uneven = sum(1 for h in holds[1:] if h not in ok)
    origin = times[0]
    av = [(times[i] - origin - offset(n)) / 1e4 for n, i in out]
    return {"slots": len(out), "lost": lost, "repeats": repeats, "uneven": uneven,
            "av_ms": [round(statistics.mean(av), 2), round(min(av), 2), round(max(av), 2)]}


def evaluate(trace):
    sel = Selector(trace["times"], trace["ids"])
    res = {"name": trace["name"], "src_fps": trace["src_fps"], "static": trace["static"], "captures": len(trace["ids"])}
    res["grid0"] = metrics(trace, sel.run(Fixed(0)))
    sweep = []
    for step in range(0, int(T), 1000 if len(trace["times"]) < 5000 else 5000) if not FAST else [0]:  # 0.1 ms (0.5 ms long traces)
        m = metrics(trace, sel.run(Fixed(step)))
        sweep.append((m["lost"] + m["uneven"], step, m))
    sweep.sort(key=lambda s: s[0])
    res["sweep"] = {"best": sweep[0][2] | {"delta_ms": sweep[0][1] / 1e4},
                    "median": sweep[len(sweep) // 2][2] | {"delta_ms": sweep[len(sweep) // 2][1] / 1e4},
                    "worst": sweep[-1][2] | {"delta_ms": sweep[-1][1] / 1e4}}
    ids, times = trace["ids"], trace["times"]
    arrivals = [t for k, t in enumerate(times) if k == 0 or ids[k] != ids[k - 1]]
    res["fixed_half"] = metrics(trace, sel.run(Fixed(T / 2)))
    res["arc2s"] = metrics(trace, sel.run(ArcTracker(sel, arrivals)))
    for label, cls, feed in (("lock_changes", PhaseLock, arrivals), ("lock_timestamps", PhaseLock, times),
                             ("density_changes", DensityLock, arrivals),
                             # The shipped design (phase.rs): lost-then-margin with
                             # change hints (4.2 ms kernel); density, 1 ms kernel, without.
                             ("shipped_changes", LexLock, arrivals), ("shipped_timestamps", DensityLock, times)):
        pl = cls(sel, feed, width=8 / 128) if label == "shipped_timestamps" else cls(sel, feed)
        res[label] = metrics(trace, sel.run(pl)) | {"moves": pl.moves, "wraps": pl.wraps}
    return res


def synthetic(name, refresh, src_fps, seconds, seed, redundant=0.0, late=0.0, jitter_ms=0.3, ppm=40.0):
    """A modelled WGC trace: the source presents at src_fps with jitter; the
    compositor shows it at the next vblank (display clock off by `ppm`);
    `late` of presents miss one vblank; `redundant` of the other vblanks
    produce a capture without a new picture (other screen activity)."""
    import random
    rnd = random.Random(seed)
    vb = HNS / (refresh * (1 + ppm * 1e-6))
    v0 = rnd.uniform(0, vb)
    p0 = rnd.uniform(0, HNS / src_fps)
    caps = {}
    k = 0
    end = seconds * HNS
    while True:
        t = p0 + k * HNS / src_fps + rnd.gauss(0, jitter_ms * 1e4) + 0.5e4
        if t > end:
            break
        j = math.ceil((t - v0) / vb) + (1 if rnd.random() < late else 0)
        caps[j] = k  # a later picture in the same vblank replaces an earlier one
        k += 1
    times, ids, last = [], [], None
    for j in range(min(caps), max(caps) + 1):
        if j in caps:
            last = caps[j]
        elif last is None or rnd.random() >= redundant:
            continue
        times.append(int(v0 + j * vb))
        ids.append(last)
    return {"name": name, "times": times, "ids": ids, "src_fps": src_fps, "static": False}


SYNTHETIC = [
    # name, display Hz, source fps, seconds, redundant, late
    ("60on60", 60, 60, 120, 0.0, 0.0),
    ("60on60-late", 60, 60, 120, 0.0, 0.01),
    ("5994on60", 60, 59.94, 600, 0.0, 0.0),
    ("5994on250-redundant", 250, 59.94, 600, 0.5, 0.01),
    ("60on144-redundant", 144, 60, 120, 0.5, 0.01),
    ("60on165", 165, 60, 120, 0.0, 0.0),
    ("60on240-busy", 240, 60, 120, 0.9, 0.01),
    ("6006on250", 250, 60.06, 600, 0.2, 0.0),
    ("30on144", 144, 30, 120, 0.3, 0.0),
    ("50on250", 250, 50, 120, 0.2, 0.0),
    ("144on144", 144, 144, 60, 0.0, 0.0),
    ("300on250", 250, 300, 60, 0.0, 0.0),
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("folders", nargs="*")
    ap.add_argument("--json")
    ap.add_argument("--fast", action="store_true", help="skip the fixed-phase sweep")
    ap.add_argument("--synthetic", type=int, metavar="SEEDS", help="modelled traces instead, SEEDS random phases each")
    a = ap.parse_args()
    if a.synthetic:
        traces = [synthetic(f"{c[0]}#{seed}", *c[1:4], seed, *c[4:]) for c in SYNTHETIC for seed in range(a.synthetic)]
    else:
        folders = [Path(f) for f in a.folders] or sorted(p for p in DEFAULT.iterdir() if (p / "trace.jsonl").exists())
        traces = []
        for f in folders:
            t = load(f)
            if t is None:
                print(f"{f.name}: no per-capture content ids, skipped")
            else:
                traces.append(t)
    results = []
    for t in traces:
        r = evaluate(t)
        results.append(r)
        row = lambda m: f"lost {m['lost']:3} rep {m['repeats']:3} uneven {m['uneven']:3} av {m['av_ms']}"
        print(f"\n{r['name']}  src {r['src_fps']} fps{' static' if r['static'] else ''}  captures {r['captures']}  slots {r['grid0']['slots']}")
        print(f"  grid0          {row(r['grid0'])}")
        for k in ("best", "median", "worst"):
            print(f"  sweep {k:7}  {row(r['sweep'][k])}  delta {r['sweep'][k]['delta_ms']:.1f} ms")
        for k in ("fixed_half", "arc2s"):
            print(f"  {k:15}{row(r[k])}")
        for k in ("lock_changes", "lock_timestamps", "density_changes", "shipped_changes", "shipped_timestamps"):
            print(f"  {k:15}{row(r[k])}  moves {r[k]['moves']} wraps {r[k]['wraps']}")
    if a.json:
        Path(a.json).write_text(json.dumps(results, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
