"""Independent offline audit: GPU frame IDs -> grid oracle -> decoded pictures.
No capture, device activation or audio playback. Missing evidence fails closed.
"""
import argparse,bisect,collections,json,subprocess
from pathlib import Path

def schedule(times,ids,origin,outputs,fps=60):
    # Independent bisect oracle. No imports from the Rust scheduler.
    return [ids[bisect.bisect_right(times,origin+n*10_000_000//fps)-1] for n in outputs]

def motion_one_to_one(ids,fps,static=False):
    # Stronger than a selector oracle: a known moving 60 FPS source must retain
    # every consecutive source identity. Matching the capture oracle alone can
    # still hide repeat/skip pairs caused by display/output-grid phase.
    if fps != 60 or static:
        return None
    return len(ids)>1 and all(b==a+1 for a,b in zip(ids,ids[1:]))

def load_lines(p):
    return [json.loads(line) for line in p.read_text(encoding='utf-8-sig').splitlines() if line.strip()]

def audit(folder):
    root=Path(__file__).resolve().parents[2];folder=Path(folder)
    events=load_lines(folder/'trace.jsonl');native=json.loads((folder/'native.json').read_text());g=json.loads((folder/'fixture.geometry.json').read_text())
    captures=sorted((e for e in events if e['stage']=='capture_dequeued'),key=lambda e:e['capture']);times=[e['capture_hns'] for e in captures]
    content={e['capture']:e for e in events if e['stage']=='captured_content'}
    converted=[e for e in events if e['stage']=='converted_content'];selected=sorted((e for e in events if e['stage']=='selected'),key=lambda e:e['output'])
    if len(captures)!=len(content) or native['probe_dropped']:raise ValueError('Incomplete source-content probes')
    capture_ids=[content[e['capture']]['content_id'] for e in captures]
    selected_ids=[content[e['capture']]['content_id'] for e in selected]
    expected=schedule(times,capture_ids,times[0],[e['output'] for e in selected])
    source_conversion_errors=[{'capture':e['capture'],'expected':content[e['capture']]['content_id'],'actual':e['content_id']} for e in converted if content[e['capture']]['content_id']!=e['content_id']]
    grid_errors=[{'output':s['output'],'chosen':a,'expected':b} for s,a,b in zip(selected,selected_ids,expected) if a!=b]
    timing=[(e['capture_hns']-times[0])/10_000_000 for e in captures]
    lock=json.loads((root/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'));bins=root/'tools'/lock['runtime_dir']
    def run(args):return subprocess.run(list(map(str,args)),capture_output=True,check=True,timeout=120).stdout
    clip=folder/'capture.ts'
    if not clip.exists():clip=folder/'capture.mp4'
    frames=json.loads(run([bins/'ffprobe.exe','-v','error','-select_streams','v:0','-show_frames','-show_entries','frame=best_effort_timestamp_time,key_frame,pict_type','-of','json',clip]))['frames']
    raw=run([bins/'ffmpeg.exe','-v','error','-xerror','-threads','2','-filter_threads','1','-i',clip,'-vf',f"crop={g['width']}:{g['height']}:{g['left']}:{g['top']},scale=320:180:flags=area,format=gray",'-fps_mode','passthrough','-f','rawvideo','-'])
    decoded=[]
    for off in range(0,len(raw),57600):
        pix=raw[off:off+57600]
        if len(pix)!=57600:raise ValueError('Truncated decoded picture')
        decoded.append(sum(1<<i for i in range(24) if sum(pix[(9+dy)*320+10+i*12+dx] for dy in range(4) for dx in range(4))/16>128))
    encoded_errors=[{'output':s['output'],'selected':uid,'decoded':decoded[s['output']]} for s,uid in zip(selected,selected_ids) if decoded[s['output']]!=uid]
    all_frames_accounted=len(decoded)==len(selected)==len(frames)==native['frames_out']
    pts=[float(f['best_effort_timestamp_time']) for f in frames];dt=[(b-a)*1000 for a,b in zip(pts,pts[1:])]
    unavailable=0;source_info={};presented_missing=[]
    if (folder/'source.jsonl').exists():
        source=load_lines(folder/'source.jsonl');metadata=source[0];submitted={r['present_count']:r for r in source if r['stage']=='source_present'}
        visible=[r for r in source if r['stage']=='presentation_stats' and r['present_count'] in submitted and r['sync_qpc_hns']>0]
        confirmed={submitted[r['present_count']]['id'] for r in visible};known={r['id'] for r in submitted.values()}
        unavailable=sum(uid not in known for uid in capture_ids)
        presented_missing=sorted(uid for uid in confirmed if capture_ids[0]<uid<capture_ids[-1] and uid not in set(capture_ids))
        src=list(submitted.values());intervals=[(b['submit_hns']-a['submit_hns'])/10000 for a,b in zip(src,src[1:])]
        source_info={'fps_requested':metadata['fps'],'static':metadata['static'],'submitted':len(src),'dxgi_presented_ids':len(confirmed),'dxgi_presented_but_not_captured_ids':presented_missing,'unknown_captured_ids':unavailable,'submit_interval_ms_min':min(intervals),'submit_interval_ms_max':max(intervals),'graceful_source_end':source[-1]['stage']=='source_end'}
    increments=collections.Counter(b-a for a,b in zip(decoded,decoded[1:]));oracle_inc=collections.Counter(b-a for a,b in zip(expected,expected[1:]));capture_dt=[(b-a)/10000 for a,b in zip(times,times[1:])]
    keys=[i for i,f in enumerate(frames) if f['key_frame']]
    lease_violations=[e for e in events if e['stage']=='surface_available' and e['leases']!=0]
    result={'clip':str(clip),'native_pacing':native.get('frame_pacing','legacy-ffmpeg'),'captures':len(captures),'capture_rate':len(captures)/native['seconds'],'capture_gap_ms_min':min(capture_dt),'capture_gap_ms_max':max(capture_dt),'capture_arrival_events':sum(e['stage']=='frame_arrived' for e in events),'arrival_to_dequeue_max_ms':max((e['qpc_hns']-e['capture_hns'])/10000 for e in captures),'frames':len(decoded),'all_frames_accounted':all_frames_accounted,'source_conversion_errors':source_conversion_errors,'grid_choice_errors':grid_errors,'selected_decode_errors':encoded_errors,'repeated_pictures':increments[0],'expected_repeated_pictures':oracle_inc[0],'decoded_increments':dict(sorted(increments.items())),'expected_increments':dict(sorted(oracle_inc.items())),'missing_source_ticks':sum(max(0,b-a-1) for a,b in zip(capture_ids,capture_ids[1:])),'pts_monotonic':all(x>0 for x in dt),'frame_interval_ms_min':min(dt),'frame_interval_ms_max':max(dt),'keyframes':keys,'keyframes_correct':keys==list(range(0,len(frames),120)),'b_frames':sum(f['pict_type']=='B' for f in frames),'surface_lease_violations':len(lease_violations),'live_input_samples':native['live_input_samples'],'probe_dropped':native['probe_dropped'],'source':source_info,'native_cpu_one_core_percent':native.get('cpu_percent_of_one_core'),'encode_latency':native.get('encode_latency')}
    result['source60_one_to_one']=motion_one_to_one(decoded,source_info.get('fps_requested'),source_info.get('static',False))
    result['correct']=all_frames_accounted and not(source_conversion_errors or grid_errors or encoded_errors or lease_violations or unavailable) and result['pts_monotonic'] and result['keyframes_correct'] and not result['b_frames'] and result['live_input_samples']==0 and result['source60_one_to_one'] is not False
    (folder/'audit.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    (folder/'decoded-ids.json').write_text(json.dumps(decoded),encoding='utf-8')
    return result

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('folder',type=Path);ap.add_argument('--allow-legacy-failure',action='store_true');a=ap.parse_args();r=audit(a.folder)
    print(json.dumps({k:r[k] for k in ['correct','captures','frames','repeated_pictures','expected_repeated_pictures','capture_gap_ms_max','all_frames_accounted']},indent=2));print('Stage mismatches: conversion',len(r['source_conversion_errors']),'grid',len(r['grid_choice_errors']),'decode',len(r['selected_decode_errors']))
    if not r['correct'] and not a.allow_legacy_failure:raise SystemExit(1)
if __name__=='__main__':main()
