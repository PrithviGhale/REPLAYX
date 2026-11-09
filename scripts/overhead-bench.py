"""Measure REPLAYX's impact on a game-like workload and the smoothness of what it records.

For each configuration: start replayx-gameproxy (heavy shader on the
high-performance GPU, full screen), measure its FPS alone, then run a real
REPLAYX capture (`replayx-cli capture-test`, silent) while it keeps running and
measure again. Records per-engine GPU utilisation and CPU use during the
capture window, and analyses the saved clip with gameproxy-check.py.
Captures the screen: run only when authorised and nothing else is on screen.

usage: python scripts/overhead-bench.py --configs none,qsv,qsv-lp [--fps 60] [--repeat 1]
       [--iters 200] [--warm 10] [--capture 20]
"""
import argparse
import glob
import importlib.util
import json
import os
import statistics
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
rel = os.path.join(root, "src-tauri", "target", "release")
CLI, PROXY = os.path.join(rel, "replayx-cli.exe"), os.path.join(rel, "replayx-gameproxy.exe")
spec = importlib.util.spec_from_file_location("wc", os.path.join(root, "scripts", "window-check.py"))
wc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wc)

# label -> (capture-test overrides, REPLAYX_VIDEO_TUNING); None = no capture (control).
CONFIGS = {
    "none": None,
    "qsv": (["encoder=h264_qsv"], ""),
    "qsv-lp": (["encoder=h264_qsv"], "qsv_low_power=1"),
    "qsv-lp-sfc": (["encoder=h264_qsv"], "qsv_low_power=1,vpp_scale_mode=1"),
    "qsv-lp-ve": (["encoder=h264_qsv"], "qsv_low_power=1,vpp_scale_mode=5"),
    "qsv-lp-sfc-nomouse": (["encoder=h264_qsv"], "qsv_low_power=1,vpp_scale_mode=1,draw_mouse=0"),
    "qsv-nomouse": (["encoder=h264_qsv"], "draw_mouse=0"),
    "nvenc-hybrid": (["encoder=h264_nvenc"], ""),
    "nvenc-hybrid-sfc": (["encoder=h264_nvenc"], "vpp_scale_mode=1"),
    "cpu-scale-nvenc": (["encoder=h264_nvenc", "gpu=0"], ""),
    # Automatic path selection with and without raised capture priority.
    "smooth": (["scale_policy=smooth"], ""),
    "wallclock": ([], "audio_wallclock=1"),
    "split": ([], "split_graphs=1"),
    "la20": ([], "nvenc_lookahead=20"),
    "la32": ([], "nvenc_lookahead=32"),
    "la40": ([], "nvenc_lookahead=40"),
    "split-la20": ([], "split_graphs=1,nvenc_lookahead=20"),
    "split-wallclock": ([], "split_graphs=1,audio_wallclock=1"),
    "split-il0": ([], "split_graphs=1,interleave_delta=0"),
    "vqueue64": ([], "video_queue=64"),
    "split-vqueue": ([], "split_graphs=1,video_queue=64"),
    "interleave0": ([], "interleave_delta=0"),
    "smooth-os2": (["scale_policy=smooth"], "oversample=2"),
    "smooth-os4": (["scale_policy=smooth"], "oversample=4"),
    "exact": (["scale_policy=exact"], ""),
    "prio-none": ([], "cpu_priority=0,gpu_priority=0"),
    "prio-gpu": ([], "cpu_priority=0,gpu_priority=1"),
    "prio-cpu": ([], "cpu_priority=1,gpu_priority=0"),
    "prio-both": ([], "cpu_priority=1,gpu_priority=1"),
    # Codecs with the default (Balanced, capped constant quality on NVENC).
    "h264": (["codec=h264"], ""),
    "hevc": (["codec=hevc"], ""),
    "av1": (["codec=av1"], ""),
    "av1-quality": (["codec=av1", "preset=quality"], ""),
    "h264-quality": (["codec=h264", "preset=quality"], ""),
}

GPU_PS = r"""
$s = Get-Counter -Counter '\GPU Engine(*)\Utilization Percentage','\Process(*)\% Processor Time' -SampleInterval 1 -MaxSamples {n} -ErrorAction SilentlyContinue
$acc = @{{}}; $cpu = @{{}}
foreach ($set in $s) {{ foreach ($x in $set.CounterSamples) {{
  if ($x.Path -like '*gpu engine*' -and $x.InstanceName -match 'pid_(\d+)_luid_(0x\w+_0x\w+)_phys_\d+_eng_\d+_engtype_(.*)$') {{
    $p = (Get-Process -Id $matches[1] -ErrorAction SilentlyContinue).ProcessName
    $k = "$p|$($matches[2])|$($matches[3])"; $acc[$k] += $x.CookedValue / {n} }}
  elseif ($x.Path -like '*% processor time*' -and $x.InstanceName -match '^(ffmpeg|replayx-cli|replayx-gameproxy|dwm|robloxplayerbeta)') {{
    $k = $x.InstanceName -replace '#\d+$',''; $cpu[$k] += $x.CookedValue / {n} / [Environment]::ProcessorCount }} }} }}
@{{ gpu = $acc; cpu = $cpu }} | ConvertTo-Json -Compress
"""


