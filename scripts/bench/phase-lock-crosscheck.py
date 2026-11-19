"""Check the Rust phase-locked grid against the Python reference model.

For every saved trace (and modelled traces), the timeline goes through
`replayx-native-proto --pacing-replay` and through the reference model in
cfr-scheduler-eval.py with the same constants (`LexLock` with change hints,
`DensityLock` without). Every output slot must pick the
same capture. Pure scheduling: no device is opened, nothing is captured.

usage: py -3 scripts/bench/phase-lock-crosscheck.py [--synthetic-seeds 2]
"""
import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROTO = ROOT / "src-tauri" / "target" / "release" / "replayx-native-proto.exe"
spec = importlib.util.spec_from_file_location("evalmod", Path(__file__).with_name("cfr-scheduler-eval.py"))
ev = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ev)

# Must equal src-tauri/src/bin/native/phase.rs.
PARAMS = dict(keep=60, bins=128, structure=0.2, hysteresis=0.5)
WIDTH = {"content": 32 / 128, "none": 8 / 128}


def rust(trace, hints, tmp):
    path = Path(tmp) / "timeline.json"
    path.write_text(json.dumps({"captures": [{"time_hns": t, "content_id": i} for t, i in zip(trace["times"], trace["ids"])]}))
    r = subprocess.run([str(PROTO), "--pacing-replay", str(path), "--phase-hints", hints], capture_output=True, text=True, timeout=120)
    if r.returncode:
        sys.exit(f"pacing-replay failed: {r.stderr[-800:]}")
    return json.loads(r.stdout)


def python(trace, hints):
    sel = ev.Selector(trace["times"], trace["ids"])
    ids, times = trace["ids"], trace["times"]
    feed = [t for k, t in enumerate(times) if k == 0 or ids[k] != ids[k - 1]] if hints == "content" else times
    cls = ev.LexLock if hints == "content" else ev.DensityLock
    lock = cls(sel, feed, width=WIDTH[hints], **PARAMS)
    out = sel.run(lock)
    return [i for _, i in out], lock


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--synthetic-seeds", type=int, default=2)
    a = ap.parse_args()
    traces = []
    for p in sorted((ROOT / "test-output" / "native-smoothness").iterdir()):
        if (p / "trace.jsonl").exists():
            t = ev.load(p)
            if t:
                traces.append(t)
    for c in ev.SYNTHETIC:
        for seed in range(a.synthetic_seeds):
            t = ev.synthetic(f"{c[0]}#{seed}", c[1], c[2], min(c[3], 120), seed, *c[4:])
            traces.append(t)
    binary_hash = hashlib.sha256(PROTO.read_bytes()).hexdigest()
    print(f"Binary SHA-256: {binary_hash}", flush=True)
    failures = 0
    with tempfile.TemporaryDirectory(prefix="replayx-phase-") as tmp:
        for t in traces:
            for hints in ("content", "none"):
                r = rust(t, hints, tmp)
                py, lock = python(t, hints)
                n = min(len(py), len(r["picks"]))
                diff = next((k for k in range(n) if py[k] != r["picks"][k]), None)
                same_counts = (r["moves"], r["wraps"]) == (lock.moves, lock.wraps) or diff is None
                ok = diff is None and n >= len(py) - 2
                failures += not ok
                print(f"{t['name']:26} hints {hints:7} slots {n:6}  {'SAME' if ok else 'DIFFER at ' + str(diff)}"
                      f"  moves rust {r['moves']} py {lock.moves}  wraps rust {r['wraps']} py {lock.wraps}")
    if hashlib.sha256(PROTO.read_bytes()).hexdigest() != binary_hash:
        print("FAIL: native binary changed during crosscheck; results are invalid")
        return 1
    print("PASS" if not failures else f"FAIL: {failures} traces differ")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
