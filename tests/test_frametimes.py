import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('frametimes', Path(__file__).parents[1]/'scripts/bench/compare-frametimes.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class FrameTimes(unittest.TestCase):
    def test_spikes_variance_p95_and_gpu_wait(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d)/'spikes.csv'
            f.write_text('Application,FrameTime,GPUWait\n' + 'game.exe,4,0.5\n'*4900
                         + 'game.exe,20,1.5\n'*100)
            r = m.summarize(f, trim=0)
            self.assertEqual(r['p95_ms'], 4)
            self.assertEqual(r['p99_ms'], 20)
            self.assertEqual(r['spikes_over_2x_median'], 100)
            self.assertEqual(r['spikes_over_16_67ms'], 100)
            self.assertAlmostEqual(r['variance_ms2'], 5.0176)
            self.assertAlmostEqual(r['optional_frame_metrics']['gpu_wait_ms']['mean'], .52)

    def test_mismatched_identity_or_interval_rejects_comparison(self):
        a = dict(fps=250, cap_limited=False, application='game.exe', process_id='1',
                 selected_swapchain='main', interval_column='FrameTime')
        for field, value in [('process_id','2'), ('selected_swapchain','menu'),
                             ('application','other.exe'), ('interval_column','MsBetweenPresents')]:
            b = {**a, field:value}
            result = m.compare(a, b, a)
            self.assertFalse(result['valid_comparison'])
            self.assertTrue(any(field in reason for reason in result['invalid_reasons']))

    def test_optional_gpu_metrics_are_reported_as_milliseconds(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d)/'metrics.csv'
            f.write_text('Application,FrameTime,MsGPUTime,MsCPUBusy,MsCPUWait\n' + 'game.exe,4,2.5,1.1,0.4\n'*5000)
            r = m.summarize(f, trim=5)
            self.assertAlmostEqual(r['optional_frame_metrics']['gpu_time_ms']['mean'], 2.5)
            self.assertAlmostEqual(r['optional_frame_metrics']['cpu_busy_ms']['mean'], 1.1)
    def test_dominant_swapchain_and_application_are_not_mixed(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d)/'frames.csv'
            f.write_text('Application,ProcessID,SwapChainAddress,MsBetweenPresents\n'
                         + 'game.exe,1,main,4\n'*5000 + 'game.exe,1,menu,100\n'*500
                         + 'other.exe,2,x,1\n'*6000)
            r = m.summarize(f, 'game.exe', trim=5, cap=360)
            self.assertAlmostEqual(r['fps'], 250)
            self.assertAlmostEqual(r['low1_fps'], 250)
            self.assertEqual(r['selected_swapchain'], 'main')
            self.assertFalse(r['cap_limited'])

    def test_invalid_short_and_unknown_csv_refused(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d)/'bad.csv'
            for text in ['wrong\n4\n', 'FrameTime\nnan\n-1\n0\n', 'FrameTime\n4\n']:
                f.write_text(text)
                with self.assertRaises(ValueError):
                    m.summarize(f)

    def test_cap_and_drift_invalidate_overhead_claim(self):
        def row(fps, capped=False):
            return dict(fps=fps, cap_limited=capped)
        self.assertFalse(m.compare(row(350, True), row(300), row(350, True))['valid_comparison'])
        self.assertFalse(m.compare(row(200), row(190), row(230))['valid_comparison'])
        r = m.compare(row(200), row(180), row(200))
        self.assertTrue(r['valid_comparison'])
        self.assertAlmostEqual(r['overhead_percent'], 10)


if __name__ == '__main__':
    unittest.main()
