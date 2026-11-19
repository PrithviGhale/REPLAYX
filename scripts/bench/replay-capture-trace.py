"""Exact CPU replay of reported hints, calibration and WGC watermark decisions.
No device is opened. For older traces watermark calls are reconstructed from
selected slots whose trigger equals their pending capture (push cannot do this).
"""
import argparse,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]

def load(folder):
 events=[json.loads(s) for s in (folder/'trace.jsonl').read_text(encoding='utf-8-sig').splitlines() if s.strip()]
 caps=sorted((e for e in events if e['stage']=='capture_dequeued'),key=lambda e:e['capture'])
 ids={e['capture']:e['content_id'] for e in events if e['stage']=='captured_content'}
 selected=[e for e in events if e['stage']=='selected']
 calibration=[{'time_hns':e['capture_hns'],'changed':e['changed']} for e in events if e['stage']=='phase_calibration']
 advances=[{'before_capture':e['before_capture'],'end_hns':e['end_hns'],'idle':e.get('idle',False)} for e in events if e['stage']=='grid_watermark']
 inferred=not advances
 if inferred:
  for e in selected:
   if e['capture']==e['trigger_capture']:
    a={'before_capture':e['trigger_capture']+1,'end_hns':e['deadline_hns']+1}
    if advances and advances[-1]['before_capture']==a['before_capture']:advances[-1]=a
    else:advances.append(a)
 timeline={'captures':[{'time_hns':c['capture_hns'],'content_id':ids[c['capture']],'changed':c['changed'],'phase_changed':c.get('phase_changed',c['changed'])} for c in caps],
           'calibration':calibration,'advances':advances,'reseeds':[e for e in events if e['stage']=='phase_reseed']}
 starts=[e for e in events if e['stage']=='phase_start']
 if starts:timeline['phase_start']=starts[-1]
 return timeline,selected,ids,inferred

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('trace',type=Path);ap.add_argument('--out',required=True,type=Path);a=ap.parse_args()
 folder=a.trace.resolve();out=a.out.resolve()
 if not out.is_relative_to(ROOT/'test-output'):raise ValueError('Output must be within this worktree/test-output')
 out.mkdir(exist_ok=False,parents=True);tl,selected,ids,inferred=load(folder);path=out/'timeline.json';path.write_text(json.dumps(tl))
 proto=ROOT/'src-tauri/target/release/replayx-native-proto.exe';r=json.loads(subprocess.check_output([str(proto),'--pacing-replay',str(path),'--phase-hints','reported'],timeout=15));expected=[e['capture'] for e in selected];matches=r['picks']==expected
 differences=[i for i,(x,y) in enumerate(zip(r['picks'],expected)) if x!=y];decoded=[ids[i] for i in r['picks']];lost=sorted({i for i in ids.values() if decoded[0]<=i<=decoded[-1]}-set(decoded));result={'matches_recorded_picks':matches,'first_difference':differences[0] if differences else None,'replayed_slots':len(decoded),'recorded_slots':len(expected),'lost_available_source_ids':lost,'repeated_source_ids':sum(x==y for x,y in zip(decoded,decoded[1:])),'inferred_watermarks':inferred,'calibration_observations':len(tl['calibration'])};(out/'result.json').write_text(json.dumps(result,indent=2));(out/'replayed.json').write_text(json.dumps(r));print(json.dumps(result,indent=2))
 if not matches:raise SystemExit(1)
if __name__=='__main__':main()
