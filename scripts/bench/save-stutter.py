"""What an Instant Replay save costs a game: frame times around each save.

Runs replayx-gameproxy (a game-like GPU workload, borderless full screen) and
`replayx-cli save-stutter` (Instant Replay with the app's 1080p60 Balanced
settings, saves at fixed times, the on-screen notice) together, then lines up
the workload's presents with the save events (same QPC clock).

For each save it compares the window from the request to 4 s after the clip
is ready (notice included) with a baseline: the 20 s of capture just before
the first save. Reports average FPS, 99th percentile and maximum frame time,
and hitches (frames longer than 2.5x the baseline median).

Synthetic workload: the results are not game results. Captures the screen:
run only when nothing else needs it.

usage: python scripts/bench/save-stutter.py [--saves 130,160,161,190,190.4,190.8]
       [--end 220] [--iters 200] [--notice 1] [--overlay 1] [--thumb 0] [--proxy-args "..."]
       [--tag NAME] [--cli PATH]
       python scripts/bench/save-stutter.py --analyse test-output/save-stutter/NAME
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
rel = os.path.join(root, "src-tauri", "target", "release")
CLI, PROXY = os.path.join(rel, "replayx-cli.exe"), os.path.join(rel, "replayx-gameproxy.exe")


def frame_stats(times_ms):
    if not times_ms:
        return None
    s = sorted(times_ms)
    pick = lambda q: s[min(len(s) - 1, int(q * len(s)))]
    return {
        "frames": len(s),
        "fps": round(1000 * len(s) / sum(s), 1),
        "median_ms": round(statistics.median(s), 3),
        "p99_ms": round(pick(0.99), 3),
        "p999_ms": round(pick(0.999), 3),
        "max_ms": round(s[-1], 3),
    }


def window(presents, lo, hi):
    """Frame times (ms) of presents in [lo, hi) seconds (QPC)."""
    out = []
    for a, b in zip(presents, presents[1:]):
        if lo <= b < hi:
            out.append((b - a) * 1000)
    return out


def analyse(run_dir):
    proxy = json.load(open(os.path.join(run_dir, "gameproxy.json"), encoding="utf-8"))
    report = json.load(open(os.path.join(run_dir, "events.json"), encoding="utf-8"))
    base = proxy["start_qpc_s"]
    presents = [base + t for t in proxy["presents_s"]]
    events = report["events"]
    by_save = {}
    for e in events:
        by_save.setdefault(e["save"], []).append(e)
    saves = sorted(k for k in by_save if k >= 0)
    if not saves:
        sys.exit("no saves in events.json")
    first = min(e["t"] for e in by_save[saves[0]])
    capturing = next((e["t"] for e in by_save.get(-1, []) if e["event"] == "capturing"), presents[0])
    baseline_lo = max(capturing + 5, first - 20)
    baseline = window(presents, baseline_lo, first)
    b = frame_stats(baseline)
    hitch = 2.5 * b["median_ms"]
    rows = []
    for n in saves:
        ev = {e["event"]: e["t"] for e in by_save[n]}
        lo = ev["requested"]
        end = ev.get("ready", ev.get("failed", lo))
        hi = end + 4.0
        ft = window(presents, lo, hi)
        st = frame_stats(ft)
        st.update({
            "save": n,
            "window_s": round(hi - lo, 2),
            "handover_ms": round((ev.get("handed over", lo) - lo) * 1000, 1),
            "ready_ms": round((end - lo) * 1000),
            "hitches": sum(1 for x in ft if x > hitch),
            "stages_ms": {k: round((v - lo) * 1000) for k, v in ev.items() if k not in ("requested",)},
        })
        # Where the worst frame fell, relative to the save's stages.
        worst = max(range(len(ft)), key=lambda i: ft[i]) if ft else None
        if worst is not None:
            frames_in = [b2 for a2, b2 in zip(presents, presents[1:]) if lo <= b2 < hi]
            st["worst_at_ms"] = round((frames_in[worst] - lo) * 1000)
        rows.append(st)
    # 250 ms bins of the whole run (FPS over time).
    t0 = presents[0]
    bins = {}
    for a2, b2 in zip(presents, presents[1:]):
        k = int((b2 - t0) / 0.25)
        bins.setdefault(k, []).append((b2 - a2) * 1000)
    timeline = [{"t": round(k * 0.25, 2), "fps": round(1000 * len(v) / sum(v), 1), "max_ms": round(max(v), 2)} for k, v in sorted(bins.items())]
    modes = proxy.get("presentation_modes", [])
    summary = {
        "baseline": b, "hitch_ms": round(hitch, 3), "saves": rows,
        "save_events_s": {n: round(min(e["t"] for e in by_save[n]) - t0, 2) for n in saves},
        "presentation_composed_pct": round(100 * sum(1 for m in modes if m["mode"] == 0) / max(1, len(modes)), 1),
        "encoder": report.get("status", {}).get("encoder"),
        "capture_filter": report.get("status", {}).get("capture_filter"),
        "save_results": report.get("saves"),
        "note": "Synthetic workload (replayx-gameproxy); not a game result.",
    }
    json.dump(summary, open(os.path.join(run_dir, "summary.json"), "w"), indent=1)
    json.dump(timeline, open(os.path.join(run_dir, "timeline.json"), "w"))
    print(f"encoder {summary['encoder']} | capture {summary['capture_filter']} | composed {summary['presentation_composed_pct']}%")
    print(f"baseline (capturing, no save): {b}")
    print(f"{'save':>4} {'fps':>7} {'p99':>7} {'p99.9':>7} {'max':>7} {'hitch':>5} {'ready':>6} {'worst@':>7}")
    for r in rows:
        print(f"{r['save']:>4} {r['fps']:>7} {r['p99_ms']:>7} {r['p999_ms']:>7} {r['max_ms']:>7} {r['hitches']:>5} {r['ready_ms']:>6} {r.get('worst_at_ms', ''):>7}")
    return summary


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--saves", default="130,160,161,190,190.4,190.8")
    ap.add_argument("--end", type=float, default=220)
    ap.add_argument("--replay", type=int, default=120)
    ap.add_argument("--iters", type=int, default=200)
    ap.add_argument("--notice", default="1")
    ap.add_argument("--thumb", default="0")
    ap.add_argument("--overlay", default="1", help="In-game status overlay (as the app, updated once a second)")
    ap.add_argument("--proxy-args", default="", help="Extra replayx-gameproxy arguments, e.g. \"--cpu-us 1500 --workers 12\"")
    ap.add_argument("--capture-api", default="wgc")
    ap.add_argument("--encoder", default="auto")
    ap.add_argument("--audio", default="silent")
    ap.add_argument("--tag", default=time.strftime("%H%M%S"))
    ap.add_argument("--cli", default=CLI)
    ap.add_argument("--analyse")
    a = ap.parse_args()
    if a.analyse:
        analyse(a.analyse)
        return
    out = os.path.join(root, "test-output", "save-stutter", a.tag)
    os.makedirs(out, exist_ok=True)
    proxy = subprocess.Popen([PROXY, "--seconds", str(a.end + 12), "--iters", str(a.iters), "--topmost", "1",
                              "--log", os.path.join(out, "gameproxy.json")] + a.proxy_args.split(), stdout=subprocess.DEVNULL)
    time.sleep(4)
    cli = subprocess.run([a.cli, "save-stutter", out, f"saves={a.saves}", f"end={a.end}", f"replay={a.replay}",
                          f"notice={a.notice}", f"thumb={a.thumb}", f"overlay={a.overlay}", f"capture_api={a.capture_api}",
                          f"encoder={a.encoder}", f"audio={a.audio}"], capture_output=True, text=True)
    open(os.path.join(out, "cli.log"), "w", encoding="utf-8").write(cli.stdout + cli.stderr)
    proxy.wait(timeout=a.end + 60)
    if cli.returncode != 0:
        sys.exit(f"save-stutter failed: {cli.stderr[-2000:]}")
    analyse(out)


if __name__ == "__main__":
    main()
