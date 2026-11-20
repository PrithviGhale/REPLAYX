"""Offline benchmark acceptance, scheduling and resume gates. No game/capture."""
import copy,contextlib,importlib.util,io,json,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
BENCH=Path(__file__).parents[1]/"scripts/bench"
sys.path.insert(0,str(BENCH))
import cs2_ab as ab
sys.path.pop(0)
def summary():
 phases=[]
 for n,config in enumerate([None,ab.FF,None,ab.NATIVE,None]):
  phases.append(dict(phase=str(n),config=config,frames=dict(fps=480 if config is None else 400,low1_fps=200,p99_ms=5,application="cs2.exe",process_id=123,selected_swapchain="0x123",interval_column="MsBetweenPresents",cap_limited=False),cpu_effective_clock_percent=100,
   recorder_verified=True,recorder_exit_code=0,recorder=dict(recording_engine="native" if config==ab.NATIVE else "ffmpeg",encoder="h264_nvenc",engine_fallback=None,recovery_sessions=0),
   native_worker_commands=[["proto","--pacing","phase-lock","--phase-source","spatial"]] if config==ab.NATIVE else [],
   nvidia=dict(temp_c=65,graphics_mhz=2200),process_cpu_percent={"replayx-cli.exe":.5},process_rss_mib={"replayx-cli.exe":dict(peak=110,growth=1)},gpu_engines_percent={"encode":20}))
 return dict(errors=[],presentmon=True,phases=phases)
