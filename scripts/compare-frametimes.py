"""Read-only PresentMon/FrameView A/B/A CSV comparison. Never launches or hooks a game.
python scripts/compare-frametimes.py A1.csv B.csv A2.csv --process FortniteClient-Win64-Shipping.exe --cap 360
Uses present intervals (not displayed FPS). Disable frame generation manually for comparisons.
"""
import argparse
import csv
import json
import math
import statistics
from collections import defaultdict


def summarize(file, process=None, trim=5.0, cap=0):
    groups = defaultdict(list)
    discarded = 0
    with open(file, newline='', encoding='utf-8-sig') as f:
        reader = csv.DictReader(f)
        headers = {h.strip().lower(): h for h in reader.fieldnames or []}
        column = next((headers[h] for h in ['msbetweenpresents', 'frametime', 'cpuframetime'] if h in headers), None)
        if not column:
            raise ValueError('No supported frame-time column (milliseconds)')
        for row in reader:
            get = lambda h: row.get(headers.get(h, ''), '')
            if process and get('application').lower() != process.lower():
                continue
            if get('frametype') and get('frametype').lower() not in ['application', '1']:
                continue
            try:
                value = float(row[column])
            except (ValueError, TypeError):
                discarded += 1
                continue
            if not math.isfinite(value) or value <= 0:
                discarded += 1
                continue
            extra = {}
            for metric, aliases in {
                'gpu_time_ms': ['msgputime', 'gputime'],
                'gpu_busy_ms': ['msgpubusy', 'gpubusy'],
                'gpu_wait_ms': ['msgpuwait', 'gpuwait'],
                'cpu_busy_ms': ['mscpubusy', 'cpubusy'],
                'cpu_wait_ms': ['mscpuwait', 'cpuwait'],
            }.items():
                field = next((h for h in aliases if h in headers), None)
                try:
                    v = float(get(field)) if field else None
                    if v is not None and math.isfinite(v) and v >= 0:
                        extra[metric] = v
                except (ValueError, TypeError):
                    pass
            groups[(get('application'), get('processid'), get('swapchainaddress'))].append((value, extra))
    if not groups:
        raise ValueError('No matching application frame intervals')
    key, times = max(groups.items(), key=lambda pair: len(pair[1]))
    elapsed, used, metrics = 0., [], defaultdict(list)
    for value, extra in times:
        elapsed += value / 1000
        if elapsed > trim:
            used.append(value)
            for name, v in extra.items(): metrics[name].append(v)
    if len(used) < 300 or sum(used) < 10_000:
        raise ValueError('Need at least 300 frames and ten seconds after warmup')
    ordered = sorted(used)
    fps = 1000 * len(used) / sum(used)
    median = statistics.median(used)
    return dict(frames=len(used), seconds=sum(used)/1000, fps=fps,
        low1_fps=1000/statistics.mean(ordered[-max(1, len(used)//100):]),
        p95_ms=ordered[int(.95*(len(used)-1))],
        p99_ms=ordered[int(.99*(len(used)-1))],
        median_ms=median, mean_ms=statistics.mean(used),
        variance_ms2=statistics.pvariance(used), stddev_ms=statistics.pstdev(used),
        max_ms=max(used), spikes_over_2x_median=sum(v > 2*median for v in used),
        spikes_over_8_33ms=sum(v > 1000/120 for v in used),
        spikes_over_16_67ms=sum(v > 1000/60 for v in used),
        cap_limited=bool(cap and fps >= cap*.95),
        application=key[0], process_id=key[1],
        selected_swapchain=key[2], groups_seen=len(groups), discarded=discarded,
        interval_column=column,
        optional_frame_metrics={name: dict(mean=statistics.mean(v), p99=sorted(v)[int(.99*(len(v)-1))]) for name,v in metrics.items()})


def compare(a1, b, a2):
    controls = statistics.mean([a1['fps'], a2['fps']])
    drift = 100*(a2['fps']/a1['fps']-1)
    phases = [a1, b, a2]
    reasons = []
    if abs(drift) > 5:
        reasons.append('Control FPS drift exceeds 5%')
    if any(x['cap_limited'] for x in phases):
        reasons.append('A phase is within 5% of the configured frame cap')
    for field in ['application', 'process_id', 'selected_swapchain', 'interval_column']:
        if len({str(x.get(field, '')).lower() for x in phases}) > 1:
            reasons.append(f'Phase {field} does not match')
    return dict(before=a1, capture=b, after=a2,
        overhead_percent=100*(1-b['fps']/controls), control_drift_percent=drift,
        valid_comparison=not reasons, invalid_reasons=reasons,
        limits='Present-call intervals, not displayed FPS. This checks CSV comparability only; foreground, scene, idle throttling and settings must also be verified. A cap, >5% drift or mismatched process/swapchain/interval prevents a reliable overhead conclusion. Repeat three times.')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('csv', nargs=3)
    p.add_argument('--process')
    p.add_argument('--trim', type=float, default=5)
    p.add_argument('--cap', type=float, default=0)
    a = p.parse_args()
    print(json.dumps(compare(*(summarize(f, a.process, a.trim, a.cap) for f in a.csv)), indent=2))
