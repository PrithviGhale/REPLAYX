"""Read-only stage audit of a completed, frame-ID app motion capture.
Writes evidence beside the trace; it never captures, launches or repairs video.
"""
import argparse, bisect, collections, json, statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def rows(path): return [json.loads(x) for x in path.read_text(encoding="utf-8-sig").splitlines() if x.strip()]
def stats(values):
 values=sorted(values)
 return {"n":len(values),"min":values[0],"median":statistics.median(values),"p99":values[min(len(values)-1,int(.99*len(values)))],"max":values[-1]} if values else None
def audit(folder):
 folder=folder.resolve();folder.relative_to(ROOT/"test-output")
 events=rows(folder/"trace.jsonl");source=rows(folder/"source.jsonl")
 caps=[e for e in events if e["stage"]=="capture_dequeued"]
 ids={e["capture"]:e["content_id"] for e in events if e["stage"]=="captured_content"}
 selected=[e for e in events if e["stage"]=="selected"]
 shown=[ids[e["capture"]] for e in selected];captured=set(ids.values())
 lost=sorted((captured & set(range(shown[0],shown[-1]+1)))-set(shown))
 submitted={e["id"]:e for e in source if e["stage"]=="source_present"}
 presented={e["present_count"] for e in source if e["stage"]=="presentation_stats" and e["sync_qpc_hns"]>0}
 missing=sorted(set(range(shown[0],shown[-1]+1))-captured)
 changes=[c for i,c in enumerate(caps) if i==0 or ids[c["capture"]]!=ids[caps[i-1]["capture"]]]
 first={ids[c["capture"]]:c for c in changes}; times=[s["grid_hns"] for s in selected]
 details=[]
 for uid in lost:
  c=first[uid];ci=caps.index(c);slot=bisect.bisect_right(times,c["capture_hns"])-1
  source_row=submitted.get(uid);prev=submitted.get(uid-1)
  nxt=first.get(uid+1)
  details.append({"id":uid,"first_capture":c,"previous_capture":caps[max(0,ci-1)],"next_source_capture":nxt,
   "captured_content_lifetime_ms":(nxt["capture_hns"]-c["capture_hns"])/10000 if nxt else None,
   "source_submit_interval_ms":(source_row["submit_hns"]-prev["submit_hns"])/10000 if source_row and prev else None,
   "slots_near_arrival":[dict(s,content_id=ids[s["capture"]]) for s in selected[max(0,slot-2):slot+3]]})
 hint_fp=sum(c["changed"] and ids[c["capture"]]==ids[caps[i-1]["capture"]] for i,c in enumerate(caps) if i)
 hint_fn=sum(not c["changed"] and ids[c["capture"]]!=ids[caps[i-1]["capture"]] for i,c in enumerate(caps) if i)
 submits={e["output"]:e for e in events if e["stage"]=="encoder_submit"}
 outputs=[e for e in events if e["stage"]=="encoder_output"]
 encoded_pts=[e["pts"] for e in outputs]
 timings=[e for e in events if e["stage"]=="encoder_stage_times"]
 wall={name:stats([(e[end]-e[begin])/10000 for e in timings]) for name,begin,end in [("input_credit_wait_ms","begin_hns","credit_hns"),("process_input_ms","process_input_started_hns","process_input_finished_hns"),("collect_after_input_ms","process_input_finished_hns","collect_finished_hns")]}
 for name,begin,end in [("process_output_ms","process_output_started_hns","process_output_finished_hns"),("transport_write_ms","transport_write_started_hns","transport_write_finished_hns")]:
  wall[name]=stats([(e[end]-e[begin])/10000 for e in outputs if e.get(begin) is not None])
 wall["flush_ms"]=stats([(e["end_hns"]-e["begin_hns"])/10000 for e in events if e["stage"]=="encoder_flush"])
 result={"encoder_stage_wall_times":wall,"captures":len(caps),"slots":len(selected),"capture_interval_ms":stats([(b["capture_hns"]-a["capture_hns"])/10000 for a,b in zip(caps,caps[1:])]),
  "arrival_events":sum(e["stage"]=="frame_arrived" for e in events),"capture_dequeue_minus_timestamp_ms":stats([(c["qpc_hns"]-c["capture_hns"])/10000 for c in caps]),
  "changed_content_interval_ms":stats([(b["capture_hns"]-a["capture_hns"])/10000 for a,b in zip(changes,changes[1:])]),
  "source_submit_interval_ms":stats([(b["submit_hns"]-a["submit_hns"])/10000 for a,b in zip(submitted.values(),list(submitted.values())[1:])]),
  "hint_false_positive":hint_fp,"hint_false_negative":hint_fn,"lost_available_ids":lost,"lost_details":details,
  "never_captured_ids":[{"id":uid,"source_submitted":uid in submitted,"presentation_confirmed":submitted.get(uid,{}).get("present_count") in presented} for uid in missing],
  "encode_submit_to_output_ms":stats([(e["qpc_hns"]-submits[e["output"]]["qpc_hns"])/10000 for e in outputs]),
  "encoded_pts_strictly_increasing":all(b>a for a,b in zip(encoded_pts,encoded_pts[1:])),
  "surface_lease_violations":sum(e["stage"]=="surface_available" and e["leases"]!=0 for e in events),
  "limits":"Dirty hints concern the dominant test region only; other desktop regions can legitimately change. Negative dequeue/timestamp differences are retained, not clamped. An upstream missing ID is not proof Windows displayed it unless DXGI statistics confirm it."}
 (folder/"reliability-audit.json").write_text(json.dumps(result,indent=2))
 print(json.dumps({k:v for k,v in result.items() if k!="lost_details"},indent=2));return result
if __name__=="__main__":
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument("folder",type=Path);audit(ap.parse_args().folder)