class Cs2Preparation(unittest.TestCase):
 def test_plan_duration_includes_all_reservations(self):
  p=ab.plan();self.assertEqual(p["expected_capture_seconds"],273);self.assertEqual(p["maximum_offline_audit_seconds"],120);self.assertEqual(p["maximum_total_seconds"],543)
  self.assertEqual(ab.plan(units_per_run=2)["phases"][1][1][1],ab.NATIVE)
 def test_default_plan_starts_nothing(self):
  with patch.object(sys,"argv",["cs2_ab"]),patch.object(ab.subprocess,"Popen",side_effect=AssertionError("launched")),contextlib.redirect_stdout(io.StringIO()) as out:ab.main()
  self.assertEqual(json.loads(out.getvalue())["target_pairs"],2)
 def test_crosshair_dry_run_uses_same_ffmpeg_with_balanced_orders(self):
  with patch.object(sys,"argv",["cs2_ab","--crosshair","--warmup","30","--units-per-run","2"]),patch.object(ab.subprocess,"Popen",side_effect=AssertionError("launched")),contextlib.redirect_stdout(io.StringIO()) as out:
   ab.main()
  plan = json.loads(out.getvalue())
  self.assertEqual(plan["expected_capture_seconds"],486)
  selected = [[config for _, config in phases if config] for phases in plan["phases"]]
  self.assertEqual(selected, [[ab.FF, ab.CROSSHAIR], [ab.CROSSHAIR, ab.FF]])
 def test_crosshair_requires_visible_excluded_overlay_and_explicit_clip_review(self):
  result = summary()
  phase = result["phases"][3]
  phase["config"] = ab.CROSSHAIR
  phase["recorder"]["recording_engine"] = "ffmpeg"
  phase["native_worker_commands"] = []
  self.assertFalse(ab.evaluate(result, crosshair=True)["eligible"])
  phase["crosshair"] = dict(shown=True, excluded_from_capture=True)
  self.assertTrue(ab.evaluate(result, crosshair=True)["eligible"])
  self.assertFalse(ab.evaluate(result)["eligible"])
  clip = dict(sha256="hash", crosshair_enabled=True)
  review = dict(clip_sha256="hash", reviewer="tester", motion_review_passed=True,
                picture_quality_review_passed=True, av_sync_review_passed=True, audio_clean_passed=True)
  self.assertFalse(ab.review_ok([clip], [review]))
  review["crosshair_exclusion_review_passed"] = True
  self.assertEqual(ab.review_state([clip], [review]), "pending")
  review["crosshair_visible_in_game_review_passed"] = False
  self.assertEqual(ab.review_state([clip], [review]), "rejected")
  review["crosshair_visible_in_game_review_passed"] = True
  self.assertTrue(ab.review_ok([clip], [review]))
 def test_clean_measurement_is_eligible_but_not_quality_verified(self):
  self.assertTrue(ab.evaluate(summary())["eligible"])
  self.assertFalse(ab.review_ok([dict(sha256="abc")],[]))
 def test_missing_final_baseline_not_counted(self):
  s=summary();s["phases"].pop();self.assertFalse(ab.evaluate(s)["eligible"])
 def test_drift_focus_fallback_and_missing_telemetry_rejected(self):
  cases=[]
  s=summary();s["phases"][-1]["frames"]["fps"]=300;cases.append(s)
  s=summary();s["errors"]=["focus lost"];cases.append(s)
  s=summary();s["phases"][3]["recorder"]["engine_fallback"]="driver failure";cases.append(s)
  s=summary();s["phases"][-1]["nvidia"]["temp_c"]=75;cases.append(s)
  s=summary();s["phases"][0]["nvidia"]["graphics_mhz"]=1000;cases.append(s)
  s=summary();s["phases"][1]["process_rss_mib"]={};cases.append(s)
  s=summary();s["phases"][3]["native_worker_commands"]=[];cases.append(s)
  s=summary();s["phases"][0]["frames"]["cap_limited"]=True;cases.append(s)
  s=summary();s["phases"][3]["frames"]["selected_swapchain"]="changed";cases.append(s)
  s=summary();s["phases"][0]["cpu_effective_clock_percent"]=50;cases.append(s)
  for s in cases:self.assertFalse(ab.evaluate(s)["eligible"])
 def test_quality_reviews_are_hash_bound_and_require_all_checks(self):
  r=dict(clip_sha256="abc",reviewer="tester",motion_review_passed=True,picture_quality_review_passed=True,av_sync_review_passed=True,audio_clean_passed=True)
  self.assertTrue(ab.review_ok([dict(sha256="abc")],[r]))
  self.assertFalse(ab.review_ok([dict(sha256="changed")],[r]))
  r["av_sync_review_passed"]=False;self.assertFalse(ab.review_ok([dict(sha256="abc")],[r]))
 def test_rejected_and_unfinished_units_cannot_support_fps_claim(self):
  e=ab.evaluate(summary());c=[dict(sha256="abc")];good=dict(evaluation=e,clips=c,summary=summary())
  ledger=dict(settings=dict(target_pairs=3),attempts=[dict(copy.deepcopy(good),order=ab.order(i)) for i in range(3)]+[dict(state="running")])
  result=ab.totals(ledger,[]);self.assertEqual(result["quality_pending_pairs"],3);self.assertFalse(result["performance_claim_allowed"])
  reviews=[dict(clip_sha256="abc",reviewer="tester",motion_review_passed=True,picture_quality_review_passed=True,av_sync_review_passed=True,audio_clean_passed=True)]
  self.assertTrue(ab.totals(ledger,reviews)["performance_claim_allowed"])
  ledger["attempts"][1]["evaluation"]["eligible"]=False
  self.assertEqual(ab.totals(ledger,reviews)["accepted_pairs"],2)
 def test_explicit_quality_failure_is_rejected_instead_of_counting_as_pending(self):
  good=dict(evaluation=ab.evaluate(summary()),clips=[dict(sha256="good")])
  bad=dict(evaluation=ab.evaluate(summary()),clips=[dict(sha256="bad")])
  reviews=[dict(clip_sha256="good",reviewer="tester",motion_review_passed=True,picture_quality_review_passed=True,av_sync_review_passed=True,audio_clean_passed=True),dict(clip_sha256="bad",reviewer="tester",motion_review_passed=False,picture_quality_review_passed=True,av_sync_review_passed=True,audio_clean_passed=True)]
  result=ab.totals(dict(settings=dict(target_pairs=3),attempts=[good,good,bad]),reviews)
  self.assertEqual(result["accepted_pairs"],2)
  self.assertEqual(result["quality_pending_pairs"],0)
  self.assertEqual(result["quality_rejected_pairs"],1)
  self.assertFalse(result["performance_claim_allowed"])
 def test_historical_configuration_names_remain_analyzable(self):
  historical=summary()
  aliases={ab.FF:"app-ffmpeg-nvenc-fixed",ab.NATIVE:"app-native-nvenc-phase"}
  for phase in historical["phases"]:
   phase["config"]=aliases.get(phase["config"],phase["config"])
  self.assertTrue(ab.evaluate(historical)["eligible"])
  ledger=dict(settings=dict(target_pairs=2),attempts=[dict(evaluation=ab.evaluate(historical),summary=historical,clips=[dict(sha256="old")],order=[aliases[c] for c in ab.order(i)]) for i in range(2)])
  reviews=[dict(clip_sha256="old",reviewer="historical tester",motion_review_passed=True,picture_quality_review_passed=True,av_sync_review_passed=True,audio_clean_passed=True)]
  self.assertEqual(ab.totals(ledger,reviews)["accepted_pairs"],2)
  self.assertTrue(ab.totals(ledger,reviews)["order_balanced"])
 def test_resume_ledger_write_preserves_all_attempts(self):
  with tempfile.TemporaryDirectory() as temp:
   p=Path(temp)/"ledger.json";d=dict(attempts=[dict(state="rejected"),dict(state="quality_pending")]);ab.write(p,d)
   self.assertEqual(json.loads(p.read_text()),d);self.assertFalse(p.with_suffix(".json.tmp").exists())
 def test_execute_requires_explicit_approval_before_any_process(self):
  with patch.object(sys,"argv",["cs2_ab","--execute","--out",str(ab.ROOT/"test-output/never-started")]),patch.object(ab.subprocess,"Popen",side_effect=AssertionError("launched")),contextlib.redirect_stderr(io.StringIO()),self.assertRaises(SystemExit):ab.main()
 def test_timeout_and_attach_failure_close_the_entire_owned_job(self):
  from types import SimpleNamespace
  import subprocess
  for attach_fails in [False,True]:
   calls=[]
   class Job:
    def attach(self,process):
     calls.append("attach")
     if attach_fails:raise RuntimeError("attach failure")
    def close(self):calls.append("close")
   class Process:
    def wait(self,timeout):
     calls.append("wait")
     if calls.count("wait")==1 and not attach_fails:raise subprocess.TimeoutExpired("fixture",timeout)
     return 0
   with tempfile.TemporaryDirectory() as temp,patch.dict(sys.modules,{"native_v1_support":SimpleNamespace(Job=Job)}),patch.object(ab.subprocess,"Popen",return_value=Process()),self.assertRaises((subprocess.TimeoutExpired,RuntimeError)):
    ab.run_unit(["fixture"],Path(temp)/"unit",1)
   self.assertIn("close",calls)


