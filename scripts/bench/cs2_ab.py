"""Bounded, resumable CS2 OFF/FFmpeg/OFF/Native/OFF preparation.
Default is an offline plan. --execute --approved-gameplay requires explicit
approval and an already-running foreground CS2; it never launches games.
Each invocation checkpoints one short matched unit by default.
Metadata cannot prove gameplay smoothness or acoustic sync: those remain explicit
review gates, tied to the exact clip hashes, before any performance claim.
"""
import argparse, hashlib, json, math, os, statistics, subprocess, sys, time
from pathlib import Path
import session_timing
ROOT=Path(__file__).resolve().parents[2]
FF="app-ffmpeg-matched-steady"
NATIVE="app-native-matched-phase"
CROSSHAIR="app-ffmpeg-matched-crosshair"
AUDIT_SECONDS=120
LEGACY_CONFIGS = {"app-ffmpeg-nvenc-fixed": FF, "app-native-nvenc-phase": NATIVE}


def config_id(value):
    return LEGACY_CONFIGS.get(value, value)


def digest(path):
 with Path(path).open("rb") as source:
  return hashlib.file_digest(source,"sha256").hexdigest()
def write(path,value):
 tmp=path.with_suffix(path.suffix+".tmp");tmp.write_text(json.dumps(value,indent=2),encoding="utf-8");os.replace(tmp,path)
def order(index, crosshair=False):
    pair = [FF, CROSSHAIR if crosshair else NATIVE]
    return pair if index % 2 == 0 else pair[::-1]


def plan(seconds=20, warmup=60, pairs=2, attempts=4, units_per_run=1, native_interval_us=6666, crosshair=False):
    units = min(pairs, units_per_run, attempts)
    unit = session_timing.estimate(order(0, crosshair), seconds, warmup, 15)["expected_total_seconds"]
    return {
        "phase_seconds": seconds, "presentmon_retained_seconds_per_phase": seconds-5, "initial_warmup_seconds": warmup,
        "target_pairs": pairs, "attempt_limit": attempts, "units_this_invocation": units,
        "expected_capture_seconds": units * unit,
        "maximum_offline_audit_seconds": units * AUDIT_SECONDS,
        "maximum_total_seconds": units * (unit + AUDIT_SECONDS + 30 + 120),
        "maximum_focus_wait_seconds_per_unit": 120,
        "phases": [session_timing.phase_plan(order(i, crosshair)) for i in range(units)],
        "profiles": ({"ffmpeg": "Steady 2x WGC, Crosshair OFF",
                      "ffmpeg_crosshair": "Same FFmpeg profile, Crosshair ON"}
                     if crosshair else
                     {"ffmpeg": "Explicit steady 2x WGC, existing quality settings",
                      "native": f"Phase-lock with explicit {native_interval_us} us capture interval"}),
        "timing_scope": "Scheduled capture time includes warm-up, countdown, settling and cleanup. Offline decoding and foreground waiting have separate upper bounds. Each invocation checkpoints a complete five-phase unit. At least two fresh opposite-order units are required. No game is launched."
    }


def select_hardware(displays, topology):
    if len(displays) != 1:
        raise RuntimeError("The comparison requires one active display, either the built-in panel or an external monitor")
    display = displays[0]
    adapters = [a for a in topology["adapters"] if a["adapter_luid"] == display["adapter_luid"]]
    if len(adapters) != 1:
        raise RuntimeError("Capture display has no unique adapter")
    adapter = adapters[0]
    outputs = [o for o in adapter["outputs"] if o["attached"] and o["name"] == display["device"]]
    if len(outputs) != 1 or not outputs[0]["currently_selectable_in_app"] or display["index"] != 0:
        raise RuntimeError("The current native selector cannot address this output safely")
    encoders = {0x10de: "h264_nvenc", 0x8086: "h264_qsv", 0x1002: "h264_amf"}
    encoder = encoders.get(adapter["vendor_id"])
    if encoder is None or adapter["vendor_id"] != display["adapter_vendor"]:
        raise RuntimeError("Unsupported or inconsistent capture adapter")
    high, low = map(int, display["adapter_luid"].split(":"))
    return {"display": display, "capture_adapter": adapter["description"],
            "vendor_id": adapter["vendor_id"], "encoder": encoder,
            "native_luid": f"{high & 0xffffffff:08X}{low:08X}",
            "counter_luid": f"0x{high & 0xffffffff:08x}_0x{low:08x}",
            "qualification": "Detected only; actual encoder report and GPU-engine adapter must match in each fresh phase"}


