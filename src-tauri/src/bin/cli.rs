use anyhow::{bail, Result};
use replayx_core::{engine::Engine, media::Media, model::Settings};
use std::{path::PathBuf, thread, time::Duration};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let action = args.get(1).map(String::as_str).unwrap_or("help");
    if action == "audio-test" {
        return audio_test(&args);
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
    if !["self-test", "capture-test", "record-test", "recover"].contains(&action) {
        println!("replayx-cli encoders | devices | device-formats | audio-sessions [icon.png] | detect [seconds] | library-bench | save-bench [seconds] | displays | audio-test [seconds] [system|microphone] [device-id] | audio-test [seconds] application PID | self-test | capture-test [seconds:6..7200] [silent|system|microphone|app:PID] [desktop|stretch1080] [fps=N mode=M height=H encoder=E bitrate=MBPS] | probe FILE | recover DATA_DIR");
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
        height: 480,
        bitrate_mbps: 3,
        ..Default::default()
    };
    if action == "self-test" {
        settings.audio.clear();
        settings.audio.push(Default::default());
        settings.audio.push(Default::default());
    } else {
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
            Some(("encoder", v)) => settings.encoder = v.into(),
            Some(("bitrate", v)) => settings.bitrate_mbps = v.parse()?,
            Some(("gpu", v)) => settings.gpu_scaling = v != "0",
            Some(("replay", v)) => settings.replay_seconds = v.parse()?,
            Some(("buffer", v)) => settings.buffer_mb = v.parse()?,
            _ => {}
        }
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
    let first = if action == "record-test" {
        engine.stop_recording()?
    } else {
        engine.save_replay()?
    };
    let second = engine.save_replay()?;
    let worker = thread::spawn(move || first.run());
    let b = second.run();
    engine.complete_save(&b);
    let a = worker.join().unwrap();
    engine.complete_save(&a);
    engine.shutdown()?;
    let (path, probe) = a?;
    let (other, _) = b?;
    let result = serde_json::json!({"kind":action,"first":path,"concurrent":other,"probe":probe,"status":engine.status(),"note":"Synthetic tests do not validate real device synchronization or game performance."});
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
    let capture = replayx_core::audio::record_test(source, seconds, |_, _| {})?;
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
