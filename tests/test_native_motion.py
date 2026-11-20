import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location("native_motion",Path(__file__).parents[1]/"scripts/bench/native-motion-check.py")
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class MotionAudit(unittest.TestCase):
 def test_static_desktop_is_not_mistaken_for_smooth_capture(self):
  with self.assertRaises(ValueError):m.motion_metrics([0]*100,[[n/60,n] for n in range(100)])
 def test_counts_repeats_skips_and_corruption(self):
  d=m.motion_metrics(list(range(15))+[14,16,1000],[[n/60,n] for n in range(20)])
  self.assertEqual(d['repeated_pictures'],1);self.assertEqual(d['invalid_source_ticks'],1);self.assertGreater(d['skipped_fixture_ticks'],0)
 def test_unique_content_can_be_smooth(self):
  d=m.motion_metrics(list(range(60)),[[n/60,n] for n in range(60)])
  self.assertEqual((d['repeated_pictures'],d['skipped_fixture_ticks'],d['backwards_steps'],d['fixture_missed_ticks']),(0,0,0,0))