def detect_hardware():
    cli = ROOT / "src-tauri/target/release/replayx-cli.exe"
    def query(action):
        result = subprocess.run([str(cli), action], capture_output=True, text=True, encoding="utf-8", check=True, timeout=15)
        return json.loads(result.stdout)
    return select_hardware(query("displays"), query("native-topology"))


def evaluate(summary, max_spread=5., max_temp_delta=5., crosshair=False):
 reasons=[];phases=summary.get("phases",[])
 if summary.get("errors"): reasons.append("Session/focus/process failure: "+str(summary["errors"]))
 expected_sequences = [[None, pair[0], None, pair[1], None] for pair in (order(0, crosshair), order(1, crosshair))]
 if [config_id(p.get("config")) for p in phases] not in expected_sequences:
  reasons.append("Five complete alternating phases with both neighboring OFF baselines required")
 if not summary.get("presentmon"): reasons.append("PresentMon FPS unavailable")
 comparisons=session_timing.neighboring_comparisons(phases)
 if len(comparisons)!=2 or not all(c.get("usable") for c in comparisons): reasons.append("Invalid recording/neighbor baseline")
 if any(c.get("baseline_spread_percent",math.inf)>max_spread for c in comparisons): reasons.append("OFF FPS drift exceeds declared threshold")
 for p in phases:
  nv=p.get("nvidia") or {};f=p.get("frames") or {}
  if f.get("cap_limited"):reasons.append(p["phase"]+": frame cap limits comparison")
  if not isinstance(p.get("cpu_effective_clock_percent"),(int,float)) or not math.isfinite(p["cpu_effective_clock_percent"]):reasons.append(p["phase"]+": CPU clock telemetry unavailable")
  if not all(isinstance(f.get(k),(int,float)) and math.isfinite(f[k]) and f[k]>0 for k in ["fps","low1_fps","p99_ms"]): reasons.append(p["phase"]+": invalid FPS/lows/P99")
  if not all(isinstance(nv.get(k),(int,float)) and math.isfinite(nv[k]) for k in ["temp_c","graphics_mhz"]): reasons.append(p["phase"]+": thermal/clock telemetry unavailable")
  if not p.get("process_cpu_percent") or not p.get("process_rss_mib") or not p.get("gpu_engines_percent"): reasons.append(p["phase"]+": resource telemetry unavailable")
  overlay = p.get("crosshair") or {}
  overlay_expected = crosshair and config_id(p.get("config")) == CROSSHAIR
  if overlay_expected:
   if overlay.get("shown") is not True or overlay.get("excluded_from_capture") is not True or overlay.get("error"):
    reasons.append(p["phase"] + ": Crosshair visibility or capture exclusion unverified")
  elif overlay.get("shown"):
   reasons.append(p["phase"] + ": Crosshair unexpectedly shown")
  if p.get("config"):
   r=p.get("recorder") or {}
   expected="native" if config_id(p["config"])==NATIVE else "ffmpeg"
   if expected=="native" and not any("--pacing" in c and c[c.index("--pacing")+1:c.index("--pacing")+2]==["phase-lock"] and "--phase-source" in c and c[c.index("--phase-source")+1:c.index("--phase-source")+2]==["spatial"] for c in p.get("native_worker_commands",[])):
    reasons.append(p["phase"]+": experimental worker pacing was not verified")
   hardware = summary.get("hardware")
   encoder = hardware["encoder"] if hardware else "h264_nvenc"
   if r.get("recording_engine")!=expected or r.get("engine_fallback") or r.get("encoder")!=encoder or r.get("recovery_sessions",0): reasons.append(p["phase"]+": wrong backend/fallback/recovery")
   if hardware:
    encode_engines = [key.lower() for key in p.get("gpu_engines_percent", {}) if "videoencode" in key.lower() and key.startswith(("ffmpeg.exe|", "replayx-native-proto.exe|"))]
    if not encode_engines or any(hardware["counter_luid"] not in key for key in encode_engines):
     reasons.append(p["phase"]+": encoder GPU was not verified on the capture adapter")
    if expected == "native" and (p.get("native_completion") or {}).get("capture_encoder_adapter_luid") != hardware["native_luid"]:
     reasons.append(p["phase"]+": native capture/encoder adapter mismatch")
   if any(v.get("growth",0)>64 or v.get("peak",0)>512 for k,v in p.get("process_rss_mib",{}).items() if k.startswith("replayx") or k=="ffmpeg.exe"): reasons.append(p["phase"]+": recorder memory guard")
 for key in ["application","process_id","selected_swapchain","interval_column"]:
  values=[p.get("frames",{}).get(key) for p in phases]
  if any(v is None for v in values) or len(set(map(str,values)))!=1:reasons.append("PresentMon "+key+" mismatch/unavailable")
 off=[p for p in phases if not p.get("config")]
 if len(off)==3:
  cpu_clocks=[p.get("cpu_effective_clock_percent") for p in off]
  if all(isinstance(c,(int,float)) for c in cpu_clocks) and min(cpu_clocks)<.9*max(cpu_clocks):reasons.append("OFF CPU clock drift")
 if len(off)==3 and all(p.get("nvidia") for p in off):
  temps=[p["nvidia"].get("temp_c") for p in off];clocks=[p["nvidia"].get("graphics_mhz") for p in off]
  if all(isinstance(t,(int,float)) for t in temps) and max(temps)-min(temps)>max_temp_delta: reasons.append("OFF thermal drift")
  if all(isinstance(c,(int,float)) for c in clocks) and min(clocks)<.9*max(clocks): reasons.append("OFF GPU clock drift")
 return {"eligible":not reasons,"reasons":sorted(set(reasons)),"comparisons":comparisons}

