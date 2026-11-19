//! Native backend adapter. Reuses Engine's ring, SaveJob, recovery and audio controls.
use super::*;
use std::net::{Shutdown, TcpListener, TcpStream};

/// Windows can inherit the nonblocking listener mode on accepted sockets.
/// An idle relay must block in recv, rather than spin on WSAEWOULDBLOCK.
fn configure_video_transport(socket: &TcpStream) -> Result<()> {
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(Duration::from_millis(500)))?;
    Ok(())
}

fn native_failure_detail(log: &str) -> String {
    if log.contains("Waiting for live encoder input (500 ms budget)") {
        return "The native hardware encoder stopped accepting frames for 500 ms. Capture stopped; completed segments were kept for recovery. Try Existing FFmpeg Engine or reduce GPU load.".into();
    }
    if log.lines().any(|line| {
        line.contains("Activate NVIDIA H.264 Encoder MFT") && line.contains("0x8000FFFF")
    }) {
        return format!(
            "Windows Media Foundation could not activate the NVIDIA H.264 encoder (0x8000FFFF). Native encoding did not start. {}",
            log.lines().find(|line| line.contains("hardware H.264 on adapter")).unwrap_or("").trim_start_matches("Error: ").trim()
        ).trim_end().to_owned();
    }
    log.chars()
        .rev()
        .take(2000)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn capture_interval_us() -> Result<Option<u32>> {
    let Some(value) = std::env::var_os("REPLAYX_NATIVE_CAPTURE_INTERVAL_US") else {
        return Ok(None);
    };
    let interval: u32 = value
        .to_string_lossy()
        .parse()
        .context("Experimental native capture interval must be an integer")?;
    if interval > 8333 {
        bail!("Experimental native capture interval must be 0..8333 microseconds")
    }
    Ok(Some(interval))
}

pub(super) fn enabled() -> bool {
    cfg!(test) || crate::native_engine_available()
}
pub(super) fn executable() -> PathBuf {
    std::env::var_os("REPLAYX_NATIVE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("replayx-native-proto.exe")
        })
}
pub(super) fn compatible(s: &Settings) -> Result<()> {
    if !enabled() {
        bail!("Native Engine requires the bundled worker or the isolated Native Alpha launch")
    }
    if !s.gpu_scaling {
        bail!("Native Alpha requires GPU scaling; using FFmpeg for your CPU-scaling preference")
    }
    if s.capture != "desktop-duplication" || s.source_rect.is_some() || s.canvas_mode != "stretch" {
        bail!("Native Alpha supports whole-display Stretch to fill only")
    }
    if s.output_width() != 1920 || s.height != 1080 || s.fps != 60 {
        bail!("Native Alpha currently supports 1920 ? 1080 at 60 FPS")
    }
    if !["auto", "h264"].contains(&s.codec.as_str())
        || !["auto", "h264_qsv", "h264_qsv_lp", "h264_nvenc", "h264_amf"]
            .contains(&s.encoder.as_str())
    {
        bail!("Native Alpha requires H.264 and a compatible hardware encoder on the capture GPU")
    }
    if !["balanced", "custom"].contains(&s.preset.as_str()) {
        bail!("Native Alpha supports Balanced or Custom bitrate; other presets use FFmpeg")
    }
    if !executable().is_file() {
        bail!("Native sidecar is missing; run npm run desktop:native to build it")
    }
    Ok(())
}
fn selected_vendor(
    s: &Settings,
    geometry: Option<&DisplayGeometry>,
) -> Result<crate::native_gpu::Vendor> {
    crate::native_gpu::Vendor::from_pci(geometry.map_or_else(
        || match s.encoder.as_str() {
            "h264_nvenc" => 0x10de,
            "h264_amf" => 0x1002,
            _ => 0x8086,
        },
        |g| g.adapter_vendor,
    ))
}
pub(super) struct Session {
    child: Child,
    _job: crate::process_job::CaptureJob,
    stop: PathBuf,
    report: PathBuf,
    log: PathBuf,
    socket: Option<TcpStream>,
    relay: Option<thread::JoinHandle<()>>,
    relay_error: Arc<Mutex<Option<String>>>,
    stopped: bool,
    startup_complete: bool,
    capture_vfr: bool,
    phase_lock: bool,
    pub(super) end_hns: Option<u64>,
}
impl Session {
    fn start(
        req: &PipelineRequest,
        dir: &Path,
        expected_display: Option<&DisplayGeometry>,
    ) -> Result<(Self, TcpStream, i64)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let stop = dir.join("native.stop");
        let report = dir.join("native.json");
        let log = dir.join("native.log");
        let origin = dir.join("origin.json");
        let mut command = media::command(executable());
        if let Some(display) = expected_display {
            command.args(["--expected-adapter-luid", &display.adapter_luid]);
        }
        // Explicit, alpha-only pacing experiments; the default is unchanged.
        let test_pacing = if crate::native_alpha_enabled() {
            std::env::var("REPLAYX_NATIVE_TIMESTAMP_TEST").ok()
        } else if cfg!(feature = "bundled-native") {
            Some("phase-lock".into())
        } else {
            None
        };
        let capture_vfr = test_pacing.as_deref() == Some("capture-vfr");
        let phase_lock = test_pacing.as_deref() == Some("phase-lock");
        if capture_vfr {
            command.args(["--pacing", "capture-vfr"]);
        }
        if phase_lock {
            command.args(["--pacing", "phase-lock"]);
            if let Ok(source) = std::env::var("REPLAYX_NATIVE_PHASE_SOURCE") {
                command.args(["--phase-source", &source]);
            }
        }
        if crate::native_alpha_enabled() {
            if let Some(interval) = capture_interval_us()? {
                command.args(["--interval-us", &interval.to_string()]);
            }
            if let Some(roi) = std::env::var_os("REPLAYX_NATIVE_TRACE_ROI") {
                command
                    .arg("--trace-file")
                    .arg(dir.join("native.trace.jsonl"))
                    .arg("--trace-roi")
                    .arg(roi);
            }
        }

