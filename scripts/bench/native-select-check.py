"""Check the native prototype's frame selector against FFmpeg itself.

For each delivery pattern, numbered synthetic frames with gfxcapture-like
timestamps (100 ns time base, an advertised 80 FPS like gfxcapture at the
12.5 ms floor, whatever the real delivery rate) go through REPLAYX's real
1x path in FFmpeg - regrid setpts, then `-r 60 -fps_mode cfr` - and the same
timestamps through `replayx-native-proto --select-test`. Every output slot
must show the same source frame, and the drop/duplicate counters must match.

usage: py -3 scripts/bench/native-select-check.py [--seconds 12] [--proto PATH]
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_lock = json.load(open(os.path.join(ROOT, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig"))
FFMPEG = os.path.join(ROOT, "tools", _lock["runtime_dir"], "ffmpeg.exe")
PROTO = os.path.join(ROOT, "src-tauri", "target", "release", "replayx-native-proto.exe")
REGRID = "setpts=round(PTS*TB*60)/(TB*60)"

# name -> (source rate, capture time of frame N in seconds, frames removed)
SCENARIOS = {
    "regular80": (80, "N*0.0125", None),
    "jitter": (80, "0.01+N*0.0125+(random(0)-0.5)*0.006", None),
    "gap": (80, "N*0.0125", "mid"),
    "starve50": (50, "N*0.02", None),
    "refresh240": (80, "(floor(N/2)*7+mod(N\\,2)*3)/240", None),
    "uneven": (80, "N*0.0125+mod(N\\,7)*0.002", None),
    "exact60": (60, "N/60", None),
}


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if r.returncode != 0:
        sys.exit(f"failed: {' '.join(cmd)}\n{r.stderr[-1500:]}")
    return r


def source_chain(name, seconds):
    rate, expr, cut = SCENARIOS[name]
    # Generated at 80 FPS (gfxcapture's advertised rate at the 12.5 ms
    # floor), then re-timed to the pattern; the frame rate metadata stays 80.
    frames = int(seconds * rate)
    chain = (f"testsrc2=s=320x180:r=80:d={frames / 80},settb=1/10000000,"
             f"setpts='({expr})/TB'")
    if cut == "mid":
        first = seconds * rate // 2
        chain += f",select='not(between(n\\,{first}\\,{first + rate - 1}))'"
    return chain


def md5s(text):
    return [line.rsplit(",", 1)[1].strip() for line in text.splitlines() if line and not line.startswith("#")]


def check(name, seconds, tmp):
    src = source_chain(name, seconds)
    info = run([FFMPEG, "-hide_banner", "-f", "lavfi", "-i", src, "-vf", "showinfo", "-f", "null", "-"])
    times = [int(t) for t in re.findall(r" pts:\s*(-?\d+)", info.stderr)]
    src_md5 = md5s(run([FFMPEG, "-v", "error", "-f", "lavfi", "-i", src, "-fps_mode", "passthrough",
                        "-f", "framemd5", "-"]).stdout)
    prog = os.path.join(tmp, name + ".progress")
    out_md5 = md5s(run([FFMPEG, "-v", "error", "-f", "lavfi", "-i", src, "-vf", REGRID, "-r", "60",
                        "-fps_mode", "cfr", "-progress", prog, "-f", "framemd5", "-"]).stdout)
    counters = {}
    for line in open(prog, encoding="utf-8"):
        k, _, v = line.strip().partition("=")
        if k in ("dup_frames", "drop_frames"):
            counters[k] = int(v)
    times_file = os.path.join(tmp, name + ".times")
    open(times_file, "w").write("\n".join(map(str, times)))
    proto = json.loads(run([PROTO, "--select-test", times_file]).stdout)
    predicted = [src_md5[i] for i in proto["slots"]]
    first_diff = next((i for i, (a, b) in enumerate(zip(predicted, out_md5)) if a != b), None)
    same = len(predicted) == len(out_md5) and first_diff is None
    same_counters = counters == {"dup_frames": proto["dup_frames"], "drop_frames": proto["drop_frames"]}
    unique = len(set(src_md5)) == len(src_md5)
    ok = same and same_counters and len(times) == len(src_md5)
    print(f"{name:11} {'PASS' if ok else 'FAIL'}  frames in {len(times)}  slots ffmpeg {len(out_md5)} / native "
          f"{len(predicted)}  first difference {first_diff}  counters ffmpeg {counters} / native "
          f"{{'dup_frames': {proto['dup_frames']}, 'drop_frames': {proto['drop_frames']}}}"
          + ("" if unique else "  (source frames not all distinct)"))
    return ok


def main():
    global PROTO
    ap = argparse.ArgumentParser()
    ap.add_argument("--seconds", type=int, default=12)
    ap.add_argument("--proto", default=PROTO)
    a = ap.parse_args()
    PROTO = a.proto
    with tempfile.TemporaryDirectory(prefix="replayx-select-") as tmp:
        results = [check(name, a.seconds, tmp) for name in SCENARIOS]
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
