import importlib.util
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/bench/frame-pacing.py'
SPEC = importlib.util.spec_from_file_location('frame_pacing', SCRIPT)
PACER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACER)


class MotionWindows(unittest.TestCase):
    def test_fresh_motion_and_static_holds_are_counted_separately(self):
        pts = [4 + i/8 for i in range(9)]
        differences = [0, 0, 0, 0, 0, 1.3, 1.7, 4.4]
        static, motion = PACER.analyze_motion_windows(pts, differences, [(0, .5), (.5, 1)])
        self.assertEqual(static['decoded_frames'], 4)
        self.assertEqual(static['near_identical_pairs'], 3)
        self.assertEqual(static['longest_near_identical_run_frames'], 4)
        self.assertEqual(motion['compared_pairs'], 3)
        self.assertEqual(motion['near_identical_pairs'], 1)
        self.assertEqual(motion['longest_near_identical_run_frames'], 2)
        self.assertEqual(motion['interval_ms']['max'], 125)

    def test_rejects_ambiguous_alignment_and_bad_windows(self):
        with self.assertRaisesRegex(ValueError, 'counts'):
            PACER.analyze_motion_windows([0, .25, .5], [1], [(0, .5)])
        with self.assertRaisesRegex(ValueError, 'increasing'):
            PACER.analyze_motion_windows([0, .25, .25], [1, 0], [(0, .25)])
        with self.assertRaisesRegex(ValueError, 'outside'):
            PACER.analyze_motion_windows([0, .25, .5], [1, 0], [(0, .75)])
        with self.assertRaisesRegex(ValueError, 'fewer than two'):
            PACER.analyze_motion_windows([0, .25, .5], [1, 0], [(0, .25)])
