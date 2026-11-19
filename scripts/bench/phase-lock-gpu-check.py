"""Real GPU encode/decode audit of the phase-locked 60 FPS grid on recorded timelines.

Each saved WGC trace (capture times + source frame IDs) is replayed through
the worker's actual recording path (`--synthetic --replay-timeline`): generated
D3D11 textures carrying the source frame ID as a 24-bit pattern, fixed-function
conversion, the hardware H.264 encoder of the chosen vendor. The encoded
stream is decoded with FFmpeg and checked independently:
- every decoded frame's ID equals the capture the scheduler chose
  (`shown_captures`), and equals `--pacing-replay` (the pure scheduler);
- frame count, keyframes every 120, no B-frames, rational 60 FPS timestamps,
  no leaked encoder input surfaces;
- repeats / skipped source IDs counted from the decoded IDs, for
  `--pacing phase-lock` (after) and `--pacing clocked` (before).
Generated textures only: no desktop capture, no window, no audio.

usage: py -3 scripts/bench/phase-lock-gpu-check.py [--vendor 10de|8086] [--hints content|none]
       [--release-delay-ms 40]  (encoder backpressure injection)
"""
import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROTO = ROOT / "src-tauri" / "target" / "release" / "replayx-native-proto.exe"
LOCK = json.loads((ROOT / "tools" / "ffmpeg-lock.json").read_text(encoding="utf-8-sig"))
BINS = ROOT / "tools" / LOCK["runtime_dir"]
TRACES = ROOT / "test-output" / "native-smoothness"


def run(cmd, timeout=120):
    r = subprocess.run(list(map(str, cmd)), capture_output=True, timeout=timeout)
    if r.returncode:
        sys.exit(f"failed: {' '.join(map(str, cmd))}\n{r.stderr.decode(errors='replace')[-1500:]}")
    return r.stdout


def timeline(folder):
    events = [json.loads(l) for l in (folder / "trace.jsonl").read_text(encoding="utf-8-sig").splitlines() if l.strip()]
    content = {e["capture"]: e["content_id"] for e in events if e["stage"] == "captured_content"}
    caps = sorted((e for e in events if e["stage"] == "capture_dequeued"), key=lambda e: e["capture"])
    if not caps or len(caps) != len(content) or len(caps) > 4096:
        return None
    return {"captures": [{"time_hns": c["capture_hns"], "content_id": content[c["capture"]]} for c in caps]}


def decode_ids(clip):
    # Bit b of the ID is a 32x32 block at x 48+48b, y 32 of the 2560x1600
    # source; after scaling to 1920x1080 its centre is at x 48+36b, y 32.
    raw = run([BINS / "ffmpeg.exe", "-v", "error", "-xerror", "-threads", "2", "-i", clip,
               "-vf", "crop=1920:8:0:28,extractplanes=y", "-f", "rawvideo", "-"])
    row = 1920 * 8
    ids = []
    for off in range(0, len(raw), row):
        f = raw[off:off + row]
        v = 0
        for b in range(24):
            x = 48 + 36 * b
            s = sum(f[y * 1920 + x + dx] for y in range(2, 6) for dx in range(-3, 4)) / 28
            if s > 126:
                v |= 1 << b
        ids.append(v)
    return ids


def transitions(ids):
    rep = sum(1 for a, b in zip(ids, ids[1:]) if a == b)
    skip = sum(max(0, b - a - 1) for a, b in zip(ids, ids[1:]))
    back = sum(1 for a, b in zip(ids, ids[1:]) if b < a)
    return rep, skip, back


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--vendor", default="10de")
    ap.add_argument("--hints", default="content", choices=["content", "none"])
    ap.add_argument("--trace", action="append")
    ap.add_argument("--release-delay-ms", type=int, default=0,
                    help="hold encoder input surfaces longer (backpressure injection); requires surface waits")
    a = ap.parse_args()
    folders = [TRACES / t for t in a.trace] if a.trace else sorted(p for p in TRACES.iterdir() if (p / "trace.jsonl").exists())
    failures = 0
    with tempfile.TemporaryDirectory(prefix="phase-gpu-", dir=ROOT / "test-output") as tmp:
        for folder in folders:
            tl = timeline(folder)
            if tl is None:
                continue
            tl_path = Path(tmp) / f"{folder.name}.json"
            tl_path.write_text(json.dumps(tl))
            source_ids = [c["content_id"] for c in tl["captures"]]
            captured = set(source_ids)
            line = []
            for pacing in ("clocked", "phase-lock"):
                clip = Path(tmp) / f"{folder.name}-{pacing}.h264"
                r = json.loads(run([PROTO, "--synthetic", "regular80", clip, "--replay-timeline", tl_path,
                                    "--vendor", a.vendor, "--pacing", pacing, "--phase-hints", a.hints,
                                    "--trace-schedule", "--test-release-delay-ms", a.release_delay_ms]))
                shown = r["shown_captures"]
                expected = [source_ids[i] for i in shown]
                decoded = decode_ids(clip)
                sched = json.loads(run([PROTO, "--pacing-replay", tl_path, "--pacing", pacing, "--phase-hints", a.hints]))
                n = min(len(shown), len(sched["picks"]))
                frames = json.loads(run([BINS / "ffprobe.exe", "-v", "error", "-select_streams", "v:0", "-show_frames",
                                         "-show_entries", "frame=key_frame,pict_type", "-of", "json", clip]))["frames"]
                checks = {
                    "decoded_ids_match": decoded == expected,
                    "matches_pure_scheduler": shown[:n] == sched["picks"][:n] and abs(len(shown) - len(sched["picks"])) <= 2,
                    "frame_count": len(decoded) == r["frames_out"] == r["encoded"] == len(shown),
                    "gop_120": [i for i, f in enumerate(frames) if f["key_frame"]] == list(range(0, len(frames), 120)),
                    "no_b_frames": not any(f["pict_type"] == "B" for f in frames),
                    "regular_rational_timestamps": r["output_timestamps_regular"] is True,
                    "no_leaked_surfaces": r["live_input_samples"] == 0,
                }
                if a.release_delay_ms:
                    checks["backpressure_exercised"] = r["surface_reuse_waits"] > 0
                rep, skip, back = transitions(decoded)
                lost = len({x for x in captured if decoded[0] <= x <= decoded[-1]} - set(decoded))
                ok = all(checks.values()) and back == 0
                failures += not ok
                lock = r.get("phase_lock") or {}
                line.append(f"{pacing}: {'PASS' if ok else 'FAIL ' + str([k for k, v in checks.items() if not v])} "
                            f"{len(decoded)} frames, repeats {rep}, lost {lost}"
                            + (f", moves {lock.get('moves')}, hints {lock.get('change_hints')}" if lock else ""))
            print(f"{a.vendor} {folder.name:24} " + " | ".join(line), flush=True)
    print("PASS" if not failures else f"FAIL ({failures})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