class DecodedClipValidation(unittest.TestCase):
 def test_mkv_audio_without_duration_uses_decoded_timeline(self):
  frames=[dict(media_type="video",best_effort_timestamp_time=str(i/60),key_frame=int(i%120==0),pict_type="I" if i%120==0 else "P") for i in range(120)]
  frames += [dict(media_type="audio",stream_index=1,best_effort_timestamp_time=str(i*1024/48000),nb_samples=1024) for i in range(94)]
  data=dict(streams=[dict(codec_type="video",codec_name="h264",width=1920,height=1080),dict(codec_type="audio",index=1,sample_rate="48000")],frames=frames)
  self.assertEqual(ab.validate_clip_streams(data,"ffmpeg",2)["frames"],120)
  data["frames"][-1]["best_effort_timestamp_time"]="4.0"
  with self.assertRaises(RuntimeError):ab.validate_clip_streams(data,"ffmpeg",2)
 def test_only_bounded_ffmpeg_segment_join_delay_is_accepted(self):
  streams = [dict(codec_type="video", codec_name="h264", width=1920, height=1080),
             dict(codec_type="audio", index=1, sample_rate="48000")]
  def clip(shifts):
   video = [
    dict(media_type="video", best_effort_timestamp_time=str(round(i / 60, 3) +
         sum(delay for index, delay in shifts if i >= index)),
         key_frame=int(i % 120 == 0), pict_type="I" if i % 120 == 0 else "P")
    for i in range(360)
   ]
   audio = [dict(media_type="audio", stream_index=1, best_effort_timestamp_time=str(i * 1024 / 48000),
                 nb_samples=1024) for i in range(282)]
   return dict(streams=streams, frames=video + audio)
  joined = clip([(120, .004)])
  audit = ab.validate_clip_streams(joined, "ffmpeg", 6)
  self.assertEqual(audit["frames"], 360)
  self.assertEqual(audit["subframe_join_intervals"], [dict(frame=120, timestamp=2.004, interval_ms=21.0)])
  for engine, shifts in [
      ("native", [(120, .004)]), ("ffmpeg", [(121, .004)]),
      ("ffmpeg", [(120, 1 / 60)]), ("ffmpeg", [(120, .003), (240, .003)]),
  ]:
   with self.subTest(engine=engine, shifts=shifts), self.assertRaisesRegex(RuntimeError, "cadence failure"):
    ab.validate_clip_streams(clip(shifts), engine, 6)
 def test_short_restarted_clips_cannot_pass_metadata_alone(self):
  data=dict(streams=[dict(codec_type="video",codec_name="h264",width=1920,height=1080),dict(codec_type="audio",index=1,sample_rate="48000")],frames=[])
  with self.assertRaises(RuntimeError):ab.validate_clip_streams(data,"native",20)


