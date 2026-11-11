"""Isolated synthetic replay soak + another NVENC session. No screen/mic/audio output.
Not a Discord or gameplay test. Both save jobs run while the second encoder runs.
Usage: python scripts/concurrent-soak.py --seconds 900 --out test-output/soak-unique
"""
import argparse
import json
import pathlib
import statistics
import subprocess
import time
import psutil

ROOT = pathlib.Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--seconds', type=int, default=900)
p.add_argument('--out', type=pathlib.Path, required=True)
a = p.parse_args()
out = a.out.resolve()
out.relative_to(ROOT / 'test-output')  # refuse writes outside the fixture tree
out.mkdir(parents=True, exist_ok=False)
cli = ROOT / 'src-tauri/target/release/replayx-cli.exe'
lock = json.loads((ROOT / 'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
ffmpeg = ROOT / 'tools' / lock['runtime_dir'] / 'ffmpeg.exe'
samples, prior, streams = [], {}, []
children = []
start = time.monotonic()
with (out / 'capture.log').open('w') as log:
    recorder = subprocess.Popen([str(cli), 'self-test', str(a.seconds), 'silent', 'fps=60',
        'height=1080', 'width=1920', 'encoder=h264_nvenc', 'replay=60', 'buffer=256'], cwd=out, stdout=log, stderr=log)
    proc = psutil.Process(recorder.pid)
    launched = set()
    try:
        while recorder.poll() is None:
            elapsed = time.monotonic() - start
            for at, duration in [(max(10, a.seconds // 3), 30), (max(45, a.seconds - 20), 60)]:
                if elapsed >= at and at not in launched:
                    launched.add(at)
                    handle = (out / f'concurrent-{at}.log').open('w')
                    child = subprocess.Popen([str(ffmpeg), '-hide_banner', '-nostdin', '-v', 'error', '-re', '-f', 'lavfi',
                        '-i', 'testsrc2=s=1920x1080:r=60', '-t', str(duration), '-an', '-c:v', 'h264_nvenc',
                        '-preset', 'p5', '-cq', '28', '-f', 'null', '-'], stdout=handle, stderr=handle)
                    children.append((child, handle))
                    streams.append(dict(start_seconds=elapsed, requested_seconds=duration, pid=child.pid))
            cpu, memory = 0., 0
            try:
                processes = [proc, *proc.children(recursive=True)]
            except psutil.Error:
                processes = []
            for item in processes:
                try:
                    t = item.cpu_times(); total = t.user + t.system
                    key = (item.pid, item.create_time())
                    cpu += max(0., total - prior.get(key, total)); prior[key] = total
                    memory += item.memory_info().rss
                except psutil.Error:
                    pass
            samples.append(dict(seconds=elapsed, cpu_seconds=cpu, working_set_mb=memory / 1024**2))
            if elapsed > a.seconds + 120:
                raise TimeoutError('Recorder exceeded its capture/save deadline')
            time.sleep(1)
        for (child, handle), stream in zip(children, streams):
            stream['exit'] = child.wait(timeout=90)
            handle.close()
    finally:
        if recorder.poll() is None:
            recorder.terminate(); recorder.wait(timeout=15)
        for child, handle in children:
            if child.poll() is None:
                child.terminate(); child.wait(timeout=15)
            handle.close()

runs = list((out / 'test-output').glob('self-test-*'))
run = runs[0] if len(runs) == 1 else None
report = json.loads((run / 'report.json').read_text()) if run and (run / 'report.json').exists() else {}
decoded = []
for key in ['first', 'concurrent']:
    if report.get(key):
        result = subprocess.run([str(ffmpeg), '-v', 'error', '-i', report[key], '-map', '0', '-f', 'null', '-'], capture_output=True, timeout=90)
        decoded.append(dict(clip=key, exit=result.returncode, clean=not result.stderr.strip()))
steady = [s for s in samples if 60 <= s['seconds'] <= a.seconds - 30]
summary = dict(kind='synthetic; no desktop or physical audio', exit=recorder.returncode,
    duration_seconds=time.monotonic()-start, concurrent_encoders=streams, decode=decoded,
    first_ready_ms=report.get('first_ready_ms'), second_ready_ms=report.get('second_ready_ms'),
    mean_cpu_percent=sum(s['cpu_seconds'] for s in steady)/max(1, len(steady))/psutil.cpu_count()*100,
    peak_resident_mb=max((s['working_set_mb'] for s in samples), default=0),
    first_minute_resident_mb=statistics.mean([s['working_set_mb'] for s in steady[:60]]) if steady else None,
    last_minute_resident_mb=statistics.mean([s['working_set_mb'] for s in steady[-60:]]) if steady else None,
    remaining_buffer_files=len(list((run / 'data' / 'buffer').rglob('*.mkv'))) if run else None)
(out / 'resources.json').write_text(json.dumps(samples), encoding='utf-8')
(out / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
print(json.dumps(summary, indent=2))
if recorder.returncode or any(s['exit'] for s in streams) or len(decoded) != 2 or any(d['exit'] or not d['clean'] for d in decoded):
    raise SystemExit(1)