def review_state(clips,reviews):
 if not clips:return "pending"
 flags=["motion_review_passed","picture_quality_review_passed","av_sync_review_passed","audio_clean_passed"]
 pending=False
 for clip in clips:
  required = flags
  if clip.get("crosshair_enabled"):
   required = flags + ["crosshair_visible_in_game_review_passed", "crosshair_exclusion_review_passed"]
  matching=[r for r in reviews if r.get("clip_sha256")==clip["sha256"] and r.get("reviewer")]
  if not matching:pending=True;continue
  # Last explicit review wins; earlier reviews remain in the supplied artifact.
  review=matching[-1]
  if any(review.get(k) is False for k in required):return "rejected"
  pending |= not all(review.get(k) is True for k in required)
 return "pending" if pending else "accepted"

def review_ok(clips,reviews):return review_state(clips,reviews)=="accepted"

def validate_clip_streams(data, expected, minimum_seconds):
    videos = [stream for stream in data["streams"] if stream["codec_type"] == "video"]
    audio = [stream for stream in data["streams"] if stream["codec_type"] == "audio"]
    if len(videos) != 1 or videos[0]["codec_name"] != "h264" or (videos[0]["width"], videos[0]["height"]) != (1920, 1080):
        raise RuntimeError("Wrong output canvas/codec")
    if not audio:
        raise RuntimeError("System audio track absent")
    frames = [frame for frame in data["frames"] if frame["media_type"] == "video"]
    pts = [float(frame["best_effort_timestamp_time"]) for frame in frames]
    if len(pts) < minimum_seconds * 60 or not all(math.isfinite(t) for t in pts):
        raise RuntimeError("Output frame count too low or timestamp missing")
    keys = [i for i, frame in enumerate(frames) if frame["key_frame"]]
    if not keys or keys[0] != 0 or any(b-a > 121 for a, b in zip(keys, keys[1:])) or len(pts)-keys[-1] > 121:
        raise RuntimeError("Keyframe gap/start failure")
    # Matroska's millisecond time base allows 16 and 17 ms intervals at 60 FPS.
    # FFmpeg segment joins can add a small phase step without losing a frame.
    # Report the step and bound its total. Keep Native and intra-GOP timing strict.
    joins = []
    extra_join_delay = 0.0
    for index, (before, after) in enumerate(zip(pts, pts[1:]), 1):
        interval = after - before
        excess = interval - 1/60
        if abs(excess) <= .0011:
            continue
        if expected == "ffmpeg" and frames[index]["key_frame"] and 0 < excess <= .005:
            extra_join_delay += excess
            if extra_join_delay <= .005:
                joins.append({"frame": index, "timestamp": after, "interval_ms": round(interval * 1000, 3)})
                continue
        raise RuntimeError(f"Output cadence failure at frame {index}: {interval * 1000:.3f} ms")
    b_frames = sum(frame["pict_type"] == "B" for frame in frames)
    if expected == "native" and (b_frames or keys != list(range(0, len(frames), 120))):
        raise RuntimeError("Unexpected native frame ordering or keyframes")
    audio_timings = []
    for stream in audio:
        decoded = [frame for frame in data["frames"] if frame["media_type"] == "audio" and frame["stream_index"] == stream["index"]]
        if not decoded:
            raise RuntimeError("Audio track has no decoded frames")
        start = float(decoded[0]["best_effort_timestamp_time"])
        end = float(decoded[-1]["best_effort_timestamp_time"]) + int(decoded[-1]["nb_samples"]) / int(stream["sample_rate"])
        # MKV does not require stream.duration. Compare actual decoded timelines.
        if not all(map(math.isfinite, [start, end])) or abs(start-pts[0]) > .15 or abs(end-(pts[-1]+1/60)) > .15:
            raise RuntimeError("Audio/video timeline mismatch")
        audio_timings.append({"stream_index": stream["index"], "start": start, "end": end})
    return {"frames": len(frames), "b_frames": b_frames, "keyframes": keys,
            "subframe_join_intervals": joins, "audio_timings": audio_timings,
            "video_duration": pts[-1]-pts[0]+1/60}


