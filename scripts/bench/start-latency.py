"""Compare recording start latency of two REPLAYX CLI builds from the outside.

Runs `<cli> record-test 6 silent` in a fresh folder (real capture of the
selected display, no audio) and watches the file system every 5 ms:
  first_output_ms   launch -> first segment file written (the muxer writes it
                    once every stream has an encoded packet)
  first_segment_ms  launch -> first 2 s segment finalized (listed in segments.csv)
Launch time includes process start and engine setup, identical for both builds,
so the numbers compare like for like. Each run uses a new data folder, so this
is the first start for the display in that folder.
usage: python scripts/bench/start-latency.py LABEL=PATH_TO_CLI [LABEL=PATH ...] [--runs 3]
"""
import glob
import json
import os
import subprocess
import sys
import tempfile
import time


def one(cli):
    work = tempfile.mkdtemp(prefix="replayx-start-")
    t0 = time.perf_counter()
    proc = subprocess.Popen([cli, "record-test", "6", "silent"], cwd=work,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    first_output = first_segment = None
    while proc.poll() is None and time.perf_counter() - t0 < 60:
        if first_output is None:
            for f in glob.glob(os.path.join(work, "test-output", "*", "data", "buffer", "*", "segment-00000000.mkv")):
                if os.path.getsize(f) > 0:
                    first_output = (time.perf_counter() - t0) * 1000
        if first_segment is None:
            for f in glob.glob(os.path.join(work, "test-output", "*", "data", "buffer", "*", "segments.csv")):
                if open(f).read().count("\n") >= 1:
                    first_segment = (time.perf_counter() - t0) * 1000
        if first_output is not None and first_segment is not None:
            break
        time.sleep(0.005)
    proc.wait(timeout=90)
    return {"first_output_ms": round(first_output or -1), "first_segment_ms": round(first_segment or -1), "folder": work}


def main():
    runs = 3
    builds = []
    for a in sys.argv[1:]:
        if a.startswith("--runs"):
            continue
        if a.isdigit():
            runs = int(a)
            continue
        label, path = a.split("=", 1)
        builds.append((label, path))
    results = {}
    for n in range(runs):
        for label, path in builds:  # interleaved, so drift affects both equally
            r = one(path)
            results.setdefault(label, []).append(r)
            print(json.dumps({"build": label, "run": n + 1, **r}), flush=True)
    for label, rs in results.items():
        out = sorted(r["first_output_ms"] for r in rs)
        seg = sorted(r["first_segment_ms"] for r in rs)
        print(f"{label}: first output median {out[len(out)//2]} ms (all {out}); first segment median {seg[len(seg)//2]} ms (all {seg})")


if __name__ == "__main__":
    main()
