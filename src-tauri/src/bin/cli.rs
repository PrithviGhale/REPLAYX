use anyhow::{bail, Result};
use replayx_core::{engine::Engine, media::Media, model::Settings};
use std::{path::PathBuf, thread, time::Duration};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let action = args.get(1).map(String::as_str).unwrap_or("help");
    if action == "audio-test" {
        return audio_test(&args);
    }
    if action == "voice-eval" {
        return voice_eval(&args);
    }
    if action == "overlay-render" {
        return overlay_render(&args);
    }
    if action == "overlay-test" {
        return overlay_test(&args);
    }
    if action == "start-bench" {
        return start_bench(&args);
    }
    if action == "displays" {
        println!(
            "{}",
            serde_json::to_string_pretty(&replayx_core::geometry::displays())?
        );
        return Ok(());
    }
    if action == "encoders" {
        println!(
            "{}",
            serde_json::to_string_pretty(&Media::discover()?.encoders())?
        );
        return Ok(());
    }
    if action == "devices" {
        println!(
            "{}",
            serde_json::to_string_pretty(&replayx_core::audio::devices()?)?
        );
        return Ok(());
    }
    if action == "audio-sessions" {
        let sessions = replayx_core::winutil::audio_sessions()?;
        for s in &sessions {
            println!(
                "{:>6}  {:<8} {}  {}",
                s.pid,
                if s.active { "playing" } else { "idle" },
                s.name,
                s.path
            );
        }
        if let (Some(s), Some(out)) = (sessions.iter().find(|s| !s.path.is_empty()), args.get(2)) {
            let png = replayx_core::winutil::exe_icon_png(std::path::Path::new(&s.path))
                .ok_or_else(|| anyhow::anyhow!("No icon"))?;
            std::fs::write(out, png)?;
        }
        return Ok(());
    }
    if action == "library-bench" {
        // Test D: library load time on fixture clips (hard links of one small
        // real MKV) in test-output; never touches the user's recordings.
        let media = Media::discover()?;
        let base = std::env::current_dir()?
            .join("test-output")
            .join("library-bench");
        std::fs::create_dir_all(&base)?;
        let sample = base.join("sample.mkv");
        if !sample.exists() {
            let ok = std::process::Command::new(&media.ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=s=1920x1080:r=60:d=4",
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=r=48000:cl=stereo",
                    "-t",
                    "4",
                    "-c:v",
                    "libopenh264",
                    "-c:a",
                    "aac",
                    "-shortest",
                ])
                .arg(&sample)
                .status()?
                .success();
            if !ok {
                bail!("Could not create the fixture clip")
            }
        }
        for n in [10usize, 100, 1000] {
            let root = base.join(format!("clips-{n}"));
            if !root.exists() {
                std::fs::create_dir_all(root.join("Fortnite"))?;
                std::fs::create_dir_all(root.join("Roblox"))?;
                for i in 0..n {
                    let rel = match i % 3 {
                        0 => format!("Fortnite/Fortnite_{i:05}.mkv"),
                        1 => format!("Roblox/Roblox_{i:05}.mkv"),
                        _ => format!("Replay-{i:05}.mkv"),
                    };
                    // NTFS allows 1023 links per file: rotate through copies.
                    let source = base.join(format!("sample-{}.mkv", i / 900));
                    if !source.exists() {
                        std::fs::copy(&sample, &source)?;
                    }
                    std::fs::hard_link(&source, root.join(rel))?;
                }
            }
            let index_file = base.join(format!("index-{n}.json"));
            let _ = std::fs::remove_file(&index_file);
            let t = std::time::Instant::now();
            if n <= 100 {
                for (_, path, _) in replayx_core::library::scan(&root)? {
                    let _ = media.probe(&path);
                }
            }
            let old_ms = (n <= 100).then(|| t.elapsed().as_millis());
            let t = std::time::Instant::now();
            let mut index = replayx_core::library::Index::load(&index_file);
            let cold = index.list(&root, 4, |p| media.probe(p).ok())?;
            index.save(&index_file)?;
            let cold_ms = t.elapsed().as_millis();
            let t = std::time::Instant::now();
            let mut warm_index = replayx_core::library::Index::load(&index_file);
            let warm = warm_index.list(&root, 4, |p| media.probe(p).ok())?;
            let warm_ms = t.elapsed().as_millis();
            let t = std::time::Instant::now();
            let filtered = warm
                .iter()
                .filter(|c| c.category == "Roblox" && c.name.contains('5'))
                .count();
            let filter_us = t.elapsed().as_micros();
            assert_eq!(cold.len(), n);
            println!("{n:>5} clips: old sequential probe {} | new cold (4 threads) {cold_ms} ms | new warm (index reload) {warm_ms} ms | filter {filter_us} us ({filtered} hits)",
                old_ms.map(|m| format!("{m} ms")).unwrap_or_else(|| "not run".into()));
        }
        return Ok(());
    }
    if action == "save-bench" {
        // Test B/C offline: save-path timing on synthetic 2 s segments written
        // by the engine's segment muxer settings. No screen or audio capture.
        let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
        let media = Media::discover()?;
        let base = std::env::current_dir()?
            .join("test-output")
            .join("save-bench")
            .join(format!("{seconds}s"));
        if !base.join("segments.csv").exists() {
            std::fs::create_dir_all(&base)?;
            let ok = std::process::Command::new(&media.ffmpeg)
                .current_dir(&base)
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("testsrc2=s=1920x1080:r=60:d={seconds}"),
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("sine=f=440:r=48000:d={seconds}"),
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("sine=f=660:r=48000:d={seconds}"),
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-map",
                    "2:a",
                    "-c:v",
                    "h264_qsv",
                    "-b:v",
                    "12M",
                    "-g",
                    "120",
                    "-bf",
                    "0",
                    "-forced_idr",
                    "1",
                    "-force_key_frames",
                    "expr:gte(t,n_forced*2)",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-f",
                    "segment",
                    "-segment_time",
                    "2",
                    "-segment_format",
                    "matroska",
                    "-segment_list",
                    "segments.csv",
                    "-segment_list_type",
                    "csv",
                    "-reset_timestamps",
                    "1",
                    "segment-%08d.mkv",
                ])
                .status()?
                .success();
            if !ok {
                bail!("Could not create fixture segments")
            }
        }
        let csv = std::fs::read_to_string(base.join("segments.csv"))?;
        let segments: Vec<replayx_core::buffer::Segment> = csv
            .lines()
            .filter_map(|l| {
                let mut f = l.split(',');
                let name = f.next()?;
                let start: f64 = f.next()?.parse().ok()?;
                let end: f64 = f.next()?.parse().ok()?;
                let path = base.join(name);
                let bytes = std::fs::metadata(&path).ok()?.len();
                Some(replayx_core::buffer::Segment {
                    path,
                    start,
                    end,
                    bytes,
                    origin: None,
                    start_time: None,
                })
            })
            .collect();
        let out = base.join("out");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out)?;
        // Old (0.2.0): sequential full probe of every segment, remux, full decode.
        let t = std::time::Instant::now();
        for s in &segments {
            media.probe(&s.path)?;
        }
        let probe_old = t.elapsed().as_millis();
        let old = media.concat(&segments, &out.join("old.mkv"), 2)?;
        let old_total = t.elapsed().as_millis();
        // New: parallel minimal probes, remux, boundary decode; stage timings.
        let t = std::time::Instant::now();
        let new =
            media.concat_progress(&segments, &out.join("new.mkv"), 2, false, &[], &mut |_| {})?;
        let new_total = t.elapsed().as_millis();
        println!(
            "{seconds} s replay, {} segments, {:.0} MB",
            segments.len(),
            old.bytes as f64 / 1e6
        );
        println!("  0.2.0 path: {old_total} ms total (sequential segment probes alone {probe_old} ms; concat() itself probes again and fully decodes)");
        print!("  new path:   {new_total} ms total |");
        for st in &new.save_timings {
            print!(" {} @{} ms |", st.stage, st.elapsed_ms);
        }
        println!();
        println!(
            "  durations: old {:.3} s, new {:.3} s",
            old.duration, new.duration
        );
        // With background probing done (normal during capture): no save-time probes.
        let mut known = segments.clone();
        for (s, info) in known.iter_mut().zip(media.segment_infos(&segments)?) {
            s.start_time = Some(info.start_time);
        }
        let t = std::time::Instant::now();
        let warm =
            media.concat_progress(&known, &out.join("warm.mkv"), 2, false, &[], &mut |_| {})?;
        print!(
            "  new path, start times known from capture: {} ms |",
            t.elapsed().as_millis()
        );
        for st in &warm.save_timings {
            print!(" {} @{} ms |", st.stage, st.elapsed_ms);
        }
        println!(" duration {:.3} s", warm.duration);
        return Ok(());
    }
    if action == "detect" {
        // Read-only: prints the foreground process classification once a second.
        let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
        for _ in 0..seconds {
            let (exe, what) = match replayx_core::games::foreground_process() {
                Some((pid, path, _)) => (
                    format!("{pid} {}", path.rsplit(['\\', '/']).next().unwrap_or(&path)),
                    replayx_core::games::classify(&path, "", &[]),
                ),
                None => ("(none)".into(), replayx_core::games::Foreground::App),
            };
            println!(
                "{} {exe:<48} -> {what:?}",
                chrono::Local::now().format("%H:%M:%S")
            );
            thread::sleep(Duration::from_secs(1));
        }
        return Ok(());
    }
    if action == "device-formats" {
        for (kind, name, format) in replayx_core::audio::device_formats()? {
            println!("{kind:<10} {name}: {format}");
        }
        return Ok(());
    }
    if action == "probe" {
        let p = Media::discover()?.probe(&PathBuf::from(args.get(2).expect("path required")))?;
        println!("{}", serde_json::to_string_pretty(&p)?);
        return Ok(());
    }
    if ![
        "self-test",
        "record-self-test",
        "capture-test",
        "record-test",
        "recover",
    ]
    .contains(&action)
    {
        println!("replayx-cli encoders | devices | device-formats | audio-sessions [icon.png] | detect [seconds] | library-bench | save-bench [seconds] | start-bench [silent|system] | displays | audio-test [seconds] [system|microphone] [device-id] | audio-test [seconds] application PID | self-test | capture-test [seconds:6..7200] [silent|system|microphone|app:PID] [desktop|stretch1080] [fps=N mode=M height=H encoder=E codec=C preset=P bitrate=MBPS] | probe FILE | recover DATA_DIR | overlay-test [out-dir]");
        return Ok(());
    }
    let root = std::env::current_dir()?.join("test-output");
    std::fs::create_dir_all(&root)?;
    let run = root.join(format!(
        "{}-{}",
        action,
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir_all(&run)?;
    let mut settings = Settings {
        output_dir: run.join("clips"),
        width: 854,
        height: 480,
        bitrate_mbps: 3,
        ..Default::default()
    };
    if action == "self-test" || action == "record-self-test" {
        settings.audio.clear();
        settings.audio.push(Default::default());
        settings.audio.push(Default::default());
    } else {
        // One source only: the default settings also contain the microphone,
        // which a capture test must never open unless asked to.
        settings.audio.truncate(1);
        match args.get(3).map(String::as_str).unwrap_or("system") {
            "silent" => settings.audio.clear(),
            "microphone" => {
                settings.audio[0].kind = "microphone".into();
                settings.audio[0].name = "Microphone".into();
            }
            app if app.starts_with("app:") => {
                settings.audio[0] = application_source(app[4..].parse()?)?;
            }
            _ => {}
        }
    }
    if args.get(4).is_some_and(|a| a == "desktop") {
        settings.capture = "desktop".into();
    }
    if args.get(4).is_some_and(|a| a == "stretch1080") {
        settings.canvas_mode = "stretch".into();
        settings.height = 1080;
        settings.bitrate_mbps = 12;
    }
    // Optional overrides: fps=60 mode=stretch|fit|crop|native height=1080 encoder=h264_nvenc
    for arg in args.iter().skip(4) {
        match arg.split_once('=') {
            Some(("fps", v)) => settings.fps = v.parse()?,
            Some(("mode", v)) => settings.canvas_mode = v.into(),
            Some(("height", v)) => settings.height = v.parse()?,
            Some(("width", v)) => settings.width = v.parse()?,
            Some(("preset", v)) => settings.preset = v.into(),
            Some(("scale_policy", v)) => settings.scale_policy = v.into(),
            Some(("encoder", v)) => settings.encoder = v.into(),
            Some(("codec", v)) => settings.codec = v.into(),
            Some(("bitrate", v)) => {
                settings.bitrate_mbps = v.parse()?;
                settings.preset = "custom".into();
            }
            Some(("gpu", v)) => settings.gpu_scaling = v != "0",
            Some(("replay", v)) => settings.replay_seconds = v.parse()?,
            Some(("buffer", v)) => settings.buffer_mb = v.parse()?,
            _ => {}
        }
    }
    if !args.iter().any(|a| a.starts_with("width=")) {
        settings.width = replayx_core::geometry::canvas(settings.height).0;
    }
    let data = if action == "recover" {
        PathBuf::from(args.get(2).expect("data directory required"))
    } else {
        run.join("data")
    };
    let engine = Engine::new(data, settings)?;
    if action == "recover" {
        println!("{:?}", engine.recover()?);
        return Ok(());
    }
    if action == "self-test" {
        engine.start_synthetic(false)?;
    } else if action == "record-self-test" {
        // Synthetic source (generated pattern and tones; nothing is captured or
        // played): replay first, then a recording on the same pipeline.
        engine.start_synthetic(false)?;
        thread::sleep(Duration::from_secs(3));
        engine.start_recording()?;
    } else if action == "record-test" {
        engine.start_recording()?;
    } else {
        engine.start_replay()?;
    }
    let seconds = args
        .get(2)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(6, 7200);
    let mut samples = Vec::new();
    for _ in 0..seconds {
        thread::sleep(Duration::from_secs(1));
        let s = engine.status();
        println!(
            "{} frames={} fps={:.1} buffer={:.2}s audio={:?}",
            s.state, s.frames, s.fps, s.buffer_seconds, s.audio_packets
        );
        if s.state == "Error" {
            std::fs::write(run.join("failure.json"), serde_json::to_vec_pretty(&s)?)?;
            bail!("{}", s.error.unwrap_or_default())
        }
        samples.push(s);
        if samples.len() % 10 == 0 {
            std::fs::write(
                run.join("samples.json"),
                serde_json::to_vec_pretty(&samples)?,
            )?;
        }
    }
    let stop_started = std::time::Instant::now();
    let frames_before = engine.status().frames;
    let first = if action == "record-test" || action == "record-self-test" {
        engine.stop_recording()?
    } else {
        engine.save_replay()?
    };
    let handover_ms = stop_started.elapsed().as_millis();
    if action == "record-self-test" {
        thread::sleep(Duration::from_secs(2));
        let s = engine.status();
        println!(
            "stop -> job handed over in {handover_ms} ms; replay still on: {} ({}); frames {} -> {} during the next 2 s",
            s.replay, s.state, frames_before, s.frames
        );
    }
    // A concurrent replay save only exists while replay runs; stopping a
    // recording-only capture discards its buffer.
    let second = if engine.status().replay {
        Some(engine.save_replay()?)
    } else {
        None
    };
    let worker = thread::spawn(move || first.run());
    let b = second.map(|job| {
        let r = job.run();
        engine.complete_save(&r);
        r
    });
    let a = worker.join().unwrap();
    engine.complete_save(&a);
    engine.shutdown()?;
    let (path, probe) = a?;
    let other = match b {
        Some(r) => Some(r?.0),
        None => None,
    };
    println!(
        "first clip ready {} ms after the stop/save request",
        stop_started.elapsed().as_millis()
    );
    let result = serde_json::json!({"kind":action,"handover_ms":handover_ms,"first":path,"concurrent":other,"probe":probe,"status":engine.status(),"note":"Synthetic tests do not validate real device synchronization or game performance."});
    std::fs::write(run.join("report.json"), serde_json::to_vec_pretty(&result)?)?;
    std::fs::write(
        run.join("samples.json"),
        serde_json::to_vec_pretty(&samples)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn audio_device_source(kind: &str, device: &str) -> Result<replayx_core::model::AudioSource> {
    let name = if device.is_empty() {
        "Windows default at start".to_owned()
    } else {
        replayx_core::audio::devices()?
            .iter()
            .find(|d| d.id == device && d.kind == kind)
            .ok_or_else(|| anyhow::anyhow!("Selected device unavailable"))?
            .name
            .clone()
    };
    Ok(replayx_core::model::AudioSource {
        kind: kind.into(),
        device: device.into(),
        name,
        ..Default::default()
    })
}

/// Per-process loopback source (the process and its children), as the app uses.
fn application_source(pid: u32) -> Result<replayx_core::model::AudioSource> {
    let mut sys = sysinfo::System::new();
    let id = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[id]), true);
    let process = sys
        .process(id)
        .ok_or_else(|| anyhow::anyhow!("Process {pid} not found"))?;
    Ok(replayx_core::model::AudioSource {
        kind: "application".into(),
        pid,
        process_started: process.start_time(),
        name: format!("{} ({pid})", process.name().to_string_lossy()),
        ..Default::default()
    })
}

// Exercise the existing real WASAPI producer without a media encoder. This
// diagnostic does not substitute for MKV, editor, or physical A/V verification.
fn audio_test(args: &[String]) -> Result<()> {
    let seconds = args
        .get(2)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(3, 60);
    let kind = args.get(3).map(String::as_str).unwrap_or("system");
    if !["system", "microphone", "application"].contains(&kind) {
        bail!("Choose system, microphone or application")
    }
    let device = args.get(4).cloned().unwrap_or_default();
    let source = if kind == "application" {
        application_source(device.parse()?)?
    } else {
        audio_device_source(kind, &device)?
    };
    let name = source.name.clone();
    let never = std::sync::atomic::AtomicBool::new(false);
    let capture = replayx_core::audio::record_test(source, seconds, &never, |_, _| {})?;
    let dir = std::env::current_dir()?.join("test-output").join(format!(
        "audio-test-{}-{}-{}",
        kind,
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        &uuid::Uuid::new_v4().to_string()[..8]
    ));
    std::fs::create_dir_all(&dir)?;
    replayx_core::audio::write_wav16(&dir.join("capture.wav"), &capture.pcm)?;
    // Exact producer output (pre-encoder) for lossless analysis.
    let raw: Vec<u8> = capture.pcm.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(dir.join("capture.f32"), raw)?;
    let packets = capture.packets;
    let mut report = serde_json::to_value(&capture)?;
    report["kind"] = kind.into();
    report["device"] = device.clone().into();
    report["name"] = name.clone().into();
    report["file"] = serde_json::json!(dir.join("capture.wav"));
    report["note"] = "Real device PCM capture. Nonzero levels do not prove intelligible speech, source isolation, editor compatibility, or A/V synchronization.".into();
    std::fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if packets == 0 {
        bail!("No real device packets received; saved samples may be idle-loopback silence")
    }
    Ok(())
}

/// Real capture startup benchmark (captures the selected display; silent by
/// default, `system` adds system-audio loopback; never the microphone).
/// Cases: first start for this display (capture paths probed), start after a
/// relaunch (remembered path), second start in the same session (cached), and
/// Record while Instant Replay already runs (joins the encoder).
fn start_bench(args: &[String]) -> Result<()> {
    use replayx_core::engine::Status;
    use std::time::Instant;
    let audio = args.get(2).map(String::as_str).unwrap_or("silent");
    let root = std::env::current_dir()?.join("test-output").join(format!(
        "start-bench-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    let data = root.join("data");
    let mut settings = Settings {
        output_dir: root.join("clips"),
        ..Default::default()
    };
    if audio != "system" {
        settings.audio.clear();
    }
    let wait = |engine: &Engine, timeout: Duration, f: &dyn Fn(&Status) -> bool| {
        let deadline = Instant::now() + timeout;
        loop {
            let s = engine.status();
            if f(&s) || Instant::now() > deadline {
                return s;
            }
            thread::sleep(Duration::from_millis(5));
        }
    };
    // Peak CPU (all cores = 100 %) and memory of this process and its encoder.
    let sample = |stop: std::sync::Arc<std::sync::atomic::AtomicBool>| {
        thread::spawn(move || {
            let own = sysinfo::Pid::from_u32(std::process::id());
            let mut sys = sysinfo::System::new();
            let cores = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1) as f32;
            let (mut cpu, mut mem) = (0f32, 0u64);
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                let family: Vec<_> = sys
                    .processes()
                    .iter()
                    .filter(|(pid, p)| **pid == own || p.parent() == Some(own))
                    .map(|(_, p)| (p.cpu_usage(), p.memory()))
                    .collect();
                cpu = cpu.max(family.iter().map(|f| f.0).sum::<f32>() / cores);
                mem = mem.max(family.iter().map(|f| f.1).sum());
                // sysinfo needs at least 200 ms between refreshes for CPU usage.
                thread::sleep(Duration::from_millis(250));
            }
            (cpu, mem)
        })
    };
    let mut report = Vec::new();
    let mut case = |engine: &Engine, label: &str, replay_first: bool| -> Result<()> {
        if replay_first {
            engine.start_replay()?;
            let s = wait(engine, Duration::from_secs(20), &|s| s.state == "Buffering");
            if s.state != "Buffering" {
                bail!("replay did not start: {} {:?}", s.state, s.error)
            }
            thread::sleep(Duration::from_secs(3));
        }
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sampler = sample(stop.clone());
        let t = Instant::now();
        engine.start_recording()?;
        let returned = t.elapsed().as_millis();
        let s = wait(engine, Duration::from_secs(20), &|s| {
            s.recording && s.state == "Recording"
        });
        let recording_ms = t.elapsed().as_millis();
        if s.state != "Recording" {
            bail!("recording did not start: {} {:?}", s.state, s.error)
        }
        let s = wait(engine, Duration::from_secs(10), &|s| {
            replay_first
                || s.start_timings
                    .iter()
                    .any(|t| t.stage.starts_with("First segment"))
        });
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let (cpu, mem) = sampler.join().unwrap();
        thread::sleep(Duration::from_secs(3));
        let stop_at = Instant::now();
        let job = engine.stop_recording()?;
        let handover = stop_at.elapsed().as_millis();
        let result = job.run();
        engine.complete_save(&result);
        let ready = stop_at.elapsed().as_millis();
        let (clip, probe) = result?;
        println!("\n== {label}");
        println!("  start() returned           {returned:>6} ms");
        println!("  state Recording            {recording_ms:>6} ms (status polled every 5 ms)");
        if !replay_first {
            for t in &s.start_timings {
                println!("  {:<44} {:>6} ms", t.stage, t.elapsed_ms);
            }
        }
        println!(
            "  peak CPU during start      {cpu:>6.1} % of all cores; peak memory {} MB",
            mem / 1_048_576
        );
        println!(
            "  stop -> clip ready         {ready:>6} ms (handover {handover} ms), {:.1} s, {}",
            probe.duration,
            clip.display()
        );
        report.push(serde_json::json!({"case":label,"start_returned_ms":returned,"recording_state_ms":recording_ms,
            "timings":s.start_timings,"peak_cpu_percent":cpu,"peak_memory_mb":mem / 1_048_576,"stop_handover_ms":handover,
            "stop_ready_ms":ready,"clip_seconds":probe.duration,"encoder":s.encoder,"scaling":s.scaling}));
        if replay_first {
            engine.stop_replay()?;
        }
        Ok(())
    };
    {
        let engine = Engine::new(data.clone(), settings.clone())?;
        case(
            &engine,
            "First start for this display (capture paths probed)",
            false,
        )?;
        case(&engine, "Second start, same session (cached path)", false)?;
        engine.shutdown()?;
    }
    thread::sleep(Duration::from_millis(500));
    {
        let engine = Engine::new(data.clone(), settings.clone())?;
        case(&engine, "Start after relaunch (remembered path)", false)?;
        case(
            &engine,
            "Record while Instant Replay runs (joins encoder)",
            true,
        )?;
        let s = engine.status();
        println!("\nleftover interrupted sessions: {}", s.recovery_sessions);
        engine.shutdown()?;
    }
    std::fs::write(
        root.join("start-bench.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("\nwrote {}", root.join("start-bench.json").display());
    Ok(())
}

/// Proof that the in-game overlay never reaches clips: show it over a solid
/// backdrop and grab real Desktop Duplication frames (the capture REPLAYX
/// records) into lossless files, with the overlay hidden, shown excluded from
/// capture, and shown capturable (control: proves the test can see it).
fn overlay_test(args: &[String]) -> Result<()> {
    use replayx_core::status_overlay::{Frame, Icon, StatusOverlay, Tile};
    use windows::{
        core::w,
        Win32::{Foundation::RECT, UI::WindowsAndMessaging::*},
    };
    let out = PathBuf::from(
        args.get(2)
            .map(String::as_str)
            .unwrap_or("test-output/overlay"),
    );
    std::fs::create_dir_all(&out)?;
    let media = Media::discover()?;
    let overlay = StatusOverlay::spawn();
    let frame = |capturable: bool| Frame {
        tiles: vec![
            Tile {
                icon: Icon::Replay,
                color: (160, 146, 255),
                slash: false,
            },
            Tile {
                icon: Icon::Mic,
                color: (62, 214, 160),
                slash: false,
            },
            Tile {
                icon: Icon::Record,
                color: (255, 77, 94),
                slash: false,
            },
        ],
        position: "bottom-right".into(),
        opacity: 100,
        capturable,
    };
    overlay.update(Some(frame(false)));
    thread::sleep(Duration::from_millis(800));
    let hwnd = unsafe { FindWindowW(w!("ReplayxStatusOverlay"), None) }?;
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }?;
    // Backdrop under the overlay: a solid colour, so only the overlay can
    // change the captured pixels.
    let pad = 40;
    let back = RECT {
        left: r.left - pad,
        top: r.top - pad,
        right: r.right + pad,
        bottom: r.bottom + pad,
    };
    let backdrop = thread::spawn(move || unsafe { backdrop_window(back, Duration::from_secs(12)) });
    thread::sleep(Duration::from_millis(500));
    overlay.update(Some(frame(false)));
    thread::sleep(Duration::from_millis(300));
    // Desktop Duplication output containing the overlay.
    let display = replayx_core::geometry::displays()
        .into_iter()
        .find(|d| {
            r.left >= d.left
                && r.left < d.left + d.desktop_width as i32
                && r.top >= d.top
                && r.top < d.top + d.desktop_height as i32
        })
        .ok_or_else(|| anyhow::anyhow!("overlay is not on a known display"))?;
    let (w, h) = (
        ((r.right - r.left) as u32 + 1) & !1,
        ((r.bottom - r.top) as u32 + 1) & !1,
    );
    let (x, y) = ((r.left - display.left) as u32, (r.top - display.top) as u32);
    let grab = |name: &str| -> Result<Vec<u8>> {
        let file = out.join(format!("{name}.mkv"));
        let ok = replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("ddagrab=output_idx={}:framerate=10:video_size={w}x{h}:offset_x={x}:offset_y={y},hwdownload,format=bgra", display.index))
            .args(["-frames:v", "8", "-c:v", "ffv1"])
            .arg(&file)
            .status()?
            .success();
        if !ok {
            bail!("ddagrab failed for {name}");
        }
        let png = out.join(format!("{name}.png"));
        let _ = replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-y", "-sseof", "-0.3", "-i"])
            .arg(&file)
            .args(["-frames:v", "1"])
            .arg(&png)
            .status();
        let raw = replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-sseof", "-0.3", "-i"])
            .arg(&file)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()?
            .stdout;
        Ok(raw)
    };
    let excluded = grab("overlay-excluded")?;
    overlay.update(Some(frame(true)));
    thread::sleep(Duration::from_millis(400));
    let visible = grab("overlay-capturable-control")?;
    overlay.update(None);
    thread::sleep(Duration::from_millis(400));
    let hidden = grab("overlay-hidden")?;
    let _ = backdrop.join();
    let diff = |a: &[u8], b: &[u8]| -> f64 {
        let n = a.len().min(b.len()).max(1);
        a.iter()
            .zip(b)
            .map(|(p, q)| (*p as f64 - *q as f64).abs())
            .sum::<f64>()
            / n as f64
    };
    let (ex, ctl) = (diff(&excluded, &hidden), diff(&visible, &hidden));
    let pass = !hidden.is_empty() && ex < 1.0 && ctl > 10.0;
    let report = serde_json::json!({
        "display": display.device, "region": [x, y, w, h],
        "excluded_vs_hidden_mean_abs_diff": ex, "capturable_vs_hidden_mean_abs_diff": ctl,
        "pass": pass, "files": out.display().to_string(),
    });
    std::fs::write(out.join("result.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !pass {
        bail!("overlay exclusion check failed");
    }
    Ok(())
}

unsafe extern "system" fn backdrop_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wp, lp)
}
/// A plain opaque top-most window for `overlay_test` (click-through, no focus).
unsafe fn backdrop_window(r: windows::Win32::Foundation::RECT, life: Duration) {
    use windows::{
        core::w,
        Win32::{
            Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
        },
    };
    let Ok(instance) = GetModuleHandleW(None) else {
        return;
    };
    let class = w!("ReplayxOverlayTestBackdrop");
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(backdrop_proc),
        hInstance: instance.into(),
        lpszClassName: class,
        hbrBackground: CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00_50_A0_30)),
        ..Default::default()
    });
    let Ok(hwnd) = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
        class,
        w!("REPLAYX overlay test"),
        WS_POPUP,
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        None,
        None,
        Some(instance.into()),
        None,
    ) else {
        return;
    };
    let _ = SetLayeredWindowAttributes(
        hwnd,
        windows::Win32::Foundation::COLORREF(0),
        255,
        LWA_ALPHA,
    );
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    // Keep the overlay above the backdrop.
    if let Ok(o) = FindWindowW(w!("ReplayxStatusOverlay"), None) {
        let _ = SetWindowPos(
            hwnd,
            Some(o),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    let end = std::time::Instant::now() + life;
    let mut msg = MSG::default();
    while std::time::Instant::now() < end {
        while PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        thread::sleep(Duration::from_millis(30));
    }
    let _ = DestroyWindow(hwnd);
}

/// Render the overlay's icon tiles (every state) to a PNG, without showing
/// any window: `overlay-render out.png [scale]`.
fn overlay_render(args: &[String]) -> Result<()> {
    use replayx_core::status_overlay::{render, Icon, Tile};
    let out = PathBuf::from(
        args.get(2)
            .map(String::as_str)
            .unwrap_or("test-output/overlay/icons.png"),
    );
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let all = [
        Tile {
            icon: Icon::Replay,
            color: (160, 146, 255),
            slash: false,
        },
        Tile {
            icon: Icon::Replay,
            color: (128, 128, 146),
            slash: true,
        },
        Tile {
            icon: Icon::Mic,
            color: (62, 214, 160),
            slash: false,
        },
        Tile {
            icon: Icon::Mic,
            color: (160, 146, 255),
            slash: false,
        },
        Tile {
            icon: Icon::Mic,
            color: (245, 186, 84),
            slash: true,
        },
        Tile {
            icon: Icon::Record,
            color: (255, 77, 94),
            slash: false,
        },
    ];
    let (w, h, px) = render(&all, scale, false);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let media = Media::discover()?;
    let mut child = replayx_core::media::command(&media.ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgra",
            "-s",
        ])
        .arg(format!("{w}x{h}"))
        .args(["-i", "-", "-frames:v", "1"])
        .arg(&out)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&px)?;
    if !child.wait()?.success() {
        bail!("could not write {}", out.display());
    }
    println!("{}", out.display());
    Ok(())
}

