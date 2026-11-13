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
WGC_PROBE = os.path.join(rel, "replayx-wgc-profile.exe")
_lock = json.load(open(os.path.join(root, "tools", "ffmpeg-lock.json"), encoding="utf-8-sig"))
FFMPEG = os.path.join(root, "tools", _lock["runtime_dir"], "ffmpeg.exe")
import random

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
    # Software fallback (no hardware encoder), as used when none is available.
    "openh264": (["encoder=libopenh264"], ""),
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
    "dda-1copy": ("probe", ["--hold", "0", "--copies", "1"]),
    # Windows Graphics Capture alone (replayx-wgc-profile: one GPU copy of the
    # newest frame per 60 Hz slot, Windows limited to 60 updates/s, border on).
    "wgc-monitor": ("wgc", ["--mode", "monitor"]),
    "wgc-window": ("wgc", ["--mode", "window"]),
    "wgc-monitor-unlimited": ("wgc", ["--mode", "monitor", "--limit", "0"]),
    "wgc-monitor-noborder": ("wgc", ["--mode", "monitor", "--border", "0"]),
    # WGC through REPLAYX's real pipeline (FFmpeg gfxcapture -> same filters
    # and NVENC settings as the production Desktop Duplication path).
    "wgc-h264": (["codec=h264"], "capture_api=wgc"),
    "wgc-window-h264": (["codec=h264"], "capture_api=wgc,wgc_window=replayx-gameproxy.exe"),
    # WGC's minimum update interval is a floor between frames: content at a
    # rate that is not a multiple of 60 arrives below 60/s and the encoder
    # repeats frames. Capture at twice the rate, keep the frame nearest each slot.
    "wgc-h264-os2": (["codec=h264"], "capture_api=wgc,oversample=2"),
    "wgc-h264-1x": (["codec=h264"], "capture_api=wgc,wgc_oversample=1"),
    # The user-facing setting (not the benchmark switch), and its fallback:
    # a WGC window that does not exist must fall back to Desktop Duplication.
    "wgc-setting-h264": (["codec=h264", "capture_api=wgc"], ""),
    # NVENC selected on an Intel-driven display (the November 12 Fortnite
    # test): the three paths, all through the user-facing setting.
    "dda-nvenc": (["codec=h264", "encoder=h264_nvenc"], ""),
    "wgc-nvenc-hybrid": (["codec=h264", "encoder=h264_nvenc", "capture_api=wgc"], "wgc_gpu=0"),
    "wgc-nvenc-gpu": (["codec=h264", "encoder=h264_nvenc", "capture_api=wgc"], "wgc_gpu=1"),
    "wgc-qsv": (["codec=h264", "encoder=auto", "capture_api=wgc"], ""),
    "wgc-qsv-prio0": (["codec=h264", "encoder=auto", "capture_api=wgc"], "gpu_priority=0"),
    "wgc-qsv-1x": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_oversample=1"),
    "wgc-qsv-1x16": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_oversample=1,wgc_floor_us=16000"),
    "wgc-qsv-2x": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_oversample=2"),
    "wgc-qsv-scale": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_scale=1"),
    "wgc-nvenc-scale": (["codec=h264", "encoder=h264_nvenc", "capture_api=wgc"], "wgc_scale=1"),
    "wgc-qsv-f125": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_floor_us=12500"),
    "wgc-qsv-scale-f125": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_scale=1,wgc_floor_us=12500"),
    "wgc-qsv-scale-2x": (["codec=h264", "encoder=auto", "capture_api=wgc"], "wgc_scale=1,wgc_oversample=2"),
    # The WGC + Quick Sync chain one stage at a time, raw FFmpeg into a null
    # output (nothing saved): which stage costs the game frames?
    "ff-wgc120": ("ffmpeg", "gfxcapture=monitor_idx=0:max_framerate=120:display_border=1", "fps=60"),
    "ff-wgc120-map": ("ffmpeg", "gfxcapture=monitor_idx=0:max_framerate=120:display_border=1",
                      "fps=60,hwmap=derive_device=qsv,format=qsv"),
    "ff-wgc120-vpp": ("ffmpeg", "gfxcapture=monitor_idx=0:max_framerate=120:display_border=1",
                      "fps=60,hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12"),
    "ff-wgc120-qsv": ("ffmpeg", "gfxcapture=monitor_idx=0:max_framerate=120:display_border=1",
                      "fps=60,hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12", "h264_qsv"),
    "ff-wgc60-qsv": ("ffmpeg", "gfxcapture=monitor_idx=0:max_framerate=60:display_border=1",
                     "fps=60,hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12", "h264_qsv"),
    "wgc-fallback-h264": (["codec=h264", "capture_api=wgc"], "wgc_window=no-such-game.exe"),
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


