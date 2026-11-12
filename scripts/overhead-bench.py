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
import json
import os
import statistics
import subprocess
import sys
import time
import shutil
import threading

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
rel = os.path.join(root, "src-tauri", "target", "release")
CLI, PROXY = os.path.join(rel, "replayx-cli.exe"), os.path.join(rel, "replayx-gameproxy.exe")
PROBE = os.path.join(rel, "replayx-dda-profile.exe")

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
    "prio-middle": ([], "cpu_priority=0,gpu_priority=1,gpu_priority_class=3"),
    "prio-gpu": ([], "cpu_priority=0,gpu_priority=1"),
    "prio-cpu": ([], "cpu_priority=1,gpu_priority=0"),
    "prio-both": ([], "cpu_priority=1,gpu_priority=1"),
    # Codecs with the default (Balanced, capped constant quality on NVENC).
    "h264": (["codec=h264"], ""),
    "hevc": (["codec=hevc"], ""),
    "av1": (["codec=av1"], ""),
    "av1-quality": (["codec=av1", "preset=quality"], ""),
    "h264-quality": (["codec=h264", "preset=quality"], ""),
    # Attribution experiments, not proposed production quality changes.
    "aq-off": (["codec=h264"], "nvenc_aq=0"),
    "lookahead-off": (["codec=h264"], "nvenc_lookahead=0"),
    # Desktop Duplication alone, no encoder (replayx-dda-profile, 60 acquisitions/s,
    # two GPU copies like ddagrab): release right after copying (as FFmpeg's
    # ddagrab does) or hold until just before the next acquisition (Microsoft's
    # recommendation). Separates the duplication cost from the NVENC cost.
    "dda-early": ("probe", ["--hold", "0", "--copies", "2"]),
    "dda-hold": ("probe", ["--hold", "1", "--copies", "2"]),
}

GPU_PS = r"""
Start-Sleep -Seconds 5
$names = @{{}}
Get-Process -Name ffmpeg,replayx-cli,replayx-gameproxy,replayx-dda-profile,dwm -ErrorAction SilentlyContinue | ForEach-Object {{ $names[[string]$_.Id]=$_.ProcessName }}
$s = Get-Counter -Counter '\GPU Engine(*)\Utilization Percentage','\Process(*)\% Processor Time' -SampleInterval 1 -MaxSamples {n} -ErrorAction SilentlyContinue
$acc = @{{}}; $cpu = @{{}}
foreach ($set in $s) {{ foreach ($x in $set.CounterSamples) {{
  if ($x.Path -like '*gpu engine*' -and $x.InstanceName -match 'pid_(\d+)_luid_(0x\w+_0x\w+)_phys_\d+_eng_\d+_engtype_(.*)$') {{
    $p = $names[$matches[1]]; if (!$p) {{ continue }}
    $k = "$p|$($matches[2])|$($matches[3])"; $acc[$k] += $x.CookedValue / {n} }}
  elseif ($x.Path -like '*% processor time*' -and $x.InstanceName -match '^(ffmpeg|replayx-cli|replayx-gameproxy|replayx-dda-profile|dwm|robloxplayerbeta)') {{
    $k = $x.InstanceName -replace '#\d+$',''; $cpu[$k] += $x.CookedValue / {n} / [Environment]::ProcessorCount }} }} }}
@{{ gpu = $acc; cpu = $cpu }} | ConvertTo-Json -Compress
"""


def last_input_tick():
    """GetLastInputInfo: changes with any keyboard or mouse input. A run during
    which someone used the computer measured a different workload."""
    import ctypes

    class LASTINPUTINFO(ctypes.Structure):
        _fields_ = [("cbSize", ctypes.c_uint), ("dwTime", ctypes.c_uint)]
    info = LASTINPUTINFO(ctypes.sizeof(LASTINPUTINFO), 0)
    return info.dwTime if ctypes.windll.user32.GetLastInputInfo(ctypes.byref(info)) else None


