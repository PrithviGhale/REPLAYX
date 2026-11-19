"""One consolidated, noninvasive Fortnite comparison: recorder off / on / off
for several REPLAYX capture configurations, in one run (about 8-10 minutes).

You start Fortnite yourself, stand still (or repeat one identical short
route) in the same Creative scene, and run this from the repository root:

    py -3 scripts/bench/fortnite-session.py

To also record frame times, run it from an *administrator* terminal (your
decision) with PresentMon:

    py -3 scripts/bench/fortnite-session.py --presentmon test-output\\fortnite-live-alpha8\\PresentMon-2.6.0-x64.exe

The script never activates, focuses, injects into or sends input to the
game, and never changes settings. It records through REPLAYX's own engine
(`replayx-cli capture-test`, no microphone, a fresh test-output folder) and
stops at once if Fortnite loses focus or exits, so the desktop is never
recorded. Telemetry: per-process GPU engine use (PDH), CPU effective clock,
per-process CPU, NVIDIA clocks/power, and PresentMon frame times if allowed.
Quit the REPLAYX app first (one capture at a time).
"""
import argparse
import csv
from collections import Counter
import ctypes
from datetime import datetime
from ctypes import wintypes
import importlib.util
import hashlib
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import threading
import time

import psutil
import session_timing