def foreground_pid():
    """Process owning the foreground window. Windows may refuse the fixture's
    request to come to the front (foreground lock after recent input); a
    capture would then record whatever else is on screen."""
    import ctypes
    pid = ctypes.c_ulong(0)
    ctypes.windll.user32.GetWindowThreadProcessId(ctypes.windll.user32.GetForegroundWindow(), ctypes.byref(pid))
    return pid.value


def fixture_covers(pid, windowed):
    """The fixture's window is what the display shows at the screen centre
    (and, full screen, near each corner), even when Windows' focus-stealing
    protection keeps another app in the foreground and the fixture is only
    topmost."""
    import ctypes
    from ctypes import wintypes
    user32 = ctypes.windll.user32
    user32.WindowFromPoint.restype = wintypes.HWND
    user32.WindowFromPoint.argtypes = [wintypes.POINT]
    user32.GetAncestor.restype = wintypes.HWND
    user32.GetAncestor.argtypes = [wintypes.HWND, ctypes.c_uint]
    w, h = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    points = [(w // 2, h // 2)] if windowed else [(w // 2, h // 2), (8, 8), (w - 9, 8), (8, h - 9), (w - 9, h - 9)]
    for x, y in points:
        hwnd = user32.GetAncestor(user32.WindowFromPoint(wintypes.POINT(x, y)), 2)  # GA_ROOT
        owner = ctypes.c_ulong(0)
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value != pid:
            return False
    return True


def require_fixture_foreground(proxy, windowed=False):
    if foreground_pid() != proxy.pid and not fixture_covers(proxy.pid, windowed):
        raise RuntimeError('The game fixture is not the foreground window; refusing to capture the desktop. '
                           'Leave the computer idle for a few minutes and run again.')


# DXGI_FRAME_PRESENTATION_MODE values reported by IDXGISwapChainMedia.
PRESENTATION_MODES = {0: 'composed', 1: 'overlay', 2: 'direct', 3: 'composition_failure'}


def gpu_state_window(telemetry, lo, hi):
    """Mean NVIDIA graphics clock in a window (None without telemetry)."""
    rows = [v['values'][0] for v in telemetry if lo < v['t'] < hi]
    return statistics.mean(rows) if rows else None


def stats(times):
    ft = [(b - a) * 1000 for a, b in zip(times, times[1:])]
    if len(ft) < 20:
        return None
    s = sorted(ft)
    median = s[len(s) // 2]  # once: per-frame recomputation was quadratic
    worst1 = s[-max(1, len(s) // 100):]
    worst01 = s[-max(1, len(s) // 1000):]
    return {
        "fps": round(len(ft) / ((times[-1] - times[0])), 1),
        "low1_fps": round(1000 / statistics.mean(worst1), 1),
        "low01_fps": round(1000 / statistics.mean(worst01), 1),
        "frametime_p99_ms": round(s[int(0.99 * (len(s) - 1))], 2),
        "frametime_p95_ms": round(s[int(0.95 * (len(s) - 1))], 3),
        "frametime_stdev_ms": round(statistics.pstdev(ft), 3),
        "spikes_over_2x_median": sum(1 for f in ft if f > 2 * median),
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
    # Effective CPU clock (% of base) once a second: on laptops the iGPU and
    # the CPU cores share one power budget, so GPU work can lower CPU clocks.
    cpu_perf = []
    perf_process = subprocess.Popen(["typeperf", r"\Processor Information(_Total)\% Processor Performance",
                                     "-si", "1", "-sc", str(total + 5)], stdout=subprocess.PIPE,
                                    stderr=subprocess.DEVNULL, text=True)
    def collect_perf():
        for line in perf_process.stdout:
            parts = line.strip().strip('"').split('","')
            try:
                cpu_perf.append((time.perf_counter(), float(parts[1])))
            except (IndexError, ValueError):
                pass
    perf_thread = threading.Thread(target=collect_perf, daemon=True)
    perf_thread.start()
    proxy_log = os.path.join(out, "proxy.json")
    proxy = subprocess.Popen([PROXY, "--seconds", str(total), "--log", proxy_log, "--iters", str(args.iters),
                              "--cpu-us", str(args.cpu_us), "--detail", str(args.detail),
                              "--windowed", str(args.windowed), "--topmost", str(args.topmost),
                              "--waitable", str(args.waitable),
                              "--cpu-work", str(args.cpu_work), "--workers", str(args.workers),
                              "--buffers", str(args.buffers), "--mem-reads", str(args.mem_reads),
                              "--mem-mb", str(args.mem_mb), "--mem-workers", str(args.mem_workers)])
    time.sleep(1.5)
    # The fixture opens its own full-screen window. Never manipulate unrelated
    # windows or focus through a second UI automation implementation.
    time.sleep(args.warm - 1.5)
    gpu = subprocess.Popen(["powershell", "-NoProfile", "-Command", GPU_PS.format(n=args.capture - 6)],
                           stdout=subprocess.PIPE, text=True) if cfg and args.counters else None
    # Allow a slow fixture start (large memory buffers) before refusing.
    for attempt in range(20):
        try:
            require_fixture_foreground(proxy, args.windowed)
            break
        except RuntimeError:
            if attempt == 19 or proxy.poll() is not None:
                proxy.kill()
                proxy.wait(timeout=30)
                raise
            time.sleep(.5)
    fixture_focused = foreground_pid() == proxy.pid
    t0 = time.perf_counter()
    probe_report = None
    if cfg and cfg[0] in ("probe", "wgc"):
        out_probe = subprocess.run([PROBE if cfg[0] == "probe" else WGC_PROBE, "--pid", str(proxy.pid), "--seconds", str(args.capture), *cfg[1]],
                                   capture_output=True, text=True, timeout=args.capture + 60)
        try:
            probe_report = json.loads(out_probe.stdout)
        except json.JSONDecodeError:
            probe_report = {"error": (out_probe.stderr or out_probe.stdout)[-400:]}
        if not probe_report.get("success"):
            raise RuntimeError(f"Duplication probe failed: {probe_report.get('error')}")
        time.sleep(4)
    elif cfg and cfg[0] == "ffmpeg":
        source, graph = cfg[1], cfg[2]
        encode = ["-c:v", cfg[3], "-low_power", "1", "-b:v", "16M", "-g", "120"] if len(cfg) > 3 else []
        with open(os.path.join(out, "capture.log"), "w") as capture_log:
            ff = subprocess.Popen([FFMPEG, "-hide_banner", "-nostdin", "-f", "lavfi", "-i", source, "-vf", graph,
                                   *encode, "-t", str(args.capture), "-f", "null", "-"],
                                  stdout=capture_log, stderr=capture_log)
            try:
                while ff.poll() is None:
                    require_fixture_foreground(proxy, args.windowed)
                    time.sleep(.1)
            finally:
                if ff.poll() is None:
                    ff.terminate(); ff.wait(timeout=15)
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
                    require_fixture_foreground(proxy, args.windowed)
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
    perf_process.terminate()
    perf_thread.join(timeout=3)
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
    def perf_window(lo, hi):
        v = [p for t, p in cpu_perf if lo < t < hi]
        return round(statistics.mean(v), 1) if v else None
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
    engine = None
    for f in glob.glob(os.path.join(out, "test-output", "capture-test-*", "report.json")):
        st_ = json.load(open(f, encoding="utf-8")).get("status", {})
        engine = {k: st_.get(k) for k in ("capture_api", "capture_filter", "capture_fallback", "scaling", "scaling_detail",
                                          "encoder", "fps", "dropped_frames", "duplicated_frames", "memory_mb", "cpu_percent")}
    clip = None
    clips = sorted(glob.glob(os.path.join(out, "test-output", "capture-test-*", "clips", "**", "*.mkv"), recursive=True),
                   key=os.path.getsize)
    if clips:
        clip = json.loads(subprocess.check_output([sys.executable, "-I", os.path.join(root, "scripts", "gameproxy-check.py"),
                                                   clips[-1], "--skip-seconds", "6"]))
        clip.pop("advance_histogram_ms", None)
    control_fps = statistics.mean([base['fps'], after['fps']]) if base and after else None
    drift = 100 * (after['fps'] / base['fps'] - 1) if base and after else None
    # Thermal or power throttling during B makes the comparison meaningless.
    clocks = [gpu_state_window(telemetry, lo, hi) for lo, hi in
              ((log['start_qpc_s']+2, t0-.5), (t0+5, t0+args.capture-1))]
    clock_drop = bool(all(clocks) and clocks[1] < 0.9 * clocks[0])
    valid = bool(base and during and after and drift is not None and abs(drift) <= 5 and not clock_drop
                 and log.get('detail', 0) == args.detail and not user_input)
    report = {"label": label, "fps_setting": fps, "adapter": log["adapter"], "windowed": log.get("windowed", False), "waitable": log.get("waitable", False),
              "iters": args.iters, "cpu_us": args.cpu_us, "cpu_work": args.cpu_work, "workers": args.workers, "buffers": args.buffers,
              "mem_reads": args.mem_reads, "mem_mb": args.mem_mb, "mem_workers": args.mem_workers, "baseline": base, "capture": during,
              "fixture_draw_gpu_timing": gpu_draw,
              "cpu_performance_percent": dict(before=perf_window(t0 - args.warm + 2, t0 - .5),
                                              capture=perf_window(t0 + 5, t0 + args.capture - 1),
                                              after=perf_window(t1 + 3, t1 + args.post)),
              "presentation_modes_percent": presentation_modes,
              "counter_sampler_enabled": args.counters,
              "gpu_state": dict(before=gpu_state(log['start_qpc_s']+2, t0-.5),
                                capture=gpu_state(t0+5,t0+args.capture-1), after=gpu_state(t1+3,log['start_qpc_s']+total-1)),
              "after": after, "detail": log.get('detail', 0), "requested_detail": args.detail,
              "fixture_matches_request": log.get('detail', 0) == args.detail,
              "valid_comparison": valid,
              "user_input_during_run": user_input,
              "gpu_clock_drop_during_capture": clock_drop,
              "fixture_focused": fixture_focused, "fixture_topmost": bool(args.topmost),
              "limitations": "Synthetic only. Drift >5%, missing controls or fixture mismatch invalidate comparisons. Present-call times are not displayed-frame times.",
              "aba_drop_pct": round(100 * (1 - during['fps'] / control_fps), 2) if control_fps and during else None,
              "control_drift_pct": round(100 * (after['fps'] / base['fps'] - 1), 2) if base and after else None,
              "fps_drop_pct": round(100 * (1 - during["fps"] / base["fps"]), 1) if base and during else None,
              "usage": usage, "clip": clip, "engine": engine, "duplication_probe": probe_report}
    json.dump(report, open(os.path.join(out, "report.json"), "w"), indent=1)
    return report


def main():
    try:
        import ctypes
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # per-monitor v2
    except (AttributeError, OSError):
        pass
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
    ap.add_argument("--cpu-work", type=int, default=0, help="Fixed CPU work per frame (iterations), not time")
    ap.add_argument("--workers", type=int, default=0, help="Background CPU worker threads in the fixture")
    ap.add_argument("--buffers", type=int, choices=[2, 3, 4], default=3, help="Fixture swapchain buffer count")
    ap.add_argument("--mem-reads", type=int, default=0, help="Dependent random reads per frame (memory-bound work)")
    ap.add_argument("--mem-mb", type=int, default=64)
    ap.add_argument("--mem-workers", type=int, default=0, help="Threads streaming through memory")
    ap.add_argument("--warm", type=int, default=10)
    ap.add_argument("--post", type=int, default=20, help="Capture-off A2 window after saves finish")
    ap.add_argument("--detail", type=int, choices=[0, 1], default=1)
    ap.add_argument("--capture", type=int, default=20)
    ap.add_argument("--tag", default=time.strftime("%H%M%S"))
    ap.add_argument("--shuffle", action="store_true", help="Random config order in each repeat")
    ap.add_argument("--windowed", type=int, choices=[0, 1], default=0, help="Fixture in a 1280x720 window")
    ap.add_argument("--waitable", type=int, choices=[0, 1], default=0,
                    help="Fixture paces frames on a frame-latency waitable swapchain (latency 1)")
    ap.add_argument("--topmost", type=int, choices=[0, 1], default=1,
                    help="Keep the fixture above other windows (Windows may refuse it the foreground)")
    ap.add_argument("--seed", type=int, default=1)
    args = ap.parse_args()
    # Results apply only to this display setup (panel, GPU, refresh rate).
    try:
        displays = json.loads(subprocess.check_output([CLI, "displays"], text=True, timeout=30))
    except (subprocess.SubprocessError, json.JSONDecodeError, OSError):
        displays = None
    results = []
    rng = random.Random(args.seed)
    for r in range(args.repeat):
        for fps in args.fps.split(","):
            labels = args.configs.split(",")
            if args.shuffle:
                rng.shuffle(labels)  # counterbalance order effects (drift, heat)
            for label in labels:
                out = os.path.join(root, "test-output", "overhead", args.tag, f"{label}-{fps}-r{r + 1}")
                rep = run(label, args, int(fps), args.height, args.mode, out)
                rep["displays"] = displays
                json.dump(rep, open(os.path.join(out, "report.json"), "w"), indent=1)
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