# DXGI_FRAME_PRESENTATION_MODE values reported by IDXGISwapChainMedia.
PRESENTATION_MODES = {0: 'composed', 1: 'overlay', 2: 'direct', 3: 'composition_failure'}


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
    os.makedirs(out, exist_ok=False)  # never mix old clips or reports into a new run
    cfg = CONFIGS[label]
    telemetry, telemetry_thread, telemetry_process = [], None, None
    if args.telemetry and shutil.which('nvidia-smi'):
        telemetry_process = subprocess.Popen(['nvidia-smi', '--query-gpu=clocks.current.graphics,clocks.current.memory,power.draw,utilization.gpu,memory.used,temperature.gpu',
            '--format=csv,noheader,nounits', '-lms', '1000'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        def collect():
            for line in telemetry_process.stdout:
                try:
                    row = [float(v.strip()) for v in line.split(',')]
                    if len(row) == 6 and len(telemetry) < 10000:
                        telemetry.append(dict(t=time.perf_counter(), values=row))
                except ValueError:
                    pass
        telemetry_thread = threading.Thread(target=collect, daemon=True)
        telemetry_thread.start()
    total = args.warm + args.capture + 9 + args.post
    input_before = last_input_tick()
    proxy_log = os.path.join(out, "proxy.json")
    proxy = subprocess.Popen([PROXY, "--seconds", str(total), "--log", proxy_log, "--iters", str(args.iters),
                              "--cpu-us", str(args.cpu_us), "--detail", str(args.detail)])
    time.sleep(1.5)
    # The fixture opens its own full-screen window. Never manipulate unrelated
    # windows or focus through a second UI automation implementation.
    time.sleep(args.warm - 1.5)
    gpu = subprocess.Popen(["powershell", "-NoProfile", "-Command", GPU_PS.format(n=args.capture - 6)],
                           stdout=subprocess.PIPE, text=True) if cfg and args.counters else None
    t0 = time.perf_counter()
    probe_report = None
    if cfg and cfg[0] == "probe":
        out_probe = subprocess.run([PROBE, "--pid", str(proxy.pid), "--seconds", str(args.capture), *cfg[1]],
                                   capture_output=True, text=True, timeout=args.capture + 60)
        try:
            probe_report = json.loads(out_probe.stdout)
        except json.JSONDecodeError:
            probe_report = {"error": (out_probe.stderr or out_probe.stdout)[-400:]}
        if not probe_report.get("success"):
            raise RuntimeError(f"Duplication probe failed: {probe_report.get('error')}")
        time.sleep(4)
    elif cfg:
        overrides, tuning = cfg
        env = dict(os.environ, REPLAYX_VIDEO_TUNING=tuning)
        cmd = [args.cli, "capture-test", str(args.capture), args.audio, f"fps={fps}", f"height={height}", f"mode={mode}",
               f"width={args.width}", *([f"bitrate={args.bitrate}"] if args.bitrate else []), *overrides]
        with open(os.path.join(out, "capture.log"), "w") as capture_log:
            recorder = subprocess.Popen(cmd, cwd=out, env=env, stdout=capture_log, stderr=capture_log)
            deadline = time.monotonic() + args.capture + 90
            try:
                while recorder.poll() is None:
                    if proxy.poll() is not None:
                        raise RuntimeError('Fixture ended early; stopping capture instead of recording the uncovered desktop')
                    if time.monotonic() > deadline:
                        raise TimeoutError('Capture exceeded its deadline')
                    time.sleep(.1)
                if recorder.returncode:
                    raise subprocess.CalledProcessError(recorder.returncode, cmd)
            finally:
                if recorder.poll() is None:
                    recorder.terminate(); recorder.wait(timeout=15)
    else:
        gpu = subprocess.Popen(["powershell", "-NoProfile", "-Command", GPU_PS.format(n=args.capture - 6)],
                               stdout=subprocess.PIPE, text=True) if args.counters else None
        time.sleep(args.capture + 4)
    t1 = time.perf_counter()
    proxy.wait(timeout=total + 60)
    user_input = input_before is None or last_input_tick() != input_before
    if telemetry_process:
        telemetry_process.terminate()
        telemetry_process.wait(timeout=10)
        telemetry_thread.join(timeout=3)
        json.dump(telemetry, open(os.path.join(out, 'gpu-telemetry.json'), 'w'), indent=1)
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
    after = stats([t for t in abs_t if t1 + 3 < t < log['start_qpc_s'] + total - 1])
    def draw_time(lo, hi):
        samples = [v['ms'] for v in log.get('fixture_draw_gpu_ms', [])
                   if lo < log['start_qpc_s'] + v['t'] < hi]
        if not samples:
            return None
        return dict(samples=len(samples), mean_ms=statistics.mean(samples),
                    p95_ms=sorted(samples)[int(.95*(len(samples)-1))])
    def presentation(lo, hi):
        # How Windows showed the fixture's frames (sampled every 32 presents):
        # composed by DWM, an overlay plane, or direct/independent flip.
        modes = [v['mode'] for v in log.get('presentation_modes', [])
                 if lo < log['start_qpc_s'] + v['t'] < hi]
        if not modes:
            return None
        return {name: round(100 * sum(m == code for m in modes) / len(modes), 1)
                for code, name in PRESENTATION_MODES.items()} | {'samples': len(modes)}
    presentation_modes = dict(before=presentation(log['start_qpc_s']+2, t0-.5),
                              capture=presentation(t0+5, t0+args.capture-1),
                              after=presentation(t1+3, log['start_qpc_s']+total-1))
    gpu_draw = dict(before=draw_time(log['start_qpc_s']+2, t0-.5),
                    capture=draw_time(t0+5, t0+args.capture-1),
                    after=draw_time(t1+3, log['start_qpc_s']+total-1))
    def gpu_state(lo, hi):
        rows = [v['values'] for v in telemetry if lo < v['t'] < hi]
        if not rows:
            return None
        names = ['graphics_mhz', 'memory_mhz', 'power_w', 'gpu_busy_percent', 'memory_used_mb', 'temperature_c']
        return {name: dict(mean=statistics.mean(v), minimum=min(v), maximum=max(v))
                for name, v in zip(names, zip(*rows))}
    clip = None
    clips = sorted(glob.glob(os.path.join(out, "test-output", "capture-test-*", "clips", "**", "*.mkv"), recursive=True),
                   key=os.path.getsize)
    if clips:
        clip = json.loads(subprocess.check_output([sys.executable, "-I", os.path.join(root, "scripts", "gameproxy-check.py"),
                                                   clips[-1], "--skip-seconds", "6"]))
        clip.pop("advance_histogram_ms", None)
    control_fps = statistics.mean([base['fps'], after['fps']]) if base and after else None
    drift = 100 * (after['fps'] / base['fps'] - 1) if base and after else None
    valid = bool(base and during and after and drift is not None and abs(drift) <= 5
                 and log.get('detail', 0) == args.detail and not user_input)
    report = {"label": label, "fps_setting": fps, "adapter": log["adapter"], "baseline": base, "capture": during,
              "fixture_draw_gpu_timing": gpu_draw,
              "presentation_modes_percent": presentation_modes,
              "counter_sampler_enabled": args.counters,
              "gpu_state": dict(before=gpu_state(log['start_qpc_s']+2, t0-.5),
                                capture=gpu_state(t0+5,t0+args.capture-1), after=gpu_state(t1+3,log['start_qpc_s']+total-1)),
              "after": after, "detail": log.get('detail', 0), "requested_detail": args.detail,
              "fixture_matches_request": log.get('detail', 0) == args.detail,
              "valid_comparison": valid,
              "user_input_during_run": user_input,
              "limitations": "Synthetic only. Drift >5%, missing controls or fixture mismatch invalidate comparisons. Present-call times are not displayed-frame times.",
              "aba_drop_pct": round(100 * (1 - during['fps'] / control_fps), 2) if control_fps and during else None,
              "control_drift_pct": round(100 * (after['fps'] / base['fps'] - 1), 2) if base and after else None,
              "fps_drop_pct": round(100 * (1 - during["fps"] / base["fps"]), 1) if base and during else None,
              "usage": usage, "clip": clip, "duplication_probe": probe_report}
    json.dump(report, open(os.path.join(out, "report.json"), "w"), indent=1)
    return report


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--configs", default="none,qsv")
    ap.add_argument("--cli", default=CLI, help="Recorder binary to compare; fixture stays identical")
    ap.add_argument("--counters", action="store_true", help="Separate profiling run: counter collection only during B may affect FPS")
    ap.add_argument("--telemetry", action="store_true", help="Read NVIDIA clocks/power consistently throughout A/B/A")
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
    ap.add_argument("--post", type=int, default=20, help="Capture-off A2 window after saves finish")
    ap.add_argument("--detail", type=int, choices=[0, 1], default=1)
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
                      f"freeze {k.get('longest_freeze_ms')} ms jumps {k.get('jumps_over_1_5x_pct')}%; "
                      f"presentation {rep['presentation_modes_percent']}; valid {rep['valid_comparison']}"
                      f"{' (user input)' if rep['user_input_during_run'] else ''}", flush=True)
                time.sleep(3)
    json.dump(results, open(os.path.join(root, "test-output", "overhead", args.tag, "all.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
