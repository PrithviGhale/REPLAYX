"""End-to-end offline check of the native recording prototype.

For each delivery pattern `replayx-native-proto --synthetic` runs the whole
native path on synthetic captures (Intel GPU: fixed-function conversion of
chosen frames, Quick Sync H.264 from D3D11 textures). Each capture is a flat
grey whose level encodes its index. This script decodes the output with
FFmpeg and checks that:
- every output slot shows the capture the selector chose (centre luma within
  3 levels);
- the frame count, the keyframe spacing (every 120 frames) and the
  timestamps are right.
That the selector's choices and drop/duplicate counters equal FFmpeg's own is
checked separately by native-select-check.py (the same delivery patterns).

Nothing is captured from the screen.

usage: py -3 scripts/bench/native-proto-check.py [--seconds 8]
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_lock = json.load(open(os.path.join(ROOT, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig"))
BIN = os.path.join(ROOT, "tools", _lock["runtime_dir"])
FFMPEG, FFPROBE = os.path.join(BIN, "ffmpeg.exe"), os.path.join(BIN, "ffprobe.exe")
PROTO = os.path.join(ROOT, "src-tauri", "target", "release", "replayx-native-proto.exe")
SCENARIOS = ["regular80", "jitter", "gap", "starve50", "refresh240"]


def run(cmd):
    r = subprocess.run(cmd, capture_output=True)
    if r.returncode != 0:
        sys.exit(f"failed: {' '.join(cmd)}\n{r.stderr.decode(errors='replace')[-1500:]}")
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seconds", type=int, default=8)
    ap.add_argument("--vendor", default="8086", help="PCI vendor: 8086 Intel, 10de NVIDIA, 1002 AMD; requires real hardware")
    a = ap.parse_args()
    ok_all = True
    with tempfile.TemporaryDirectory(prefix="replayx-native-") as tmp:
        for scenario in SCENARIOS:
            out = os.path.join(tmp, scenario + ".h264")
            report = json.loads(run([PROTO, "--synthetic", scenario, out, "--seconds", str(a.seconds), "--vendor", a.vendor]).stdout)
            # Centre 16x16 block of every decoded frame: the raw (limited-range)
            # luma plane, not a full-range grey conversion.
            raw = run([FFMPEG, "-v", "error", "-i", out, "-vf", "crop=16:16:952:532,extractplanes=y",
                       "-f", "rawvideo", "-"]).stdout
            frames = [raw[i:i + 256] for i in range(0, len(raw), 256)]
            luma = [sum(f) / len(f) for f in frames]
            expected = report["expected_luma"]
            wrong = [i for i, (got, want) in enumerate(zip(luma, expected)) if abs(got - want) > 3]
            keys = run([FFPROBE, "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=key_frame",
                        "-of", "csv=p=0", out]).stdout.decode().split()
            key_at = [i for i, k in enumerate(keys) if k.strip() == "1"]
            keys_ok = key_at == list(range(0, len(keys), 120))
            times_ok = report["frames_out"] == len(frames) == len(expected) == report["encoded"]
            passed = not wrong and keys_ok and times_ok and report["output_timestamps_increasing"]
            ok_all &= passed
            print(f"{scenario:11} {'PASS' if passed else 'FAIL'}  captures {report['captures']} converted "
                  f"{report['converted']} encoded {report['encoded']} decoded {len(frames)}  dup {report['dup_frames']} "
                  f"drop {report['drop_frames']} repeats-of-dropped {report['repeats_of_dropped_frames']}  "
                  f"wrong slots {len(wrong)}{' first ' + str(wrong[0]) if wrong else ''}  keyframes {key_at[:4]}... ok {keys_ok}")
    sys.exit(0 if ok_all else 1)


if __name__ == "__main__":
    main()
