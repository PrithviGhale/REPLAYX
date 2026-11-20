use anyhow::{bail, Result};
use replayx_core::{engine::Engine, media::Media, model::Settings};
use std::{path::PathBuf, thread, time::Duration};
fn main() -> Result<()> {
    let invocation_started = std::time::Instant::now();
    let args: Vec<_> = std::env::args().collect();
    let action = args.get(1).map(String::as_str).unwrap_or("help");
    if action == "native-app-test" {
        return native_app_test(&args);
    }
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
    if action == "save-stutter" {
        return save_stutter(&args);
    }
    if action == "crosshair-capture-test" {
        return crosshair_capture_test(&args);
    }
    if action == "crosshair-bench" {
        return crosshair_bench(&args);
    }
    if action == "start-bench" {
        return start_bench(&args);
    }
    if action == "native-topology" {
        println!(
            "{}",
            serde_json::to_string_pretty(&replayx_core::native_gpu::topology()?)?
        );
        return Ok(());
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
    if action == "game-icons" {
        // Read-only: which catalog games get an icon from what is installed.
        let started = std::time::Instant::now();
        let listed = replayx_core::installed_games::entries().len();
        let scan = started.elapsed();
        let mut found = 0;
        for g in &replayx_core::games::catalog().games {
            if let Some(p) = replayx_core::installed_games::icon_source(&g.name) {
                found += 1;
                let ok = replayx_core::winutil::exe_icon_png_sized(&p, 256).is_some();
                println!(
                    "{:<32} {} {}",
                    g.name,
                    if ok { "icon" } else { "NO ICON" },
                    p.display()
                );
            }
        }
        println!("{found} games with a local icon; {listed} installed entries read in {scan:?}; total {:?}", started.elapsed());
        return Ok(());
    }
    if action == "tick-cost" {
        // Read-only: CPU cost of the once-a-second background work while a
        // game is in front (foreground process, game classification, idle time).
        let n = 2000u32;
        let started = std::time::Instant::now();
        for _ in 0..n {
            if let Some((_, path, title)) = replayx_core::games::foreground_process() {
                std::hint::black_box(replayx_core::games::classify(&path, &title, &[]));
            }
            std::hint::black_box(replayx_core::playtime::idle_seconds());
        }
        let per = started.elapsed().as_secs_f64() / n as f64;
        println!(
            "per tick {:.1} us; at 1 tick/s that is {:.4}% of one CPU core",
            per * 1e6,
            per * 100.0
        );
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
        println!("replayx-cli encoders | devices | device-formats | audio-sessions [icon.png] | detect [seconds] | library-bench | save-bench [seconds] | start-bench [silent|system] | displays | audio-test [seconds] [system|microphone] [device-id] | audio-test [seconds] application PID | self-test | capture-test [seconds:6..7200] [silent|system|microphone|app:PID] [desktop|stretch1080] [fps=N mode=M height=H encoder=E codec=C preset=P bitrate=MBPS capture_api=dda|wgc] | probe FILE | recover DATA_DIR | overlay-test [out-dir]");
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
            Some(("engine", v)) => {
                if !["ffmpeg", "native"].contains(&v) {
                    bail!("Unknown recording engine")
                }
                settings.recording_engine = v.into();
                if v == "native" {
                    std::env::set_var("REPLAYX_NATIVE_ALPHA", "1");
                    std::env::set_var(
                        "REPLAYX_NATIVE_EXE",
                        std::env::current_exe()?.with_file_name("replayx-native-proto.exe"),
                    );
                    for key in [
                        "REPLAYX_ALPHA_SYNTHETIC",
                        "REPLAYX_NATIVE_TRACE_ROI",
                        "REPLAYX_NATIVE_TEST_SECONDS",
                        "REPLAYX_NATIVE_PHYSICAL_AUDIO",
                    ] {
                        std::env::remove_var(key);
                    }
                }
            }
            Some(("native_interval_us", v)) => {
                let interval: u32 = v.parse()?;
                if interval > 8333 {
                    bail!("Experimental native capture interval must be 0..8333 microseconds")
                }
                std::env::set_var("REPLAYX_NATIVE_CAPTURE_INTERVAL_US", interval.to_string());
            }
            Some(("fps", v)) => settings.fps = v.parse()?,
            Some(("mode", v)) => settings.canvas_mode = v.into(),
            Some(("height", v)) => settings.height = v.parse()?,
            Some(("width", v)) => settings.width = v.parse()?,
            Some(("region", v)) => {
                let (w, h) = v
                    .split_once('x')
                    .ok_or_else(|| anyhow::anyhow!("Use region=WIDTHxHEIGHT"))?;
                settings.source_rect = Some(replayx_core::geometry::CaptureRect {
                    x: 0,
                    y: 0,
                    width: w.parse()?,
                    height: h.parse()?,
                });
            }
            Some(("preset", v)) => settings.preset = v.into(),
            Some(("scale_policy", v)) => settings.scale_policy = v.into(),
            Some(("encoder", v)) => settings.encoder = v.into(),
            Some(("codec", v)) => settings.codec = v.into(),
            Some(("bitrate", v)) => {
                settings.bitrate_mbps = v.parse()?;
                settings.preset = "custom".into();
            }
            Some(("gpu", v)) => settings.gpu_scaling = v != "0",
            Some(("capture_api", v)) => settings.capture_api = v.into(),
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
    let benchmark_crosshair = args.iter().any(|arg| arg == "benchmark_crosshair=on");
    if benchmark_crosshair && action != "capture-test" {
        bail!("The benchmark crosshair is available only for capture-test");
    }
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
        if args.iter().any(|arg| arg == "with_replay=1") {
            engine.start_replay()?;
        }
        engine.start_recording()?;
    } else {
        engine.start_replay()?;
    }
    let crosshair = if benchmark_crosshair {
        use replayx_core::{crosshair::CrosshairSettings, crosshair_overlay::CrosshairOverlay};
        let overlay = CrosshairOverlay::default();
        overlay.apply(
            &CrosshairSettings {
                enabled: true,
                visibility: "always".into(),
                ..Default::default()
            },
            true,
        );
        Some(overlay)
    } else {
        None
    };
    let seconds = args
        .get(2)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(6, 7200);
    // Benchmarks budget the complete child process. Include initialization in
    // their acquisition deadline, while other CLI callers retain capture time
    // measured from successful startup. Poll against absolute ticks so status
    // work never adds another delay to every requested second.
    let wall_clock = args.iter().any(|arg| arg == "wall_clock=1");
    let capture_clock = if wall_clock {
        invocation_started
    } else {
        std::time::Instant::now()
    };
    let capture_limit = Duration::from_secs(seconds);
    let mut samples = Vec::new();
    while let Some(delay) = capture_poll_delay(capture_clock.elapsed(), capture_limit) {
        thread::sleep(delay);
        let s = engine.status();
        println!(
            "{} frames={} fps={:.1} buffer={:.2}s audio={:?}",
            s.state, s.frames, s.fps, s.buffer_seconds, s.audio_packets
        );
        if s.state == "Error" {
            std::fs::write(run.join("failure.json"), serde_json::to_vec_pretty(&s)?)?;
            bail!("{}", s.error.unwrap_or_default())
        }
        if let Some(overlay) = &crosshair {
            let state = overlay.state();
            if !state.shown || !state.excluded_from_capture || state.error.is_some() {
                engine.shutdown()?;
                bail!("Benchmark crosshair was not shown with capture exclusion: {state:?}");
            }
            overlay.raise_if_covered();
        }
        samples.push(s);
        if samples.len() % 10 == 0 {
            std::fs::write(
                run.join("samples.json"),
                serde_json::to_vec_pretty(&samples)?,
            )?;
        }
    }
    let crosshair_state = crosshair.as_ref().map(|overlay| overlay.state());
    drop(crosshair);
    let capture_end_ms = invocation_started.elapsed().as_millis();
    let stop_started = std::time::Instant::now();
    let frames_before = engine.status().frames;
    let first = if action == "record-test" || action == "record-self-test" {
        engine.stop_recording()?
    } else if args.iter().any(|arg| arg == "wait_for_replay=1") {
        engine.save_replay_when_ready(Duration::from_secs(4))?
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
    let worker = thread::spawn(move || {
        let result = first.run();
        (result, stop_started.elapsed().as_millis())
    });
    let mut second_ready_ms = None;
    let b = second.map(|job| {
        let r = job.run();
        second_ready_ms = Some(stop_started.elapsed().as_millis());
        engine.complete_save(&r);
        r
    });
    let (a, first_ready_ms) = worker.join().unwrap();
    engine.complete_save(&a);
    engine.shutdown()?;
    let (path, probe) = a?;
    let other = match b {
        Some(r) => Some(r?.0),
        None => None,
    };
    println!(
        "first clip ready {} ms after the stop/save request",
        first_ready_ms
    );
    let result = serde_json::json!({"kind":action,"wall_clock_budget":wall_clock,"wait_for_replay":args.iter().any(|arg| arg == "wait_for_replay=1"),"requested_seconds":seconds,"capture_end_ms":capture_end_ms,"handover_ms":handover_ms,"first_ready_ms":first_ready_ms,"second_ready_ms":second_ready_ms,"crosshair":crosshair_state,"first":path,"concurrent":other,"probe":probe,"status":engine.status(),"note":"Synthetic tests do not validate real device synchronization or game performance."});
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

/// Seconds on the QueryPerformanceCounter clock (the same clock
/// replayx-gameproxy logs its presents on, so frames and events align).
fn qpc_seconds() -> f64 {
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut c, mut f) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    c as f64 / f as f64
}

/// Instant Replay saves while a game-like workload runs: what a save costs
/// the game. Captures the screen (Windows Graphics Capture by default) with
/// the app's usual 1080p60 Balanced settings, then saves at the given offsets
/// (seconds after Instant Replay started) the way the app does: each save on
/// its own thread, the on-screen "saved" notice, and (thumb=1) the Clips
/// thumbnail. Every stage is logged on the QPC clock to events.json for
/// scripts/bench/save-stutter.py.
///
/// usage: replayx-cli save-stutter OUT_DIR saves=130,160,161,190,190.4,190.8
///        [end=220] [replay=120] [audio=silent|system] [notice=0|1] [thumb=0|1]
///        [capture_api=wgc|dda] [encoder=auto|...] [height=1080] [fps=60]
///        [overlay=0|1] (the in-game status overlay, updated once a second
///        as the app does)
fn save_stutter(args: &[String]) -> Result<()> {
    use replayx_core::overlay::{capture_notice, CaptureEvent, Overlay};
    use std::sync::{Arc, Mutex};
    let out = PathBuf::from(args.get(2).expect("output directory required"));
    std::fs::create_dir_all(&out)?;
    let opt = |k: &str, d: &str| -> String {
        args.iter()
            .skip(3)
            .find_map(|a| a.strip_prefix(&format!("{k}=")).map(str::to_owned))
            .unwrap_or_else(|| d.to_owned())
    };
    let saves: Vec<f64> = opt("saves", "130,160,161,190,190.4,190.8")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let end: f64 = opt("end", "220").parse()?;
    let mut settings = Settings {
        output_dir: out.join("clips"),
        capture_api: opt("capture_api", "wgc"),
        encoder: opt("encoder", "auto"),
        codec: "h264".into(),
        preset: "balanced".into(),
        fps: opt("fps", "60").parse()?,
        height: opt("height", "1080").parse()?,
        canvas_mode: "stretch".into(),
        replay_seconds: opt("replay", "120").parse()?,
        buffer_mb: 2048,
        ..Default::default()
    };
    settings.width = replayx_core::geometry::canvas(settings.height).0;
    settings.audio.truncate(1);
    if opt("audio", "silent") == "silent" {
        settings.audio.clear();
    }
    let notices = (opt("notice", "1") == "1").then(Overlay::spawn);
    let status_overlay =
        (opt("overlay", "1") == "1").then(replayx_core::status_overlay::StatusOverlay::spawn);
    let mut last_overlay = std::time::Instant::now();
    let thumb = opt("thumb", "0") == "1";
    let overlay_settings = settings.clone();
    let engine = Engine::new(out.join("data"), settings)?;
    let events: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let log = {
        let events = events.clone();
        move |save: i64, what: &str| {
            events
                .lock()
                .unwrap()
                .push(serde_json::json!({"t": qpc_seconds(), "save": save, "event": what}));
        }
    };
    log(-1, "start requested");
    engine.start_replay()?;
    let started = std::time::Instant::now();
    log(-1, "start returned");
    let mut workers = Vec::new();
    let mut next = 0usize;
    let mut live_logged = false;
    while started.elapsed().as_secs_f64() < end {
        let s = engine.status();
        if !live_logged && s.frames > 0 {
            log(-1, "capturing");
            live_logged = true;
        }
        if s.state == "Error" {
            bail!("capture failed: {}", s.error.unwrap_or_default());
        }
        if let Some(o) = &status_overlay {
            // As the app's 1 s loop: the frame for the current status.
            if last_overlay.elapsed() >= Duration::from_secs(1) {
                use replayx_core::status_overlay::{tiles, view, Frame};
                let os = &overlay_settings.overlay;
                let frame = Frame {
                    tiles: tiles(&view(&s, &overlay_settings), os),
                    position: os.position.clone(),
                    opacity: os.opacity,
                    capturable: false,
                };
                o.update((!frame.tiles.is_empty()).then_some(frame));
                last_overlay = std::time::Instant::now();
            }
        }
        if let Some(at) = saves.get(next).copied() {
            if started.elapsed().as_secs_f64() >= at {
                let n = next as i64;
                log(n, "requested");
                let job = engine.save_replay()?;
                log(n, "handed over");
                let (engine, log, notices) = (engine.clone(), log.clone(), notices.clone());
                workers.push(thread::spawn(move || {
                    let result = job.run_progress(|stage| log(n, stage));
                    engine.complete_save(&result);
                    match &result {
                        Ok((path, probe)) => {
                            log(n, "ready");
                            if let Some(o) = &notices {
                                o.show(capture_notice(
                                    CaptureEvent::ReplaySaved,
                                    Some("Desktop"),
                                    Some(probe.duration),
                                    "",
                                ));
                                log(n, "notice shown");
                            }
                            if thumb {
                                let m = Media::discover().expect("media");
                                let _ = m.thumbnail(path, probe.duration, 480);
                                log(n, "thumbnail done");
                            }
                            serde_json::json!({"save": n, "path": path, "duration": probe.duration, "timings": probe.save_timings})
                        }
                        Err(e) => {
                            log(n, "failed");
                            serde_json::json!({"save": n, "error": format!("{e:#}")})
                        }
                    }
                }));
                next += 1;
                continue;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    let live = engine.status();
    log(-1, "stopping");
    engine.shutdown()?;
    log(-1, "stopped");
    // Let the last notice finish its fade before the process exits.
    thread::sleep(Duration::from_millis(3500));
    let report = serde_json::json!({
        "saves": results, "events": *events.lock().unwrap(), "status": live,
        "note": "Synthetic workload; not a game result.",
    });
    std::fs::write(out.join("events.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report["saves"])?);
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
    // System audio only: a bench never opens the microphone.
    settings.audio.truncate(1);
    if audio != "system" {
        settings.audio.clear();
    }
    // Optional: start-bench system capture_api=wgc (the experimental path).
    for arg in args.iter().skip(3) {
        if let Some(("capture_api", v)) = arg.split_once('=') {
            settings.capture_api = v.into();
        }
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
        // Like the app: REPLAYX is open for a moment before the first click.
        thread::sleep(Duration::from_secs(3));
        case(
            &engine,
            "First start, 3 s after launch (capture paths probed)",
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
                text: None,
            },
            Tile {
                icon: Icon::Mic,
                color: (62, 214, 160),
                slash: false,
                text: None,
            },
            Tile {
                icon: Icon::Record,
                color: (255, 77, 94),
                slash: false,
                text: None,
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
            text: None,
        },
        Tile {
            icon: Icon::Replay,
            color: (128, 128, 146),
            slash: true,
            text: None,
        },
        Tile {
            icon: Icon::Mic,
            color: (62, 214, 160),
            slash: false,
            text: None,
        },
        Tile {
            icon: Icon::Mic,
            color: (160, 146, 255),
            slash: false,
            text: None,
        },
        Tile {
            icon: Icon::Mic,
            color: (245, 186, 84),
            slash: true,
            text: None,
        },
        Tile {
            icon: Icon::Record,
            color: (255, 77, 94),
            slash: false,
            text: None,
        },
    ];
    let mut all = all.to_vec();
    all.push(Tile {
        icon: Icon::Record,
        color: (255, 77, 94),
        slash: false,
        text: Some("12:34".into()),
    });
    let (w, h, px) = render(&all, scale, false, false);
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

/// Exercises the SAME Engine used by Tauri. Physical mode is explicit and bounded.
fn native_app_duration(
    desktop_silent: bool,
    physical: bool,
    requested: Option<u64>,
) -> Result<u64> {
    let limit = match (desktop_silent, physical) {
        (true, true) => bail!("Choose silent desktop or physical audio capture"),
        (true, false) => Some((6..=180, 6)),
        (false, true) => Some((6..=30, 15)),
        (false, false) => None,
    };
    match (limit, requested) {
        (Some((range, _)), Some(seconds)) if range.contains(&seconds) => Ok(seconds),
        (Some((range, _)), Some(_)) => bail!("Capture duration must be {range:?} seconds"),
        (Some((_, default)), None) => Ok(default),
        (None, Some(_)) => bail!("Custom duration requires an explicit physical capture mode"),
        (None, None) => Ok(8),
    }
}

fn native_app_test(args: &[String]) -> Result<()> {
    let desktop_silent = args.iter().any(|a| a == "--desktop-silent");
    let physical_requested = args.iter().any(|a| a == "--physical");
    std::env::remove_var("REPLAYX_NATIVE_TRACE_ROI");
    std::env::remove_var("REPLAYX_NATIVE_TIMESTAMP_TEST");
    std::env::remove_var("REPLAYX_NATIVE_PHASE_SOURCE");
    std::env::remove_var("REPLAYX_NATIVE_CAPTURE_INTERVAL_US");
    if let Some(index) = args.iter().position(|arg| arg == "--capture-interval-us") {
        let interval: u32 = args
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("Capture interval missing"))?
            .parse()?;
        if interval > 8333 {
            bail!("Experimental native capture interval must be 0..8333 microseconds")
        }
        std::env::set_var("REPLAYX_NATIVE_CAPTURE_INTERVAL_US", interval.to_string());
    }
    if args.iter().any(|a| a == "--phase-global") {
        if !args.iter().any(|a| a == "--phase-lock") {
            bail!("Global comparator requires explicit phase-lock")
        }
        std::env::set_var("REPLAYX_NATIVE_PHASE_SOURCE", "global");
    }
    if args.iter().any(|a| a == "--phase-lock") {
        if !(desktop_silent || physical_requested) || args.iter().any(|a| a == "--capture-vfr") {
            bail!("Phase-lock test requires an explicit desktop/device test and one pacing mode")
        }
        std::env::set_var("REPLAYX_NATIVE_TIMESTAMP_TEST", "phase-lock");
    }
    if args.iter().any(|a| a == "--capture-vfr") {
        if !desktop_silent {
            bail!("Explicit VFR experiment requires silent desktop test")
        }
        std::env::set_var("REPLAYX_NATIVE_TIMESTAMP_TEST", "capture-vfr");
    }
    if let Some(i) = args.iter().position(|a| a == "--trace-roi") {
        if !desktop_silent {
            bail!("Frame-content tracing is allowed only in the explicit silent desktop test")
        }
        let roi = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("Trace ROI missing"))?;
        std::env::set_var("REPLAYX_NATIVE_TRACE_ROI", roi);
    }

    let physical = desktop_silent || physical_requested;
    let requested_seconds = args
        .iter()
        .position(|a| a == "--seconds")
        .map(|i| {
            args.get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("Duration missing"))?
                .parse::<u64>()
                .map_err(anyhow::Error::from)
        })
        .transpose()?;
    let stop_after = native_app_duration(desktop_silent, physical_requested, requested_seconds)?;
    if args.iter().any(|arg| arg == "--plan-only") {
        println!(
            "{}",
            serde_json::json!({
                "seconds": stop_after,
                "physical": physical,
                "no_capture_started": true,
                "output_parent": std::env::current_dir()?.join("test-output"),
            })
        );
        return Ok(());
    }

    let device_only = args.iter().any(|a| a == "--devices-with-generated-video");
    // This explicit test command runs before Engine/audio threads exist.
    std::env::set_var("REPLAYX_NATIVE_ALPHA", "1");
    std::env::remove_var("REPLAYX_ALPHA_SYNTHETIC");
    if device_only {
        std::env::set_var("REPLAYX_NATIVE_PHYSICAL_AUDIO", "1");
    } else {
        std::env::remove_var("REPLAYX_NATIVE_PHYSICAL_AUDIO");
    }
    if physical || device_only {
        std::env::set_var(
            "REPLAYX_NATIVE_TEST_SECONDS",
            if requested_seconds.is_some() || physical_requested {
                (stop_after + 10).to_string()
            } else {
                "12".into()
            },
        );
    }
    if physical
        && matches!(
            replayx_core::games::sample(&[]),
            replayx_core::games::Foreground::Game(_)
        )
    {
        bail!("A game is in front; unattended device capture was not started")
    }
    let root = std::env::current_dir()?.join("test-output").join(format!(
        "native-alpha-{}-{}",
        if desktop_silent {
            "desktop-silent"
        } else if physical {
            "physical"
        } else if device_only {
            "devices"
        } else {
            "generated"
        },
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir(&root)?;
    let settings = Settings {
        recording_engine: "native".into(),
        output_dir: root.join("clips"),
        organize_by_game: false,
        audio: if desktop_silent {
            Vec::new()
        } else {
            vec![
                Default::default(),
                replayx_core::model::AudioSource::microphone(),
            ]
        },
        capture_notices: false,
        ..Default::default()
    };
    let engine = Engine::new(root.join("data"), settings)?;
    let result = (|| -> Result<()> {
        if physical {
            engine.start_recording()?;
        } else {
            engine.start_synthetic(true)?;
        }
        let ready = engine.status();
        if physical_requested && ready.recording_engine != "native" {
            bail!(
                "Physical Native qualification fell back to FFmpeg: {:?}",
                ready.engine_fallback
            );
        }
        let mut samples = Vec::new();
        let began = std::time::Instant::now();
        if physical_requested && !desktop_silent {
            eprintln!(
                "Physical capture is active. Play the fixture now. {stop_after} seconds remaining."
            );
        }
        loop {
            let status = engine.status();
            if status.state == "Error" {
                // Preserve evidence and allow the existing runtime-recovery save
                // to finish before this short-lived test process exits.
                let until = std::time::Instant::now() + std::time::Duration::from_secs(12);
                while engine.status().saving > 0 && std::time::Instant::now() < until {
                    thread::sleep(Duration::from_millis(50));
                }
                std::fs::write(
                    root.join("failure.json"),
                    serde_json::to_vec_pretty(
                        &serde_json::json!({"status":engine.status(),"samples":samples}),
                    )?,
                )?;
                bail!(
                    "{}; diagnostic evidence: {}",
                    status.error.unwrap_or_default(),
                    root.display()
                )
            }
            let elapsed = status.elapsed;
            samples.push(status);
            let ready_elapsed = if physical_requested && !desktop_silent {
                began.elapsed().as_secs_f64()
            } else {
                elapsed
            };
            if ready_elapsed >= stop_after as f64
                || began.elapsed() > std::time::Duration::from_secs(stop_after + 5)
            {
                break;
            }
            if physical
                && matches!(
                    replayx_core::games::sample(&[]),
                    replayx_core::games::Foreground::Game(_)
                )
            {
                break;
            }
            thread::sleep(Duration::from_millis(250));
        }
        let replay = engine.save_replay()?;
        let replay_worker = thread::spawn(move || replay.run());
        let recording = engine.stop_recording()?.run();
        engine.complete_save(&recording);
        let replay = replay_worker
            .join()
            .map_err(|_| anyhow::anyhow!("Replay worker failed"))?;
        engine.complete_save(&replay);
        let (recording_path, recording_probe) = recording?;
        let (replay_path, replay_probe) = replay?;
        let media = Media::discover()?;
        media.verify(&recording_path, if desktop_silent { 0 } else { 3 })?;
        media.verify(&replay_path, if desktop_silent { 0 } else { 3 })?;
        let summary = serde_json::json!({"physical":physical,"physical_audio":!desktop_silent && (physical || device_only),"recording":recording_path,"replay":replay_path,"recording_probe":recording_probe,"replay_probe":replay_probe,"status":engine.status(),"samples":samples,"note":"No sounds played. Device packets and timestamps do not prove speech intelligibility, acoustic A/V sync or long-session drift."});
        std::fs::write(
            root.join("report.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        Ok(())
    })();
    let stopped = engine.shutdown();
    result?;
    stopped
}

/// Delay to the next absolute diagnostic sampling tick, capped at the cut.
fn capture_poll_delay(elapsed: Duration, limit: Duration) -> Option<Duration> {
    if elapsed >= limit {
        return None;
    }
    let next_tick = Duration::from_secs(elapsed.as_secs().saturating_add(1));
    Some(next_tick.min(limit).saturating_sub(elapsed))
}

/// Crosshair Studio cost: REPLAYX's own CPU, memory and GDI/USER objects with
/// the crosshair off, on and off again, then the game-like GPU workload
/// (replayx-gameproxy, borderless full screen) with the crosshair off, on
/// and off (A/B/A): FPS, 1% and 0.1% lows and how Windows presented the
/// frames. Shows a crosshair and a full-screen workload on the main display.
/// usage: replayx-cli crosshair-bench [seconds per game run, default 12]
fn crosshair_bench(args: &[String]) -> Result<()> {
    use replayx_core::{crosshair::CrosshairSettings, crosshair_overlay::CrosshairOverlay};
    use windows::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{
            GetCurrentProcess, GetGuiResources, GetProcessTimes, GR_GDIOBJECTS, GR_USEROBJECTS,
        },
    };
    let seconds: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let proxy = std::env::current_exe()?.with_file_name("replayx-gameproxy.exe");
    let root = std::env::current_dir()?.join("test-output").join(format!(
        "crosshair-bench-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir_all(&root)?;
    let on = CrosshairSettings {
        enabled: true,
        visibility: "always".into(),
        ..Default::default()
    };
    let overlay = CrosshairOverlay::default();
    // (CPU ms, working set KB, GDI objects, USER objects) of this process.
    let sample = || unsafe {
        let p = GetCurrentProcess();
        let (mut c, mut e, mut k, mut u) = Default::default();
        let _ = GetProcessTimes(p, &mut c, &mut e, &mut k, &mut u);
        let ft = |f: windows::Win32::Foundation::FILETIME| {
            ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 10_000.0
        };
        let mut mem = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let _ = GetProcessMemoryInfo(p, &mut mem, mem.cb);
        (
            ft(k) + ft(u),
            mem.WorkingSetSize as f64 / 1024.0,
            GetGuiResources(p, GR_GDIOBJECTS),
            GetGuiResources(p, GR_USEROBJECTS),
        )
    };
    println!("== Resources in REPLAYX's process (5 s each)");
    let mut resources = Vec::new();
    for (label, show) in [("off", false), ("on", true), ("off again", false)] {
        if show {
            overlay.apply(&on, true);
        } else {
            overlay.stop();
        }
        thread::sleep(Duration::from_millis(500)); // drawn and settled
        let a = sample();
        thread::sleep(Duration::from_secs(5));
        let b = sample();
        println!(
            "  crosshair {label:<10} CPU {:>6.2} ms / 5 s · working set {:>7.0} KB · GDI {:>3} · USER {:>3} · window {}",
            b.0 - a.0,
            b.1,
            b.2,
            b.3,
            overlay.state().shown
        );
        resources.push(serde_json::json!({"crosshair": label, "cpu_ms_per_5s": b.0 - a.0,
            "working_set_kb": b.1, "gdi_objects": b.2, "user_objects": b.3, "shown": overlay.state().shown}));
    }
    println!(
        "\n== Game-like GPU workload, {seconds} s each (replayx-gameproxy, borderless full screen)"
    );
    let mut runs = Vec::new();
    for (n, (label, show)) in [("off", false), ("on", true), ("off", false)]
        .into_iter()
        .enumerate()
    {
        if show {
            overlay.apply(&on, true);
        } else {
            overlay.stop();
        }
        let log = root.join(format!("gameproxy-{n}-{label}.json"));
        let status = std::process::Command::new(&proxy)
            .args(["--seconds", &seconds.to_string(), "--log"])
            .arg(&log)
            .status()?;
        if !status.success() {
            bail!("replayx-gameproxy failed: {status}");
        }
        let report: serde_json::Value = serde_json::from_slice(&std::fs::read(&log)?)?;
        let presents: Vec<f64> = report["presents_s"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_default();
        // Skip the first second (start-up), then frame intervals.
        let kept: Vec<f64> = presents.into_iter().filter(|t| *t >= 1.0).collect();
        let mut frames: Vec<f64> = kept.windows(2).map(|w| (w[1] - w[0]) * 1000.0).collect();
        let span = kept.last().unwrap_or(&0.0) - kept.first().unwrap_or(&0.0);
        let fps = if span > 0.0 {
            frames.len() as f64 / span
        } else {
            0.0
        };
        frames.sort_by(|a, b| b.total_cmp(a));
        let low = |share: f64| {
            let n = ((frames.len() as f64 * share).ceil() as usize)
                .max(1)
                .min(frames.len().max(1));
            let worst = &frames[..n.min(frames.len())];
            if worst.is_empty() {
                0.0
            } else {
                1000.0 / (worst.iter().sum::<f64>() / worst.len() as f64)
            }
        };
        let mut modes = std::collections::BTreeMap::<i64, usize>::new();
        for m in report["presentation_modes"]
            .as_array()
            .into_iter()
            .flatten()
        {
            *modes.entry(m["mode"].as_i64().unwrap_or(-1)).or_default() += 1;
        }
        let mode_text: Vec<String> = modes
            .iter()
            .map(|(m, c)| {
                format!(
                    "{}:{c}",
                    match m {
                        0 => "composed",
                        1 => "overlay-plane",
                        2 => "independent-flip",
                        _ => "other",
                    }
                )
            })
            .collect();
        println!(
            "  {} crosshair {label:<4} {fps:>7.1} FPS · 1% low {:>7.1} · 0.1% low {:>7.1} · presented {}",
            n + 1,
            low(0.01),
            low(0.001),
            mode_text.join(" ")
        );
        runs.push(
            serde_json::json!({"run": n + 1, "crosshair": label, "fps": fps, "low_1pct": low(0.01),
            "low_0_1pct": low(0.001), "presentation_modes": modes, "adapter": report["adapter"],
            "width": report["width"], "height": report["height"]}),
        );
    }
    overlay.stop();
    let out = root.join("crosshair-bench.json");
    std::fs::write(
        &out,
        serde_json::to_vec_pretty(&serde_json::json!({"resources": resources, "workload": runs}))?,
    )?;
    println!("\nwrote {}", out.display());
    Ok(())
}

/// Proof that the crosshair stays out of REPLAYX's clips: real Desktop
/// Duplication frames (what REPLAYX records) of the crosshair over a solid
/// backdrop, with it hidden, shown (excluded from capture) and, as a control
/// that proves the test can see it, shown with capture allowed.
/// usage: replayx-cli crosshair-capture-test [out-dir]
fn crosshair_capture_test(args: &[String]) -> Result<()> {
    use replayx_core::{crosshair::CrosshairSettings, crosshair_overlay::CrosshairOverlay};
    use windows::Win32::{
        Foundation::{HWND, RECT},
        UI::{
            HiDpi::{SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            WindowsAndMessaging::*,
        },
    };
    // GetWindowRect and the capture crop must use the same physical pixels.
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        bail!("Could not query crosshair bounds in physical pixels");
    }
    let out = PathBuf::from(
        args.get(2)
            .map(String::as_str)
            .unwrap_or("test-output/crosshair-capture"),
    );
    std::fs::create_dir_all(&out)?;
    let media = Media::discover()?;
    let on = CrosshairSettings {
        enabled: true,
        visibility: "always".into(),
        color: "#FF00FF".into(),
        thickness: 4.0,
        length: 20.0,
        ..Default::default()
    };
    let overlay = CrosshairOverlay::default();
    overlay.apply(&on, true);
    thread::sleep(Duration::from_millis(600));
    let state = overlay.state();
    if !state.shown {
        bail!("the crosshair did not appear: {:?}", state.error);
    }
    let hwnd = HWND(state.hwnd as *mut _);
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }?;
    let pad = 40;
    let back = RECT {
        left: r.left - pad,
        top: r.top - pad,
        right: r.right + pad,
        bottom: r.bottom + pad,
    };
    let backdrop = thread::spawn(move || unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        backdrop_window(back, Duration::from_secs(12));
    });
    thread::sleep(Duration::from_millis(500));
    overlay.raise();
    thread::sleep(Duration::from_millis(300));
    let display = replayx_core::geometry::displays()
        .into_iter()
        .find(|d| {
            r.left >= d.left
                && r.left < d.left + d.desktop_width as i32
                && r.top >= d.top
                && r.top < d.top + d.desktop_height as i32
        })
        .ok_or_else(|| anyhow::anyhow!("the crosshair is not on a known display"))?;
    let (w, h) = (
        ((r.right - r.left) as u32 + 1) & !1,
        ((r.bottom - r.top) as u32 + 1) & !1,
    );
    let (x, y) = ((r.left - display.left) as u32, (r.top - display.top) as u32);
    let monitor = replayx_core::geometry::monitor_handle(display.index)?;
    // Both capture methods REPLAYX records with.
    let grab = |name: &str, wgc: bool| -> Result<Vec<u8>> {
        let name = format!("{name}-{}", if wgc { "wgc" } else { "dda" });
        let source = if wgc {
            format!(
                "gfxcapture=hmonitor={monitor}:crop_left={x}:crop_top={y}:crop_right={}:crop_bottom={}:max_framerate=10:capture_cursor=0:display_border=0,hwdownload,format=bgra",
                display.desktop_width.saturating_sub(x + w),
                display.desktop_height.saturating_sub(y + h)
            )
        } else {
            format!("ddagrab=output_idx={}:framerate=10:video_size={w}x{h}:offset_x={x}:offset_y={y},hwdownload,format=bgra", display.index)
        };
        let file = out.join(format!("{name}.mkv"));
        let ok = replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(source)
            .args(["-frames:v", "8", "-c:v", "ffv1"])
            .arg(&file)
            .status()?
            .success();
        if !ok {
            bail!("ddagrab failed for {name}");
        }
        let _ = replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-y", "-sseof", "-0.3", "-i"])
            .arg(&file)
            .args(["-frames:v", "1"])
            .arg(out.join(format!("{name}.png")))
            .status();
        Ok(replayx_core::media::command(&media.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-sseof", "-0.3", "-i"])
            .arg(&file)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()?
            .stdout)
    };
    let excluded = [
        grab("crosshair-excluded", false)?,
        grab("crosshair-excluded", true)?,
    ];
    // Control: the same window with capture allowed (test only).
    unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) }?;
    thread::sleep(Duration::from_millis(300));
    let visible = [
        grab("crosshair-capturable-control", false)?,
        grab("crosshair-capturable-control", true)?,
    ];
    overlay.stop();
    thread::sleep(Duration::from_millis(300));
    let hidden = [
        grab("crosshair-hidden", false)?,
        grab("crosshair-hidden", true)?,
    ];
    let _ = backdrop.join();
    let diff = |a: &[u8], b: &[u8]| -> f64 {
        let n = a.len().min(b.len()).max(1);
        a.iter()
            .zip(b)
            .map(|(p, q)| (*p as f64 - *q as f64).abs())
            .sum::<f64>()
            / n as f64
    };
    let mut pass = true;
    let mut methods = serde_json::Map::new();
    for (i, method) in ["desktop_duplication", "windows_graphics_capture"]
        .into_iter()
        .enumerate()
    {
        let (ex, ctl) = (
            diff(&excluded[i], &hidden[i]),
            diff(&visible[i], &hidden[i]),
        );
        let ok = !hidden[i].is_empty() && ex < 1.0 && ctl > 5.0;
        pass &= ok;
        methods.insert(method.into(), serde_json::json!({
            "excluded_vs_hidden_mean_abs_diff": ex, "capturable_vs_hidden_mean_abs_diff": ctl, "pass": ok }));
    }
    let report = serde_json::json!({
        "display": display.device, "region": [x, y, w, h], "methods": methods,
        "pass": pass, "files": out.display().to_string(),
    });
    std::fs::write(out.join("result.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !pass {
        bail!("crosshair capture exclusion check failed");
    }
    Ok(())
}

#[cfg(test)]
mod capture_clock_tests {
    use super::{capture_poll_delay, native_app_duration};
    use std::time::Duration;

    #[test]
    fn physical_audio_window_is_fifteen_seconds_and_bounded() {
        assert_eq!(native_app_duration(false, true, None).unwrap(), 15);
        assert_eq!(native_app_duration(false, true, Some(15)).unwrap(), 15);
        assert!(native_app_duration(false, true, Some(31)).is_err());
        assert!(native_app_duration(true, true, Some(15)).is_err());
        assert!(native_app_duration(false, false, Some(15)).is_err());
        assert_eq!(native_app_duration(true, false, Some(180)).unwrap(), 180);
    }

    #[test]
    fn startup_and_poll_cost_do_not_extend_the_wall_clock_capture() {
        let limit = Duration::from_secs(44);
        let mut elapsed = Duration::from_millis(5100);
        let mut samples = 0;
        while let Some(delay) = capture_poll_delay(elapsed, limit) {
            elapsed += delay + Duration::from_millis(200);
            samples += 1;
        }
        assert_eq!(elapsed, Duration::from_millis(44200));
        assert_eq!(samples, 39);
    }

    #[test]
    fn delayed_poll_skips_expired_ticks_without_a_catch_up_burst() {
        assert_eq!(
            capture_poll_delay(Duration::from_millis(3700), Duration::from_secs(10)),
            Some(Duration::from_millis(300))
        );
        assert_eq!(
            capture_poll_delay(Duration::from_millis(9900), Duration::from_secs(10)),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            capture_poll_delay(Duration::from_secs(10), Duration::from_secs(10)),
            None
        );
    }
}
