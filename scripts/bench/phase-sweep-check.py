"""Offline exhaustive timing-regime sweep for a stable rational 60 FPS source.
Includes integer equality boundaries and their neighbors, every refresh tick,
and redundant captures. No GPU/device is opened. Separate trace replay tests
cover real irregular delivery, gaps and watermark calls.
"""
import argparse,hashlib,json,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HNS=10_000_000
PERIOD=(HNS+59)//60

def phases(hz,seconds):
 # Source identity floor((capture_time-phase)*60/HNS) changes only at these
 # integer phase boundaries. Test both sides and all interior regimes.
 edges={0,PERIOD-1}
 for k in range(hz*seconds):
  t=k*HNS//hz
  n=t*60//HNS
  for tick in [n,n+1]:
   boundary=t-(tick*HNS+59)//60
   for delta in [-1,0,1]:
    p=boundary+delta
    if 0<=p<PERIOD:edges.add(p)
 ordered=sorted(edges)
 return sorted(edges|{(a+b)//2 for a,b in zip(ordered,ordered[1:])})
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument("--out",required=True,type=Path);a=ap.parse_args()
 out=a.out.resolve();out.relative_to(ROOT/"test-output");out.mkdir(exist_ok=False,parents=True)
 proto=ROOT/"src-tauri/target/release/replayx-native-proto.exe";before=hashlib.sha256(proto.read_bytes()).hexdigest();results=[]
 for hz in [120,144,240,250]:
  points=phases(hz,6);failed=[];total_slots=0
  for phase in points:
   caps=[dict(time_hns=3_000_000_000_000+k*HNS//hz,content_id=((k*HNS//hz-phase)*60)//HNS) for k in range(hz*6) if k*HNS//hz>=phase]
   change=[];last=None
   for c in caps:
    if c["content_id"]!=last:change.append(c)
    last=c["content_id"]
   start=change[9]["time_hns"]
   timeline=dict(calibration=[dict(time_hns=c["time_hns"],changed=True) for c in change[:9]],captures=[c for c in caps if c["time_hns"]>=start])
   path=out/"timeline.json";path.write_text(json.dumps(timeline))
   r=json.loads(subprocess.check_output([str(proto),"--pacing-replay",str(path)],timeout=15));chosen=[timeline["captures"][i]["content_id"] for i in r["picks"]];middle=chosen[60:-60];bad=[i+60 for i,(x,y) in enumerate(zip(middle,middle[1:])) if y!=x+1];total_slots+=len(chosen)
   if bad:
    failed.append(dict(phase_hns=phase,nonconsecutive_slots=bad));(out/f"failure-{hz}-{phase}.json").write_text(json.dumps(dict(timeline=timeline,result=r)))
  row=dict(refresh_hz=hz,phase_regimes_and_boundaries=len(points),output_slots_checked=total_slots,failures=failed);results.append(row);print(json.dumps(row),flush=True)
 result=dict(binary_sha256=before,results=results,pass_all=not any(r["failures"] for r in results),scope="Rational stable 60 FPS metadata with calibration; middle interval excludes one-second start/end boundaries. Every output pick is retained in failure files. Not a physical refresh-mode or gameplay test.")
 assert before==hashlib.sha256(proto.read_bytes()).hexdigest(),"Binary changed during sweep"
 (out/"summary.json").write_text(json.dumps(result,indent=2))
 if not result["pass_all"]:raise SystemExit(1)
if __name__=="__main__":main()
