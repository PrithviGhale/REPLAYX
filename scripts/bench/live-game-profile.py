"""Noninvasive off/on/off resource profile of an already-running foreground game.

No game input, memory access, injection, overlay, settings or permission changes.
This is NOT an FPS benchmark: use separately authorized PresentMon CSVs for that.
Stops its own recorder on focus loss, game exit or timeout. Never stops the game.
All capture files go to a fresh test-output directory; microphone is never opened.
"""
import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime
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

ROOT = Path(__file__).resolve().parents[2]
GPU_FIELDS = ['clocks.current.graphics', 'clocks.current.memory', 'power.draw',
              'utilization.gpu', 'utilization.encoder', 'temperature.gpu',
              'encoder.stats.sessionCount', 'encoder.stats.averageFps',
              'encoder.stats.averageLatency']


def phase_for(timestamp, boundaries):
    return next((b['phase'] for b in boundaries if
        b['start_unix'] <= timestamp < b.get('end_unix', b['start_unix'])), 'transition')


def parse_gpu_sample(line):
    fields = line.strip().split(',')
    if len(fields) != len(GPU_FIELDS)+1:
        return None
    try:
        measured_at = datetime.strptime(fields[0].strip(), '%Y/%m/%d %H:%M:%S.%f').timestamp()
    except ValueError:
        return None
    values = []
    for value in fields[1:]:
        try:
            number = float(value.strip())
            values.append(number if math.isfinite(number) else None)
        except ValueError:
            values.append(None)
    return dict(unix_seconds=measured_at, values=dict(zip(GPU_FIELDS, values)))