ROOT = Path(__file__).resolve().parents[2]
# Games this script may measure (it never launches, focuses or touches them).
GAMES = {'fortnite': 'fortniteclient-win64-shipping.exe', 'cs2': 'cs2.exe'}
GAME = GAMES['fortnite']
# The recorder build to measure (its own ffmpeg is in media\); --build changes it.
BUILD = ROOT / 'src-tauri' / 'target' / 'release'
CLI = BUILD / 'replayx-cli.exe'
# Recorder configurations (capture-test overrides). 1920x1080 at 60 FPS,
# H.264, Balanced, stretch, 60 s replay; silent (no audio devices opened).
# name -> (capture-test overrides, REPLAYX_VIDEO_TUNING or '').
CONFIGS = {
    # The default WGC path: scaled in gfxcapture, 12.5 ms update floor,
    # 2x only if it misses output slots.
    'wgc-qsv': (['capture_api=wgc', 'encoder=auto'], ''),
    # The November 12 session's B1: unscaled, 16 ms floor.
    'wgc-qsv-prev': (['capture_api=wgc', 'encoder=auto'], 'wgc_scale=0,wgc_floor_us=16000'),
    # The previous WGC test build: always 2x the output rate.
    'wgc-qsv-2x': (['capture_api=wgc', 'encoder=auto'], 'wgc_scale=0,wgc_oversample=2'),
    'wgc-nvenc': (['capture_api=wgc', 'encoder=h264_nvenc'], ''),
    'dda-qsv': (['capture_api=dda', 'encoder=auto'], ''),
    'dda-nvenc': (['capture_api=dda', 'encoder=h264_nvenc'], ''),
    # November 14 candidates, against 'wgc-auto' (today's WGC path with
    # Encoder = Auto, as the owner records):
    'wgc-auto': (['capture_api=wgc', 'encoder=auto'], ''),
    'dda-auto': (['capture_api=dda', 'encoder=auto'], ''),
    # EcoQoS for the capture FFmpeg (efficient cores on hybrid CPUs).
    'wgc-eco': (['capture_api=wgc', 'encoder=auto'], 'capture_eco=1'),
    # Four refreshes of a 240-250 Hz display per capture instead of three.
    'wgc-floor15': (['capture_api=wgc', 'encoder=auto'], 'wgc_floor_us=15000'),
    # Built-in 240 Hz panel (Intel-driven): today's 1x without the switch to 2x,
    # a floor just under three refreshes, and NVENC instead of Quick Sync.
    'wgc-1x': (['capture_api=wgc', 'encoder=auto'], 'wgc_adaptive=0'),
    'wgc-floor12': (['capture_api=wgc', 'encoder=auto'], 'wgc_floor_us=12000'),
    'wgc-nvenc-auto': (['capture_api=wgc', 'encoder=h264_nvenc'], ''),
    # Surplus WGC frames dropped before Intel's conversion (VPP), alone and
    # with VPP on the fixed-function scaler (SFC) instead of shader cores.
    'wgc-early': (['capture_api=wgc', 'encoder=auto'], 'wgc_early_drop=1'),
    'wgc-early-sfc': (['capture_api=wgc', 'encoder=auto'], 'wgc_early_drop=1,vpp_scale_mode=1'),
    # VPP on the fixed-function scaler alone: frame selection unchanged.
    'wgc-sfc': (['capture_api=wgc', 'encoder=auto'], 'vpp_scale_mode=1'),
    # Windows Graphics Capture alone at REPLAYX's capture rate (80/s): one GPU
    # copy per frame, no FFmpeg, scaling or encoder (replayx-wgc-profile).
    # What Windows' side of capture costs the game; the rest is REPLAYX's.
    'wgc-capture-only': ('probe', ['--mode', 'monitor', '--fps', '80', '--border', '0']),
    # Native capture-to-encode stages (replayx-wgc-profile --stage), at the
    # recorder's capture interval (12.5 ms) with one chosen frame per 60 FPS
    # slot; no encoder. none = Windows' capture alone; convert = one
    # fixed-function scale + NV12 pass; convert-xfer = plus the NV12 frame
    # to the NVIDIA GPU through system memory.
    'native-none': ('probe', ['--mode', 'monitor', '--fps', '60', '--interval-us', '12500', '--border', '0', '--stage', 'none']),
    'native-convert': ('probe', ['--mode', 'monitor', '--fps', '60', '--interval-us', '12500', '--border', '0', '--stage', 'convert']),
    'native-convert-xfer': ('probe', ['--mode', 'monitor', '--fps', '60', '--interval-us', '12500', '--border', '0', '--stage', 'convert-xfer']),
    # The same with window capture of the game instead of the display.
    'native-none-window': ('probe', ['--mode', 'window', '--fps', '60', '--interval-us', '12500', '--border', '0', '--stage', 'none']),
    'native-convert-window': ('probe', ['--mode', 'window', '--fps', '60', '--interval-us', '12500', '--border', '0', '--stage', 'convert']),
    # The native recording prototype (replayx-native-proto --record): WGC at
    # the recorder's 12.5 ms interval, today's frame selection, one
    # fixed-function conversion per chosen frame, Quick Sync H.264 from the
    # D3D11 texture; real 1080p60 encoding, no audio (as 'silent' above).
    'native-record': ('native', ['--mode', 'monitor', '--interval-us', '12500']),
    # Corrected full shared Engine path, including segmentation/MP4/replay mux.
    'app-native-nvenc': (['engine=native', 'encoder=h264_nvenc'], ''),
    # Matched 1080p60 H.264 15 Mbps; avoid comparing FFmpeg's smooth-scale
    # source-size canvas against Native's exact 1920x1080 canvas.
    'app-ffmpeg-nvenc-fixed': (['engine=ffmpeg', 'capture_api=wgc', 'encoder=h264_nvenc', 'scale_policy=exact', 'bitrate=15'], ''),
    'app-native-nvenc-phase': (['engine=native', 'encoder=h264_nvenc', 'scale_policy=exact', 'bitrate=15'], ''),
}
CONFIGS.update({
    'app-ffmpeg-matched-steady': (['engine=ffmpeg', 'capture_api=wgc', 'encoder=auto', 'scale_policy=exact', 'bitrate=15'], 'wgc_oversample=2'),
    'app-native-matched-phase': (['engine=native', 'encoder=auto', 'scale_policy=exact', 'bitrate=15'], ''),
})
CONFIGS['app-ffmpeg-matched-crosshair'] = (
    [*CONFIGS['app-ffmpeg-matched-steady'][0], 'benchmark_crosshair=on'],
    CONFIGS['app-ffmpeg-matched-steady'][1],
)
COMMON = ['silent', 'fps=60', 'width=1920', 'height=1080', 'codec=h264', 'preset=balanced',
          'mode=stretch', 'replay=60', 'buffer=256']

spec = importlib.util.spec_from_file_location('frametimes', ROOT / 'scripts' / 'bench' / 'compare-frametimes.py')
frametimes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frametimes)


def find_game(name):
    for p in psutil.process_iter(['name']):
        if (p.info['name'] or '').lower() == name:
            return p
    return None


def present_modes(csv_file, process):
    """Share of frames per PresentMon PresentMode for the game."""
    counts = Counter()
    with open(csv_file, newline='', encoding='utf-8-sig') as f:
        reader = csv.DictReader(f)
        heads = {h.strip().lower(): h for h in reader.fieldnames or []}
        app, mode = heads.get('application'), heads.get('presentmode')
        if not mode:
            return None
        for row in reader:
            if not app or (row.get(app) or '').lower() == process.lower():
                counts[row.get(mode) or 'unknown'] += 1
    total = sum(counts.values()) or 1
    return {k: round(100 * v / total, 1) for k, v in counts.most_common()}


user32 = ctypes.WinDLL('user32', use_last_error=True)
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.IsIconic.argtypes = [wintypes.HWND]


