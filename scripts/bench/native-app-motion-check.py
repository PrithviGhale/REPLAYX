"""Unattended windowed silent app/MP4/replay test with frame-ID evidence."""
import argparse,json,subprocess,time,shutil
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--name',required=True);ap.add_argument('--capture-interval-us',type=int);ap.add_argument('--source-pause-ms',type=int,default=0);ap.add_argument('--seconds',type=int,default=6);ap.add_argument('--audit-only',action='store_true');ap.add_argument('--capture-vfr',action='store_true');ap.add_argument('--phase-lock',action='store_true');ap.add_argument('--phase-global',action='store_true');ap.add_argument('--maximized',action='store_true');ap.add_argument('--foreground-source',action='store_true');ap.add_argument('--approved-game-cover',action='store_true');ap.add_argument('--noise-fps',type=int,default=0);ap.add_argument('--sample-resources',action='store_true');ap.add_argument('--fps-num',type=int,default=60);ap.add_argument('--fps-den',type=int,default=1);ap.add_argument('--start-delay-ms',type=int,default=0);a=ap.parse_args()
 if a.capture_interval_us is not None and not 0<=a.capture_interval_us<=8333:raise ValueError('Experimental interval must be 0..8333 us')
 if not 0<=a.source_pause_ms<=500:raise ValueError('Bounded source pause required')
 if a.approved_game_cover and not a.foreground_source:raise ValueError('Approved cover requires the foreground test window')
 if a.noise_fps not in [0,120,144,240,250]:raise ValueError('Invalid bounded noise rate')
 if a.capture_vfr and a.phase_lock:raise ValueError('Choose one experimental mode')
 if not a.name.replace('-','').isalnum():raise ValueError('Invalid name')
 out=ROOT/'test-output/native-smoothness'/a.name
 if a.audit_only:return audit(out)
 if not 6<=a.seconds<=24 or not 1<=a.fps_den<=1001 or not 1<=a.fps_num/a.fps_den<=250 or not 0<=a.start_delay_ms<=1000:raise ValueError('Invalid bounded source timing')
 out.mkdir();stop=out/'source.stop'
 src=subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-motion-source.exe'),'--seconds',str(a.seconds+19),'--fps-num',str(a.fps_num),'--fps-den',str(a.fps_den),'--pause-ms',str(a.source_pause_ms),'--log',str(out/'source.jsonl'),'--stop-file',str(stop)]+(['--maximized'] if a.maximized else [])+(['--foreground'] if a.foreground_source else [])+(['--approved-game-cover'] if a.approved_game_cover else []),creationflags=subprocess.CREATE_NO_WINDOW,stderr=(out/'source.log').open('w'))
 noise=None
 try:
  until=time.monotonic()+6
  while time.monotonic()<until:
   if src.poll() is not None:raise RuntimeError('Main source exited: '+(out/'source.log').read_text()[-1500:])
   try:g=json.loads((out/'source.jsonl').read_text().splitlines()[0]);break
   except (OSError,IndexError,json.JSONDecodeError):time.sleep(.02)
  else:raise RuntimeError('Source did not start')
  if a.noise_fps:
   noise=subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-motion-source.exe'),'--seconds',str(a.seconds+19),'--fps',str(a.noise_fps),'--left','1360','--top','700','--log',str(out/'noise.jsonl'),'--stop-file',str(stop)],creationflags=subprocess.CREATE_NO_WINDOW,stderr=(out/'noise.log').open('w'))
   until=time.monotonic()+6
   while time.monotonic()<until:
    if noise.poll() is not None:raise RuntimeError('Noise source exited: '+(out/'noise.log').read_text()[-1500:])
    try:
     noise_header=json.loads((out/'noise.jsonl').read_text().splitlines()[0])
     assert noise_header['fps']==a.noise_fps
     break
    except (OSError,IndexError,json.JSONDecodeError):time.sleep(.02)
   else:raise RuntimeError('Noise source did not start; motion test is invalid')
  time.sleep(a.start_delay_ms/1000)
  roi=f"{g['left']+30},{g['top']+27},36,12,12"
  with (out/'app-report.json').open('w') as so,(out/'app.log').open('w') as se:
   cmd=[str(ROOT/'src-tauri/target/release/replayx-cli.exe'),'native-app-test','--desktop-silent','--trace-roi',roi]+(['--capture-vfr'] if a.capture_vfr else ['--phase-lock'] if a.phase_lock else [])+(['--seconds',str(a.seconds)] if a.seconds!=6 else [])+(['--phase-global'] if a.phase_global else [])+(['--capture-interval-us',str(a.capture_interval_us)] if a.capture_interval_us is not None else [])
   if a.sample_resources:
    r=record_resources(cmd,so,se,out)
   else:r=subprocess.run(cmd,cwd=ROOT,stdout=so,stderr=se,timeout=35,creationflags=subprocess.CREATE_NO_WINDOW)
  if r.returncode:raise RuntimeError((out/'app.log').read_text()[-3000:])
 finally:
  stop.touch()
  for fixture in [src,noise]:
   if fixture is None:continue
   try:fixture.wait(timeout=4)
   except subprocess.TimeoutExpired:fixture.terminate();fixture.wait(timeout=3)
 requested='capture-vfr-experimental' if a.capture_vfr else 'clocked-phase-lock-experimental' if a.phase_lock else 'clocked-causal'
 audit(out,requested)