def stats(times):
    ft = [(b - a) * 1000 for a, b in zip(times, times[1:])]
    if len(ft) < 20:
        return None
    s = sorted(ft)
    worst1 = s[-max(1, len(s) // 100):]
    worst01 = s[-max(1, len(s) // 1000):]
    return {
        "fps": round(len(ft) / ((times[-1] - times[0])), 1),
        "low1_fps": round(1000 / statistics.mean(worst1), 1),
        "low01_fps": round(1000 / statistics.mean(worst01), 1),
        "frametime_p99_ms": round(s[int(0.99 * (len(s) - 1))], 2),
        "spikes_over_2x_median": sum(1 for f in ft if f > 2 * statistics.median(ft)),
    }


def run(label, args, fps, height, mode, out):
    os.makedirs(out, exist_ok=True)
    cfg = CONFIGS[label]
    total = args.warm + args.capture + 9
    proxy_log = os.path.join(out, "proxy.json")
    proxy = subprocess.Popen([PROXY, "--seconds", str(total), "--log", proxy_log, "--iters", str(args.iters),
                              "--cpu-us", str(args.cpu_us)])
    time.sleep(1.5)
    hwnd = wc.main_window(proxy.pid)
    if hwnd:
        wc.bring_to_front(hwnd)
    time.sleep(args.warm - 1.5)
    gpu = subprocess.Popen(["powershell", "-NoProfile", "-Command", GPU_PS.format(n=args.capture - 6)],
                           stdout=subprocess.PIPE, text=True) if cfg else None
    t0 = time.perf_counter()
    if cfg:
        overrides, tuning = cfg
        env = dict(os.environ, REPLAYX_VIDEO_TUNING=tuning)
        cmd = [CLI, "capture-test", str(args.capture), args.audio, f"fps={fps}", f"height={height}", f"mode={mode}",
               f"width={args.width}", *([f"bitrate={args.bitrate}"] if args.bitrate else []), *overrides]
        subprocess.run(cmd, cwd=out, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=180)
    else:
        gpu = subprocess.Popen(["powershell", "-NoProfile", "-Command", GPU_PS.format(n=args.capture - 6)],
                               stdout=subprocess.PIPE, text=True)
        time.sleep(args.capture + 4)
    t1 = time.perf_counter()
    proxy.wait(timeout=total + 60)
    usage = {}
    if gpu:
        try:
            usage = json.loads(gpu.communicate(timeout=90)[0] or "{}")
        except (subprocess.TimeoutExpired, json.JSONDecodeError):
            gpu.kill()
            usage = {"error": "GPU counter sampling timed out"}
    log = json.load(open(proxy_log))
    abs_t = [log["start_qpc_s"] + p for p in log["presents_s"]]
    base = stats([t for t in abs_t if log["start_qpc_s"] + 2 < t < t0 - 0.5])
    during = stats([t for t in abs_t if t0 + 5 < t < t0 + args.capture - 1])
    clip = None
    clips = sorted(glob.glob(os.path.join(out, "test-output", "capture-test-*", "clips", "**", "*.mkv"), recursive=True),
                   key=os.path.getsize)
    if clips:
        clip = json.loads(subprocess.check_output([sys.executable, "-I", os.path.join(root, "scripts", "gameproxy-check.py"),
                                                   clips[-1], "--skip-seconds", "6"]))
        clip.pop("advance_histogram_ms", None)
    report = {"label": label, "fps_setting": fps, "adapter": log["adapter"], "baseline": base, "capture": during,
              "fps_drop_pct": round(100 * (1 - during["fps"] / base["fps"]), 1) if base and during else None,
              "usage": usage, "clip": clip}
    json.dump(report, open(os.path.join(out, "report.json"), "w"), indent=1)
    return report


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--configs", default="none,qsv")
    ap.add_argument("--fps", default="60")
    ap.add_argument("--height", type=int, default=1080)
    ap.add_argument("--mode", default="stretch")
    ap.add_argument("--bitrate", type=int, default=0, help="0 = Balanced preset")
    ap.add_argument("--width", type=int, default=1920)
    ap.add_argument("--audio", default="silent", help="silent or system (loopback; never the microphone)")
    ap.add_argument("--repeat", type=int, default=1)
    ap.add_argument("--iters", type=int, default=200)
    ap.add_argument("--cpu-us", type=int, default=0)
    ap.add_argument("--warm", type=int, default=10)
    ap.add_argument("--capture", type=int, default=20)
    ap.add_argument("--tag", default=time.strftime("%H%M%S"))
    args = ap.parse_args()
    results = []
    for r in range(args.repeat):
        for fps in args.fps.split(","):
            for label in args.configs.split(","):
                out = os.path.join(root, "test-output", "overhead", args.tag, f"{label}-{fps}-r{r + 1}")
                rep = run(label, args, int(fps), args.height, args.mode, out)
                results.append(rep)
                b, c, k = rep["baseline"] or {}, rep["capture"] or {}, rep["clip"] or {}
                print(f"{label:<20} {fps:>3}fps  game {b.get('fps')} -> {c.get('fps')} fps ({rep['fps_drop_pct']}%), "
                      f"1% low {b.get('low1_fps')} -> {c.get('low1_fps')}; clip {k.get('size')} repeated {k.get('repeated_pct')}% "
                      f"freeze {k.get('longest_freeze_ms')} ms jumps {k.get('jumps_over_1_5x_pct')}%", flush=True)
                time.sleep(3)
    json.dump(results, open(os.path.join(root, "test-output", "overhead", args.tag, "all.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
