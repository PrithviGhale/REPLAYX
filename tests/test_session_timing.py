import contextlib
import importlib.util
import io
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('session_timing', Path(__file__).parents[1] / 'scripts/bench/session_timing.py')
timing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(timing)


class SessionTiming(unittest.TestCase):
    def test_redirected_windows_summary_handles_unicode_recorder_status(self):
        phases = [
            dict(phase='B1', config='app-ffmpeg-matched-steady', frames=dict(fps=400),
                 recorder=dict(scaling='GPU zero-copy (Direct3D 11 \u2192 encoder)'),
                 process_cpu_percent={'ffmpeg.exe': 1.06}),
        ]
        data = io.BytesIO()
        with io.TextIOWrapper(data, encoding='cp1252') as stream:
            with contextlib.redirect_stdout(stream):
                timing.print_summary(phases)
            stream.flush()
            self.assertIn(b'\\u2192', data.getvalue())
            self.assertIn(b'recorder CPU 1.06%', data.getvalue())

    def test_each_recorder_has_neighboring_off_baselines(self):
        plan = timing.phase_plan(['current', 'native', 'native', 'current'])
        self.assertEqual(len(plan), 9)
        for i, (_, config) in enumerate(plan):
            if config:
                self.assertIsNone(plan[i-1][1])
                self.assertIsNone(plan[i+1][1])

    def test_original_six_phase_duration_and_waits(self):
        estimate = timing.estimate(['current', 'native'] * 3, 45, 150, 15)
        self.assertEqual(estimate['timed_seconds'], 880)
        self.assertEqual(estimate['recorder_wait_limit_seconds'], 72)
        self.assertEqual(estimate['presentmon_wait_limit_seconds'], 93)
        self.assertEqual(estimate['expected_total_seconds'], 988)

    def test_short_balanced_repeat_duration(self):
        estimate = timing.estimate(['current', 'native', 'native', 'current'], 30, 60, 15)
        self.assertEqual(timing.clock(estimate['timed_seconds']), '7:15')

    def test_clock_rounds_up_and_never_shows_negative_time(self):
        self.assertEqual(timing.clock(59.1), '1:00')
        self.assertEqual(timing.clock(-5), '0:00')


    def test_comparison_requires_both_off_neighbors(self):
        phases = [dict(phase='A1', config=None, frames=dict(fps=100)),
                  dict(phase='B1', config='current', frames=dict(fps=80))]
        self.assertFalse(timing.neighboring_comparisons(phases)[0]['usable'])

    def test_comparison_normalizes_fps_lows_and_p99(self):
        phases = [dict(phase='A1', config=None, frames=dict(fps=100, low1_fps=80, p99_ms=6)),
                  dict(phase='B1', config='current', frames=dict(fps=88, low1_fps=60, p99_ms=8)),
                  dict(phase='A2', config=None, frames=dict(fps=120, low1_fps=80, p99_ms=6))]
        row = timing.neighboring_comparisons(phases)[0]
        self.assertTrue(row['usable'])
        self.assertAlmostEqual(row['fps_cost_percent'], 20)
        self.assertAlmostEqual(row['low1_cost_percent'], 25)
        self.assertEqual(row['p99_delta_ms'], 2)
        phases[1]['recorder_exit_code'] = 1
        self.assertFalse(timing.neighboring_comparisons(phases)[0]['usable'])
        phases[1].update(config='native-record', recorder_exit_code=0)
        self.assertFalse(timing.neighboring_comparisons(phases)[0]['usable'])


    def test_three_pair_schedule_accounts_for_all_reserved_time(self):
        configs = ['wgc-auto', 'native-record', 'native-record', 'wgc-auto', 'wgc-auto', 'native-record']
        estimate = timing.estimate(configs, 30, 60, 15)
        self.assertEqual(estimate['expected_total_seconds'], 703)
        self.assertEqual(timing.clock(estimate['expected_total_seconds']), '11:43')
        self.assertEqual(estimate['expected_total_seconds'], 15+60+5+10+sum(timing.phase_seconds(c,30) for _,c in timing.phase_plan(configs)))

    def test_paired_interval_reports_uncertainty_and_rejects_failed_pair(self):
        comparisons = []
        for difference in [8, 9, 7]:
            comparisons.extend([dict(config='wgc-auto', usable=True, fps_cost_percent=30),
                                dict(config='native-record', usable=True, fps_cost_percent=30-difference)])
        result = timing.paired_costs(comparisons)
        self.assertEqual(result['pairs'], 3)
        self.assertEqual(result['mean_gain_percentage_points'], 8)
        self.assertLess(result['exploratory_95_percent_interval'][0], 8)
        comparisons[1]['usable'] = False
        self.assertNotIn('exploratory_95_percent_interval', timing.paired_costs(comparisons))

    def test_application_pairs_and_recovery_are_not_confused_with_prototype_names(self):
        comparisons = []
        for index in range(3):
            comparisons.extend([dict(config='app-ffmpeg-nvenc-fixed', usable=True, fps_cost_percent=15),
                                dict(config='app-native-nvenc-phase', usable=True, fps_cost_percent=5)])
        self.assertEqual(timing.paired_costs(comparisons)['pairs'], 3)
        phases = [dict(phase='A1', config=None, frames=dict(fps=400)),
                  dict(phase='B1', config='app-ffmpeg-nvenc-fixed', frames=dict(fps=340),
                       recorder_verified=True, recorder_exit_code=0, recorder=dict(recovery_sessions=1)),
                  dict(phase='A2', config=None, frames=dict(fps=400))]
        self.assertFalse(timing.neighboring_comparisons(phases)[0]['usable'])