def game_in_front(pid, allow_cover=False):
    window = user32.GetForegroundWindow()
    owner = wintypes.DWORD()
    user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
    if owner.value == pid and not user32.IsIconic(window):
        return True
    if not allow_cover:
        return False
    # Test fixture only: topmost and covering the screen centre.
    user32.WindowFromPoint.restype = wintypes.HWND
    user32.WindowFromPoint.argtypes = [wintypes.POINT]
    user32.GetAncestor.restype = wintypes.HWND
    user32.GetAncestor.argtypes = [wintypes.HWND, ctypes.c_uint]
    w, h = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    root = user32.GetAncestor(user32.WindowFromPoint(wintypes.POINT(w // 2, h // 2)), 2)
    user32.GetWindowThreadProcessId(root, ctypes.byref(owner))
    return owner.value == pid


def retain_native_report(commands, folder):
    """Keep the worker's final report when its session directory is cleaned."""
    destination = folder / 'native-report.json'
    for command in commands:
        if '--origin-file' not in command:
            continue
        origin_index = command.index('--origin-file') + 1
        if origin_index >= len(command):
            continue
        report = Path(command[origin_index]).with_name('native.json').resolve()
        try:
            report.relative_to(folder.resolve())
        except ValueError:
            continue
        if not report.is_file():
            continue
        try:
            os.link(report, destination)
        except FileExistsError:
            pass
        return


def settle_native_report(commands, folder, refresh, wait_until_retry):
    """Retry once before measurement when a slower worker missed the first scan."""
    retain_native_report(commands.values(), folder)
    if not (folder / 'native-report.json').exists():
        wait_until_retry()
        refresh()
        retain_native_report(commands.values(), folder)


def validate_capture_continuity(status):
    """A restarted recording cannot qualify a fixed-path performance phase."""
    if status.get('recovery_sessions', 0):
        raise RuntimeError('Capture restarted during the phase; footage and partial results are retained, but this comparison is invalid')


def validate_native_report(report, expected_interval):
    """Fail the benchmark if the requested path did not finish intact."""
    if (report.get('success') is not True
            or not isinstance(report.get('frames_out'), int)
            or report['frames_out'] <= 0
            or report['frames_out'] != report.get('encoded')
            or report.get('live_input_samples') != 0
            or report.get('output_timestamp_contract_valid') is not True):
        raise RuntimeError('Native completion integrity check failed')
    if expected_interval is not None and (
            report.get('min_update_interval_us') != expected_interval
            or report.get('min_update_interval_accepted') is not True):
        raise RuntimeError('The requested native capture interval was not exercised')


def main():
    global CLI, BUILD
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--configs', default='wgc-qsv,wgc-qsv-prev,wgc-qsv-2x',
                    help='Comma-separated recorder configurations: ' + ', '.join(CONFIGS))
    ap.add_argument('--seconds', type=int, default=45, help='Measured seconds per phase (20-90)')
    ap.add_argument('--presentmon', type=Path, help='PresentMon console exe (needs an administrator terminal)')
    ap.add_argument('--cap', type=int, default=360, help='Your in-game frame cap, to flag capped phases')
    ap.add_argument('--game', choices=sorted(GAMES), default='fortnite', help='The game you started (default fortnite)')
    ap.add_argument('--build', type=Path, default=BUILD, help='Recorder build folder (default src-tauri/target/release)')
    ap.add_argument('--process', default=None, help=argparse.SUPPRESS)  # test fixture dry runs only
    ap.add_argument('--countdown', type=int, default=15, help=argparse.SUPPRESS)
    ap.add_argument('--focus-wait', type=int, default=0, help='Bounded pre-start foreground wait, before the scheduled countdown')
    ap.add_argument('--matched-encoder', choices=['h264_nvenc', 'h264_qsv', 'h264_amf'], help='Exact capture-adapter encoder selected by the comparison preflight')
    ap.add_argument('--warmup', type=int, default=0, help='Seconds of play, recorder off and not measured, before the first phase, so a laptop CPU leaves its short turbo window (PL2) first (e.g. 120)')
    ap.add_argument('--plan-only', action='store_true', help='Print phases and timing without starting processes or capture')
    ap.add_argument('--out', type=Path, help='Fresh results directory inside replayx-perf/test-output')
    ap.add_argument('--audio', choices=['silent', 'system'], default='silent', help='Never opens a microphone')
    ap.add_argument('--native-interval-us', type=int, choices=[0, 6666], help='Explicit native capture experiment, 0 for baseline or 6666 us for reduced capture work')
    a = ap.parse_args()
    BUILD = a.build.resolve()
    CLI = BUILD / 'replayx-cli.exe'
    game_exe = GAMES[a.game]
    process = (a.process or game_exe).lower()
    fixture = process != game_exe
    if fixture and process != 'replayx-gameproxy.exe':
        ap.error('Only the chosen game (or REPLAYX\'s own test fixture) can be measured')
    configs = [c.strip() for c in a.configs.split(',') if c.strip()]
    if any(c not in CONFIGS for c in configs) or not 20 <= a.seconds <= 90:
        ap.error('Unknown configuration or seconds out of range')
    if not 0 <= a.focus_wait <= 120:
        ap.error('Foreground wait must be 0..120 seconds')
    if a.warmup < 0 or a.countdown < 0:
        ap.error('Warm-up and countdown must be nonnegative')
    plan = session_timing.phase_plan(configs)
    estimate = session_timing.estimate(configs, a.seconds, a.warmup, a.countdown)
    print(f"Expected total: {session_timing.clock(estimate['expected_total_seconds'])}; {len(plan)} phases, "
          f"including {a.warmup} s warm-up, countdown, settling, cleanup and analysis reservations.", flush=True)
    print('Fixed deadlines apply. A phase that exceeds its budget fails with a partial summary. DONE confirms completion.', flush=True)
    if a.plan_only:
        print(json.dumps(dict(phases=plan, timing=estimate), indent=2))
        return
    if gate_value := os.environ.get('REPLAYX_BENCH_START_GATE'):
        gate = Path(gate_value).resolve()
        gate.relative_to((ROOT / 'test-output').resolve())
        until = time.monotonic() + 3
        while not gate.exists():
            if time.monotonic() >= until:
                raise SystemExit('Parent did not attach the owned-process cleanup job')
            time.sleep(.01)
    # Match the runtime used by the independent clip audit.
    lock = json.loads((ROOT / 'tools' / 'ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
    pinned_media = ROOT / 'tools' / lock['runtime_dir']
    build_media = BUILD / 'media'
    media_dir = pinned_media if a.matched_encoder or not (build_media / 'ffmpeg.exe').exists() else build_media
    needed_paths = [CLI, media_dir / 'ffmpeg.exe', media_dir / 'ffprobe.exe']
    if any(CONFIGS[c][0] == 'native' or c.startswith('app-native-') for c in configs):
        needed_paths.append(BUILD / 'replayx-native-proto.exe')
    if any(CONFIGS[c][0] == 'probe' for c in configs):
        needed_paths.append(BUILD / 'replayx-wgc-profile.exe')
    executable_builds = []
    for needed in needed_paths:
        if not needed.exists():
            raise SystemExit(f'Missing {needed}: build the release tools first (or pass --build).')
        executable_builds.append(dict(path=str(needed), bytes=needed.stat().st_size,
                                      modified_unix=needed.stat().st_mtime,
                                      sha256=hashlib.sha256(needed.read_bytes()).hexdigest()))
    game = find_game(process)
    if not game:
        raise SystemExit(f'Start {a.game} first (this script never launches it).')
    commit = subprocess.run(['git', 'log', '-1', '--format=%h %s'], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    print(f'Recorder: {CLI} (built {time.ctime(CLI.stat().st_mtime)}; source {commit})', flush=True)
    for build in executable_builds:
        print(f"Build: {build['path']} SHA-256 {build['sha256']}", flush=True)
    if any((p.info['name'] or '').lower() in ('replayx.exe', 'replayx-cli.exe', 'ffmpeg.exe')
           for p in psutil.process_iter(['name'])):
        raise SystemExit('Quit REPLAYX (tray icon -> Quit) first: one capture at a time.')
    presentmon = None
    if a.presentmon:
        if not ctypes.windll.shell32.IsUserAnAdmin():
            raise SystemExit('PresentMon needs an administrator terminal; run without --presentmon otherwise.')
        presentmon = a.presentmon.resolve()
    out = (a.out or ROOT / 'test-output' / time.strftime(f'{a.game}-session-%Y%m%d-%H%M%S')).resolve()
    out.relative_to((ROOT / 'test-output').resolve())
    out.mkdir(parents=True, exist_ok=False)
    print(f'Switch to {a.game} now and stay in the same scene. Starting in {a.countdown} s. Results: {out}', flush=True)
    if a.focus_wait:
        focus_deadline = time.monotonic() + a.focus_wait
        while not game_in_front(game.pid, fixture):
            if time.monotonic() >= focus_deadline or not game.is_running():
                raise SystemExit('Foreground wait ended; no benchmark recording began.')
            print('Waiting for foreground CS2; scheduled countdown has not started.', flush=True)
            time.sleep(1)
    session_started = time.monotonic()
    print('Scheduled completion:', time.strftime('%H:%M:%S', time.localtime(time.time() + estimate['expected_total_seconds'])), flush=True)
    time.sleep(a.countdown)
    if not game_in_front(game.pid, fixture):
        raise SystemExit(f'{a.game} is not in front; nothing was recorded.')

    samples, boundaries, children = [], [], []
    file_handles = []
    stop = threading.Event()
    # Effective CPU clock and per-process GPU engines, once a second, all phases.
    perf = subprocess.Popen(['typeperf', r'\Processor Information(_Total)\% Processor Performance', '-si', '1'],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                            creationflags=subprocess.CREATE_NO_WINDOW)
    engines_log = (out / 'gpu-engines.jsonl').open('w', encoding='utf-8')
    engines = subprocess.Popen(['powershell', '-NoProfile', '-Command',
        "while ($true) { Get-Counter -Counter '\\GPU Engine(*)\\Utilization Percentage' -SampleInterval 1 -MaxSamples 2 "
        "-ErrorAction SilentlyContinue | ForEach-Object { $o=@{'unix'=[DateTimeOffset]::new($_.Timestamp).ToUnixTimeMilliseconds()/1000; "
        "'e'=@($_.CounterSamples | Where-Object CookedValue -GT 0.05 | Select-Object InstanceName,CookedValue)}; "
        "[Console]::Out.WriteLine(($o | ConvertTo-Json -Compress -Depth 4)); [Console]::Out.Flush() } }"],
        stdout=engines_log, stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
    nvsmi = subprocess.Popen([r'C:\Windows\System32\nvidia-smi.exe', '--query-gpu=timestamp,utilization.gpu,clocks.gr,power.draw,temperature.gpu',
                              '--format=csv,noheader,nounits', '-lms', '1000'], stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
    children += [perf, engines, nvsmi]
    cpu_perf, nvidia = [], []

    def read_perf():
        for line in perf.stdout:
            parts = line.strip().strip('"').split('","')
            try:
                cpu_perf.append((time.time(), float(parts[1])))
            except (IndexError, ValueError):
                pass

    def read_nvidia():
        for line in nvsmi.stdout:
            try:
                fields = line.split(',')
                # The driver timestamps its sample. stdout may arrive buffered.
                measured = datetime.strptime(fields[0].strip(), '%Y/%m/%d %H:%M:%S.%f').timestamp()
                values = [float(v) for v in fields[1:]]
                if len(values) == 4 and all(math.isfinite(v) for v in values):
                    nvidia.append((measured, values))
            except ValueError:
                pass
    threading.Thread(target=read_perf, daemon=True).start()
    threading.Thread(target=read_nvidia, daemon=True).start()
    procs = {}
    names = {}
    commands = {}
    def refresh_processes():
        # Once per phase after startup, never enumerate every measured second.
        for p in psutil.process_iter(['name']):
            name = (p.info['name'] or '').lower()
            if name in (process, 'replayx-cli.exe', 'ffmpeg.exe', 'dwm.exe',
                        'replayx-wgc-profile.exe', 'replayx-native-proto.exe'):
                procs.setdefault(p.pid, p)
                names[str(p.pid)] = name
                if name == 'replayx-native-proto.exe' and p.pid not in commands:
                    try:
                        commands[p.pid] = p.cmdline()[:100]
                    except psutil.Error:
                        pass


    def observe(seconds, phase):
        until = time.monotonic() + seconds
        next_progress = time.monotonic() + 10
        while time.monotonic() < until:
            if time.monotonic() >= next_progress:
                print(f'{phase}: {max(0, math.ceil(until - time.monotonic()))} s remaining; '
                      f'session elapsed {session_timing.clock(time.monotonic() - session_started)}', flush=True)
                next_progress = time.monotonic() + 10
            if not game.is_running() or not game_in_front(game.pid, fixture):
                raise RuntimeError('The game lost focus or exited: stopped (nothing further recorded)')
            row = dict(unix=time.time(), phase=phase, cpu={}, rss_mib={})
            for pid, p in list(procs.items()):
                name = names[str(pid)]
                try:
                    row['cpu'][name] = row['cpu'].get(name, 0) + p.cpu_percent() / psutil.cpu_count()
                    row['rss_mib'][name] = row['rss_mib'].get(name, 0) + p.memory_info().rss / 1048576
                except psutil.Error:
                    procs.pop(pid, None)
            samples.append(row)
            stop.wait(min(1, max(0, until - time.monotonic())))

    errors = []
    try:
        setup_deadline = session_started + a.countdown + session_timing.SETUP_SECONDS
        if time.monotonic() > setup_deadline:
            raise RuntimeError('Telemetry setup exceeded the scheduled five seconds')
        refresh_processes()
        observe(max(0, setup_deadline-time.monotonic()), 'setup')
        if a.warmup > 0:
            print(f'Warm-up: {a.warmup} s, recorder off, not measured', flush=True)
            observe(a.warmup, 'warmup')
        scheduled_phase_start = session_started + a.countdown + session_timing.SETUP_SECONDS + a.warmup
        for phase_number, (phase, config) in enumerate(plan, 1):
            phase_start = scheduled_phase_start
            phase_end = phase_start + session_timing.phase_seconds(config, a.seconds)
            scheduled_phase_start = phase_end
            print(f'Phase {phase_number}/{len(plan)}: {phase}; '
                  f'{len(plan)-phase_number} phases remain; elapsed {session_timing.clock(time.monotonic() - session_started)}', flush=True)
            recorder = None
            commands.clear()
            if config:
                folder = out / phase
                folder.mkdir()
                log = (folder / 'capture.log').open('w', encoding='utf-8')
                file_handles.append(log)
                env = {k: v for k, v in os.environ.items() if k != 'REPLAYX_VIDEO_TUNING' and not k.startswith('REPLAYX_NATIVE_') and k != 'REPLAYX_ALPHA_SYNTHETIC'}
                env['REPLAYX_FFMPEG_DIR'] = str(media_dir)
                if config in ['app-native-nvenc-phase', 'app-native-matched-phase']:
                    env['REPLAYX_NATIVE_TIMESTAMP_TEST'] = 'phase-lock'
                    env['REPLAYX_NATIVE_PHASE_SOURCE'] = 'spatial'
                overrides, tuning = CONFIGS[config]
                if overrides == 'probe':
                    probe_out = (folder / 'probe.json').open('w', encoding='utf-8')
                    file_handles.append(probe_out)
                    recorder = subprocess.Popen([str(BUILD / 'replayx-wgc-profile.exe'), '--pid', str(game.pid),
                                                 '--seconds', str(a.seconds + 12), *tuning],
                                                cwd=folder, stdout=probe_out, stderr=log,
                                                creationflags=subprocess.CREATE_NO_WINDOW)
                elif overrides == 'native':
                    probe_out = (folder / 'probe.json').open('w', encoding='utf-8')
                    file_handles.append(probe_out)
                    recorder = subprocess.Popen([str(BUILD / 'replayx-native-proto.exe'), '--record',
                                                 '--pid', str(game.pid), '--seconds', str(a.seconds + 12),
                                                 '--out', str(folder / 'native.h264'), *tuning],
                                                cwd=folder, stdout=probe_out, stderr=log,
                                                creationflags=subprocess.CREATE_NO_WINDOW)
                else:
                    if tuning:
                        env['REPLAYX_VIDEO_TUNING'] = tuning
                    capture_overrides = list(overrides)
                    if a.matched_encoder:
                        capture_overrides.append(f'encoder={a.matched_encoder}')
                    if config.startswith('app-native-') and a.native_interval_us is not None:
                        capture_overrides.append(f'native_interval_us={a.native_interval_us}')
                    recorder = subprocess.Popen([str(CLI), 'capture-test', str(a.seconds + 14), a.audio, *COMMON[1:], *capture_overrides, 'wall_clock=1', 'wait_for_replay=1'],
                                                cwd=folder, env=env, stdout=log, stderr=log,
                                                creationflags=subprocess.CREATE_NO_WINDOW)
                children.append(recorder)
            print(f'{phase}: settling 10 s', flush=True)
            if time.monotonic() > phase_start + 10:
                raise RuntimeError(f'{phase}: startup exceeded its settling budget')
            observe(max(0, phase_start+3-time.monotonic()), phase + '-startup')
            refresh_processes()
            if config and config.startswith('app-native-'):
                settle_native_report(commands, folder, refresh_processes,
                                     lambda: observe(max(0, phase_start+8-time.monotonic()), phase + '-settle'))
            observe(max(0, phase_start+10-time.monotonic()), phase + '-settle')
            if recorder and recorder.poll() is not None:
                raise RuntimeError(f'{phase}: recorder stopped early (see {phase}/capture.log)')
            pm = None
            if presentmon:
                pm_log = (out / f'{phase}-presentmon.log').open('w', encoding='utf-8')
                file_handles.append(pm_log)
                pm = subprocess.Popen([str(presentmon), '--process_id', str(game.pid),
                                       '--session_name', f'REPLAYX-{os.getpid()}-{time.monotonic_ns()}',
                                       '--output_file', str(out / f'{phase}.csv'), '--timed', str(a.seconds),
                                       '--terminate_after_timed', '--no_console_stats'],
                                      stdout=pm_log, stderr=pm_log,
                                      creationflags=subprocess.CREATE_NO_WINDOW)
                children.append(pm)
            boundaries.append(dict(phase=phase, config=config, start=time.time()))
            print(f'{phase}: measuring {a.seconds} s', flush=True)
            observe(max(0, phase_start+10+a.seconds-time.monotonic()), phase)
            boundaries[-1]['end'] = time.time()
            boundaries[-1]['native_worker_commands'] = list(commands.values())
            print(f'{phase}: cleanup within scheduled phase deadline', flush=True)
            pending = [p for p in (pm, recorder) if p is not None]
            while any(p.poll() is None for p in pending) and time.monotonic() < phase_end:
                observe(min(0.5, max(0, phase_end-time.monotonic())), phase + '-save')
            if any(p.poll() is None for p in pending):
                if recorder:
                    boundaries[-1]['recorder_exit_code'] = -1
                raise RuntimeError(f'{phase}: process cleanup exceeded its fixed budget')
            if pm and pm.returncode != 0:
                raise RuntimeError(f'{phase}: PresentMon failed with code {pm.returncode}; see {phase}-presentmon.log')
            if recorder:
                boundaries[-1]['recorder_exit_code'] = recorder.returncode
                if recorder.returncode != 0:
                    raise RuntimeError(f'{phase}: recorder exited with code {recorder.returncode}; inspect capture.log')
                probe_file = out / phase / 'probe.json'
                if probe_file.exists():
                    try:
                        probe = json.loads(probe_file.read_text(encoding='utf-8'))
                        boundaries[-1]['recorder'] = {k: v for k, v in probe.items()
                                                      if not isinstance(v, (list, dict))}
                    except ValueError:
                        boundaries[-1]['recorder'] = {'probe_error': probe_file.read_text(encoding='utf-8')[-400:]}
                reports = list((out / phase).glob('test-output/capture-test-*/report.json'))
                if reports:
                    capture_report = json.loads(reports[0].read_text(encoding='utf-8'))
                    if capture_report.get('wall_clock_budget') is not True:
                        raise RuntimeError(f'{phase}: CLI did not confirm the requested wall-clock capture budget; rebuild the CLI')
                    boundaries[-1]['capture_timing'] = {key: capture_report.get(key) for key in (
                        'requested_seconds', 'capture_end_ms', 'first_ready_ms', 'second_ready_ms')}
                    boundaries[-1]['crosshair'] = capture_report.get('crosshair')
                    status = capture_report.get('status', {})
                    boundaries[-1]['recorder'] = {k: status.get(k) for k in (
                        'recording_engine', 'engine_fallback', 'capture_api', 'capture_filter', 'capture_fallback', 'scaling', 'encoder', 'fps',
                        'dropped_frames', 'duplicated_frames', 'encoder_speed', 'video_lag_ms',
                        'recovery_sessions')}
                    validate_capture_continuity(status)
                    if config.startswith('app-native-') and (status.get('recording_engine') != 'native' or status.get('engine_fallback')):
                        raise RuntimeError(f'{phase}: Native fallback occurred; this phase is not a valid Native comparison')
                    if config.startswith('app-native-'):
                        worker_report = folder / 'native-report.json'
                        if not worker_report.is_file():
                            raise RuntimeError(f'{phase}: Native completion report unavailable; retained clips are unqualified')
                        native = json.loads(worker_report.read_text(encoding='utf-8'))
                        boundaries[-1]['native_completion'] = native
                        validate_native_report(native, a.native_interval_us)
                    # recovery_sessions 1 = the adaptive rate switched this phase to 2x.
            if time.monotonic() > phase_end:
                raise RuntimeError(f'{phase}: report processing exceeded the phase deadline')
            observe(max(0, phase_end-time.monotonic()), phase + '-cleanup')
    except (Exception, KeyboardInterrupt) as e:  # report what was measured so far
        errors.append(str(e) or "Cancelled by user")
        print('Stopped:', e, flush=True)
    finally:
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
        for child in reversed(children):
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                errors.append(f'Telemetry or recorder process {child.pid} did not exit after termination')
        engines_log.close()
        for handle in file_handles:
            handle.close()

    def window(rows, b):
        return [v for t, v in rows if b['start'] < t < b.get('end', 0)]
    lines = []
    for line in (out / 'gpu-engines.jsonl').read_text(encoding='utf-8-sig').splitlines():
        if not line.strip():
            continue
        try:
            lines.append(json.loads(line))
        except ValueError:
            errors.append('Incomplete GPU telemetry row was excluded from the summary')
    # Names were cached while workers were alive, including workers now exited.
    for p in procs.values():
        try:
            names[str(p.pid)] = p.name().lower()
        except psutil.Error:
            pass
    summary = []
    for b in boundaries:
        if 'end' not in b:
            continue
        cpu = window(cpu_perf, b)
        nv = window(nvidia, b)
        engine = {}
        rows = [l for l in lines if b['start'] < l['unix'] < b['end']]
        for l in rows:
            for e in l.get('e') or []:
                m = re.match(r'pid_(\d+)_luid_(0x\w+_0x\w+)_phys_(\d+)_eng_(\d+)_engtype_(.*)$', e['InstanceName'])
                if m and names.get(m.group(1)):
                    key = f"{names[m.group(1)]}|{m.group(2)}|phys{m.group(3)}|engine{m.group(4)}|{m.group(5)}"
                    engine[key] = engine.get(key, 0) + e['CookedValue'] / max(1, len(rows))
        proc_cpu, proc_rss = {}, {}
        for s in (s for s in samples if s['phase'] == b['phase']):
            for k, v in s['cpu'].items():
                proc_cpu.setdefault(k, []).append(v)
            for k, v in s['rss_mib'].items():
                proc_rss.setdefault(k, []).append(v)
        entry = dict(phase=b['phase'], config=b['config'], recorder=b.get('recorder'),
                     crosshair=b.get('crosshair'),
                     native_worker_commands=b.get('native_worker_commands', []),
                     native_completion=b.get('native_completion'), capture_timing=b.get('capture_timing'),
                     recorder_exit_code=b.get('recorder_exit_code'),
                     recorder_verified=(b.get('recorder_exit_code') == 0 and bool(b.get('recorder'))
                                        and not b['recorder'].get('recovery_sessions', 0)),
                     cpu_effective_clock_percent=round(statistics.mean(cpu), 1) if cpu else None,
                     process_cpu_percent={k: round(statistics.mean(v[1:] or v), 2) for k, v in proc_cpu.items()},
                     process_rss_mib={k: dict(mean=statistics.mean(v), peak=max(v), growth=v[-1]-v[0]) for k, v in proc_rss.items()},
                     gpu_engines_percent={
                         k: round(v, 3) for k, v in sorted(engine.items(), key=lambda kv: -kv[1])
                         if v >= 0.5 or ('videoencode' in k.lower()
                                         and k.startswith(('ffmpeg.exe|', 'replayx-native-proto.exe|')))
                     },
                     nvidia=dict(zip(['busy_percent', 'graphics_mhz', 'power_w', 'temp_c'],
                                     [round(statistics.mean(c), 1) for c in zip(*nv)])) if nv else None)
        csv_file = out / f"{b['phase']}.csv"
        if csv_file.exists():
            try:
                entry['frames'] = frametimes.summarize(csv_file, process=game.name(), cap=a.cap)
                entry['present_mode_percent'] = present_modes(csv_file, game.name())
            except Exception as e:
                entry['frames'] = {'error': str(e)}
        summary.append(entry)
    comparisons = session_timing.neighboring_comparisons(summary)
    analysis_end = session_started + estimate['expected_total_seconds']
    if not errors:
        if time.monotonic() > analysis_end:
            errors.append('Summary analysis exceeded its scheduled reservation')
        else:
            print('Capture complete. Finalizing results within the scheduled analysis reservation.', flush=True)
            stop.wait(max(0, analysis_end-time.monotonic()))
    result = dict(timing=dict(estimate, elapsed_seconds=round(time.monotonic() - session_started, 3),
                              completed_phases=len(summary), planned_phases=len(plan)), errors=errors, presentmon=bool(presentmon), seconds_per_phase=a.seconds, recorder=str(CLI),
                  audio=a.audio, matched_encoder=a.matched_encoder, native_capture_interval_us=a.native_interval_us, source=commit, executables=executable_builds, process=game.name(), phases=summary,
                  neighboring_comparisons=comparisons, paired_costs=session_timing.paired_costs(comparisons),
                  limits='Without PresentMon, game FPS is not measured: GPU-engine and CPU telemetry only. '
                         'Telemetry is once a second. Scene and camera must be held identical by the player.')
    (out / 'summary.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    session_timing.print_summary(summary)
    print("Paired comparison:", json.dumps(result["paired_costs"]))
    print(f"{'STOPPED' if errors else 'DONE'} after {session_timing.clock(time.monotonic() - session_started)}: "
          f"{len(summary)}/{len(plan)} phases. Saved {out / 'summary.json'}", flush=True)
    raise SystemExit(1 if errors else 0)


if __name__ == '__main__':
    main()