class MatchedHardwareAndResume(unittest.TestCase):
 def hardware(self, vendor=0x10de):
  displays=[dict(index=0,device="display-one",adapter_luid="0:78795",adapter_vendor=vendor,desktop_width=2560,desktop_height=1600,refresh_hz=240)]
  topology=dict(adapters=[dict(adapter_luid="0:78795",description="test adapter",vendor_id=vendor,outputs=[dict(name="display-one",attached=True,currently_selectable_in_app=True)])])
  return displays,topology
 def test_hardware_matching_follows_active_capture_adapter(self):
  for vendor,encoder in [(0x10de,"h264_nvenc"),(0x8086,"h264_qsv"),(0x1002,"h264_amf")]:
   displays,topology=self.hardware(vendor)
   detected=ab.select_hardware(displays,topology)
   self.assertEqual(detected["encoder"],encoder)
   self.assertEqual(detected["native_luid"],"00000000000133CB")
  displays,topology=self.hardware()
  with self.assertRaises(RuntimeError):ab.select_hardware(displays*2,topology)
  topology["adapters"][0]["outputs"][0]["currently_selectable_in_app"]=False
  with self.assertRaises(RuntimeError):ab.select_hardware(displays,topology)
 def test_encoder_gpu_must_match_detected_capture_gpu(self):
  hardware=ab.select_hardware(*self.hardware());s=summary();s["hardware"]=hardware
  for phase in s["phases"]:
   if phase["config"]:
    phase["gpu_engines_percent"]={"ffmpeg.exe|0x00000000_0x000133cb|phys0|engine6|videoencode":10}
    if phase["config"]==ab.NATIVE:phase["native_completion"]={"capture_encoder_adapter_luid":hardware["native_luid"]}
  self.assertTrue(ab.evaluate(s)["eligible"])
  s["phases"][3]["native_completion"]["capture_encoder_adapter_luid"]="0000000000099999"
  self.assertFalse(ab.evaluate(s)["eligible"])
 def test_unstable_off_phase_preserves_clip_audit_and_rejects_fps(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary)
   output=root/"test-output/motion"
   presentmon=root/"presentmon.exe"
   presentmon.touch()
   hardware=ab.select_hardware(*self.hardware())
   phases=summary()
   phases["phases"][-1]["frames"]["fps"]=1
   def complete_unit(command,unit,timeout):
    unit.mkdir()
    (unit/"summary.json").write_text(json.dumps(phases))
    return 0
   clips=[dict(sha256="audited")]
   args=["cs2_ab","--execute","--approved-gameplay","--out",str(output),"--presentmon",str(presentmon)]
   with patch.object(ab,"ROOT",root),patch.object(ab,"benchmark_provenance",return_value={}),patch.object(ab,"detect_hardware",return_value=hardware),patch.object(ab,"run_unit",side_effect=complete_unit),patch.object(ab,"audit_clips",return_value=clips) as audit,patch.object(sys,"argv",args),contextlib.redirect_stdout(io.StringIO()),self.assertRaises(SystemExit) as stop:
    ab.main()
   audit.assert_called_once()
   self.assertEqual(stop.exception.code,1)
   ledger=json.loads((output/"benchmark-ledger.json").read_text())
   attempt=ledger["attempts"][0]
   self.assertIn("OFF FPS drift exceeds declared threshold",attempt["evaluation"]["reasons"])
   self.assertEqual(attempt["state"],"rejected")
   self.assertEqual(attempt["clips"],clips)
   self.assertFalse(ab.totals(ledger,[])["performance_claim_allowed"])
 def test_checkpoint_resume_alternates_order_and_pins_build(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);output=root/"test-output/fresh";presentmon=root/"presentmon.exe";presentmon.write_bytes(b"test")
   hardware=ab.select_hardware(*self.hardware())
   for name in ["replayx-cli.exe","replayx-native-proto.exe"]:
    path=root/"src-tauri/target/release"/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b"fixture")
   runtime=root/"tools/runtime";runtime.mkdir(parents=True)
   for name in ["ffmpeg.exe","ffprobe.exe"]:(runtime/name).write_bytes(b"fixture")
   (root/"tools/ffmpeg-lock.json").write_text(json.dumps(dict(runtime_dir="runtime")))
   orders=[]
   def fake_unit(command,unit,timeout):
    unit.mkdir();chosen=command[command.index("--configs")+1].split(",");orders.append(chosen)
    self.assertIn("--focus-wait",command);self.assertEqual(command[command.index("--seconds")+1],"20")
    self.assertEqual(command[command.index("--matched-encoder")+1],"h264_nvenc")
    value=summary()
    if chosen[0]==ab.NATIVE:value["phases"][1],value["phases"][3]=value["phases"][3],value["phases"][1]
    for phase in value["phases"]:
     if phase["config"]:
      phase["gpu_engines_percent"]={"ffmpeg.exe|0x00000000_0x000133cb|phys0|engine6|videoencode":10}
      if phase["config"]==ab.NATIVE:phase["native_completion"]={"capture_encoder_adapter_luid":hardware["native_luid"]}
    (unit/"summary.json").write_text(json.dumps(value));return 0
   base=["cs2_ab","--execute","--approved-gameplay","--out",str(output),"--presentmon",str(presentmon)]
   with patch.object(ab,"ROOT",root),patch.object(ab,"detect_hardware",return_value=hardware),patch.object(ab,"run_unit",side_effect=fake_unit),patch.object(ab,"audit_clips",return_value=[dict(sha256="test")]),contextlib.redirect_stdout(io.StringIO()):
    with patch.object(sys,"argv",base):ab.main()
    first=json.loads((output/"benchmark-ledger.json").read_text())["attempts"][0]
    with patch.object(sys,"argv",base+["--resume"]):ab.main()
    ledger=json.loads((output/"benchmark-ledger.json").read_text())
    self.assertEqual(ledger["attempts"][0],first)
    self.assertEqual(orders,[ab.order(0),ab.order(1)])
    self.assertEqual(len(ledger["attempts"]),2)
    (root/"src-tauri/target/release/replayx-native-proto.exe").write_bytes(b"changed")
    with patch.object(sys,"argv",base+["--resume"]),contextlib.redirect_stderr(io.StringIO()),self.assertRaises(SystemExit):ab.main()
    self.assertEqual(len(orders),2)
    (root/"src-tauri/target/release/replayx-native-proto.exe").write_bytes(b"fixture")
    failed_output=root/"test-output/failed"
    failed_args=["cs2_ab","--execute","--approved-gameplay","--out",str(failed_output),"--presentmon",str(presentmon),"--units-per-run","2"]
    with patch.object(ab,"run_unit",return_value=1) as failed_run,patch.object(sys,"argv",failed_args),self.assertRaises(SystemExit):
     ab.main()
    failed_run.assert_called_once()
    failed_ledger=json.loads((failed_output/"benchmark-ledger.json").read_text())
    self.assertEqual(len(failed_ledger["attempts"]),1)
    self.assertEqual(failed_ledger["attempts"][0]["state"],"rejected")
 def test_rejected_order_is_retried_without_biasing_the_accepted_order(self):
  row=dict(order=ab.order(0),evaluation=dict(eligible=False),clips=[dict(sha256="bad")])
  ledger=dict(attempts=[row]);self.assertEqual(ab.next_order(ledger,[]),ab.order(0))
  row["evaluation"]["eligible"]=True
  self.assertEqual(ab.next_order(ledger,[]),ab.order(1))

