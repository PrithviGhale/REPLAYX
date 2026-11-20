"""Bounded silent desktop app soak. Captures no audio; launches no game/fullscreen."""
import argparse,json,subprocess,time,statistics,importlib.util,os
from pathlib import Path
import psutil
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--name',required=True);ap.add_argument('--seconds',type=int,default=90);ap.add_argument('--capture-vfr',action='store_true');ap.add_argument('--phase-lock',action='store_true');ap.add_argument('--phase-global',action='store_true');a=ap.parse_args()
 if a.phase_global and not a.phase_lock:raise ValueError('Global comparison requires explicit phase-lock')
 if a.capture_vfr and a.phase_lock:raise ValueError('Choose one experimental mode')
 if not 6<=a.seconds<=90 or not a.name.replace('-','').isalnum():raise ValueError('Invalid bounded soak')
 out=ROOT/'test-output/native-smoothness'/a.name;out.mkdir();stop=out/'source.stop'
 source=subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-motion-source.exe'),'--seconds',str(a.seconds+25),'--fps','60','--log',str(out/'source.jsonl'),'--stop-file',str(stop)],creationflags=subprocess.CREATE_NO_WINDOW,stderr=(out/'source.log').open('w'))
 cli=None;rows=[]
 spec=importlib.util.spec_from_file_location('profile_counters',ROOT/'scripts/bench/native-profile.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);gpu=module.Gpu()
 try:
  until=time.monotonic()+6
  while time.monotonic()<until:
   if (out/'source.jsonl').exists() and (out/'source.jsonl').stat().st_size:break
   if source.poll() is not None:raise RuntimeError('Motion source refused startup: '+(out/'source.log').read_text()[-1500:])
   time.sleep(.02)
  else:raise RuntimeError('Motion source did not start; capture was not launched')
  so=(out/'app-report.json').open('w');se=(out/'app.log').open('w')
  cli=subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-cli.exe'),'native-app-test','--desktop-silent','--seconds',str(a.seconds)]+(['--capture-vfr'] if a.capture_vfr else ['--phase-lock'] if a.phase_lock else [])+(['--phase-global'] if a.phase_global else []),cwd=ROOT,stdout=so,stderr=se,creationflags=subprocess.CREATE_NO_WINDOW)
  started=time.monotonic();print(f'SOAK: {a.seconds}s capture target, bounded startup/save budget 35s; source window only, no audio.',flush=True)
  time.sleep(3)
  owned=[psutil.Process(cli.pid),*psutil.Process(cli.pid).children(recursive=True)]
  # Only the identified persistent capture processes; transient verifiers/frontend excluded.
  persistent=[]
  for proc in owned:
   try:
    if proc.name().lower() in ['replayx-cli.exe','replayx-native-proto.exe','ffmpeg.exe']:persistent.append(proc)
   except psutil.Error:pass
  owned=persistent
  worker=[p for p in owned if '--native-worker' in p.cmdline()]
  if len(worker)!=1:raise RuntimeError('Native encoder worker not identified')
  command=worker[0].cmdline()
  if a.phase_lock:assert '--pacing' in command and command[command.index('--pacing')+1]=='phase-lock','Requested phase-lock was not exercised'
  origin_path=Path(command[command.index('--origin-file')+1]).resolve()
  if not origin_path.is_relative_to(ROOT.resolve()):raise RuntimeError('Owned native origin file is outside the performance worktree')
  origin=json.loads(origin_path.read_text())['origin_qpc_hns']
  # Keep the worker's completion report after normal session cleanup. This
  # adds one NTFS link, not a copy/readback or a recorder-path modification.
  os.link(origin_path.with_name('native.json'),out/'native.json')
  discovered=[{'pid':p.pid,'name':p.name(),'worker':'--native-worker' in p.cmdline()} for p in owned]
  while cli.poll() is None:
   if time.monotonic()-started>a.seconds+35:raise RuntimeError('App exceeded bounded soak/save time')
   row={'elapsed':time.monotonic()-started,'qpc_hns':time.perf_counter_ns()//100,'gpu':gpu.sample(),'processes':[]}
   for proc in owned:
    try:
     m=proc.memory_info();c=proc.cpu_times()
     if m.rss/1048576>256:raise RuntimeError('Unexpected owned-process memory use; test stopped')
     row['processes'].append({'pid':proc.pid,'rss_mb':m.rss/1048576,'private_mb':getattr(m,'private',m.vms)/1048576,'cpu_seconds':c.user+c.system})
    except psutil.Error:pass
   rows.append(row);time.sleep(.5)
  so.close();se.close()
  if cli.returncode:raise RuntimeError((out/'app.log').read_text()[-3000:])
  d=json.loads((out/'app-report.json').read_text());assert d['status']['recording_engine']=='native' and not d['status']['engine_fallback'];assert not d['status']['error'];assert not d['physical_audio']
  native=json.loads((out/'native.json').read_text())
  assert native['worker_pid']==worker[0].pid and native['live_input_samples']==0
  expected='capture-vfr-experimental' if a.capture_vfr else 'clocked-phase-lock-experimental' if a.phase_lock else 'clocked-causal'
  assert native['frame_pacing']==expected and native['output_timestamp_contract_valid']
  # Restrict to pre-save capture, even if replay remains active while clips
  # finish. The conservative requested-duration bound excludes save/drain work.
  end=min(native['presentation_end_hns'],origin+(a.seconds-1)*10000000)
  steady=[r for r in rows if origin+20000000<=r['qpc_hns']<=end-2000000];wr=[];total=[]
  if len(steady)<3:raise RuntimeError('Insufficient capture-clock-aligned steady samples; no CPU verdict')
  for r in steady:
   w=[p for p in r['processes'] if p['pid']==worker[0].pid]
   if w:wr.append(w[0]['rss_mb'])
   total.append(sum(p['rss_mb'] for p in r['processes']))
  first=steady[0];last=steady[-1];elapsed=last['elapsed']-first['elapsed']
  prior={p['pid']:p['cpu_seconds'] for p in first['processes']}
  per_cpu={str(p['pid']):(p['cpu_seconds']-prior[p['pid']])/elapsed*100 for p in last['processes'] if p['pid'] in prior}
  remaining=[]
  for proc in owned:
   try:
    if proc.is_running():remaining.append(proc.pid)
   except psutil.Error:pass
  case=Path(d['recording']).parents[1];buffer_files=list((case/'data/buffer').rglob('segment-*.mkv'))
  result={'status':'PASS','pacing':'capture-vfr-experimental' if a.capture_vfr else 'clocked-phase-lock-experimental' if a.phase_lock else 'clocked-cfr','capture_target_seconds':a.seconds,'performance_window':{'origin_qpc_hns':origin,'end_qpc_hns':end,'start_margin_seconds':2,'end_margin_seconds':.2},'observed_controller_seconds':time.monotonic()-started,'identified_processes':discovered,'encoder_rss_first_5_samples_mb':statistics.mean(wr[:5]),'encoder_rss_last_5_samples_mb':statistics.mean(wr[-5:]),'encoder_rss_peak_mb':max(wr),'persistent_capture_rss_first_5_samples_mb':statistics.mean(total[:5]),'persistent_capture_rss_last_5_samples_mb':statistics.mean(total[-5:]),'persistent_capture_rss_peak_mb':max(total),'persistent_cpu_one_core_percent':sum(per_cpu.values()),'persistent_cpu_percent_of_machine':sum(per_cpu.values())/psutil.cpu_count(),'cpu_one_core_percent_by_pid':per_cpu,'remaining_capture_pids':remaining,'remaining_buffer_segment_files':len(buffer_files),'max_replay_buffer_seconds':max(s['buffer_seconds'] for s in d['samples']),'max_replay_buffer_bytes':max(s['buffer_bytes'] for s in d['samples']),'recording':d['recording'],'replay':d['replay'],'recording_probe':d['recording_probe'],'replay_probe':d['replay_probe'],'samples':rows,'gpu_device_mean_percent':statistics.mean(r['gpu']['gpu_percent'] for r in steady if 'gpu_percent' in r['gpu']),'gpu_encoder_mean_percent':statistics.mean(r['gpu']['encoder_percent'] for r in steady if 'encoder_percent' in r['gpu']),'gpu_samples':[r['gpu'] for r in steady],'limits':'Trace-disabled bounded silent Engine performance check. Not motion acceptance, game FPS, a long-session leak proof, physical A/V test or Tauri frontend measurement. GPU readings are device-wide. Summed RSS can double-count shared DLL pages.'}
  assert not remaining and not buffer_files
  (out/'soak.json').write_text(json.dumps(result,indent=2))
  print('DONE',json.dumps({k:result[k] for k in ['encoder_rss_first_5_samples_mb','encoder_rss_last_5_samples_mb','encoder_rss_peak_mb','persistent_capture_rss_first_5_samples_mb','persistent_capture_rss_last_5_samples_mb','max_replay_buffer_seconds','max_replay_buffer_bytes','remaining_capture_pids','remaining_buffer_segment_files']}),flush=True)
 finally:
  gpu.close()
  if cli and cli.poll() is None:cli.terminate();cli.wait(timeout=5)
  stop.touch()
  try:source.wait(timeout=4)
  except subprocess.TimeoutExpired:source.terminate();source.wait(timeout=3)
if __name__=='__main__':main()
