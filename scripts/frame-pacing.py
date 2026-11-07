"""Read-only media diagnostics. Requires numpy and the pinned FFmpeg runtime.

Reports container timestamps separately from decoded motion. Near-identical
frames do NOT establish where a repeat originated (game/capture/encoder/player).
Use --fixture for the silent motion fixture's binary 60Hz wall-clock counter.
"""
import argparse
import collections
import json
import pathlib
import subprocess
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parents[1]


def summary(values):
    a = np.asarray(values, dtype=float)
    return dict(zip(('median', 'p95', 'p99', 'max'),
                    map(float, np.percentile(a, [50, 95, 99, 100])))) if len(a) else None


def analyze(path, seconds=0, fixture=False):
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
    diffs, ids, holds, last, run = [], [], [], None, 0
    with subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as p:
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
                if delta < 0.15:
                    run += 1
                elif run:
                    holds.append(run)
                    run = 0
            if fixture:
                # Fixture is scaled to the entire capture canvas, fullscreen.
                bits = [frame[9:13, 10+i*12:14+i*12].mean() > 128 for i in range(24)]
                ids.append(sum(int(b) << i for i, b in enumerate(bits)))
            last = frame.astype(np.int16)
        error = p.stderr.read().decode(errors='replace')
        code = p.wait()
    if run:
        holds.append(run)
    result = dict(file=str(path.resolve()), format=data['format'], tracks=tracks,
                  decoded_frames=len(diffs)+int(last is not None), decode_exit=code, decode_errors=error,
                  mean_absolute_luma_difference=summary(diffs),
                  near_identical_pairs=sum(d < .15 for d in diffs),
                  longest_near_identical_run_frames=max(holds, default=0)+1,
                  limitations='Visual repeats are content dependent. Capture drops, encoder drops, physical AV offset and playback smoothness cannot be attributed from this file alone.')
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
    parser.add_argument('--out', type=pathlib.Path, required=True)
    args = parser.parse_args()
    result = analyze(args.file, args.seconds, args.fixture)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k not in ('fixture',)}, indent=2))