/// Evaluate the Voice Enhancer on a recording (any format FFmpeg reads):
/// `voice-eval input.wav [out-dir] [clean.wav]`. Writes one processed WAV per
/// preset and suppression strength, and prints level, noise floor, peak and
/// CPU cost. With the clean (noise-free) version of the same speech it also
/// reports how much speech changed versus how much noise was removed.
fn voice_eval(args: &[String]) -> Result<()> {
    use replayx_core::voice::{preset, process_buffer, VoiceConfig, PRESETS};
    let input = PathBuf::from(
        args.get(2)
            .ok_or_else(|| anyhow::anyhow!("voice-eval input.wav [out-dir] [clean.wav]"))?,
    );
    let out = PathBuf::from(
        args.get(3)
            .map(String::as_str)
            .unwrap_or("test-output/voice"),
    );
    std::fs::create_dir_all(&out)?;
    let media = Media::discover()?;
    let read = |p: &std::path::Path| -> Result<Vec<f32>> {
        let o = replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-i"])
            .arg(p)
            .args(["-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
            .output()?;
        if !o.status.success() {
            bail!("could not read {}", p.display());
        }
        Ok(o.stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect())
    };
    let x = read(&input)?;
    let clean = args
        .get(4)
        .map(|p| read(std::path::Path::new(p)))
        .transpose()?;
    let frame_db = |s: &[f32]| -> Vec<f32> {
        s.chunks(480)
            .map(|c| {
                10.0 * ((c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).max(1e-12)).log10()
            })
            .collect()
    };
    let stats = |s: &[f32]| {
        let mut f = frame_db(s);
        f.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f32| f[((f.len() - 1) as f32 * p) as usize];
        let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
        (pct(0.10), pct(0.90), 20.0 * peak.max(1e-9).log10())
    };
    let (n0, s0, p0) = stats(&x);
    println!(
        "{:<22} floor {:>6.1} dB  speech {:>6.1} dB  peak {:>6.1} dBFS",
        "input", n0, s0, p0
    );
    let mut configs: Vec<(String, VoiceConfig)> = PRESETS
        .iter()
        .map(|p| (p.to_string(), preset(p).unwrap()))
        .collect();
    for s in ["light", "balanced", "strong"] {
        configs.push((
            format!("suppression-{s}"),
            VoiceConfig {
                suppression: s.into(),
                ..preset("raw").unwrap()
            },
        ));
    }
    for (name, cfg) in configs {
        let t = std::time::Instant::now();
        let (y, matched) = process_buffer(&cfg, 0.0, &x);
        let cpu = t.elapsed().as_secs_f64() / (x.len() as f64 / 48_000.0) * 100.0;
        let (n, s, p) = stats(&y);
        let mut line = format!(
            "{name:<22} floor {n:>6.1} dB  speech {s:>6.1} dB  peak {p:>6.1} dBFS  noise -{:>4.1} dB  cpu {cpu:.2}% of a core  match {:+.1} dB",
            (n0 - n) - (s0 - s), 20.0 * matched.log10()
        );
        if let Some(c) = &clean {
            // Speech distortion: error vs the clean speech after level match
            // (lower is better); computed on frames where the clean speech is active.
            // Reference: the clean speech through the same processing, so EQ,
            // filtering and compression are not counted as errors; what is
            // left is remaining noise plus suppression artefacts.
            let (cy, _) = process_buffer(&cfg, 0.0, c);
            let act: Vec<usize> = frame_db(c)
                .iter()
                .enumerate()
                .filter(|(_, d)| **d > -45.0)
                .map(|(i, _)| i)
                .collect();
            let (mut e, mut sig) = (0f64, 0f64);
            for i in act {
                for k in i * 480..((i + 1) * 480).min(c.len()).min(y.len()) {
                    let target = cy[k] as f64;
                    e += (y[k] as f64 - target).powi(2);
                    sig += target.powi(2);
                }
            }
            line += &format!("  SDR {:>5.1} dB", 10.0 * (sig / e.max(1e-12)).log10());
        }
        println!("{line}");
        let wav = out.join(format!("{name}.wav"));
        let stereo: Vec<f32> = y.iter().flat_map(|v| [*v, *v]).collect();
        replayx_core::audio::write_wav16(&wav, &stereo)?;
    }
    Ok(())
}
