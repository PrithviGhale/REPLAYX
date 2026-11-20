"""Unattended windowed recorder-only profile, with adjacent OFF baselines.
No game, audio, setting changes or GPU readback probes. GPU counters are device-wide.
"""
import argparse,ctypes,json,os,statistics,subprocess,time
from pathlib import Path
import psutil
ROOT=Path(__file__).resolve().parents[2]
class Util(ctypes.Structure):_fields_=[('gpu',ctypes.c_uint),('memory',ctypes.c_uint)]
class Mem(ctypes.Structure):_fields_=[('total',ctypes.c_ulonglong),('free',ctypes.c_ulonglong),('used',ctypes.c_ulonglong)]
class Gpu:
 def __init__(self):
  self.error=None;self.lib=None
  try:
   self.lib=ctypes.WinDLL(str(Path(os.environ.get('WINDIR','C:/Windows'))/'System32/nvml.dll'));assert self.lib.nvmlInit_v2()==0
   self.handle=ctypes.c_void_p();assert self.lib.nvmlDeviceGetHandleByIndex_v2(0,ctypes.byref(self.handle))==0
  except Exception as e:self.error=str(e);self.lib=None
 def sample(self):
  if not self.lib:return {'unavailable':self.error}
  u=Util();m=Mem();enc=ctypes.c_uint();period=ctypes.c_uint();r={}
  if self.lib.nvmlDeviceGetUtilizationRates(self.handle,ctypes.byref(u))==0:r.update(gpu_percent=u.gpu,memory_activity_percent=u.memory)
  if self.lib.nvmlDeviceGetMemoryInfo(self.handle,ctypes.byref(m))==0:r['device_memory_mb']=m.used/1048576
  if self.lib.nvmlDeviceGetEncoderUtilization(self.handle,ctypes.byref(enc),ctypes.byref(period))==0:r.update(encoder_percent=enc.value,encoder_sample_period_us=period.value)
  return r
 def close(self):
  if self.lib:self.lib.nvmlShutdown()
