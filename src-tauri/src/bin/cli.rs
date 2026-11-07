use anyhow::{bail, Result};
use replayx_core::{engine::Engine, media::Media, model::Settings};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
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
    if !["self-test", "capture-test", "recover"].contains(&action) {
        println!("replayx-cli encoders | devices | device-formats | displays | audio-test [seconds] [system|microphone] [device-id] | audio-test [seconds] application PID | self-test | capture-test [seconds:6..7200] [silent|system|microphone|app:PID] [desktop|stretch1080] [fps=N mode=M height=H encoder=E bitrate=MBPS] | probe FILE | recover DATA_DIR");
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
    let first = engine.save_replay()?;
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
    let feed = replayx_core::audio::spawn(source)?;
    let mut socket = TcpStream::connect(feed.url.strip_prefix("tcp://").unwrap())?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    let began = Instant::now();
    let mut pcm = Vec::new();
    let mut input = [0u8; 8192];
    let limit = seconds as usize * 48000 * 2 * 4;
    while pcm.len() < limit {
        if let Some(error) = feed.meter.error.lock().unwrap().clone() {
            bail!("Audio capture error: {error}")
        }
        if began.elapsed() > Duration::from_secs(seconds + 5) {
            bail!("Audio capture did not deliver enough samples")
        }
        let count = socket.read(&mut input[..8192.min(limit - pcm.len())])?;
        if count == 0 {
            bail!("Audio transport closed unexpectedly")
        }
        pcm.extend_from_slice(&input[..count]);
    }
    use std::sync::atomic::Ordering;
    let packets = feed.meter.packets.load(Ordering::Relaxed);
    let gaps = feed.meter.gaps.load(Ordering::Relaxed);
    let discontinuities = feed.meter.discontinuities.load(Ordering::Relaxed);
    let device_glitches = feed.meter.device_glitches.load(Ordering::Relaxed);
    let timestamp_errors = feed.meter.timestamp_errors.load(Ordering::Relaxed);
    let correction_ppm = f32::from_bits(feed.meter.correction_ppm.load(Ordering::Relaxed));
    let clock_ppm = f32::from_bits(feed.meter.clock_ppm.load(Ordering::Relaxed));
    let format = feed.meter.format.lock().unwrap().clone();
    drop(socket);
    drop(feed);
    let mut data = Vec::with_capacity(pcm.len() / 2);
    let mut power = 0f64;
    let mut peak = 0f32;
    for bytes in pcm.as_chunks::<4>().0 {
        let sample = f32::from_le_bytes(*bytes);
        if !sample.is_finite() {
            bail!("Nonfinite audio sample")
        }
        power += f64::from(sample).powi(2);
        peak = peak.max(sample.abs());
        data.extend_from_slice(&((sample.clamp(-1., 1.) * 32767.) as i16).to_le_bytes());
    }
    let rms = (power / (pcm.len() / 4) as f64).sqrt();
    let dir = std::env::current_dir()?.join("test-output").join(format!(
        "audio-test-{}-{}-{}",
        kind,
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        &uuid::Uuid::new_v4().to_string()[..8]
    ));
    std::fs::create_dir_all(&dir)?;
    let mut wave = std::fs::File::create(dir.join("capture.wav"))?;
    wave.write_all(b"RIFF")?;
    wave.write_all(&(36 + data.len() as u32).to_le_bytes())?;
    wave.write_all(b"WAVEfmt ")?;
    wave.write_all(&16u32.to_le_bytes())?;
    wave.write_all(&1u16.to_le_bytes())?;
    wave.write_all(&2u16.to_le_bytes())?;
    wave.write_all(&48000u32.to_le_bytes())?;
    wave.write_all(&192000u32.to_le_bytes())?;
    wave.write_all(&4u16.to_le_bytes())?;
    wave.write_all(&16u16.to_le_bytes())?;
    wave.write_all(b"data")?;
    wave.write_all(&(data.len() as u32).to_le_bytes())?;
    wave.write_all(&data)?;
    wave.sync_all()?;
    // Exact producer output (pre-encoder) for lossless analysis.
    std::fs::write(dir.join("capture.f32"), &pcm)?;
    let report = serde_json::json!({"kind":kind,"device":device,"name":name,"seconds":seconds,"packets":packets,"queue_gaps":gaps,"discontinuities":discontinuities,"device_glitches":device_glitches,"timestamp_errors":timestamp_errors,"drift_correction_ppm":correction_ppm,"measured_device_clock_ppm":clock_ppm,"endpoint_mix_format":format,"peak":peak,"rms":rms,"file":dir.join("capture.wav"),"note":"Real device PCM capture. Nonzero levels do not prove intelligible speech, source isolation, editor compatibility, or A/V synchronization."});
    std::fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if packets == 0 {
        bail!("No real device packets received; saved samples may be idle-loopback silence")
    }
    Ok(())
}