def audit_clips(unit, deadline, minimum_seconds=20):
    lock = json.loads((ROOT / "tools/ffmpeg-lock.json").read_text(encoding="utf-8-sig"))
    bins = ROOT / "tools" / lock["runtime_dir"]
    clips, verified = [], {}
    def run(command):
        left = deadline-time.monotonic()
        if left <= 0:
            raise RuntimeError("Offline quality audit deadline exceeded")
        return subprocess.run(list(map(str, command)), capture_output=True, check=True, timeout=left).stdout
    reports = list(unit.glob("B*/test-output/capture-test-*/report.json"))
    if len(reports) != 2:
        raise RuntimeError("Exactly two app recording reports required")
    for report in reports:
        data = json.loads(report.read_text(encoding="utf-8-sig"))
        expected = "native" if "app-native-" in str(report) else "ffmpeg"
        for name, timing_key in [("first", "first_ready_ms"), ("concurrent", "second_ready_ms")]:
            clip = Path(data[name]).resolve()
            clip.relative_to(unit.resolve())
            sha = digest(clip)
            if sha in verified:
                row = dict(verified[sha], path=str(clip), same_bytes_as=verified[sha]["path"])
            else:
                streams = json.loads(run([bins / "ffprobe.exe", "-v", "error", "-show_streams", "-show_frames", "-of", "json", clip]))
                try:
                    row = validate_clip_streams(streams, expected, minimum_seconds)
                except RuntimeError as error:
                    raise RuntimeError(f"{expected} clip {clip.name}: {error}") from error
                # One independent full decode hashes video and decodes every audio track.
                output = run([bins / "ffmpeg.exe", "-v", "error", "-xerror", "-threads", "2", "-i", clip,
                              "-map", "0:v:0", "-map", "0:a?", "-c:v", "rawvideo", "-threads:v", "1",
                              "-c:a", "pcm_s16le", "-fps_mode", "passthrough", "-f", "framemd5", "-"]).decode()
                hashes = [line.rsplit(",", 1)[-1].strip() for line in output.splitlines()
                          if line and not line.startswith("#") and line.split(",", 1)[0].strip() == "0"]
                if len(hashes) != row["frames"]:
                    raise RuntimeError("Independent decoded frame count mismatch")
                row.update(path=str(clip), sha256=sha,
                           exact_decoded_repeats=sum(a == b for a, b in zip(hashes, hashes[1:])),
                           metadata_and_full_decode_passed=True,
                           quality_limit="Static scenes legitimately repeat. Motion fidelity and acoustic synchronization require the hash-bound review.")
                verified[sha] = row
            if expected == "native" and (row["b_frames"] or row["keyframes"] != list(range(0,row["frames"],120))):
                raise RuntimeError("Unexpected native B frames")
            row = dict(row, save_ready_ms=data[timing_key],
                       crosshair_enabled=bool((data.get("crosshair") or {}).get("shown")))
            if not isinstance(row["save_ready_ms"], (int, float)) or row["save_ready_ms"] < 0:
                raise RuntimeError("Save availability timing missing")
            clips.append(row)
    return clips