def summarize(rows):
 keys=['gpu_percent','encoder_percent','device_memory_mb'];return {k:{'mean':statistics.mean([r['gpu'][k] for r in rows if k in r['gpu']]),'max':max(r['gpu'][k] for r in rows if k in r['gpu'])} for k in keys if any(k in r['gpu'] for r in rows)}
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--name',required=True);ap.add_argument('--seconds',type=int,default=20);ap.add_argument('--pacing',choices=['legacy','clocked','capture-vfr'],default='clocked');ap.add_argument('--exe',type=Path);a=ap.parse_args()
 if not 5<=a.seconds<=120 or not a.name.replace('-','').isalnum():raise ValueError('Invalid bounded profile')
 exe=(a.exe or ROOT/'src-tauri/target/release/replayx-native-proto.exe').resolve();assert ROOT in exe.parents
 out=ROOT/'test-output/native-smoothness'/a.name;out.mkdir();stop=out/'source.stop'
 src=subprocess.Popen([str(ROOT/'src-tauri/target/release/replayx-motion-source.exe'),'--seconds',str(min(120,a.seconds+24)),'--fps','60','--log',str(out/'source.jsonl'),'--stop-file',str(stop)],creationflags=subprocess.CREATE_NO_WINDOW,stderr=(out/'source.log').open('w'))
 gpu=Gpu();rows=[];proc=None;worker=None
 try:
  until=time.monotonic()+6
  while time.monotonic()<until:
   if (out/'source.jsonl').exists() and (out/'source.jsonl').stat().st_size:break
   time.sleep(.02)
  else:raise RuntimeError('Source preflight failed')
  begin=time.monotonic();active_budget=a.seconds+8;total=5+active_budget+5
  print(f'EXPECTED MEASURED WINDOW {total}s: OFF 5s, recording slot {active_budget}s ({a.seconds}s capture + 8s startup/drain reservation), OFF 5s. Preflight finished.',flush=True)
  def sample(phase,end):
   nonlocal worker
   print('PHASE',phase,'remaining measured seconds',round(begin+total-time.monotonic(),1),flush=True)
   while time.monotonic()<end:
    row={'t':time.monotonic()-begin,'qpc_hns':time.perf_counter_ns()//100,'phase':phase,'gpu':gpu.sample()}
    if worker:
     try:
      mi=worker.memory_info();ct=worker.cpu_times();row['worker']={'pid':worker.pid,'rss_mb':mi.rss/1048576,'private_mb':getattr(mi,'private',mi.vms)/1048576,'cpu_seconds':ct.user+ct.system}
     except psutil.Error:pass
    rows.append(row);time.sleep(min(.2,max(0,end-time.monotonic())))
  sample('OFF-before',begin+5)
  so=(out/'native.json').open('w');se=(out/'native.log').open('w')
  cmd=[str(exe),'--record','--desktop','--seconds',str(a.seconds),'--out',str(out/'capture.ts'),'--stream-ts','--pacing',a.pacing,'--interval-us','12500' if a.pacing=='legacy' else '0']
  (out/'command.json').write_text(json.dumps(cmd,indent=2));proc=subprocess.Popen(cmd,stdout=so,stderr=se,creationflags=subprocess.CREATE_NO_WINDOW)
  # Discover the owned worker ONCE, not on every frame or sample.
  time.sleep(2);children=psutil.Process(proc.pid).children();
  workers=[p for p in children if '--native-worker' in p.cmdline()]
  if len(workers)!=1:raise RuntimeError('Could not identify the owned encoder worker; RAM measurement refused')
  worker=workers[0]
  sample('ON',begin+5+active_budget)
  if proc.poll() is None:proc.terminate();proc.wait(timeout=5);raise RuntimeError('Recording exceeded its fixed reservation')
  so.close();se.close()
  if proc.returncode:raise RuntimeError((out/'native.log').read_text()[-1500:])
  sample('OFF-after',begin+total)
  report=json.loads((out/'native.json').read_text());origin=report['origin_qpc_hns'];end=origin+report['seconds']*1e7
  active=[r for r in rows if origin<=r['qpc_hns']<=end];off=[r for r in rows if r['phase'].startswith('OFF')]
  mem=[r for r in active if 'worker' in r];early=mem[:min(20,len(mem))];late=mem[-min(20,len(mem)):]
  result={'status':'PASS','scheduled_seconds':total,'elapsed_seconds':time.monotonic()-begin,'pacing':a.pacing,'rss_sampling_verified':True,'worker_pid':worker.pid,'exe':str(exe),'native':report,'cpu_one_core_percent':report['cpu_percent_of_one_core'],'cpu_percent_of_machine':report['cpu_percent_of_one_core']/psutil.cpu_count(),'logical_cpus':psutil.cpu_count(),'worker_rss_first_mb':statistics.mean(r['worker']['rss_mb'] for r in early),'worker_rss_last_mb':statistics.mean(r['worker']['rss_mb'] for r in late),'worker_rss_peak_mb':max(r['worker']['rss_mb'] for r in mem),'gpu_active':summarize(active),'gpu_off_neighbors':summarize(off),'samples':rows,'limitations':'Native worker only; GPU counters include the windowed source and desktop and are coarse NVML averages. No gameplay FPS or full-app overhead claim.'}
  (out/'profile.json').write_text(json.dumps(result,indent=2))
  print('DONE',json.dumps({k:result[k] for k in ['cpu_one_core_percent','cpu_percent_of_machine','worker_rss_first_mb','worker_rss_last_mb','worker_rss_peak_mb','gpu_active','gpu_off_neighbors']}),flush=True)
 finally:
  stop.touch()
  if proc and proc.poll() is None:proc.terminate();proc.wait(timeout=5)
  try:src.wait(timeout=4)
  except subprocess.TimeoutExpired:src.terminate();src.wait(timeout=3)
  gpu.close()
if __name__=='__main__':main()
