"""Video quality per gigabyte: encoder/rate-control comparison on one source.

1. Builds a lossless, game-like 1080p60 test source (FFV1) from FFmpeg
   generators: zooming fractal detail, a panning camera, shimmering foliage,
   saturated particle-like cells, a dark gradient zone, a minimap and small HUD
   text with a changing score. Or use --source CLIP (e.g. a real recording;
   note a compressed source favours codecs that reproduce its artefacts).
2. Encodes it with each configuration on the hardware encoders (same options
   REPLAYX uses unless the configuration overrides them).
3. Scores each encode against the source with VMAF (v0.6.1 model), SSIM and
   PSNR, and reports size, bitrate and encoder throughput.
No screen capture, no audio. Output: test-output/quality/<tag>/results.json
usage: python scripts/bench/quality-bench.py [--seconds 12] [--configs a,b] [--source CLIP]
"""
import argparse
import json
import os
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
lock = json.loads(open(os.path.join(root, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig").read())
bindir = os.path.join(root, "tools", lock["runtime_dir"])
FFMPEG, FFPROBE = os.path.join(bindir, "ffmpeg.exe"), os.path.join(bindir, "ffprobe.exe")
FONT = "C\\:/Windows/Fonts/segoeui.ttf"

NV_COMMON = ["-tune", "hq", "-spatial-aq", "1", "-rc-lookahead", "20", "-bf", "0", "-g", "120"]
# name -> encoder args (after -c:v). VBR targets are Mb/s with a 1.5x peak.
def vbr(enc, mbps, preset="p5", extra=()):
    return [enc, "-preset", preset, *NV_COMMON, "-rc", "vbr", "-b:v", f"{mbps}M", "-maxrate", f"{int(mbps * 1.5)}M",
            "-bufsize", f"{mbps * 2}M", *extra]
def cq(enc, q, cap_mbps, preset="p5", extra=()):
    return [enc, "-preset", preset, *NV_COMMON, "-rc", "vbr", "-cq", str(q), "-b:v", "0", "-maxrate", f"{cap_mbps}M",
            "-bufsize", f"{cap_mbps * 2}M", *extra]
CONFIGS = {
    "h264-vbr16-p5": vbr("h264_nvenc", 16),
    "h264-vbr12-p5": vbr("h264_nvenc", 12),
    "h264-vbr10-p5": vbr("h264_nvenc", 10),
    "h264-vbr16-p5-tq": vbr("h264_nvenc", 16, extra=("-temporal-aq", "1")),
    "h264-cq23": cq("h264_nvenc", 23, 40),
    "h264-cq26": cq("h264_nvenc", 26, 40),
    "hevc-vbr10-p5": vbr("hevc_nvenc", 10),
    "hevc-vbr8-p5": vbr("hevc_nvenc", 8),
    "hevc-cq26": cq("hevc_nvenc", 26, 40),
    "hevc-cq28": cq("hevc_nvenc", 28, 40),
    "av1-vbr8-p5": vbr("av1_nvenc", 8),
    "av1-vbr6-p5": vbr("av1_nvenc", 6),
    "av1-cq30": cq("av1_nvenc", 30, 40),
    "av1-cq34": cq("av1_nvenc", 34, 40),
    "h264-vbr16-p1": vbr("h264_nvenc", 16, preset="p1"),
    "h264-cq28": cq("h264_nvenc", 28, 24),
    "h264-cq30": cq("h264_nvenc", 30, 24),
    "hevc-cq30": cq("hevc_nvenc", 30, 20),
    "hevc-cq32": cq("hevc_nvenc", 32, 20),
    "av1-cq36": cq("av1_nvenc", 36, 18),
    "av1-cq38": cq("av1_nvenc", 38, 18),
    "av1-cq34-cap18": cq("av1_nvenc", 34, 18),
    "h264-qsv-vbr16": ["h264_qsv", "-low_power", "1", "-preset", "medium", "-b:v", "16M", "-maxrate", "24M", "-bufsize", "32M", "-g", "120", "-bf", "0"],
    "hevc-qsv-vbr10": ["hevc_qsv", "-low_power", "1", "-preset", "medium", "-b:v", "10M", "-maxrate", "15M", "-bufsize", "20M", "-g", "120", "-bf", "0"],
}


def make_source(path, seconds):
    if os.path.exists(path):
        return
    os.makedirs(os.path.dirname(path), exist_ok=True)
    n = seconds * 60
    graph = (
        f"mandelbrot=s=1920x700:rate=60:start_scale=3:end_scale=0.02:end_pts={n}:inner=mincol,format=yuv420p,"
        f"scroll=h=0.0015:v=0.0004[sky];"
        f"color=c=0x1d5a1a:s=1920x380:r=60,noise=alls=28:allf=t+u,format=yuv420p,scroll=h=-0.003[grass];"
        f"[sky][grass]vstack=inputs=2[world];"
        f"gradients=s=640x260:r=60:c0=0x020306:c1=0x0e1830:speed=0.02,format=yuv420p[dark];"
        f"life=s=480x270:r=60:mold=10:ratio=0.08:life_color=#ff3a20:death_color=#202060:mold_color=#ffd000,"
        f"scale=960x540:flags=neighbor,format=yuva420p,colorchannelmixer=aa=0.55[cells];"
        f"testsrc2=s=320x180:r=60,format=yuv420p[map];"
        f"[world][dark]overlay=x=1220:y=760[w1];[w1][cells]overlay=x=180:y=220[w2];[w2][map]overlay=x=1580:y=20[w3];"
        f"[w3]drawtext=fontfile='{FONT}':text='HP 100  AMMO 30/120  ELIMS 4':fontsize=22:fontcolor=white:x=40:y=1040,"
        f"drawtext=fontfile='{FONT}':text='SCORE %{{eif\\:n*7\\:d}}':fontsize=30:fontcolor=#ffd34d:x=40:y=40,"
        f"drawtext=fontfile='{FONT}':text='Party chat  >  nice shot!':fontsize=16:fontcolor=#c8c8d0:x=40:y=1000[out]"
    )
    subprocess.check_call([FFMPEG, "-hide_banner", "-v", "error", "-y", "-filter_complex", graph, "-map", "[out]",
                           "-frames:v", str(n), "-c:v", "ffv1", "-level", "3", "-threads", "8", "-slices", "16", path])


START = 0.0


def encode(src, out, args, seconds):
    t = time.perf_counter()
    subprocess.check_call([FFMPEG, "-hide_banner", "-v", "error", "-y", "-ss", str(START), "-i", src, "-t", str(seconds), "-an",
                           "-pix_fmt", "yuv420p", "-c:v", *args, out])
    return time.perf_counter() - t


def score(enc, src, seconds, log):
    lavfi = (f"[0:v]setpts=PTS-STARTPTS[d];[1:v]trim=duration={seconds},setpts=PTS-STARTPTS[r];"
             f"[d][r]libvmaf=model=version=vmaf_v0.6.1:n_threads=8:log_fmt=json:log_path='{log.replace(chr(92), '/').replace(':', chr(92) + ':')}':"
             f"feature=name=psnr|name=float_ssim")
    subprocess.check_call([FFMPEG, "-hide_banner", "-v", "error", "-i", enc, "-ss", str(START), "-i", src, "-lavfi", lavfi, "-f", "null", "-"])
    m = json.load(open(log))["pooled_metrics"]
    frames = json.load(open(log))["frames"]
    vmaf = sorted(f["metrics"]["vmaf"] for f in frames)
    return {"vmaf": round(m["vmaf"]["mean"], 2), "vmaf_p5": round(vmaf[int(0.05 * len(vmaf))], 2),
            "ssim": round(m["float_ssim"]["mean"], 4), "psnr_y": round(m["psnr_y"]["mean"], 2)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seconds", type=int, default=12)
    ap.add_argument("--configs", default=",".join(CONFIGS))
    ap.add_argument("--source", default=None)
    ap.add_argument("--tag", default="synthetic")
    ap.add_argument("--start", type=float, default=0.0, help="seconds into --source")
    a = ap.parse_args()
    global START
    START = a.start
    out_dir = os.path.join(root, "test-output", "quality", a.tag)
    os.makedirs(out_dir, exist_ok=True)
    src = a.source or os.path.join(root, "test-output", "quality", f"source-{a.seconds}s.mkv")
    if not a.source:
        make_source(src, a.seconds)
    results = []
    for name in a.configs.split(","):
        out = os.path.join(out_dir, f"{name}.mkv")
        try:
            secs = encode(src, out, CONFIGS[name], a.seconds)
        except subprocess.CalledProcessError:
            print(f"{name:<20} encoder not available", flush=True)
            continue
        size = os.path.getsize(out)
        r = {"config": name, "bytes": size, "mbps": round(size * 8 / a.seconds / 1e6, 2),
             "mb_per_minute": round(size / a.seconds * 60 / 1e6, 1), "encode_fps": round(a.seconds * 60 / secs, 1),
             **score(out, src, a.seconds, os.path.join(out_dir, f"{name}.vmaf.json"))}
        r["vmaf_per_mbps"] = round(r["vmaf"] / max(r["mbps"], 0.01), 2)
        results.append(r)
        print(f"{name:<20} {r['mbps']:>6} Mb/s {r['mb_per_minute']:>6} MB/min  VMAF {r['vmaf']:>6} (p5 {r['vmaf_p5']})  "
              f"SSIM {r['ssim']}  PSNR {r['psnr_y']}  enc {r['encode_fps']} fps", flush=True)
    json.dump(results, open(os.path.join(out_dir, "results.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
