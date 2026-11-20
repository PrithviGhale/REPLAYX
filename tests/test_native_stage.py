import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('stage',Path(__file__).parents[1]/'scripts/bench/native-stage-audit.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class GridOracle(unittest.TestCase):
 def test_zero_order_hold_and_legitimate_repetition(self):
  self.assertEqual(m.schedule([0,333333,666666],[0,1,2],0,range(5)),[0,0,1,1,2])
 def test_dropped_capture_still_available_at_future_grid(self):
  self.assertEqual(m.schedule([0,50000,400000],[0,1,2],0,range(4)),[0,1,1,2])
 def test_static_source_is_allowed(self):
  self.assertEqual(m.schedule([0,100000,200000],[42,42,42],0,range(3)),[42]*3)

 def test_sixty_fps_motion_requires_consecutive_source_ids(self):
  self.assertTrue(m.motion_one_to_one([42,43,44,45],60))
 def test_capture_oracle_alone_cannot_pass_repeat_skip_pairs(self):
  self.assertFalse(m.motion_one_to_one([42,42,44,45],60))
 def test_static_and_under_rate_inputs_do_not_require_unique_pictures(self):
  self.assertIsNone(m.motion_one_to_one([42,42,42],60,True))
  self.assertIsNone(m.motion_one_to_one([0,0,1,1],30))
