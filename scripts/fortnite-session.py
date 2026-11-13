"""One consolidated, noninvasive Fortnite comparison: recorder off / on / off
for several REPLAYX capture configurations, in one run (about 8-10 minutes).

You start Fortnite yourself, stand still (or repeat one identical short
route) in the same Creative scene, and run this from the repository root:

    py -3 scripts/fortnite-session.py

To also record frame times, run it from an *administrator* terminal (your
decision) with PresentMon:

    py -3 scripts/fortnite-session.py --presentmon test-output\\fortnite-live-alpha8\\PresentMon-2.6.0-x64.exe

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
from ctypes import wintypes
import importlib.util
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

ROOT = Path(__file__).resolve().parents[1]
GAME = 'fortniteclient-win64-shipping.exe'
# The test build with scaled WGC capture. Its own ffmpeg is in media\.
BUILD = ROOT / 'src-tauri' / 'target-wgc4' / 'release'
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
}
COMMON = ['silent', 'fps=60', 'width=1920', 'height=1080', 'codec=h264', 'preset=balanced',
          'mode=stretch', 'replay=60', 'buffer=256']

spec = importlib.util.spec_from_file_location('frametimes', ROOT / 'scripts' / 'compare-frametimes.py')
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


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--configs', default='wgc-qsv,wgc-qsv-prev,wgc-qsv-2x',
                    help='Comma-separated recorder configurations: ' + ', '.join(CONFIGS))
    ap.add_argument('--seconds', type=int, default=45, help='Measured seconds per phase (30-90)')
    ap.add_argument('--presentmon', type=Path, help='PresentMon console exe (needs an administrator terminal)')
    ap.add_argument('--cap', type=int, default=360, help='Your Fortnite frame cap, to flag capped phases')
    ap.add_argument('--process', default=GAME, help=argparse.SUPPRESS)  # test fixture dry runs only
    ap.add_argument('--countdown', type=int, default=15, help=argparse.SUPPRESS)
    a = ap.parse_args()
    process = a.process.lower()
    fixture = process != GAME
    if fixture and process != 'replayx-gameproxy.exe':
        ap.error('Only Fortnite (or REPLAYX\'s own test fixture) can be measured')
    configs = [c.strip() for c in a.configs.split(',') if c.strip()]
    if any(c not in CONFIGS for c in configs) or not 30 <= a.seconds <= 90:
        ap.error('Unknown configuration or seconds out of range')
    for needed in (CLI, BUILD / 'media' / 'ffmpeg.exe'):
        if not needed.exists():
            raise SystemExit(f'Missing {needed}: the target-wgc4 test build is required.')
    game = find_game(process)
    if not game:
        raise SystemExit('Start Fortnite first (this script never launches it).')
    commit = subprocess.run(['git', 'log', '-1', '--format=%h %s'], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    print(f'Recorder: {CLI} (built {time.ctime(CLI.stat().st_mtime)}; source {commit})', flush=True)
    if any((p.info['name'] or '').lower() in ('replayx.exe', 'replayx-cli.exe', 'ffmpeg.exe')
           for p in psutil.process_iter(['name'])):
        raise SystemExit('Quit REPLAYX (tray icon -> Quit) first: one capture at a time.')
    presentmon = None
    if a.presentmon:
        if not ctypes.windll.shell32.IsUserAnAdmin():
            raise SystemExit('PresentMon needs an administrator terminal; run without --presentmon otherwise.')
        presentmon = a.presentmon.resolve()
    out = ROOT / 'test-output' / time.strftime('fortnite-session-%Y%m%d-%H%M%S')
    out.mkdir(parents=True)
    print(f'Switch to Fortnite now and stay in the same scene. Starting in {a.countdown} s. Results: {out}', flush=True)
    time.sleep(a.countdown)
    if not game_in_front(game.pid, fixture):
        raise SystemExit('Fortnite is not in front; nothing was recorded.')

    samples, boundaries, children = [], [], []
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
    nvsmi = subprocess.Popen([r'C:\Windows\System32\nvidia-smi.exe', '--query-gpu=utilization.gpu,clocks.gr,power.draw,temperature.gpu',
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
                nvidia.append((time.time(), [float(v) for v in line.split(',')]))
            except ValueError:
                pass
    threading.Thread(target=read_perf, daemon=True).start()
    threading.Thread(target=read_nvidia, daemon=True).start()
    procs = {}

    def observe(seconds, phase):
        until = time.time() + seconds
        while time.time() < until:
            if not game.is_running() or not game_in_front(game.pid, fixture):
                raise RuntimeError('Fortnite lost focus or exited: stopped (nothing further recorded)')
            row = dict(unix=time.time(), phase=phase, cpu={})
            for p in psutil.process_iter(['name']):
                name = (p.info['name'] or '').lower()
                if name in (process, 'replayx-cli.exe', 'ffmpeg.exe', 'dwm.exe'):
                    p = procs.setdefault(p.pid, p)
                    try:
                        row['cpu'][name] = row['cpu'].get(name, 0) + p.cpu_percent() / psutil.cpu_count()
                    except psutil.Error:
                        pass
            samples.append(row)
            stop.wait(1)

    plan = []
    for n, config in enumerate(configs):
        plan += [(f'A{n + 1}', None), (f'B{n + 1}-{config}', config)]
    plan.append((f'A{len(configs) + 1}', None))
    errors = []
    try:
        for phase, config in plan:
            recorder = None
            if config:
                folder = out / phase
                folder.mkdir()
                log = (folder / 'capture.log').open('w', encoding='utf-8')
                env = {k: v for k, v in os.environ.items() if k != 'REPLAYX_VIDEO_TUNING'}
                overrides, tuning = CONFIGS[config]
                if tuning:
                    env['REPLAYX_VIDEO_TUNING'] = tuning
                recorder = subprocess.Popen([str(CLI), 'capture-test', str(a.seconds + 14), *COMMON, *overrides],
                                            cwd=folder, env=env, stdout=log, stderr=log,
                                            creationflags=subprocess.CREATE_NO_WINDOW)
                children.append(recorder)
            print(f'{phase}: settling 10 s', flush=True)
            observe(10, phase + '-settle')
            if recorder and recorder.poll() is not None:
                raise RuntimeError(f'{phase}: recorder stopped early (see {phase}/capture.log)')
            pm = None
            if presentmon:
                pm = subprocess.Popen([str(presentmon), '--process_name', game.name(),
                                       '--output_file', str(out / f'{phase}.csv'), '--timed', str(a.seconds),
                                       '--terminate_after_timed', '--stop_existing_session', '--no_console_stats'],
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                      creationflags=subprocess.CREATE_NO_WINDOW)
                children.append(pm)
            boundaries.append(dict(phase=phase, config=config, start=time.time()))
            print(f'{phase}: measuring {a.seconds} s', flush=True)
            observe(a.seconds, phase)
            boundaries[-1]['end'] = time.time()
            if pm:
                try:
                    pm.wait(timeout=60)
                except subprocess.TimeoutExpired:  # never lose the session over it
                    pm.terminate()
            if recorder:
                deadline = time.time() + 90
                while recorder.poll() is None and time.time() < deadline:
                    observe(1, phase + '-save')
                if recorder.poll() is None:
                    recorder.terminate()
                    raise RuntimeError(f'{phase}: recorder did not finish')
                reports = list((out / phase).glob('test-output/capture-test-*/report.json'))
                if reports:
                    status = json.loads(reports[0].read_text(encoding='utf-8')).get('status', {})
                    boundaries[-1]['recorder'] = {k: status.get(k) for k in (
                        'capture_api', 'capture_filter', 'capture_fallback', 'scaling', 'encoder', 'fps',
                        'dropped_frames', 'duplicated_frames', 'encoder_speed', 'video_lag_ms',
                        'recovery_sessions')}
                    # recovery_sessions 1 = the adaptive rate switched this phase to 2x.
    except Exception as e:  # report what was measured so far
        errors.append(str(e))
        print('Stopped:', e, flush=True)
    finally:
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
        engines_log.close()

    def window(rows, b):
        return [v for t, v in rows if b['start'] < t < b.get('end', 0)]
    lines = [json.loads(l) for l in (out / 'gpu-engines.jsonl').read_text(encoding='utf-8-sig').splitlines() if l.strip()]
    names = {}
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
                m = re.match(r'pid_(\d+)_luid_(0x\w+_0x\w+)_phys_\d+_eng_\d+_engtype_(.*)$', e['InstanceName'])
                if m and names.get(m.group(1)):
                    key = f"{names[m.group(1)]}|{m.group(2)}|{m.group(3)}"
                    engine[key] = engine.get(key, 0) + e['CookedValue'] / max(1, len(rows))
        proc_cpu = {}
        for s in (s for s in samples if s['phase'] == b['phase']):
            for k, v in s['cpu'].items():
                proc_cpu.setdefault(k, []).append(v)
        entry = dict(phase=b['phase'], config=b['config'], recorder=b.get('recorder'),
                     cpu_effective_clock_percent=round(statistics.mean(cpu), 1) if cpu else None,
                     process_cpu_percent={k: round(statistics.mean(v[1:] or v), 2) for k, v in proc_cpu.items()},
                     gpu_engines_percent={k: round(v, 1) for k, v in sorted(engine.items(), key=lambda kv: -kv[1]) if v >= 0.5},
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
    result = dict(errors=errors, presentmon=bool(presentmon), seconds_per_phase=a.seconds, recorder=str(CLI),
                  source=commit, process=game.name(), phases=summary,
                  limits='Without PresentMon, game FPS is not measured: GPU-engine and CPU telemetry only. '
                         'Telemetry is once a second. Scene and camera must be held identical by the player.')
    (out / 'summary.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2))
    print('Saved', out / 'summary.json')


if __name__ == '__main__':
    main()