class ReverseContinuation(unittest.TestCase):
 def test_offline_preparation_and_exactly_one_reverse_execution(self):
  with tempfile.TemporaryDirectory() as temporary:
   root = Path(temporary)
   source = root / "test-output/original"
   source.mkdir(parents=True)
   destination = root / "test-output/continuation"
   presentmon = root / "PresentMon.exe"
   presentmon.write_bytes(b"fixture")
   clip = source / "clip.mkv"
   clip.write_bytes(b"clip")
   controller = str(Path(ab.__file__).resolve())
   provenance = {controller: "updated-audit", "live-recorder": "unchanged"}
   hardware = {"encoder": "h264_nvenc", "counter_luid": "gpu", "native_luid": "worker", "capture_adapter": "fixture"}
   first = summary()
   first["hardware"] = hardware
   for phase in first["phases"]:
    if phase["config"]:
     phase["gpu_engines_percent"] = {"ffmpeg.exe|gpu|phys0|videoencode": 18}
     if phase["config"] == ab.NATIVE:
      phase["native_completion"] = {"capture_encoder_adapter_luid": "worker"}
   original = source / "benchmark-ledger.json"
   ab.write(original, dict(comparison_schema=2, hardware=hardware,
      settings=dict(cap=1000, seconds=20, target_pairs=2, max_attempts=4, warmup=30,
                    audio="system", native_interval_us=6666, max_off_spread_percent=5.0, max_off_temp_delta_c=5.0),
      provenance={**provenance, controller: "legacy-audit"},
      attempts=[dict(attempt=1, state="rejected", error="Output cadence/count failure",
                     order=ab.order(0), summary=first)]))
   original_bytes = original.read_bytes()
   clips = [dict(path=str(clip), sha256=ab.digest(clip), subframe_join_intervals=[{"interval_ms": 21.0}])] * 4
   common = ["cs2_ab", "--cap", "1000", "--warmup", "30", "--out", str(destination),
             "--presentmon", str(presentmon)]
   with patch.object(ab, "ROOT", root), patch.object(ab, "benchmark_provenance", return_value=provenance), patch.object(ab, "audit_clips", return_value=clips), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
    with patch.object(ab, "run_unit", side_effect=AssertionError("launched")), patch.object(sys, "argv", common + ["--prepare-reverse-from", str(source)]):
     ab.main()
    self.assertEqual(json.loads((destination/"reverse-plan.json").read_text())["phases"],
                     [[list(phase) for phase in ab.session_timing.phase_plan(ab.order(1))]])
    self.assertEqual(original.read_bytes(), original_bytes)
    start = common + ["--execute", "--approved-gameplay", "--resume"]
    def fake_unit(cmd, unit, timeout):
     self.assertEqual(cmd[cmd.index("--configs")+1].split(","), ab.order(1))
     self.assertEqual(unit.name, "pair-02")
     unit.mkdir()
     value = copy.deepcopy(first)
     value["phases"][1], value["phases"][3] = value["phases"][3], value["phases"][1]
     (unit/"summary.json").write_text(json.dumps(value))
     return 0
    with patch.object(ab, "detect_hardware", return_value=hardware), patch.object(ab, "run_unit", side_effect=fake_unit) as fake:
     with patch.object(sys, "argv", start + ["--units-per-run", "2"]), self.assertRaises(SystemExit):
      ab.main()
     fake.assert_not_called()
     with patch.object(sys, "argv", start):
      ab.main()
      with self.assertRaises(SystemExit):
       ab.main()
     fake.assert_called_once()
   self.assertEqual(original.read_bytes(), original_bytes)


if __name__=="__main__":unittest.main()
