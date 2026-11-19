"""Offline integrity audit. Never starts a recorder, game or audio device."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]

def load_module(name, path):
    spec=importlib.util.spec_from_file_location(name,path)
    mod=importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
    return mod

def run(command):
    result=subprocess.run(list(map(str,command)),capture_output=True,timeout=180)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace')[-3000:])
    return result.stdout

def audit_video(folder, ffmpeg, ffprobe, out):
    video=folder/'native.h264'
    report=json.loads((folder/'probe.json').read_text(encoding='utf-8-sig'))
    probe=json.loads(run([ffprobe,'-v','error','-select_streams','v:0','-show_streams','-show_frames',
                         '-show_entries','frame=key_frame,pict_type,best_effort_timestamp_time,pkt_duration_time:stream=codec_name,width,height,r_frame_rate,avg_frame_rate,time_base',
                         '-of','json',video]))
    frames=probe['frames']; keys=[i for i,f in enumerate(frames) if f['key_frame']]
    # Full-resolution decoded hashes catch identical output pictures, not just timestamps.
    hashes=run([ffmpeg,'-v','error','-xerror','-threads','2','-i',video,'-map','0:v:0','-f','framemd5','-']).decode()
    rows=[line for line in hashes.splitlines() if line and not line.startswith('#')]
    digests=[line.split(',')[-1].strip() for line in rows]
    duplicates=[i for i in range(1,len(digests)) if digests[i]==digests[i-1]]
    # Small luma frames permit streaming motion statistics without storing a recording in RAM.
    command=[ffmpeg,'-v','error','-xerror','-threads','2','-i',video,'-vf','scale=160:90,format=gray','-f','rawvideo','-']
    diffs=[]; previous=None; count=0
    with subprocess.Popen(list(map(str,command)),stdout=subprocess.PIPE,stderr=subprocess.DEVNULL) as process:
        while True:
            frame=process.stdout.read(14400)
            if not frame: break
            if len(frame)!=14400: raise RuntimeError('Truncated analysis frame')
            if previous is not None:
                diffs.append(sum(abs(a-b) for a,b in zip(frame,previous))/len(frame))
            previous=frame; count+=1
        if process.wait(timeout=30): raise RuntimeError('Motion decode failed')
    checks=dict(full_decode=True,frame_count_matches=len(frames)==report['frames_out']==len(digests)==count,
                expected_keyframes=keys==list(range(0,len(frames),120)),
                dimensions=probe['streams'][0]['width']==1920 and probe['streams'][0]['height']==1080,
                encoder_complete=report['success'] and report['encoded']==report['frames_out'],
                timestamps_ordered_report=report['output_timestamps_increasing'],
                released_surfaces=report['live_input_samples']==0,
                bytes_match=video.stat().st_size==report['bytes_written'])
    headers=subprocess.run(list(map(str,[ffmpeg,'-v','verbose','-i',video,'-c:v','copy','-bsf:v','trace_headers','-frames:v','1','-f','null','-'])),capture_output=True,timeout=30,check=True).stderr.decode(errors='replace')
    fields={}
    for name in ('num_units_in_tick','time_scale','fixed_frame_rate_flag','frame_mbs_only_flag'):
        match=re.search(r'\b'+name+r'\s+[01]+\s+=\s+(\d+)',headers)
        if match: fields[name]=int(match.group(1))
    checks['sps_60fps']=(fields.get('time_scale')==120 and fields.get('num_units_in_tick')==1 and fields.get('fixed_frame_rate_flag')==1 and fields.get('frame_mbs_only_flag')==1)
    # Four chronological thumbnails; these are inspected separately by the reviewer.
    sheet=out/(folder.name+'.jpg')
    run([ffmpeg,'-v','error','-i',video,'-vf',r'select=eq(n\,0)+eq(n\,1080)+eq(n\,2160)+eq(n\,3240),scale=640:360,tile=2x2','-frames:v','1','-y',sheet])
    with video.open('rb') as source:
        digest=hashlib.file_digest(source,'sha256').hexdigest()
    return dict(phase=folder.name,sha256=digest,bytes=video.stat().st_size,
                checks=checks,sps_timing=fields,frames=len(frames),keyframes=keys,stream=probe['streams'][0],
                duration_at_60=len(frames)/60,wall_seconds=report['seconds'],
                adjacent_identical_decoded_frames=duplicates,
                motion_luma_mad=dict(mean=statistics.mean(diffs),minimum=min(diffs),maximum=max(diffs),
                                     p99=sorted(diffs)[int(.99*(len(diffs)-1))],below_point_one=sum(d<.1 for d in diffs)),
                timestamps_limitation='Raw H264 has no container PTS; only encoder report verifies submitted timestamp order. Frame delivery jitter and QPC continuity cannot be reconstructed.',
                surface_reuse_waits=report['surface_reuse_waits'],live_input_samples=report['live_input_samples'],
                diagnostics=report,contact_sheet=str(sheet))

def main():
    parser=argparse.ArgumentParser(); parser.add_argument('session',type=Path); parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args(); args.out.mkdir(parents=True,exist_ok=True)
    lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
    runtime=ROOT/'tools'/lock['runtime_dir']; summary=json.loads((args.session/'summary.json').read_text())
    timing=load_module('timing',ROOT/'scripts/bench/session_timing.py')
    frametimes=load_module('frames',ROOT/'scripts/bench/compare-frametimes.py')
    rows=[]
    for phase in summary['phases']:
        row=dict(phase)
        row['frames']=frametimes.summarize(args.session/(row['phase']+'.csv'),process='cs2.exe',cap=0)
        rows.append(row)
    comparisons=timing.neighboring_comparisons(rows)
    groups={}
    for config in (None,'wgc-auto','native-record'):
        phases=[p for p in rows if p['config']==config]
        groups[config or 'OFF']=dict(phases=len(phases),**{k:statistics.mean(p['frames'][k] for p in phases) for k in ('fps','low1_fps','p99_ms')},
             recorder_cpu_percent=statistics.mean(sum(v for n,v in p['process_cpu_percent'].items() if n.startswith('replayx') or n=='ffmpeg.exe') for p in phases))
    result=dict(source=str(args.session),errors=summary['errors'],timing=summary['timing'],groups=groups,
                neighboring_comparisons=comparisons,paired_costs=timing.paired_costs(comparisons),
                excluded_baseline='A7 is incomplete and excluded even though its partial CSV exists.',
                memory_limitation='No resident/private memory time series was collected; no measured memory-growth conclusion is possible.',videos=[])
    for phase in rows:
        if phase['config']=='native-record':
            print('Auditing',phase['phase'],flush=True)
            result['videos'].append(audit_video(args.session/phase['phase'],runtime/'ffmpeg.exe',runtime/'ffprobe.exe',args.out))
    (args.out/'audit.json').write_text(json.dumps(result,indent=2))
    print(json.dumps(dict(groups=groups,paired_costs=result['paired_costs'],videos=[{k:v[k] for k in ('phase','checks','frames','adjacent_identical_decoded_frames','motion_luma_mad')} for v in result['videos']]),indent=2))
    if not all(all(v['checks'].values()) for v in result['videos']): raise SystemExit(1)
if __name__=='__main__': main()