def record_resources(cmd,so,se,out):
 import psutil,importlib.util
 spec=importlib.util.spec_from_file_location('profile_counters',ROOT/'scripts/bench/native-profile.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);gpu=module.Gpu()
 process=subprocess.Popen(cmd,cwd=ROOT,stdout=so,stderr=se,creationflags=subprocess.CREATE_NO_WINDOW)
 began=time.monotonic();owned=[];discovered=False;rows=[];descriptions=[];rss_first={}
 try:
  while process.poll() is None:
   elapsed=time.monotonic()-began
   if elapsed>35:raise RuntimeError('Bounded app validation timed out')
   if not discovered and elapsed>=3:
    discovered=True
    candidates=[psutil.Process(process.pid),*psutil.Process(process.pid).children(recursive=True)]
    for candidate in candidates:
     try:
      name=candidate.name();command=candidate.cmdline()
      if name.lower() in ['replayx-cli.exe','replayx-native-proto.exe','ffmpeg.exe']:
       owned.append(candidate);descriptions.append({'pid':candidate.pid,'name':name,'worker':'--native-worker' in command})
     except psutil.Error:pass # A startup verifier can exit during discovery.

   row={'elapsed':elapsed,'qpc_hns':time.perf_counter_ns()//100,'gpu':gpu.sample(),'processes':[]}
   for proc in owned:
    try:
     memory=proc.memory_info();cpu=proc.cpu_times();rss=memory.rss/1048576
     rss_first.setdefault(proc.pid,rss)
     if rss>256 or rss-rss_first[proc.pid]>64:
      row['guard_failure']={'pid':proc.pid,'name':proc.name(),'rss_mib':rss,'first_rss_mib':rss_first[proc.pid]};rows.append(row)
      raise RuntimeError(f'Owned-process memory guard: {row["guard_failure"]}; test stopped')
     row['processes'].append({'pid':proc.pid,'rss_mib':rss,'cpu_seconds':cpu.user+cpu.system})
    except psutil.Error:pass
   rows.append(row);time.sleep(.2)
  return process
 finally:
  if process.poll() is None:process.terminate();process.wait(timeout=5)
  gpu.close()
  (out/'resource-samples.json').write_text(json.dumps({'identified_processes':descriptions,'samples':rows,'scope':'Trace-enabled short app capture; GPU device-wide includes desktop/source. Not game FPS or a long-session leak proof.'},indent=2))

def audit(out,requested=None):
 d=json.loads((out/'app-report.json').read_text());assert d['status']['recording_engine']=='native' and not d['status']['engine_fallback'],d['status']['engine_fallback']
 case=Path(d['recording']).parents[1];trace=list((case/'data/buffer').glob('*/native.trace.jsonl'));assert len(trace)==1,trace
 shutil.copy2(trace[0],out/'trace.jsonl')
 if trace[0].with_name('native.complete.json').exists():shutil.copy2(trace[0].with_name('native.complete.json'),out/'native.json')
 elif trace[0].with_name('native.json').exists():shutil.copy2(trace[0].with_name('native.json'),out/'native.json')
 g=json.loads((out/'source.jsonl').read_text().splitlines()[0])
 (out/'fixture.geometry.json').write_text(json.dumps(g))
 print('Saved actual native MP4 recording/replay:',d['recording'],d['replay'],flush=True)
 # Decode both MP4s independently; find their exact GOP-aligned position in the full trace.
 import importlib.util
 spec=importlib.util.spec_from_file_location('audit',ROOT/'scripts/bench/native-stage-audit.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
 ev=m.load_lines(out/'trace.jsonl');caps=sorted((e for e in ev if e['stage']=='capture_dequeued'),key=lambda e:e['capture']);content={e['capture']:e['content_id'] for e in ev if e['stage']=='captured_content'};sel=sorted((e for e in ev if e['stage']=='selected'),key=lambda e:e['output']);allids=[content[e['capture']] for e in sel]
 if not g['static'] and len(set(content.values()))<10:raise RuntimeError('Invalid motion test: source fixture was occluded or not updating; no smoothness verdict')
 native=json.loads((out/'native.json').read_text())
 if requested is not None:assert native['frame_pacing']==requested,'The requested pacing mode was not exercised'
 phase=native['frame_pacing']=='clocked-phase-lock-experimental'
 vfr=sel[0]['kind']=='CaptureVfr'
 if vfr:
  groups={}
  for c in caps:groups[((c['capture_hns']-caps[0]['capture_hns'])*90000+5000000)//10000000]=c
  expected_captures=[c['capture'] for c in groups.values()]
  assert [s['capture'] for s in sel]==expected_captures,'Variable timeline lost/reordered a capture'
  expected=[content[i] for i in expected_captures]
 else:
  import bisect
  times=[e['capture_hns'] for e in caps]
  expected=[content[caps[bisect.bisect_right(times,s['deadline_hns'] if phase else caps[0]['capture_hns']+s['output']*10000000//60,hi=s['trigger_capture']+1)-1]['capture']] for s in sel]

 assert allids==expected,'Incorrect source schedule before mux'
 conversions=[e for e in ev if e['stage']=='converted_content']
 assert all(e['content_id']==content[e['capture']] for e in conversions),'GPU conversion changed content'
 if (out/'native.json').exists():
  native=json.loads((out/'native.json').read_text());assert native['live_input_samples']==0 and not native['probe_dropped'] and native['output_timestamp_contract_valid']

 lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'));b=ROOT/'tools'/lock['runtime_dir'];results=[]
 for kind in ['recording','replay']:
  capture_size=native.get('capture_size',[1920,1080])
  clip=Path(d[kind]);p=subprocess.run([str(b/'ffmpeg.exe'),'-v','error','-xerror','-threads','2','-filter_threads','1','-i',str(clip),'-vf',f"scale={capture_size[0]}:{capture_size[1]}:flags=neighbor,crop=960:540:{g['left']}:{g['top']},scale=320:180:flags=area,format=gray",'-fps_mode','passthrough','-f','rawvideo','-'],capture_output=True,timeout=40,check=True)
  dec=[]
  for off in range(0,len(p.stdout),57600):
   pix=p.stdout[off:off+57600];dec.append(sum(1<<i for i in range(24) if sum(pix[(9+dy)*320+10+i*12+dx] for dy in range(4) for dx in range(4))/16>128))
  offsets=[i for i in range(len(allids)-len(dec)+1) if allids[i:i+len(dec)]==dec];assert len(offsets)==1,(kind,'MP4 changed/misordered frame IDs',offsets)
  f=json.loads(subprocess.check_output([str(b/'ffprobe.exe'),'-v','error','-select_streams','v:0','-show_frames','-show_entries','frame=best_effort_timestamp_time,key_frame,pict_type','-of','json',str(clip)]))['frames'];keys=[i for i,x in enumerate(f) if x['key_frame']];ts=[float(x['best_effort_timestamp_time']) for x in f]
  assert keys and keys[0]==0 and not any(x['pict_type']=='B' for x in f)
  if not vfr:assert keys==list(range(0,len(f),120))
  else:assert max([ts[b]-ts[a] for a,b in zip(keys,keys[1:])]+[ts[-1]-ts[keys[-1]]])<=2.05
  assert all(y>x for x,y in zip(ts,ts[1:]));assert len(f)==len(dec)
  row={'kind':kind,'path':str(clip),'frames':len(dec),'source_trace_offset_frames':offsets[0],'wrong_decoded_ids':0,'wrong_grid_choices':0,'repeated_pictures':sum(x==y for x,y in zip(dec,dec[1:])),'expected_repeated_pictures':sum(x==y for x,y in zip(expected[offsets[0]:offsets[0]+len(dec)],expected[offsets[0]+1:offsets[0]+len(dec)])),'source60_one_to_one':m.motion_one_to_one(dec,g['fps'],g['static']),'first_source_id':dec[0],'last_source_id':dec[-1],'keyframes':keys,'ready_ms':d[kind+'_probe']['save_timings'][-1]['elapsed_ms'],'timestamp_error_max_ms':max(abs((t-ts[0])-n/60)*1000 for n,t in enumerate(ts))}
  if vfr:
   unique=[x for i,x in enumerate(dec) if i==0 or x!=dec[i-1]]
   wanted_pts=[s['pts']/1e7 for s in sel[offsets[0]:offsets[0]+len(dec)]]
   error=max(abs((t-ts[0])-(p-wanted_pts[0]))*1000 for t,p in zip(ts,wanted_pts))
   assert error<=1.1,('VFR timestamps were rewritten',error)
   stream=json.loads(subprocess.check_output([str(b/'ffprobe.exe'),'-v','error','-select_streams','v:0','-show_entries','stream=level','-of','json',str(clip)]))['streams'][0]
   assert stream['level']==52,('VFR AVC level not signalled',stream)
   header=subprocess.run([str(b/'ffmpeg.exe'),'-v','info','-i',str(clip),'-map','0:v','-c','copy','-bsf:v','trace_headers','-frames:v','1','-f','null','-'],capture_output=True,check=True).stderr.decode(errors='replace')
   import re
   fixed=re.findall(r'fixed_frame_rate_flag\s+\S+\s+=\s+(\d+)',header)
   assert fixed and set(fixed)=={'0'},('VFR fixed-rate flag not cleared',fixed)
   row['avc_level']=stream['level'];row['fixed_frame_rate_flag']=0
   expected_slice=expected[offsets[0]:offsets[0]+len(dec)]
   available_unique=[x for i,x in enumerate(expected_slice) if i==0 or x!=expected_slice[i-1]]
   captured=set(content.values())
   row.update(pacing='capture-vfr-experimental',source60_one_to_one=None,source_updates_in_order=all(b==a+1 for a,b in zip(unique,unique[1:])),all_available_source_updates_retained=unique==available_unique,capture_omitted_source_ids=sorted(set(range(unique[0],unique[-1]+1))-captured),unique_source_updates=len(unique),timestamp_error_max_ms=error,nominal_60_cfr=False)
   assert row['all_available_source_updates_retained'],'Available source updates were lost between capture and playback'
  if not vfr:
   from collections import Counter
   captured=set(content.values());first,last=dec[0],dec[-1]
   lost=sorted((captured & set(range(first,last+1)))-set(dec))
   settled=dec[60:] # Report startup separately; do not erase its counts.
   settled_lost=sorted((captured & set(range(settled[0],settled[-1]+1)))-set(settled)) if settled else []
   backwards=sum(b<a for a,b in zip(dec,dec[1:]))
   holds=[];run=1
   for x,y in zip(settled,settled[1:]):
    if x==y:run+=1
    else:holds.append(run);run=1
   holds.append(run)
   row.update(pacing=native['frame_pacing'],lost_available_source_updates=lost,lost_available_after_first_second=settled_lost,backwards=backwards,
      repeats_after_first_second=sum(a==b for a,b in zip(settled,settled[1:])),interior_source_hold_lengths=dict(Counter(holds[1:-1])),
      capture_omitted_source_ids=sorted(set(range(first,last+1))-captured))
   if g['fps']==60:row['motion_gate_after_settling']=not settled_lost and not backwards and row['repeats_after_first_second']==0
   elif g['fps']==30:row['motion_gate_after_settling']=not settled_lost and not backwards and all(n==2 for n in holds[1:-1])
   else:row['motion_gate_after_settling']=not settled_lost and not backwards
   assert row['timestamp_error_max_ms']<=0.6,'CFR timeline moved'
  results.append(row);print(json.dumps(row),flush=True)
 (out/'mp4-audit.json').write_text(json.dumps(results,indent=2))
 if (out/'resource-samples.json').exists():
  import statistics,psutil
  raw=json.loads((out/'resource-samples.json').read_text());origin=native['origin_qpc_hns'];end=native['presentation_end_hns']
  steady=[r for r in raw['samples'] if origin+10000000<=r['qpc_hns']<=end-1000000 and r['processes']]
  workers=[p['pid'] for p in raw['identified_processes'] if p['worker']];assert workers==[native['worker_pid']],'Resource sampling did not identify the encoder worker'
  assert len(steady)>=3,'Insufficient capture-only resource samples'
  first,last=steady[0],steady[-1];before={p['pid']:p['cpu_seconds'] for p in first['processes']};seconds=(last['qpc_hns']-first['qpc_hns'])/1e7
  cpu={str(p['pid']):(p['cpu_seconds']-before[p['pid']])/seconds*100 for p in last['processes'] if p['pid'] in before}
  peaks=[sum(p['rss_mib'] for p in r['processes']) for r in steady]
  resource={'pacing':native['frame_pacing'],'measured_capture_seconds':seconds,'persistent_cpu_one_core_percent':sum(cpu.values()),'persistent_cpu_percent_of_machine':sum(cpu.values())/psutil.cpu_count(),'cpu_one_core_by_pid':cpu,
   'persistent_rss_start_mib':peaks[0],'persistent_rss_end_mib':peaks[-1],'persistent_rss_peak_mib':max(peaks),'worker_pid':native['worker_pid'],
   'gpu_device_mean_percent':statistics.mean(r['gpu']['gpu_percent'] for r in steady if 'gpu_percent' in r['gpu']),
   'gpu_encoder_mean_percent':statistics.mean(r['gpu']['encoder_percent'] for r in steady if 'encoder_percent' in r['gpu']),
   'limits':raw['scope']}
  (out/'resource-summary.json').write_text(json.dumps(resource,indent=2));print(json.dumps(resource),flush=True)

 if any(r.get('motion_gate_after_settling') is False for r in results):raise RuntimeError('FAIL moving 60 FPS source-identity gate: matching the capture oracle is insufficient to pass smoothness. See source-delivery-audit.json / phase-sweep.json when present.')
if __name__=='__main__':main()