def next_order(ledger, reviews):
    crosshair = ledger.get("settings", {}).get("crosshair", False)
    first, second = order(0, crosshair), order(1, crosshair)
    counts = {tuple(first): 0, tuple(second): 0}
    for row in ledger["attempts"]:
        if row.get("evaluation", {}).get("eligible") and row.get("clips") and review_state(row["clips"], reviews) != "rejected":
            key = tuple(config_id(c) for c in row.get("order", []))
            if key in counts:
                counts[key] += 1
    return first if counts[tuple(first)] <= counts[tuple(second)] else second


def totals(ledger,reviews):
 crosshair = ledger["settings"].get("crosshair", False)
 comparison = CROSSHAIR if crosshair else NATIVE
 accepted=[];pending=[];rejected=[]
 for row in ledger["attempts"]:
  if not row.get("evaluation",{}).get("eligible") or not row.get("clips"):continue
  {"accepted":accepted,"pending":pending,"rejected":rejected}[review_state(row["clips"],reviews)].append(row)
 gains=[]
 for r in accepted:
  by={config_id(c["config"]):c for c in r["evaluation"]["comparisons"]}
  difference = by[comparison]["fps_cost_percent"] - by[FF]["fps_cost_percent"]
  gains.append(difference if crosshair else -difference)
 means={}
 for config in [None,FF,comparison]:
  phases=[p for row in accepted for p in row.get("summary",{}).get("phases",[]) if config_id(p.get("config"))==config]
  if phases:means[config or "OFF"]={key:statistics.mean(p["frames"][key] for p in phases) for key in ["fps","low1_fps","p99_ms"]}
 balanced = {tuple(config_id(c) for c in r.get("order", [])) for r in accepted} >= {tuple(order(0, crosshair)), tuple(order(1, crosshair))}
 result={"order_balanced":balanced,"accepted_phase_means":means,"accepted_pairs":len(accepted),"quality_pending_pairs":len(pending),"quality_rejected_pairs":len(rejected),"attempted_pairs":len(ledger["attempts"]),("crosshair_cost_percentage_points" if crosshair else "gain_percentage_points"):gains,
  "performance_claim_allowed":balanced and len(accepted)>=ledger["settings"]["target_pairs"],"limit":"Short paired comparisons are exploratory. Opposite-order accepted units and clip quality reviews are required. Prior sessions are not pooled automatically."}
 if len(gains)>=2:
  critical={2:12.706205,3:4.302653,4:3.182446,5:2.776445,6:2.570582,7:2.446912,8:2.364624}[len(gains)]
  mean=statistics.mean(gains);margin=critical*statistics.stdev(gains)/math.sqrt(len(gains))
  result["exploratory_95_percent_interval"] = [mean-margin, mean+margin]
  if crosshair:
   result["mean_crosshair_cost_percentage_points"] = mean
  else:
   result.update(mean_gain_percentage_points=mean, direction_favors_native=mean-margin>0)
 return result

def run_unit(cmd,unit,timeout):
 from native_v1_support import Job
 gate=unit.parent/(unit.name+".start")
 job=Job();process=None
 try:
  env=dict(os.environ,REPLAYX_BENCH_START_GATE=str(gate))
  process=subprocess.Popen(cmd,cwd=ROOT,env=env)
  job.attach(process)
  # The child waits before launching telemetry/recorders, closing the attach race.
  gate.touch(exist_ok=False)
  return process.wait(timeout=timeout)
 finally:
  job.close() # Kill every descendant on timeout/cancellation too.
  if process is not None:
   process.wait(timeout=5)

