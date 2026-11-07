"""Sample a recorder subprocess tree without querying the UI or game.
CPU is summed CPU seconds per sample interval; memory is summed working set
(shared pages may be counted more than once). GPU/encode latency not measured.
"""
import argparse
import json
import pathlib
import subprocess
import time
import psutil

ROOT = pathlib.Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--out', type=pathlib.Path, required=True)
p.add_argument('--exe', type=pathlib.Path, default=ROOT/'src-tauri/target/release/replayx-cli.exe')
p.add_argument('command', nargs=argparse.REMAINDER)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=False)
samples, prior = [], {}
start = time.monotonic()
with (a.out/'cli.log').open('w') as log:
    child = subprocess.Popen([str(a.exe), *a.command], stdout=log, stderr=log)
    proc = psutil.Process(child.pid)
    while child.poll() is None:
        now, cpu, mem, write = time.monotonic(), 0., 0, 0
        try:
            processes = [proc, *proc.children(recursive=True)]
        except psutil.Error:
            processes = []
        for item in processes:
            try:
                times = item.cpu_times()
                total = times.user+times.system
                key = (item.pid, item.create_time())
                cpu += max(0., total-prior.get(key, total))
                prior[key] = total
                mem += item.memory_info().rss
                write += item.io_counters().write_bytes
            except psutil.Error:
                pass
        samples.append(dict(seconds=now-start, cpu_seconds_delta=cpu, working_set_bytes=mem, live_process_write_bytes=write))
        (a.out/'samples.json').write_text(json.dumps(samples), encoding='utf-8')
        time.sleep(1)
result = dict(exit=child.returncode, wall_seconds=time.monotonic()-start, sample_count=len(samples),
              cpu_seconds=sum(s['cpu_seconds_delta'] for s in samples), peak_working_set_bytes=max((s['working_set_bytes'] for s in samples), default=0),
              gpu_load=None, game_fps=None, encode_latency=None)
(a.out/'report.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps(result))
