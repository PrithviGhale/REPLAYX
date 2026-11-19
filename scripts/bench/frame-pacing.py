"""Read-only media diagnostics. Requires numpy and the pinned FFmpeg runtime.

Reports container timestamps separately from decoded motion. Near-identical
frames do NOT establish where a repeat originated (game/capture/encoder/player).
Use --fixture for the silent motion fixture's binary 60Hz wall-clock counter.
"""
import argparse
import collections
import json
import math
import pathlib
import subprocess
import tempfile
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parents[2]


def summary(values):
    a = np.asarray(values, dtype=float)
    return dict(zip(('median', 'p95', 'p99', 'max'),
                    map(float, np.percentile(a, [50, 95, 99, 100])))) if len(a) else None


NEAR_IDENTICAL_LUMA = 0.15


def motion_stats(differences):
    longest, run = 0, 0
    for delta in differences:
        run = run + 1 if delta < NEAR_IDENTICAL_LUMA else 0
        longest = max(longest, run)
    return dict(compared_pairs=len(differences),
                mean_absolute_luma_difference=summary(differences),
                near_identical_pairs=sum(d < NEAR_IDENTICAL_LUMA for d in differences),
                longest_near_identical_run_frames=longest+1)


def analyze_motion_windows(pts, differences, windows):
    if len(pts) < 2 or len(pts) != len(differences)+1:
        raise ValueError('Decoded timestamps and image counts do not match')
    if not all(math.isfinite(t) for t in pts) or any(b <= a for a, b in zip(pts, pts[1:])):
        raise ValueError('Motion windows require finite, increasing decoded timestamps')
    elapsed = np.asarray(pts) - pts[0]
    result = []
    for start, end in windows:
        if not all(map(math.isfinite, (start, end))) or not 0 <= start < end <= elapsed[-1]:
            raise ValueError(f'Motion window {start:g}-{end:g} s falls outside decoded video timestamps')
        # Half-open window relative to the first decoded picture. Do not count
        # a pair straddling either end of the motion interval.
        left, right = np.searchsorted(elapsed, [start, end])
        if right-left < 2:
            raise ValueError(f'Motion window {start:g}-{end:g} s contains fewer than two frames')
        result.append(dict(start_seconds=start, end_seconds=end, decoded_frames=int(right-left),
                           interval_ms=summary(np.diff(elapsed[left:right])*1000),
                           **motion_stats(differences[left:right-1])))
    return result


def analyze(path, seconds=0, fixture=False, windows=()):
    if seconds and windows:
        raise ValueError('Motion windows require a full decode. Omit --seconds.')
    lock = json.loads((ROOT / 'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
    runtime = ROOT / 'tools' / lock['runtime_dir']
    probe = str(runtime / 'ffprobe.exe')
    ffmpeg = str(runtime / 'ffmpeg.exe')
    limit = ['-read_intervals', f'%+{seconds}'] if seconds else []
    cmd = [probe, '-v', 'error', *limit, '-show_streams', '-show_format',
           '-show_packets', '-show_entries',
           'packet=stream_index,pts_time,dts_time,duration_time,flags:stream=index,codec_type,codec_name,width,height,avg_frame_rate,r_frame_rate,time_base:format=duration,start_time',
           '-of', 'json', str(path)]
    data = json.loads(subprocess.check_output(cmd))
    tracks = []
    for stream in data['streams']:
        packets = [p for p in data['packets'] if p['stream_index'] == stream['index']]
        pts = [float(p['pts_time']) for p in packets if 'pts_time' in p]
        dts = [float(p['dts_time']) for p in packets if 'dts_time' in p]
        ordered = sorted(pts)
        intervals = np.diff(ordered) * 1000
        tracks.append(dict(stream=stream, packets=len(packets), first_pts=min(pts) if pts else None,
                           last_pts=max(pts) if pts else None,
                           interval_ms=summary(intervals),
                           interval_histogram_ms=dict(sorted(collections.Counter(round(float(x), 3) for x in intervals).items())),
                           non_increasing_dts=sum(b <= a for a, b in zip(dts, dts[1:])),
                           duplicate_pts=len(pts)-len(set(pts))))
    # Full decode, bounded decoder concurrency and tiny comparison images; no
    # huge raw-video allocation. Timing remains in the packet report above.
    args = [ffmpeg, '-v', 'error', '-xerror', '-threads', '2', '-i', str(path)]
    if seconds:
        args += ['-t', str(seconds)]
    args += ['-map', '0:v:0', '-vf', 'scale=320:180:flags=area,format=gray',
             '-threads', '1', '-fps_mode', 'passthrough', '-f', 'rawvideo', '-']
    diffs, ids, last = [], [], None
    with tempfile.TemporaryFile() as stderr, subprocess.Popen(args, stdout=subprocess.PIPE, stderr=stderr) as p:
        while True:
            raw = p.stdout.read(320 * 180)
            if not raw:
                break
            if len(raw) != 320 * 180:
                raise RuntimeError('Truncated decoder output')
            frame = np.frombuffer(raw, dtype=np.uint8).reshape(180, 320)
            if last is not None:
                delta = float(np.abs(frame.astype(np.int16)-last).mean())
                diffs.append(delta)
            if fixture:
                # Fixture is scaled to the entire capture canvas, fullscreen.
                bits = [frame[9:13, 10+i*12:14+i*12].mean() > 128 for i in range(24)]
                ids.append(sum(int(b) << i for i, b in enumerate(bits)))
            last = frame.astype(np.int16)
        code = p.wait()
        stderr.seek(0)
        error = stderr.read().decode(errors='replace')
    result = dict(file=str(path.resolve()), format=data['format'], tracks=tracks,
                  decoded_frames=len(diffs)+int(last is not None), decode_exit=code, decode_errors=error,
                  **motion_stats(diffs),
                  limitations='Visual repeats are content dependent. Capture drops, encoder drops, physical AV offset and playback smoothness cannot be attributed from this file alone.')
    if windows:
        if code:
            raise RuntimeError(f'Cannot assess motion windows after decode failure: {error}')
        frames = json.loads(subprocess.check_output(
            [probe, '-v', 'error', '-threads', '2', '-select_streams', 'v:0', '-show_frames',
             '-show_entries', 'frame=best_effort_timestamp_time', '-of', 'json', str(path)]))
        pts = [float(frame['best_effort_timestamp_time']) for frame in frames['frames']]
        result['motion_windows'] = analyze_motion_windows(pts, diffs, windows)
    if fixture:
        changes = np.diff(ids)
        result['fixture'] = dict(source_ticks=ids, increments_histogram=dict(collections.Counter(map(int, changes))),
                                 repeats=int(sum(changes == 0)), backward=int(sum(changes < 0)))
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('file', type=pathlib.Path)
    parser.add_argument('--seconds', type=float, default=0)
    parser.add_argument('--fixture', action='store_true')
    parser.add_argument('--motion-window', action='append', nargs=2, type=float, default=[],
                        metavar=('START', 'END'), help='Review interval in seconds from the first decoded picture. Repeat for multiple windows.')
    parser.add_argument('--out', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if args.seconds and args.motion_window:
        parser.error('--motion-window requires a full decode. Omit --seconds.')
    result = analyze(args.file, args.seconds, args.fixture, args.motion_window)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k not in ('fixture',)}, indent=2))
