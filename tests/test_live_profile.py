import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('live_profile', Path(__file__).parents[1]/'scripts/live-game-profile.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class LiveProfile(unittest.TestCase):
    def test_timestamp_attribution_ignores_delayed_receipt(self):
        sample = m.parse_gpu_sample('2026/11/11 12:00:01.000, 2505, 8001, 50, 45, 20, 60, 1, 85, 1100')
        t = sample['unix_seconds']
        bounds = [dict(phase='A1', start_unix=t-1, end_unix=t+1),
                  dict(phase='B', start_unix=t+2, end_unix=t+10)]
        self.assertEqual(m.phase_for(t, bounds), 'A1')
        self.assertEqual(m.phase_for(t+1, bounds), 'transition')
        self.assertEqual(m.phase_for(t+2, bounds), 'B')

    def test_unavailable_and_invalid_metrics_are_not_zero(self):
        sample = m.parse_gpu_sample('2026/11/11 12:00:01.000, N/A, 8001, nan, 45, 20, 60, 1, 85, 1100')
        self.assertIsNone(sample['values']['clocks.current.graphics'])
        self.assertIsNone(sample['values']['power.draw'])
        self.assertIsNone(m.parse_gpu_sample('wrong'))

    def test_inactive_engine_samples_count_as_zero_and_engines_stay_separate(self):
        def row(second, engines):
            return json.dumps(dict(utc=f'2026-11-11T12:00:0{second}Z', engines=engines))
        def engine(kind, value, index):
            return dict(InstanceName=f'pid_1_luid_0x0000_0x1234_phys_0_eng_{index}_engtype_{kind}', CookedValue=value)
        from datetime import datetime
        t = datetime.fromisoformat('2026-11-11T12:00:00+00:00').timestamp()
        bounds = [dict(phase='B', start_unix=t, end_unix=t+10)]
        r = m.engine_summary([row(1, [engine('3d', 80, 0), engine('Copy', 4, 1)]), row(2, []), 'bad'], bounds, {'1':'ffmpeg.exe'})
        self.assertEqual(r['phases']['B']['samples'], 2)
        values = list(r['phases']['B']['engines'].values())
        self.assertEqual(sorted(v['mean_busy_percent'] for v in values), [2, 40])
        self.assertEqual(r['malformed_rows'], 1)


if __name__ == '__main__':
    unittest.main()