        let limit = std::env::var("REPLAYX_NATIVE_TEST_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| (5..=86400).contains(v))
            .unwrap_or(86400)
            .to_string();
        command.arg("--stream-ts");
        command
            .args([
                "--app-session",
                "--graceful-stop",
                "--seconds",
                if req.synthetic && !physical_audio_test() {
                    "60"
                } else {
                    &limit
                },
                "--stream-address",
                &listener.local_addr()?.to_string(),
                "--origin-file",
            ])
            .arg(&origin)
            .arg("--stop-file")
            .arg(&stop);
        if req.synthetic {
            let vendor = selected_vendor(&req.settings, None)?;
            command.args([
                "--vendor",
                match vendor {
                    crate::native_gpu::Vendor::Intel => "8086",
                    crate::native_gpu::Vendor::Nvidia => "10de",
                    crate::native_gpu::Vendor::Amd => "1002",
                },
            ]);
            if vendor != crate::native_gpu::Vendor::Intel && !capture_vfr && !phase_lock {
                command.args(["--pacing", "clocked"]);
            }
            if physical_audio_test() || pcm_fixture(req) {
                command.arg("--clocked-synthetic");
            }
            command
                .args(["--synthetic", "regular80"])
                .arg(dir.join("unused.h264"))
                .args(["--paced", "1"]);
        } else {
            let rate = media::rate_plan(&req.settings, "h264_qsv", 1920, 1080);
            command
                .args([
                    "--record",
                    "--desktop",
                    "--display",
                    &req.settings.display.to_string(),
                    "--out",
                ])
                .arg(dir.join("unused.h264"))
                .args([
                    "--bitrate",
                    &(rate.target_mbps * 1_000_000).to_string(),
                    "--max-bitrate",
                    &(rate.cap_mbps * 1_000_000).to_string(),
                ]);
        }
        let child = command
            .stdout(fs::File::create(&report)?)
            .stderr(fs::File::create(&log)?)
            .spawn()?;
        let job = match crate::process_job::CaptureJob::attach(&child) {
            Ok(job) => job,
            Err(e) => {
                let mut child = child;
                let _ = child.kill();
                return Err(e);
            }
        };
        let mut session = Self {
            child,
            _job: job,
            stop,
            report,
            log,
            socket: None,
            relay: None,
            relay_error: Default::default(),
            stopped: false,
            startup_complete: false,
            capture_vfr,
            phase_lock,
            end_hns: None,
        };
        let until = Instant::now() + Duration::from_secs(10);
        let socket = loop {
            session.check()?;
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= until {
                bail!("Native encoder initialization timed out")
            }
            thread::sleep(Duration::from_millis(10));
        };
        configure_video_transport(&socket)?;
        session.socket = Some(socket.try_clone()?);
        let origin = loop {
            if let Ok(bytes) = fs::read(&origin) {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if let Some(time) = value["origin_qpc_hns"].as_i64() {
                        break time;
                    }
                }
            }
            session.check()?;
            if Instant::now() >= until {
                bail!("Native capture did not deliver its first frame")
            }
            thread::sleep(Duration::from_millis(10));
        };
        session.startup_complete = true;
        Ok((session, socket, origin))
    }
    fn connect(&mut self, mut input: TcpStream, mut output: std::process::ChildStdin) {
        let errors = self.relay_error.clone();
        self.relay = Some(thread::spawn(move || {
            let result = (|| -> Result<()> {
                let mut block = [0u8; 65536];
                loop {
                    match input.read(&mut block) {
                        Ok(0) => break,
                        Ok(n) => output.write_all(&block[..n])?,
                        Err(e)
                            if [std::io::ErrorKind::TimedOut, std::io::ErrorKind::WouldBlock]
                                .contains(&e.kind()) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                *errors.lock().unwrap() = Some(format!("Native video transport: {e:#}"));
            }
        }));
    }
    pub(super) fn check(&mut self) -> Result<()> {
        if let Some(e) = self.relay_error.lock().unwrap().clone() {
            bail!(e)
        }
        if let Some(status) = self.child.try_wait()? {
            if !self.stopped {
                let log = fs::read_to_string(&self.log).unwrap_or_default();
                bail!(
                    "Native encoder exited ({status}): {}",
                    native_failure_detail(&log)
                )
            }
        }
        Ok(())
    }
    pub(super) fn stop(&mut self) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        fs::write(&self.stop, [])?;
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = self.child.try_wait()? {
                if !status.success() {
                    bail!("Native encoder could not drain; completed segments are preserved")
                }
                let report: serde_json::Value = serde_json::from_slice(&fs::read(&self.report)?)?;
                if report["frames_out"] != report["encoded"]
                    || report["live_input_samples"] != 0
                    || report["output_timestamp_contract_valid"] != true
                    || (!self.capture_vfr && report["output_timestamps_regular"] != true)
                    || (self.capture_vfr && report["frame_pacing"] != "capture-vfr-experimental")
                    || (self.phase_lock
                        && report["frame_pacing"] != "clocked-phase-lock-experimental")
                {
                    bail!("Native completion integrity check failed; recover the retained segments")
                }
                if self.report.with_file_name("native.trace.jsonl").exists() {
                    fs::write(
                        self.report.with_file_name("native.complete.json"),
                        serde_json::to_vec(&report)?,
                    )?;
                }
                self.end_hns = report["presentation_end_hns"].as_u64().or_else(|| {
                    report["origin_qpc_hns"]
                        .as_u64()
                        .zip(report["frames_out"].as_u64())
                        .map(|(origin, frames)| origin + frames * 10_000_000 / 60)
                });
                return Ok(());
            }
            if Instant::now() >= until {
                let _ = self.child.kill();
                bail!("Native encoder shutdown timed out; completed segments preserved")
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        // Without the origin handshake no muxer has received footage. A worker
        // stuck in initialization cannot drain and should not delay fallback.
        if self.startup_complete {
            let _ = self.stop();
        }
        let _ = self.child.kill();
        if let Some(socket) = &self.socket {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(relay) = self.relay.take() {
            let until = Instant::now() + Duration::from_secs(2);
            while !relay.is_finished() && Instant::now() < until {
                thread::sleep(Duration::from_millis(10));
            }
            if relay.is_finished() {
                let _ = relay.join();
            }
        }
    }
}
fn audio_graph(s: &Settings, feeds: &[AudioFeed], origin: i64, synthetic: bool) -> String {
    let sources: Vec<_> = s.audio.iter().filter(|s| s.enabled).collect();
    let mut graph = Vec::new();
    for (n, source) in sources.iter().enumerate() {
        let delta = if synthetic {
            0
        } else {
            ((feeds[n].meter.timeline_origin_hns.load(Ordering::Acquire) as i128 - origin as i128)
                * 48000
                / 10_000_000) as i64
        };
        let chain = media::source_chain(source);
        let chain = if chain.is_empty() {
            String::new()
        } else {
            format!(",{chain}")
        };
        let end = if s.separate_tracks {
            format!(",asplit=2[m{n}][a{n}]")
        } else {
            format!("[m{n}]")
        };
        graph.push(format!(
            "[{}:a]atrim=start_sample={},asetpts=PTS-STARTPTS,adelay={}S:all=1{chain}{end}",
            n + 1,
            (-delta).max(0),
            delta.max(0)
        ));
    }
    if !sources.is_empty() {
        graph.push(format!(
            "{}amix=inputs={}:normalize=1[mix]",
            (0..sources.len())
                .map(|n| format!("[m{n}]"))
                .collect::<String>(),
            sources.len()
        ));
    }
    graph.join(";")
}
fn pcm_fixture(req: &PipelineRequest) -> bool {
    cfg!(test) && req.synthetic && req.settings.diagnostics
}
fn physical_audio_test() -> bool {
    crate::native_alpha_enabled()
        && std::env::var_os("REPLAYX_NATIVE_PHYSICAL_AUDIO").is_some_and(|v| v == "1")
        && !std::env::var_os("REPLAYX_ALPHA_SYNTHETIC").is_some_and(|v| v == "1")
}
pub(super) fn prepare(req: &PipelineRequest) -> Result<Prepared> {
    compatible(&req.settings)?;
    let s = &req.settings;
    let geometry = if req.synthetic {
        None
    } else {
        let g = geometry::display(s.display)?;
        crate::native_gpu::Vendor::from_pci(g.adapter_vendor)?.choose(&s.encoder)?;
        if g.rotation != 1
            || g.active_width != g.desktop_width
            || g.active_height != g.desktop_height
        {
            bail!("Native capture geometry is ambiguous after a display-mode change; using FFmpeg to preserve stretched capture")
        }
        Some(g)
    };
    let plan = geometry::plan(
        geometry
            .as_ref()
            .map(|g| g.region(None))
            .transpose()?
            .unwrap_or(geometry::CaptureRect {
                x: 0,
                y: 0,
                width: 2560,
                height: 1600,
            }),
        1920,
        1080,
        "stretch",
    )?;
    preflight_once(&req.media, false)?;
    let dir = req
        .data_dir
        .join("buffer")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("session.json"), serde_json::to_vec_pretty(s)?)?;
    let generated_audio = req.synthetic && !physical_audio_test() && !pcm_fixture(req);
    let shared_origin = Arc::new(AtomicU64::new(0));
    let feeds = if generated_audio {
        Vec::new()
    } else {
        #[cfg(test)]
        if pcm_fixture(req) {
            return prepare_with_fixture(req, &dir, geometry, plan);
        }
        thread::scope(|scope| -> Result<Vec<AudioFeed>> {
            let opening: Vec<_> = s
                .audio
                .iter()
                .filter(|a| a.enabled)
                .cloned()
                .map(|source| {
                    let clock = shared_origin.clone();
                    scope.spawn(move || audio::spawn_native_clocked(source, clock))
                })
                .collect();
            opening
                .into_iter()
                .map(|task| {
                    task.join()
                        .map_err(|_| anyhow::anyhow!("Audio initialization worker failed"))?
                })
                .collect()
        })?
    };
    prepare_with_feeds(
        req,
        dir,
        geometry,
        plan,
        feeds,
        generated_audio,
        Some(shared_origin),
    )
}
fn prepare_with_feeds(
    req: &PipelineRequest,
    dir: PathBuf,
    geometry: Option<DisplayGeometry>,
    plan: geometry::VideoPlan,
    feeds: Vec<AudioFeed>,
    generated_audio: bool,
    shared_origin: Option<Arc<AtomicU64>>,
) -> Result<Prepared> {
    let s = &req.settings;
    let (mut native, socket, origin) = Session::start(req, &dir, geometry.as_ref())?;
    if let Some(clock) = &shared_origin {
        clock.store(origin as u64, Ordering::Release);
    }
    let aligned_audio = generated_audio || shared_origin.is_some();
    fs::write(
        dir.join("native-clocks.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"video_origin_hns":origin,"audio_origins_hns":feeds.iter().map(|f|f.meter.timeline_origin_hns.load(Ordering::Acquire)).collect::<Vec<_>>(),"graph":audio_graph(s,&feeds,origin,aligned_audio),"shared_audio_origin_hns":shared_origin.as_ref().map(|_|origin),"generated_video":req.synthetic,"generated_audio":generated_audio}),
        )?,
    )?;
    let count = s.audio.iter().filter(|a| a.enabled).count();
    let mut c = media::command(&req.media.ffmpeg);
    let capture_vfr = native.capture_vfr;
    c.args([
        "-hide_banner",
        "-v",
        "warning",
        "-nostdin",
        "-n",
        "-progress",
        "pipe:1",
        "-stats_period",
        "0.25",
        "-probesize",
        "32",
        "-analyzeduration",
        "0",
        "-fpsprobesize",
        "0",
    ]);
    if !capture_vfr {
        c.args(["-r", "60"]);
    }
    c.args(["-f", "mpegts", "-i", "pipe:0"]);
    for (n, _) in s.audio.iter().filter(|a| a.enabled).enumerate() {
        c.args([
            "-thread_queue_size",
            "256",
            "-probesize",
            "32",
            "-analyzeduration",
            "0",
        ]);
        if generated_audio {
            c.args([
                "-f",
                "lavfi",
                "-i",
                &format!(
                    "aevalsrc=if(between(mod(t\\,2)\\,1\\,1.02)\\,0.4*sin(2*PI*{}*t)\\,0):s=48000",
                    440 + n * 440
                ),
            ]);
        } else {
            c.args([
                "-f",
                "f32le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-i",
                &feeds[n].url,
            ]);
        }
    }
    if count > 0 {
        c.args([
            "-filter_complex",
            &audio_graph(s, &feeds, origin, aligned_audio),
        ]);
    }
    c.args(["-map", "0:v:0", "-c:v", "copy"]);
    if !capture_vfr {
        c.args([
            "-bsf:v",
            "setts=time_base=1/60000:prescale=1:pts=N*1000:dts=N*1000:duration=1000",
        ]);
    } else {
        c.args(["-bsf:v", "h264_metadata=fixed_frame_rate_flag=0"]);
    }
    if count > 0 {
        c.args([
            "-map",
            "[mix]",
            "-metadata:s:a:0",
            "title=Playback mix",
            "-disposition:a:0",
            "default",
        ]);
        if s.separate_tracks {
            for (n, source) in s.audio.iter().filter(|a| a.enabled).enumerate() {
                c.args([
                    "-map",
                    &format!("[a{n}]"),
                    &format!("-metadata:s:a:{}", n + 1),
                    &format!("title={}", source.name),
                    &format!("-disposition:a:{}", n + 1),
                    "0",
                ]);
            }
        }
        c.args(["-c:a", "aac", "-b:a", &format!("{}k", s.audio_bitrate_kbps)]);
    }
    if generated_audio && count > 0 {
        c.args(["-shortest", "-shortest_buf_duration", "1"]);
    }
    c.args([
        "-max_muxing_queue_size",
        "512",
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
    ]);
    fs::write(dir.join("native-command.txt"), format!("{c:?}"))?;
    let mut child = c
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let job = match crate::process_job::CaptureJob::attach(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            return Err(e);
        }
    };
    native.connect(socket, child.stdin.take().unwrap());
    let progress = Arc::new(Progress::default());
    let started = Instant::now();
    let readers = progress_readers(&mut child, &progress, started, req.began);
    let expected_audio = if count == 0 {
        0
    } else if s.separate_tracks {
        count + 1
    } else {
        1
    };
    let encoder = selected_vendor(s, geometry.as_ref())?.choose(&s.encoder)?;
    let scaling_detail = format!("Native same-adapter D3D11 scaling + {encoder}; FFmpeg copies video and encodes audio; GPU LUID {}; {}", geometry.as_ref().map_or("generated Intel".to_owned(), |g|g.adapter_luid.clone()), if capture_vfr {"EXPLICIT timestamp-preserving VFR test, not 60 FPS CFR"}else if native.phase_lock {"60 FPS CFR, EXPERIMENTAL phase-locked selection"}else{"60 FPS CFR"});
    let mut prepared = Prepared {
        pipeline: Pipeline {
            child,
            _job: job,
            native: Some(native),
            discard_tail: false,
            feeds,
            dir,
            manifest_offset: 0,
            progress,
            started,
            expected_audio,
            readers,
            geometry,
            last_geometry_check: Instant::now(),
            last_segment_ms: 0,
            validated: false,
            expected_size: Some((1920, 1080)),
            scaler: Scaler::Cpu,
            synthetic: req.synthetic,
            began: req.began,
            reported_first_frame: false,
            wgc_adaptive: false,
            dup_window: None,
            starving_windows: 0,
        },
        encoder: encoder.into(),
        scaling_detail: Some(scaling_detail),
        capture_api: "wgc",
        capture_filter: "native-wgc",
        capture_fallback: None,
        engine_fallback: None,
        plan: Some(plan),
        expected_audio,
        cache: None,
        timings: Vec::new(),
    };
    // Do not claim a native start until a closed segment is independently playable.
    let until = Instant::now() + Duration::from_secs(10);
    let verified = (|| -> Result<()> {
        loop {
            for feed in &prepared.pipeline.feeds {
                if let Some(error) = feed.meter.error.lock().unwrap().clone() {
                    bail!("Audio initialization failed: {error}")
                }
            }
            prepared
                .pipeline
                .native
                .as_mut()
                .unwrap()
                .check()
                .with_context(|| prepared.pipeline.progress.lines.lock().unwrap().join(" | "))?;
            if let Some(exit) = prepared.pipeline.child.try_wait()? {
                bail!("Native muxer exited ({exit})")
            }
            let csv = manifest_tail(&prepared.pipeline.dir.join("segments.csv"), 0)?;
            if let Some(row) = csv.split_inclusive('\n').find(|row| row.ends_with('\n')) {
                let file = row.split(',').next().unwrap_or_default().trim_matches('"');
                if !file.starts_with("segment-") || file.contains(['/', '\\']) {
                    bail!("Invalid native segment manifest")
                }
                let probe = req
                    .media
                    .verify_playable(&prepared.pipeline.dir.join(file), expected_audio)?;
                if (probe.width, probe.height) != (1920, 1080) {
                    bail!("Native encoded unexpected dimensions")
                }
                return Ok(());
            }
            if Instant::now() >= until {
                bail!(
                    "Native encoder did not produce a playable segment in ten seconds: {}",
                    prepared.pipeline.progress.lines.lock().unwrap().join(" | ")
                )
            }
            thread::sleep(Duration::from_millis(40));
        }
    })();
    if let Err(error) = verified {
        let mut pipeline = prepared.pipeline;
        if let Some(mut native) = pipeline.native.take() {
            let _ = native.stop();
        }
        let _ = pipeline.child.kill();
        let _ = pipeline.child.wait();
        pipeline.feeds.clear();
        for reader in pipeline.readers.drain(..) {
            let _ = reader.join();
        }
        fs::write(
            pipeline.dir.join("failure.txt"),
            format!("Native initialization failed; source segments retained: {error:#}"),
        )?;
        return Err(error);
    }
    mark(
        &mut prepared.timings,
        req.began,
        "Native first segment validated",
    );
    Ok(prepared)
}
#[cfg(test)]
fn prepare_with_fixture(
    req: &PipelineRequest,
    dir: &Path,
    geometry: Option<DisplayGeometry>,
    plan: geometry::VideoPlan,
) -> Result<Prepared> {
    let clock = Arc::new(AtomicU64::new(0));
    let feeds = req
        .settings
        .audio
        .iter()
        .filter(|s| s.enabled)
        .enumerate()
        .map(|(n, _)| audio::native_pcm_fixture(440. * (n + 1) as f32, clock.clone()))
        .collect::<Result<Vec<_>>>()?;
    prepare_with_feeds(
        req,
        dir.to_path_buf(),
        geometry,
        plan,
        feeds,
        false,
        Some(clock),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_video_transport_blocks_when_idle_then_delivers_data_and_eof() {
        use std::io::{Read, Write};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut writer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut reader, _) = listener.accept().unwrap();
        // Reproduce the Windows inherited mode before applying production setup.
        reader
            .set_read_timeout(Some(Duration::from_millis(80)))
            .unwrap();
        let mut block = [0u8; 8];
        let before = Instant::now();
        let previous = reader.read(&mut block).unwrap_err();
        eprintln!(
            "Before transport setup: {:?} after {:?}",
            previous.kind(),
            before.elapsed()
        );
        configure_video_transport(&reader).unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(80)))
            .unwrap();
        let idle = Instant::now();
        let error = reader.read(&mut block).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ));
        assert!(
            idle.elapsed() >= Duration::from_millis(60),
            "Idle socket spun instead of waiting: {:?}",
            idle.elapsed()
        );
        writer.write_all(b"frame").unwrap();
        writer.shutdown(Shutdown::Write).unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"frame");
    }
    #[test]
    fn legacy_settings_keep_ffmpeg_default() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.recording_engine, "ffmpeg");
    }
    #[test]
    fn incompatible_native_settings_are_explicit() {
        let s = Settings {
            fps: 30,
            recording_engine: "native".into(),
            ..Default::default()
        };
        assert!(compatible(&s).unwrap_err().to_string().contains("60 FPS"));
    }
}