def benchmark_provenance(presentmon):
    files = [ROOT / "src-tauri/target/release" / name
             for name in ["replayx-native-proto.exe", "replayx-cli.exe"]]
    files += [Path(__file__), Path(__file__).with_name("fortnite-session.py"),
              Path(__file__).with_name("session_timing.py"), Path(__file__).with_name("native_v1_support.py"),
              Path(__file__).with_name("compare-frametimes.py")]
    lock = json.loads((ROOT / "tools/ffmpeg-lock.json").read_text(encoding="utf-8-sig"))
    runtime = ROOT / "tools" / lock["runtime_dir"]
    files += [runtime / "ffmpeg.exe", runtime / "ffprobe.exe", ROOT / "src-tauri/target/release/media/ffmpeg.exe"]
    fingerprints = {str(path.resolve()): digest(path) for path in files if path.exists()}
    fingerprints[str(presentmon.resolve())] = digest(presentmon)
    return fingerprints


def prepare_reverse(source, out, settings, provenance):
    """Copy one cadence-rechecked forward unit into an isolated continuation."""
    source = source.resolve()
    source.relative_to((ROOT / "test-output").resolve())
    source_path = source / "benchmark-ledger.json"
    if out.exists():
        raise RuntimeError("Reverse continuation requires a fresh output directory")
    source_hash = digest(source_path)
    original = json.loads(source_path.read_text(encoding="utf-8-sig"))
    attempts = original["attempts"]
    if (original["comparison_schema"] != 2 or original["settings"] != settings
            or len(attempts) != 1 or attempts[0]["attempt"] != 1
            or attempts[0]["order"] != order(0) or attempts[0]["state"] != "rejected"
            or attempts[0]["error"] != "Output cadence/count failure"):
        raise RuntimeError("Source is not the matching forward unit stopped by the original cadence guard")
    controller = str(Path(__file__).resolve())
    if (controller not in original["provenance"] or controller not in provenance
            or {k: v for k, v in original["provenance"].items() if k != controller}
            != {k: v for k, v in provenance.items() if k != controller}):
        raise RuntimeError("Live recorder, benchmark support scripts or measurement tools changed")
    first = attempts[0]
    if first["summary"].get("hardware") != original["hardware"]:
        raise RuntimeError("Forward unit adapter evidence is missing or inconsistent")
    evaluation = evaluate(first["summary"])
    if not evaluation["eligible"]:
        raise RuntimeError(f"Forward unit failed the live measurement gates: {evaluation['reasons']}")
    clips = audit_clips(source / "pair-01", time.monotonic() + AUDIT_SECONDS, settings["seconds"])
    if len(clips) != 4 or not any(clip["subframe_join_intervals"] for clip in clips):
        raise RuntimeError("Recheck did not reproduce the FFmpeg subframe segment join")
    if digest(source_path) != source_hash:
        raise RuntimeError("Source ledger changed during preparation")
    continuation = dict(original, provenance=provenance,
                        attempts=[dict(first, state="quality_pending", error=None,
                                       evaluation=evaluation, clips=clips)],
                        reverse_source={"ledger": str(source_path), "sha256": source_hash,
                                        "historical_rejection": first["error"]})
    out.mkdir(parents=True, exist_ok=False)
    write(out / "benchmark-ledger.json", continuation)
    schedule = plan(seconds=settings["seconds"], warmup=settings["warmup"],
                    native_interval_us=settings["native_interval_us"])
    schedule["phases"] = [session_timing.phase_plan(order(1))]
    schedule["timing_scope"] = "One reverse-order unit. Foreground wait and offline decode are separately bounded."
    write(out / "reverse-plan.json", schedule)
    return schedule


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--execute", action="store_true")
    ap.add_argument("--crosshair", action="store_true", help="Compare Crosshair OFF/ON with the same FFmpeg recording profile")
    ap.add_argument("--approved-gameplay", action="store_true")
    ap.add_argument("--resume", action="store_true")
    ap.add_argument("--analyze-only", action="store_true")
    ap.add_argument("--prepare-reverse-from", type=Path, help="Offline continuation from a forward unit rejected by the original cadence guard")
    ap.add_argument("--reviews", type=Path)
    ap.add_argument("--readiness", type=Path, help="Optional previous qualification artifact, retained for context")
    ap.add_argument("--presentmon", type=Path)
    ap.add_argument("--cap", type=int, default=0, help="Actual game cap. This does not change it.")
    ap.add_argument("--seconds", type=int, default=20)
    ap.add_argument("--pairs", type=int, default=2)
    ap.add_argument("--max-attempts", type=int, default=4)
    ap.add_argument("--units-per-run", type=int, default=1)
    ap.add_argument("--warmup", type=int, default=60)
    ap.add_argument("--native-interval-us", type=int, choices=[0, 6666], default=6666)
    a = ap.parse_args()
    if not (0 <= a.cap <= 2000 and 20 <= a.seconds <= 90 and 2 <= a.pairs <= 6
            and a.pairs <= a.max_attempts <= 8 and 1 <= a.units_per_run <= 2 and 30 <= a.warmup <= 180):
        ap.error("Bounded duration, warm-up and pair settings required")
    if a.prepare_reverse_from and (a.execute or a.analyze_only or a.resume or a.crosshair or a.units_per_run != 1):
        ap.error("Reverse preparation is offline and takes one engine unit only")
    if not a.execute and not a.analyze_only and not a.prepare_reverse_from:
        print(json.dumps(plan(a.seconds, a.warmup, a.pairs, a.max_attempts, a.units_per_run, a.native_interval_us, a.crosshair), indent=2))
        return
    if a.out is None:
        ap.error("--out is required")
    out = a.out.resolve()
    out.relative_to((ROOT / "test-output").resolve())
    path = out / "benchmark-ledger.json"
    settings = {"cap": a.cap, "seconds": a.seconds, "target_pairs": a.pairs,
                "max_attempts": a.max_attempts, "warmup": a.warmup, "audio": "system", "native_interval_us": a.native_interval_us,
                "max_off_spread_percent": 5., "max_off_temp_delta_c": 5.}
    if a.crosshair:
        settings["crosshair"] = True
    reviews = json.loads(a.reviews.read_text(encoding="utf-8-sig")) if a.reviews else []
    if a.analyze_only:
        ledger = json.loads(path.read_text(encoding="utf-8-sig"))
        print(json.dumps(totals(ledger, reviews), indent=2))
        return
    if a.execute and not a.approved_gameplay:
        ap.error("Execution requires a new explicit gameplay approval (--approved-gameplay)")
    if not a.presentmon or not a.presentmon.is_file():
        ap.error("Existing PresentMon required; no download, installation or elevation is performed")
    provenance = benchmark_provenance(a.presentmon)
    if a.prepare_reverse_from:
        print(json.dumps(prepare_reverse(a.prepare_reverse_from, out, settings, provenance), indent=2))
        return
    hardware = detect_hardware()
    reverse_only = False
    if a.resume:
        ledger = json.loads(path.read_text(encoding="utf-8-sig"))
        if ledger["settings"] != settings or ledger["provenance"] != provenance or ledger["hardware"] != hardware:
            ap.error("Build, settings or display/adapter layout changed; keep this ledger and use a fresh session")
        if source := ledger.get("reverse_source"):
            reverse_only = True
            if a.units_per_run != 1 or len(ledger["attempts"]) != 1:
                ap.error("This continuation permits one reverse-order attempt only")
            if digest(Path(source["ledger"])) != source["sha256"] or any(
                digest(Path(clip["path"])) != clip["sha256"] for clip in ledger["attempts"][0]["clips"]
            ):
                ap.error("The original forward evidence changed after validation")
        for row in ledger["attempts"]:
            if row["state"] == "running":
                row.update(state="interrupted", error="Previous invocation ended before checkpoint completion")
    else:
        out.mkdir(parents=True, exist_ok=False)
        ledger = {"settings": settings, "provenance": provenance, "hardware": hardware, "attempts": [],
                  "native_qualification": json.loads(a.readiness.read_text(encoding="utf-8-sig")) if a.readiness else None,
                  "scope": "Fresh diagnostic matched comparisons. Prior external-display measurements are context only.", "comparison_schema": 2}
    write(path, ledger)
    status = totals(ledger, reviews)
    if status["accepted_pairs"] + status["quality_pending_pairs"] >= a.pairs:
        print(json.dumps(status, indent=2))
        return
    needed = a.pairs-status["accepted_pairs"]-status["quality_pending_pairs"]
    remaining_attempts = a.max_attempts-len(ledger["attempts"])
    if remaining_attempts <= 0:
        print("INCOMPLETE: attempt limit reached; existing results retained", flush=True)
        raise SystemExit(1)
    schedule = plan(a.seconds, a.warmup, needed, remaining_attempts, a.units_per_run, a.native_interval_us, a.crosshair)
    print("Scheduled capture time this invocation:", session_timing.clock(schedule["expected_capture_seconds"]),
          "including warm-up. Offline audits take up to", schedule["maximum_offline_audit_seconds"], "additional seconds.", flush=True)
    print("Capture adapter:", hardware["capture_adapter"], "expected encoder:", hardware["encoder"], flush=True)
    failed = False
    for _ in range(a.units_per_run):
        status = totals(ledger, reviews)
        if status["accepted_pairs"] + status["quality_pending_pairs"] >= a.pairs or len(ledger["attempts"]) >= a.max_attempts:
            break
        chosen_order = order(1) if reverse_only else next_order(ledger, reviews)
        index = len(ledger["attempts"])
        unit = out / f"pair-{index+1:02}"
        row = {"attempt": index+1, "order": chosen_order, "state": "running"}
        ledger["attempts"].append(row)
        write(path, ledger)
        print(f"Matched unit {index+1}/{a.max_attempts}, order {chosen_order}. Five phases with neighboring OFF baselines.", flush=True)
        cmd = [sys.executable, str(Path(__file__).with_name("fortnite-session.py")), "--game", "cs2", "--out", str(unit),
               "--configs", ",".join(chosen_order), "--seconds", str(a.seconds), "--warmup", str(a.warmup),
               "--audio", "system", "--cap", str(a.cap), "--presentmon", str(a.presentmon),
               "--focus-wait", "120", "--matched-encoder", hardware["encoder"],
               "--native-interval-us", str(a.native_interval_us)]
        duration = session_timing.estimate(chosen_order, a.seconds, a.warmup, 15)["expected_total_seconds"]
        try:
            code = run_unit(cmd, unit, duration+120+30)
            if (unit / "summary.json").exists():
                row["summary"] = json.loads((unit / "summary.json").read_text(encoding="utf-8-sig"))
                row["summary"]["hardware"] = hardware
                row["evaluation"] = evaluate(row["summary"], crosshair=a.crosshair)
            if code:
                raise RuntimeError("Session failed; completed phases and recordings retained")
            eligible = row.get("evaluation", {}).get("eligible")
            if eligible and detect_hardware() != hardware:
                raise RuntimeError("Display or adapter layout changed during the comparison")
            # A complete unit can have useful footage even when its FPS
            # baselines fail. Preserve the audit while keeping the unit rejected.
            row["clips"] = audit_clips(unit, time.monotonic()+AUDIT_SECONDS, a.seconds)
            if not eligible:
                raise RuntimeError(str(row.get("evaluation")))
            row["state"] = "quality_pending"
        except KeyboardInterrupt:
            row.update(state="interrupted", error="User cancellation")
            write(path, ledger)
            raise
        except (RuntimeError, subprocess.SubprocessError, OSError, ValueError, KeyError, TypeError) as error:
            row.update(state="rejected", error=str(error))
            print("FAILED matched unit:", error, flush=True)
            failed = True
        write(path, ledger)
        if failed:
            break
    result = totals(ledger, reviews)
    result["reviews_sha256"] = digest(a.reviews) if a.reviews else None
    write(out / "comparison.json", result)
    print(json.dumps(result, indent=2))
    print("FAILED; partial results retained" if failed else "CHECKPOINT COMPLETE; resume retains completed matched units and alternates order", flush=True)
    if failed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
