"""Pure schedule helpers for the desktop benchmark; no hardware access."""
import json
import math
import statistics


def phase_plan(configs):
    phases = []
    for index, config in enumerate(configs, 1):
        phases.extend([(f'A{index}', None), (f'B{index}-{config}', config)])
    phases.append((f'A{len(configs) + 1}', None))
    return phases


SETUP_SECONDS = 5
ANALYSIS_SECONDS = 10


def phase_seconds(config, measured_seconds):
    return 10 + measured_seconds + (12 if config else 3)


def estimate(configs, seconds, warmup, countdown):
    timed = countdown + warmup + len(phase_plan(configs)) * (10 + seconds)
    cleanup = len(configs) * 12 + (len(configs) + 1) * 3
    return dict(timed_seconds=timed, expected_total_seconds=timed + cleanup + SETUP_SECONDS + ANALYSIS_SECONDS,
                setup_seconds=SETUP_SECONDS, analysis_seconds=ANALYSIS_SECONDS, cleanup_seconds=cleanup,
                recorder_wait_limit_seconds=12 * len(configs),
                presentmon_wait_limit_seconds=cleanup,
                note='Fixed phase deadlines include startup/settling and cleanup. Overruns fail the run. Final analysis has a 10 second reservation.')


def clock(seconds):
    seconds = max(0, math.ceil(seconds))
    return f'{seconds // 60}:{seconds % 60:02}'


def neighboring_comparisons(phases):
    """Normalize completed recording phases against both adjacent off phases."""
    comparisons = []
    for index, phase in enumerate(phases):
        if not phase.get('config'):
            continue
        row = dict(phase=phase['phase'], config=phase['config'], usable=False)
        neighbors = phases[max(0, index - 1):index] + phases[index + 1:index + 2]
        if len(neighbors) != 2 or any(p.get('config') for p in neighbors):
            row['reason'] = 'Both neighboring recorder-off phases are required'
        elif not all(p.get('frames', {}).get('fps', 0) > 0 for p in [phase, *neighbors]):
            row['reason'] = 'Valid PresentMon FPS is required for all three phases'
        else:
            fps = [p['frames']['fps'] for p in neighbors]
            baseline = sum(fps) / 2
            row.update(usable=True, baseline_fps=baseline,
                       fps_cost_percent=100 * (1 - phase['frames']['fps'] / baseline),
                       baseline_spread_percent=100 * abs(fps[1] - fps[0]) / baseline)
            for metric, output in [('low1_fps', 'low1_cost_percent'), ('p99_ms', 'p99_delta_ms')]:
                values = [p['frames'].get(metric) for p in [phase, *neighbors]]
                if all(v is not None and v > 0 for v in values):
                    reference = (values[1] + values[2]) / 2
                    row[output] = (100 * (1 - values[0] / reference) if metric == 'low1_fps'
                                   else values[0] - reference)
            if (phase.get('recorder') or {}).get('recovery_sessions', 0):
                row.update(usable=False, reason='Capture restarted during the phase')
            if phase.get('recorder_verified') is False:
                row.update(usable=False, reason='Recording report was not verified')
            if phase.get('recorder_exit_code') not in (None, 0):
                row.update(usable=False, reason='Recorder failed')
            if phase.get('config') == 'native-record' and not (phase.get('recorder') or {}).get('success'):
                row.update(usable=False, reason='Native recording success was not verified')
        comparisons.append(row)
    return comparisons


def paired_costs(comparisons):
    """Exploratory paired interval across adjacent current/native observations."""
    differences = []
    recorder_names = {'wgc-auto': 'current', 'app-ffmpeg-nvenc-fixed': 'current',
                      'native-record': 'native', 'app-native-nvenc-phase': 'native',
                      'app-ffmpeg-matched-steady': 'current', 'app-native-matched-phase': 'native'}
    for index in range(0, len(comparisons) - 1, 2):
        pair = comparisons[index:index+2]
        by_recorder = {recorder_names.get(row['config'], row['config']): row
                       for row in pair if row.get('usable')}
        if set(by_recorder) != {'current', 'native'}:
            continue
        differences.append(by_recorder['current']['fps_cost_percent'] - by_recorder['native']['fps_cost_percent'])
    result = dict(pairs=len(differences), differences_percentage_points=differences)
    if len(differences) == 3:
        average = statistics.mean(differences)
        margin = 4.302653 * statistics.stdev(differences) / math.sqrt(3)
        result.update(mean_gain_percentage_points=average, exploratory_95_percent_interval=[average-margin, average+margin],
                      note='Three pairs give an exploratory estimate. Order, scene and thermal drift can still bias it.')
    return result


def print_summary(phases):
    print('Average game FPS and mean phase 1% lows / P99; CPU is percent of the whole machine.')
    configs = list(dict.fromkeys(p['config'] for p in phases))
    for config in configs:
        rows = [p for p in phases if p['config'] == config and p.get('frames', {}).get('fps')]
        if not rows:
            continue
        values = lambda key: [p['frames'][key] for p in rows if p['frames'].get(key) is not None]
        show = lambda key: f'{statistics.mean(values(key)):.2f}' if values(key) else 'unavailable'
        print(f"{config or 'Recorder OFF'} ({len(rows)} phases): FPS {show('fps')}, 1% low {show('low1_fps')}, P99 {show('p99_ms')} ms")
    for p in phases:
        if not p.get('config'):
            continue
        cpu = sum(value for name, value in p.get('process_cpu_percent', {}).items() if name.startswith('replayx') or name == 'ffmpeg.exe')
        recording = json.dumps(p.get('recorder'), ensure_ascii=True)
        print(f"{p['phase']}: recorder CPU {cpu:.2f}%; recording {recording}; GPU engines {p.get('gpu_engines_percent') or 'unavailable'}")
