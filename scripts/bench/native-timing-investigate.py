"""Offline reconstruction of retained source/capture/encoder/MP4 identities.
Writes only a NEW evidence directory in replayx-perf. No capture or devices.
"""
import argparse,bisect,collections,csv,json,statistics,subprocess
from pathlib import Path
from native_timing import HNS,cfr,phase_boundaries,source_spans,transitions
ROOT=Path(__file__).resolve().parents[2]
def load(p):return [json.loads(l) for l in p.read_text(encoding='utf-8-sig').splitlines() if l.strip()]
def investigate(folder,out,count=480):
    events=load(folder/'trace.jsonl');src=load(folder/'source.jsonl')
    caps=sorted((e for e in events if e['stage']=='capture_dequeued'),key=lambda e:e['capture'])
    content={e['capture']:e['content_id'] for e in events if e['stage']=='captured_content'}
    assert len(caps)==len(content),'Incomplete content probes'
    selected=sorted((e for e in events if e['stage']=='selected'),key=lambda e:e['output'])
    times=[c['capture_hns'] for c in caps];ids=[content[c['capture']] for c in caps];origin=times[0]
    out.mkdir();source={r['id']:r for r in src if r['stage']=='source_present'}
    arrived=[e['qpc_hns'] for e in events if e['stage']=='frame_arrived']
    submits={e['output']:e for e in events if e['stage']=='encoder_submit'}
    outputs={e['output']:e for e in events if e['stage']=='encoder_output'}
    converts={e['capture']:e for e in events if e['stage']=='converted_content'}
    first={}
    for c in caps:first.setdefault(content[c['capture']],c)
    oracle=cfr(times,ids,origin,len(selected));chosen=[content[e['capture']] for e in selected]
    mismatches=[i for i,(a,b) in enumerate(zip(chosen,oracle)) if a!=b]
    rows=[];repeat_leads=[]
    for e in selected:
        n=e['output'];c=caps[e['capture']];uid=content[e['capture']];s=source[uid];grid=origin+n*HNS//60
        arrival=bisect.bisect_right(arrived,c['qpc_hns'])-1
        row={'output':n,'source_id':uid,'source_planned_hns':s['planned_hns'],'source_submit_hns':s['submit_hns'],
             'source_present_return_hns':s['returned_hns'],'capture':e['capture'],'capture_hns':c['capture_hns'],
             'last_notification_before_dequeue_hns':arrived[arrival] if arrival>=0 else None,
             'dequeue_hns':c['qpc_hns'],'grid_hns':grid,'selection_hns':e['qpc_hns'],
             'convert_probe_completed_hns':converts.get(e['capture'],{}).get('qpc_hns'),
             'encoder_submit_hns':submits.get(n,{}).get('qpc_hns'),'encoder_pts_hns':submits.get(n,{}).get('pts'),
             'encoder_output_hns':outputs.get(n,{}).get('qpc_hns'),'encoder_output_pts_hns':outputs.get(n,{}).get('pts'),
             'ideal_cfr_pts_hns':n*HNS//60,'capture_oracle_id':oracle[n]}
        rows.append(row)
        if n and uid==chosen[n-1] and uid+1 in first:repeat_leads.append((first[uid+1]['capture_hns']-grid)/10000)
    # Actual decoded MP4 IDs, independently of stage-selected IDs.
    mp4=[]
    if (folder/'app-report.json').exists():
        d=json.loads((folder/'app-report.json').read_text());lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'));bins=ROOT/'tools'/lock['runtime_dir'];g=src[0]
        for kind in ['recording','replay']:
            clip=d[kind];raw=subprocess.check_output([str(bins/'ffmpeg.exe'),'-v','error','-xerror','-threads','2','-filter_threads','1','-i',clip,'-vf',f"crop=960:540:{g['left']}:{g['top']},scale=320:180:flags=area,format=gray",'-fps_mode','passthrough','-f','rawvideo','-'],timeout=45)
            decoded=[]
            for off in range(0,len(raw),57600):
                b=raw[off:off+57600];decoded.append(sum(1<<i for i in range(24) if sum(b[(9+dy)*320+10+i*12+dx] for dy in range(4) for dx in range(4))/16>128))
            matches=[i for i in range(len(chosen)-len(decoded)+1) if chosen[i:i+len(decoded)]==decoded];assert len(matches)==1,(kind,matches)
            offset=matches[0];f=json.loads(subprocess.check_output([str(bins/'ffprobe.exe'),'-v','error','-show_frames','-show_entries','frame=best_effort_timestamp_time,key_frame','-of','json',clip],timeout=30))['frames']
            for i,(uid,frame) in enumerate(zip(decoded,f)):rows[offset+i][kind+'_decoded_id']=uid;rows[offset+i][kind+'_pts_seconds']=frame['best_effort_timestamp_time']
            mp4.append({'kind':kind,'frames':len(decoded),'identities_match_selected':True,**transitions(decoded)})
    with (out/'pipeline.csv').open('w',newline='') as f:
        writer=csv.DictWriter(f,fieldnames=list(dict.fromkeys(k for r in rows for k in r)));writer.writeheader();writer.writerows(rows)
    phases=[]
    for phase in phase_boundaries(times,origin,count):phases.append({'phase_hns':phase,**transitions(cfr(times,ids,origin+phase,count))})
    (out/'phase-sweep.json').write_text(json.dumps(phases,indent=2))
    timeline={'captures':[{'time_hns':t,'content_id':i,'arrival_hns':c['qpc_hns']} for t,i,c in zip(times,ids,caps)],'end_hns':max(times[-1]+1,origin+len(selected)*HNS//60)}
    (out/'replay-timeline.json').write_text(json.dumps(timeline))
    vfr=[ids[i] for i in source_spans(times)];unique=lambda a:[v for i,v in enumerate(a) if not i or v!=a[i-1]]
    result={'source_qpc_frequency':src[0]['qpc_frequency'],'captures':len(caps),'capture_gap_ms':{'min':min((b-a)/10000 for a,b in zip(times,times[1:])),'max':max((b-a)/10000 for a,b in zip(times,times[1:]))},'captured_grid_mismatches':len(mismatches),'selected':len(chosen),'selected_transitions':transitions(chosen),'mp4':mp4,
      'repeated_slot_next_source_capture_ms':{'min':min(repeat_leads),'median':statistics.median(repeat_leads),'max':max(repeat_leads)} if repeat_leads else None,
      'encoder_rounding_max_us':max(abs(r['encoder_pts_hns']-r['ideal_cfr_pts_hns'])/10 for r in rows),
      'old_fixed_step_drift_ms_per_hour':((3600*60)*166666-3600*HNS)/10000,
      'phase_sweep':{'phase_interval_hns':[0,(HNS+59)//60-1],'tested_boundary_regimes':len(phases),'all_integer_phase_outcomes_covered':True,'best_repeats':min(r['repeats'] for r in phases),'worst_repeats':max(r['repeats'] for r in phases),'pictures_each':count},
      'source_timestamp_policy':{'encoded':len(vfr),'transport_ties_coalesced':len(ids)-len(vfr),'ordered_available_source_ids_retained':unique(vfr)==unique(ids),'unique_transitions':transitions(unique(vfr))},
      'notification_warning':'FrameArrived is a coalescible wake notification, not a per-frame identity. CSV links the latest notice observed before dequeue, not a claimed one-to-one correspondence.',
      'conversion_warning':'Old converted_content time is asynchronous probe completion, not exact GPU conversion execution time.'}
    (out/'summary.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2));return result
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('folder',type=Path);p.add_argument('--name',required=True);a=p.parse_args()
    if not a.name.replace('-','').isalnum():raise ValueError('Invalid output name')
    investigate(a.folder,ROOT/'test-output/native-grid-v2'/a.name)
