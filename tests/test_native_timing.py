import importlib.util,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parents[1]/'scripts/bench'))
from native_timing import cfr,phase_boundaries,source_spans,transitions,HNS
class NativeTiming(unittest.TestCase):
 def test_boundary_partition_includes_every_bruteforce_outcome(self):
  times=[0,61,100,150,200,257,300,400,500];ids=list(range(len(times)))
  got={tuple(cfr(times,ids,p,4,100000)) for p in phase_boundaries(times,0,4,100000)}
  brute={tuple(cfr(times,ids,p,4,100000)) for p in range(100)}
  self.assertEqual(got,brute)
 def test_vfr_has_no_phase_tuning_input_and_preserves_gaps(self):
  for offset in [0,1,83333,166666,123456789123]:
   times=[offset+t for t in [0,166667,333333,1_500_000,1_666_667]]
   self.assertEqual(source_spans(times),list(range(5)))
 def test_transport_ties_keep_latest(self):
  self.assertEqual(source_spans([0,0,1,166667]),[2,3])
 def test_capture_metadata_cannot_identify_the_source_frame_phase(self):
  times=list(range(0,2*HNS,40000)) # Physical 250 Hz capture clock.
  # Same capture timestamps, multiple plausible 60 Hz producer phases.
  # No fixed output origin can make all source timelines one-to-one.
  candidates=set(range(0,166667,500))
  for source_phase in range(0,166667,500):
   ids=[(t-source_phase)*60//HNS for t in times]
   candidates={p for p in candidates if all(b==a+1 for a,b in zip(cfr(times,ids,p,100),cfr(times,ids,p,100)[1:]))}
   if not candidates:break
  self.assertFalse(candidates)
 def test_under_rate_source_correctly_requires_repetition(self):
  times=[n*HNS//30 for n in range(30)]
  self.assertEqual(transitions(cfr(times,list(range(30)),0,60))['repeats'],30)