def engine_summary(lines, boundaries, names):
    phases = {name: [] for name in ['A1', 'B', 'A2']}
    pattern = re.compile(r'pid_(\d+)_luid_(0x\w+_0x\w+)_phys_(\d+)_eng_(\d+)_engtype_(.+)', re.I)
    invalid = 0
    for line in lines:
        try:
            row = json.loads(line)
            phase = phase_for(datetime.fromisoformat(row['utc'].replace('Z', '+00:00')).timestamp(), boundaries)
            if phase not in phases:
                continue
            engines = row.get('engines') or []
            if isinstance(engines, dict):
                engines = [engines]
            values = {}
            for engine in engines:
                m = pattern.fullmatch(engine['InstanceName'])
                value = float(engine['CookedValue'])
                if not m or m[1] not in names or not math.isfinite(value) or not 0 <= value <= 100:
                    continue
                # Separate physical engine IDs; no sum of independent engines.
                key = f'{names[m[1]]}|{m[2]}|phys{m[3]}|engine{m[4]}|{m[5].lower()}'
                values[key] = value
            phases[phase].append(values)
        except (ValueError, KeyError, TypeError):
            invalid += 1
    result = {}
    for phase, rows in phases.items():
        keys = set().union(*(r.keys() for r in rows))
        result[phase] = dict(samples=len(rows), engines={key: dict(
            mean_busy_percent=statistics.mean(r.get(key, 0.) for r in rows),
            max_busy_percent=max(r.get(key, 0.) for r in rows)) for key in sorted(keys)})
    return dict(phases=result, malformed_rows=invalid,
                scope='Per-process, per-physical-engine PDH busy percent; absent inactive instances count as zero. Never total GPU utilization.')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--pid', type=int, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--seconds', type=int, default=60)
    p.add_argument('--audio', choices=['silent', 'system'], default='silent')
    a = p.parse_args()
    if not 60 <= a.seconds <= 120:
        p.error('Use 60–120 second measurement windows')
    game = psutil.Process(a.pid)
    if game.name().lower() != 'fortniteclient-win64-shipping.exe':
        p.error('This authorized live profile targets the existing Fortnite process only')
    out = a.out.resolve()
    out.relative_to(ROOT / 'test-output')
    out.mkdir(parents=True, exist_ok=False)
    # Read-only foreground/minimized state is a validity and capture-privacy gate.
    # No SetForegroundWindow, input APIs, game handles or private memory access.
    ui = ctypes.WinDLL('user32', use_last_error=True)
    ui.GetForegroundWindow.restype = wintypes.HWND
    ui.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    ui.IsIconic.argtypes = [wintypes.HWND]

    def foreground():
        window = ui.GetForegroundWindow()
        pid = wintypes.DWORD()
        ui.GetWindowThreadProcessId(window, ctypes.byref(pid))
        return pid.value == a.pid and not ui.IsIconic(window)

    if not foreground():
        raise RuntimeError('Game must already be foreground; this tool never activates it')
    if any(p.info['name'].lower() in ['ffmpeg.exe', 'replayx-cli.exe']
           for p in psutil.process_iter(['name']) if p.info['name']):
        raise RuntimeError('Another media job is active; refusing to contaminate the profile')
    started = time.perf_counter()
    samples, gpu_rows, children, errors, boundaries = [], [], [], [], []
    stop = threading.Event()
    phase = 'warmup'
    recorder = None
    gpu_log = (out / 'nvidia-smi.csv').open('w', encoding='utf-8')
    gpu = subprocess.Popen([r'C:\Windows\System32\nvidia-smi.exe',
        '--query-gpu=timestamp,' + ','.join(GPU_FIELDS), '--format=csv,noheader,nounits', '-lms', '1000'],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
        creationflags=subprocess.CREATE_NO_WINDOW)
    children.append(gpu)

    def read_gpu():
        for line in gpu.stdout:
            gpu_log.write(line); gpu_log.flush()
            # NVIDIA buffers stdout on Windows: attribute by driver sample time.
            if (sample := parse_gpu_sample(line)) is not None:
                gpu_rows.append(sample)

    gpu_thread = threading.Thread(target=read_gpu, daemon=True)
    gpu_thread.start()
    # Same observer in all phases. Raw PDH engine counters retain PID/adapter
    # attribution; do not sum different engines into a misleading GPU percent.
    engine_log = (out/'gpu-engines.jsonl').open('w', encoding='utf-8')
    engine_sampler = subprocess.Popen(['powershell', '-NoProfile', '-Command',
        # Refresh wildcard expansion regularly so FFmpeg GPU contexts created
        # after A1 are included. One perpetual PDH query can miss new instances.
        "$ErrorActionPreference='Stop'; while ($true) { "
        "Get-Counter -Counter '\\GPU Engine(*)\\Utilization Percentage' "
        "-SampleInterval 1 -MaxSamples 2 -ErrorAction SilentlyContinue | ForEach-Object { "
        "$s=$_; $o=@{'utc'=$s.Timestamp.ToUniversalTime().ToString('o'); "
        "'engines'=@($s.CounterSamples | Where-Object CookedValue -GT 0 | "
        "Select-Object InstanceName,CookedValue)}; "
        "[Console]::Out.WriteLine(($o | ConvertTo-Json -Compress -Depth 4)); [Console]::Out.Flush() } }"],
        stdout=engine_log, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW)
    children.append(engine_sampler)
    cache = {}

    def observe(duration):
        until = time.perf_counter() + duration
        while time.perf_counter() < until:
            if not game.is_running() or not foreground():
                raise RuntimeError('Game exited, minimized or lost focus; profile invalidated')
            processes = [game]
            processes += [p for p in psutil.process_iter(['name'])
                          if (p.info['name'] or '').lower() in ['replayx.exe', 'replayx-cli.exe', 'ffmpeg.exe']]
            resources = {}
            for proc in processes:
                try:
                    proc = cache.setdefault(proc.pid, proc)
                    resources[str(proc.pid)] = dict(name=proc.name(),
                        cpu_percent_all_cores=proc.cpu_percent()/psutil.cpu_count(),
                        resident_mb=proc.memory_info().rss/2**20)
                except psutil.Error:
                    continue
            samples.append(dict(seconds=time.perf_counter()-started, phase=phase,
                                unix_seconds=time.time(), foreground=True, processes=resources))
            stop.wait(min(1., max(0., until-time.perf_counter())))

    try:
        for control in ['A1', 'B', 'A2']:
            if control == 'B':
                phase = 'capture-start'
                log = (out / 'capture.log').open('w', encoding='utf-8')
                env = os.environ.copy()
                env.pop('REPLAYX_VIDEO_TUNING', None)
                recorder = subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-cli.exe'),
                    'capture-test', str(a.seconds+25), a.audio, 'fps=60', 'width=1920',
                    'height=1080', 'encoder=h264_nvenc', 'codec=h264', 'preset=balanced',
                    'mode=stretch', 'replay=60', 'buffer=256'],
                    cwd=out, env=env, stdout=log, stderr=log,
                    creationflags=subprocess.CREATE_NO_WINDOW)
                children.append(recorder)
                observe(15)
                if recorder.poll() is not None:
                    raise RuntimeError('Recorder exited before the measurement window')
            phase = control
            boundaries.append(dict(phase=phase, start_unix=time.time()))
            print('Phase', phase, flush=True)
            observe(a.seconds)
            boundaries[-1]['end_unix'] = time.time()
            if control == 'B':
                phase = 'save-and-stop'
                deadline = time.perf_counter() + 90
                while recorder.poll() is None and time.perf_counter() < deadline:
                    observe(1)
                if recorder.poll() != 0:
                    raise RuntimeError('Recorder failed or exceeded its bounded deadline')
                log.close()
                phase = 'settle'
                observe(10)
    except Exception as e:
        errors.append(str(e))
    finally:
        # Kill only processes created by this run, never games or existing apps.
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
            try:
                child.wait(timeout=15)
            except subprocess.TimeoutExpired:
                child.kill(); child.wait(timeout=5)
        gpu_thread.join(timeout=5)
        gpu_log.close()
        engine_log.close()
        (out/'gpu-engines-errors.txt').write_bytes(engine_sampler.stderr.read())
        (out/'samples.json').write_text(json.dumps(samples, indent=2), encoding='utf-8')
        for row in gpu_rows:
            row['phase'] = phase_for(row['unix_seconds'], boundaries)
        (out/'gpu.json').write_text(json.dumps(gpu_rows, indent=2), encoding='utf-8')
        (out/'phases.json').write_text(json.dumps(boundaries, indent=2), encoding='utf-8')
    summary = {}
    for name in ['A1', 'B', 'A2']:
        rows = [r for r in gpu_rows if r['phase'] == name]
        summary[name] = {field: statistics.mean(v) if (v := [r['values'][field] for r in rows
            if r['values'].get(field) is not None]) else None for field in GPU_FIELDS}
        rows = [r for r in samples if r['phase'] == name][1:]  # first CPU delta may span transition
        summary[name]['resources'] = {}
        for kind in ['FortniteClient-Win64-Shipping.exe', 'replayx.exe', 'replayx-cli.exe', 'ffmpeg.exe']:
            selected = [v for r in rows for v in r['processes'].values() if v['name'] == kind]
            summary[name]['resources'][kind] = dict(
                cpu_percent_all_cores=statistics.mean(v['cpu_percent_all_cores'] for v in selected) if selected else None,
                peak_resident_mb=max((v['resident_mb'] for v in selected), default=None))
    report = dict(phases=summary, errors=errors, render_fps=None, valid_fps_comparison=False,
        limits='Resource telemetry only. ETW permissions unavailable. Stationary Creative camera; no gameplay inputs. '
               'GPU values are device-wide and NVML encoder latency is not capture-to-file latency. '
               'Foreground checks do not prove an identical scene or exclude idle throttling.',
        audio=a.audio, seconds_per_window=a.seconds)
    report['gpu_sample_counts'] = {name: sum(r['phase'] == name for r in gpu_rows) for name in ['A1', 'B', 'A2']}
    report['gpu_phase_attribution_valid'] = all(n >= a.seconds*.8 for n in report['gpu_sample_counts'].values())
    names = {pid: data['name'] for row in samples for pid, data in row['processes'].items()}
    report['gpu_engines'] = engine_summary((out/'gpu-engines.jsonl').read_text(encoding='utf-8-sig').splitlines(), boundaries, names)
    (out/'summary.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report, indent=2), flush=True)
    if errors:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
