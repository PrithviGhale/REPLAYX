"""Deterministic offline check of `wgc-sfc` (Quick Sync VPP on the fixed-function
scaler, vpp_scale_mode=1) against today's VPP, on the real Intel hardware chain
REPLAYX uses for WGC on an Intel-driven display:

    BGRA D3D11 frames -> re-timing (regrid) -> hwmap to QSV -> vpp_qsv NV12
    (BT.709, limited) -> h264_qsv low power -> 60 FPS CFR output, with audio

Input timing scenarios (all synthetic, no capture, nothing on screen):
    regular80  80 deliveries/s (WGC's 12.5 ms floor)
    jitter     80/s with +-3 ms random timing jitter
    gap        80/s with a 1 s gap (missing frames)
    starve50   50/s (a game below the output rate)
    refresh240 240 Hz refresh pattern: alternating 12.5 / 16.7 ms

For each scenario both modes must give the same output frame count, the same
drop/duplicate counters, identical video and audio packet timestamps, and the
same source frame in every output slot (per-frame PSNR between the two
outputs stays high; a different source frame would drop it far lower).

usage: py -3 scripts/bench/sfc-offline.py [--seconds 12] [--keep]
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_lock = json.load(open(os.path.join(ROOT, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig"))
BIN = os.path.join(ROOT, "tools", _lock["runtime_dir"])
FFMPEG, FFPROBE = os.path.join(BIN, "ffmpeg.exe"), os.path.join(BIN, "ffprobe.exe")

SCENARIOS = {
    "regular80": "N*0.0125",
    "jitter": "N*0.0125+(random(0)-0.5)*0.006",
    # 80/s with one second of frames missing in the middle (select below).
    "gap": "N*0.0125",
    # 50/s.
    "starve50": "N*0.02",
    # 240 Hz refreshes: deliveries alternate 3 and 4 refreshes apart.
    "refresh240": "(floor(N/2)*7+mod(N,2)*3)/240",
}
# REPLAYX's 1x re-timing (media::regrid_filter) and VPP colour settings.
REGRID = "setpts=round(PTS*TB*60)/(TB*60)"
VPP = "vpp_qsv=w=1920:h=1080:format=nv12{mode}:out_color_matrix=bt709:out_range=1"


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"failed: {' '.join(cmd)}\n{r.stderr[-1500:]}")
    return r


def encode(scenario, sfc, seconds, out_dir):
    expr = SCENARIOS[scenario]
    rate = 50 if scenario == "starve50" else 80
    src = f"testsrc2=s=1920x1080:r={rate}:d={seconds},setpts='({expr})/TB'"
    if scenario == "gap":
        # One second of missing frames in the middle of the run.
        first = seconds * rate // 2
        src += f",select='not(between(n\\,{first}\\,{first + rate - 1}))'"
    # Audio: a 1 kHz beep in the first 50 ms of every second (sync marker).
    audio = f"sine=f=1000:r=48000:d={seconds},volume='if(lt(mod(t\\,1)\\,0.05)\\,1\\,0)':eval=frame"
    vf = (f"format=bgra,hwupload=extra_hw_frames=16,{REGRID},hwmap=derive_device=qsv,format=qsv,"
          + VPP.format(mode=":scale_mode=1" if sfc else "") + ",setsar=1")
    name = f"{scenario}-{'sfc' if sfc else 'default'}"
    out = os.path.join(out_dir, name + ".mkv")
    prog = os.path.join(out_dir, name + ".progress")
    run([FFMPEG, "-hide_banner", "-v", "error", "-y",
         "-init_hw_device", "d3d11va=dx", "-init_hw_device", "qsv=qs@dx", "-filter_hw_device", "dx",
         "-f", "lavfi", "-i", src, "-f", "lavfi", "-i", audio,
         "-vf", vf, "-map", "0:v", "-map", "1:a",
         "-c:v", "h264_qsv", "-low_power", "1", "-b:v", "15M", "-maxrate", "23M", "-bufsize", "30M",
         "-g", "120", "-bf", "0", "-forced_idr", "1",
         "-c:a", "aac", "-b:a", "192k",
         "-r", "60", "-fps_mode", "cfr", "-progress", prog, out])
    counters = {}
    for line in open(prog, encoding="utf-8"):
        k, _, v = line.strip().partition("=")
        if k in ("frame", "drop_frames", "dup_frames"):
            counters[k] = int(v)
    return out, counters


def packets(path, stream):
    r = run([FFPROBE, "-v", "error", "-select_streams", stream, "-show_entries", "packet=pts_time",
             "-of", "csv=p=0", path])
    return [l.strip() for l in r.stdout.splitlines() if l.strip()]


def per_frame_psnr(a, b):
    r = subprocess.run([FFMPEG, "-hide_banner", "-i", a, "-i", b, "-lavfi",
                        "[0:v][1:v]psnr=stats_file=-", "-f", "null", "-"], capture_output=True, text=True)
    values = [float(m) for m in re.findall(r"psnr_avg:([0-9.]+|inf)", r.stdout) if m != "inf"]
    avg = re.search(r"average:([0-9.inf]+)", r.stderr)
    return values, (avg.group(1) if avg else "?")


def shifted_psnr(a):
    """Negative control: the same output against itself one frame later. A
    different source frame must score clearly below the 35 dB threshold."""
    r = subprocess.run([FFMPEG, "-hide_banner", "-i", a, "-i", a, "-lavfi",
                        "[0:v]trim=start_frame=1,setpts=PTS-STARTPTS[x];[1:v]setpts=PTS-STARTPTS[y];"
                        "[x][y]psnr=stats_file=-", "-f", "null", "-"], capture_output=True, text=True)
    values = [float(m) for m in re.findall(r"psnr_avg:([0-9.]+)", r.stdout)]
    return max(values) if values else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seconds", type=int, default=12)
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    tmp = tempfile.mkdtemp(prefix="replayx-sfc-")
    ok = True
    try:
        for scenario in SCENARIOS:
            d_out, d_cnt = encode(scenario, False, a.seconds, tmp)
            s_out, s_cnt = encode(scenario, True, a.seconds, tmp)
            same_counters = d_cnt == s_cnt
            same_video_pts = packets(d_out, "v:0") == packets(s_out, "v:0")
            same_audio_pts = packets(d_out, "a:0") == packets(s_out, "a:0")
            psnr, avg = per_frame_psnr(d_out, s_out)
            low = min(psnr) if psnr else 0.0
            # Same source frame in every slot: no frame below 35 dB (a
            # neighbouring testsrc2 frame scores far lower).
            same_frames = bool(psnr) and low >= 35.0
            passed = same_counters and same_video_pts and same_audio_pts and same_frames
            ok &= passed
            if scenario == "regular80":
                control = shifted_psnr(d_out)
                ok &= control is not None and control < 35.0
                print(f"control: one frame apart scores at most {control:.1f} dB (must be < 35)")
            print(f"{scenario:11} {'PASS' if passed else 'FAIL'}  default {d_cnt}  sfc {s_cnt}  "
                  f"video pts same {same_video_pts}  audio pts same {same_audio_pts}  "
                  f"PSNR default vs sfc: avg {avg} dB, min frame {low:.1f} dB")
    finally:
        if a.keep:
            print("outputs kept in", tmp)
        else:
            shutil.rmtree(tmp, ignore_errors=True)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