#[cfg(test)]
mod app_integration {
    use super::*;
    #[test]
    fn live_starvation_message_names_the_failure_without_benign_codec_warnings() {
        let message = native_failure_detail("Encoder property b_frames=0 rejected\nError: Waiting for live encoder input (500 ms budget); capture stopped\nEncoder operation timed out");
        assert!(message.contains("500 ms"));
        assert!(message.contains("Capture stopped"));
        assert!(!message.contains("b_frames"));
        assert_eq!(native_failure_detail("driver failed"), "driver failed");
    }
    #[test]
    fn nvidia_mft_activation_error_preserves_adapter_and_hresult() {
        let log = "Error: VEN_10de hardware H.264 on adapter 0000000000013833\n\nCaused by:\n    Activate NVIDIA H.264 Encoder MFT: Catastrophic failure (0x8000FFFF)";
        let detail = native_failure_detail(log);
        assert!(detail.contains("Media Foundation"));
        assert!(detail.contains("0x8000FFFF"));
        assert!(detail.contains("0000000000013833"));
        assert_eq!(
            native_failure_detail("Other error (0x8000FFFF)"),
            "Other error (0x8000FFFF)"
        );
    }
    #[test]
    fn startup_cleanup_cancels_worker_without_a_graceful_drain() {
        let dir = tempfile::tempdir().unwrap();
        let child = media::command("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .spawn()
            .unwrap();
        let job = crate::process_job::CaptureJob::attach(&child).unwrap();
        let marker = dir.path().join("stop.request");
        let session = Session {
            child,
            _job: job,
            stop: marker.clone(),
            report: dir.path().join("native.json"),
            log: dir.path().join("native.log"),
            socket: None,
            relay: None,
            relay_error: Default::default(),
            stopped: false,
            startup_complete: false,
            capture_vfr: false,
            phase_lock: false,
            end_hns: None,
        };
        let start = Instant::now();
        drop(session);
        assert!(!marker.exists());
        assert!(start.elapsed() < Duration::from_secs(4));
    }
    fn test_root() -> PathBuf {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("test-output/native-v2");
        fs::create_dir_all(&folder).unwrap();
        let dir = tempfile::Builder::new()
            .prefix("app-")
            .tempdir_in(folder)
            .unwrap()
            .keep();
        eprintln!("Native app integration evidence: {}", dir.display());
        dir
    }
    fn test_engine(root: &Path, native: bool, fps: u32) -> Engine {
        let settings = Settings {
            recording_engine: if native { "native" } else { "ffmpeg" }.into(),
            output_dir: root.join("clips"),
            width: 1920,
            height: 1080,
            fps,
            diagnostics: true,
            organize_by_game: false,
            audio: vec![Default::default(), Default::default()],
            ..Default::default()
        };
        Engine::with_media(root.join("data"), settings, Media::discover().unwrap()).unwrap()
    }
    fn assert_mp4_audio_ends_with_video(path: &Path) {
        let media = Media::discover().unwrap();
        media.verify(path, 3).unwrap();
        let output = media::command(&media.ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=start_time,duration",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let streams = report["streams"].as_array().unwrap();
        assert_eq!(streams.len(), 4);
        let end = |track: &serde_json::Value| {
            track["start_time"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                + track["duration"].as_str().unwrap().parse::<f64>().unwrap()
        };
        for audio in &streams[1..] {
            let delta = end(audio) - end(&streams[0]);
            assert!(
                (-0.040..=0.002).contains(&delta),
                "{}: A/V end delta {delta:+.6} s",
                path.display()
            );
        }
    }

    #[test]
    #[ignore = "Opt-in terminal-only generated native app lifecycle"]
    fn native_app_replay_recording_concurrent_mp4_and_library() {
        let root = test_root();
        let engine = test_engine(&root, true, 60);
        let began = Instant::now();
        engine.start_synthetic(false).unwrap();
        assert!(
            began.elapsed() < Duration::from_secs(6),
            "Native must finalize live segments before EOF"
        );
        assert_eq!(
            engine.status().recording_engine,
            "native",
            "{:?}",
            engine.status().engine_fallback
        );
        thread::sleep(Duration::from_secs(1));
        engine.start_recording().unwrap();
        let crosshair = crate::crosshair::CrosshairSettings {
            enabled: true,
            ..Default::default()
        };
        let saved = engine.set_crosshair(crosshair).unwrap();
        assert!(saved.enabled && engine.settings().crosshair.enabled);
        assert!(engine.status().recording && engine.status().replay);
        thread::sleep(Duration::from_secs(3));
        let first = engine.save_replay().unwrap();
        let second = engine.save_replay().unwrap();
        let a = thread::spawn(move || first.run());
        let b = thread::spawn(move || second.run());
        for result in [a.join().unwrap(), b.join().unwrap()] {
            let (path, probe) = result.as_ref().unwrap();
            assert_eq!(path.extension().unwrap(), "mp4");
            assert_eq!(probe.audio_tracks, 3);
            assert_eq!(probe.audio_titles.len(), 3);
            assert!(probe.audio_titles.iter().all(|title| !title.is_empty()));
            assert!(probe.kind.is_some());
            assert_mp4_audio_ends_with_video(path);
            engine.complete_save(&result);
        }
        let job = engine.stop_recording().unwrap();
        let result = job.run();
        let (path, probe) = result.as_ref().unwrap();
        assert_eq!(path.extension().unwrap(), "mp4");
        assert_eq!((probe.width, probe.height), (1920, 1080));
        assert_eq!(probe.audio_tracks, 3);
        assert_mp4_audio_ends_with_video(path);
        engine.complete_save(&result);
        assert!(engine.status().replay);
        assert!(path.exists());
        assert!(crate::library::scan(&engine.settings().output_dir)
            .unwrap()
            .iter()
            .any(|(_, p, _)| p == path));
        engine.stop_replay().unwrap();
        assert!(!engine.status().recording && !engine.status().replay);
        assert!(engine.0.lock().unwrap().pipeline.is_none());
    }
    #[test]
    #[ignore = "Opt-in NVIDIA clocked PCM A/V and independent tracks"]
    fn nvidia_clocked_audio_markers_keep_sync_across_segments() {
        nvidia_audio_markers(None);
    }
    #[test]
    #[ignore = "Opt-in NVIDIA timestamp-preserving PCM A/V and independent tracks"]
    fn nvidia_capture_vfr_audio_markers_keep_sync_across_segments() {
        nvidia_audio_markers(Some("capture-vfr"));
    }
    #[test]
    #[ignore = "Opt-in NVIDIA phase-locked CFR PCM A/V and independent tracks"]
    fn nvidia_phase_lock_audio_markers_keep_sync_across_segments() {
        nvidia_audio_markers(Some("phase-lock"));
    }
    fn nvidia_audio_markers(mode: Option<&str>) {
        struct Restore(Option<std::ffi::OsString>);
        impl Drop for Restore {
            fn drop(&mut self) {
                if let Some(v) = &self.0 {
                    std::env::set_var("REPLAYX_NATIVE_TIMESTAMP_TEST", v)
                } else {
                    std::env::remove_var("REPLAYX_NATIVE_TIMESTAMP_TEST")
                }
            }
        }
        let _restore = Restore(std::env::var_os("REPLAYX_NATIVE_TIMESTAMP_TEST"));
        if let Some(mode) = mode {
            std::env::set_var("REPLAYX_NATIVE_TIMESTAMP_TEST", mode)
        } else {
            std::env::remove_var("REPLAYX_NATIVE_TIMESTAMP_TEST")
        }
        let root = test_root();
        let settings = Settings {
            recording_engine: "native".into(),
            encoder: "h264_nvenc".into(),
            output_dir: root.join("clips"),
            diagnostics: true,
            organize_by_game: false,
            audio: vec![Default::default(), crate::model::AudioSource::microphone()],
            ..Default::default()
        };
        let engine =
            Engine::with_media(root.join("data"), settings, Media::discover().unwrap()).unwrap();
        engine.start_synthetic(true).unwrap();
        assert_eq!(
            engine.status().recording_engine,
            "native",
            "{:?}",
            engine.status().engine_fallback
        );
        assert_eq!(engine.status().encoder, "h264_nvenc");
        thread::sleep(Duration::from_secs(3));
        let result = engine.stop_recording().unwrap().run();
        let (path, probe) = result.as_ref().unwrap();
        assert_eq!(probe.audio_tracks, 3);
        let media = Media::discover().unwrap();
        let data = media::command(&media.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-map", "0:a:1", "-ac", "1", "-ar", "48000", "-f", "f32le", "-",
            ])
            .output()
            .unwrap();
        assert!(data.status.success());
        let wave: Vec<_> = data
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let video_info = media::command(&media.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=start_time",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .unwrap();
        let video_info: serde_json::Value = serde_json::from_slice(&video_info.stdout).unwrap();
        let video_start: f64 = video_info["streams"][0]["start_time"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let mut sync_errors = Vec::new();
        for second in [0, 1, 2] {
            let start = (second as f64 + 0.45) * 48000.;
            let end = (second as f64 + 0.6) * 48000.;
            let onset = wave[start as usize..(end as usize).min(wave.len())]
                .iter()
                .position(|s| s.abs() > 0.02)
                .unwrap()
                + start as usize;
            let error = onset as f64 / 48000. - (video_start + second as f64 + 0.5);
            sync_errors.push(error * 1000.);
            assert!(
                error.abs() < 0.01,
                "audio marker moved across segment: {second}: {error}"
            );
        }
        let frame = media::command(&media.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-vf",
                &format!(
                    "select=gte(t\\,{:.6}),crop=16:16:952:532,extractplanes=y",
                    video_start + 0.5
                ),
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-",
            ])
            .output()
            .unwrap();
        assert!(frame.status.success());
        let luma = frame.stdout.iter().map(|v| *v as f64).sum::<f64>() / frame.stdout.len() as f64;
        assert!(
            (luma - 76.).abs() < 3.,
            "video marker at 0.5 s is the wrong capture: {luma}"
        );
        fs::write(root.join("av-sync.json"),serde_json::to_vec_pretty(&serde_json::json!({"video_start_seconds":video_start,"audio_marker_error_ms":sync_errors,"video_marker_at_half_second_luma":luma,"physical_devices":false,"backend":match mode{Some("capture-vfr")=>"NVIDIA capture-VFR experiment",Some("phase-lock")=>"NVIDIA phase-locked CFR experiment",_=>"NVIDIA clocked native"}})).unwrap()).unwrap();
        engine.complete_save(&result);
        engine.shutdown().unwrap();
    }
    #[test]
    #[ignore = "Opt-in terminal-only native incompatibility fallback"]
    fn unsupported_native_settings_use_ffmpeg_and_keep_preferences() {
        let root = test_root();
        let engine = test_engine(&root, true, 30);
        engine.start_synthetic(false).unwrap();
        assert_eq!(engine.status().recording_engine, "ffmpeg");
        assert!(engine
            .status()
            .engine_fallback
            .as_deref()
            .unwrap()
            .contains("60 FPS"));
        assert_eq!(engine.settings().recording_engine, "native");
        assert_eq!(engine.settings().fps, 30);
        thread::sleep(Duration::from_secs(3));
        engine.stop_replay().unwrap();
    }
    #[test]
    #[ignore = "Opt-in generated native runtime failure preservation"]
    fn native_runtime_failure_reports_error_and_saves_completed_recording() {
        let root = test_root();
        let engine = test_engine(&root, true, 60);
        engine.start_synthetic(true).unwrap();
        assert_eq!(
            engine.status().recording_engine,
            "native",
            "{:?}",
            engine.status().engine_fallback
        );
        thread::sleep(Duration::from_secs(3));
        assert!(
            engine.0.lock().unwrap().pipeline.is_some(),
            "native pipeline failed before injection: {:?}",
            engine.status().error
        );
        engine
            .0
            .lock()
            .unwrap()
            .pipeline
            .as_mut()
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .child
            .kill()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            let status = engine.status();
            if status.state == "Error" && status.saving == 0 {
                break;
            }
            assert!(
                Instant::now() < until,
                "runtime failure did not finish bounded cleanup"
            );
            thread::sleep(Duration::from_millis(50));
        }
        let status = engine.status();
        assert!(!status.recording && !status.replay);
        assert!(status.error.is_some());
        let clips = crate::library::scan(&engine.settings().output_dir).unwrap();
        assert!(!clips.is_empty(), "completed recording was not preserved");
        assert_mp4_audio_ends_with_video(&clips[0].1);
        assert!(engine.0.lock().unwrap().pipeline.is_none());
        let recovered = engine.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        let recovered = engine.settings().output_dir.join(&recovered[0]);
        assert_eq!(recovered.extension().unwrap(), "mp4");
        assert_mp4_audio_ends_with_video(&recovered);
    }
}
