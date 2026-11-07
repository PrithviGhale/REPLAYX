use crate::{
    audio::{self, AudioFeed},
    buffer::{snapshot, Ring, Segment},
    geometry::{self, DisplayGeometry},
    health,
    media::{self, Media, Probe, Scaler},
    model::Settings,
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Default, Clone, Serialize)]
pub struct Status {
    pub state: String,
    pub replay: bool,
    pub recording: bool,
    pub saving: u32,
    pub encoder: String,
    pub capture: String,
    pub buffer_seconds: f64,
    pub buffer_bytes: u64,
    pub free_bytes: u64,
    pub frames: u64,
    pub dropped_frames: u64,
    pub duplicated_frames: u64,
    /// Game in the foreground right now while capturing (None = desktop/app).
    pub detected_game: Option<String>,
    pub last_save_timings: Vec<media::StageTiming>,
    pub fps: f64,
    pub elapsed: f64,
    pub write_mbps: f64,
    pub memory_mb: f64,
    pub cpu_percent: f64,
    pub audio_peaks: Vec<f32>,
    pub audio_packets: Vec<u64>,
    pub audio_gaps: Vec<u64>,
    /// Audible audio timeline breaks per source (late data / gross timestamp jumps),
    /// plus packets Windows itself flagged as discontinuous.
    pub audio_discontinuities: Vec<u64>,
    /// Drift correction currently applied per source, in ppm.
    pub audio_drift_ppm: Vec<f32>,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub last_clip: Option<String>,
    pub recovery_sessions: usize,
    pub geometry: Option<DisplayGeometry>,
    pub scaling: String,
    /// Why faster scaling paths were not used (probe errors), if any.
    pub scaling_detail: Option<String>,
    pub output_width: u32,
    pub output_height: u32,
    pub segment_age_ms: u64,
    pub frame_age_ms: u64,
    pub audio_written_bytes: Vec<u64>,
    pub hotkey_error: Option<String>,
}
#[derive(Default)]
struct Progress {
    frames: AtomicU64,
    dropped: AtomicU64,
    duplicated: AtomicU64,
    last_ms: AtomicU64,
    fps: AtomicU64,
    lines: Mutex<Vec<String>>,
}
struct Pipeline {
    child: Child,
    _job: crate::process_job::CaptureJob,
    discard_tail: bool,
    feeds: Vec<AudioFeed>,
    dir: PathBuf,
    seen: usize,
    progress: Arc<Progress>,
    started: Instant,
    expected_audio: usize,
    readers: Vec<thread::JoinHandle<()>>,
    geometry: Option<DisplayGeometry>,
    last_geometry_check: Instant,
    last_segment_ms: u64,
    validated: bool,
    expected_size: Option<(u32, u32)>,
    scaler: Scaler,
    synthetic: bool,
}
struct Recovery {
    replay: bool,
    recording: bool,
    display: Option<DisplayGeometry>,
    attempts: u32,
    next: Instant,
    stable: Option<DisplayGeometry>,
}
struct Recording {
    dir: PathBuf,
    segments: Vec<Segment>,
    after: f64,
    started: Instant,
}
pub struct Inner {
    pub settings: Settings,
    pub data_dir: PathBuf,
    pub media: Media,
    pub status: Status,
    pipeline: Option<Pipeline>,
    ring: Ring,
    recording: Option<Recording>,
    recovery: Option<Recovery>,
    ring_audio_tracks: usize,
    /// Last successful video path for an identical display/plan/encoder request.
    video_path_cache: Option<(String, Scaler, String)>,
    /// GPU paths that passed a probe but failed before their first valid segment.
    denied_paths: Vec<(String, String)>,
    /// Foreground game timeline used to name and file clips.
    activity: crate::games::Activity,
    last_activity_sample: Option<Instant>,
    /// Clip paths handed to in-flight saves (not yet on disk).
    reserved: std::collections::HashSet<PathBuf>,
    _instance_lock: fs::File,
}
#[derive(Clone)]
pub struct Engine(pub Arc<Mutex<Inner>>);
pub struct SaveJob {
    media: Media,
    segments: Vec<Segment>,
    path: PathBuf,
    expected_audio: usize,
    dir: PathBuf,
    requested: Instant,
    tags: Vec<(String, String)>,
}
impl SaveJob {
    pub fn run(self) -> Result<(PathBuf, Probe)> {
        self.run_progress(|_| {})
    }
    pub fn run_progress(self, mut progress: impl FnMut(&str)) -> Result<(PathBuf, Probe)> {
        let prepared_ms = self.requested.elapsed().as_millis() as u64;
        let mut probe = self.media.concat_progress(
            &self.segments,
            &self.path,
            self.expected_audio,
            false,
            &self.tags,
            &mut progress,
        )?;
        for t in &mut probe.save_timings {
            t.elapsed_ms += prepared_ms;
        }
        probe.save_timings.insert(
            0,
            media::StageTiming {
                stage: "Snapshot prepared".into(),
                elapsed_ms: prepared_ms,
            },
        );
        // Publication succeeded. Cleanup failure must not turn a playable clip
        // into a failed save; remaining pins are recoverable on next launch.
        let _ = cleanup_saved_snapshot(&self.dir, &self.segments);
        Ok((self.path, probe))
    }
}
fn cleanup_saved_snapshot(dir: &Path, segments: &[Segment]) -> Result<()> {
    // Recording parts can contain quarantined transition footage. Never remove
    // that directory recursively when exporting the verified portion.
    for segment in segments {
        fs::remove_file(&segment.path)?;
    }
    let list = dir.join("concat.txt");
    if list.is_file() {
        fs::remove_file(list)?;
    }
    let _ = fs::remove_dir(dir);
    Ok(())
}
impl Engine {
    pub fn new(data_dir: PathBuf, settings: Settings) -> Result<Self> {
        Self::with_media(data_dir, settings, Media::discover()?)
    }
    fn with_media(data_dir: PathBuf, settings: Settings, media: Media) -> Result<Self> {
        settings.validate()?;
        fs::create_dir_all(&data_dir)?;
        let instance_lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(data_dir.join("instance.lock"))?;
        fs2::FileExt::try_lock_exclusive(&instance_lock)
            .context("Another REPLAYX instance is already using this data directory")?;
        fs::create_dir_all(&settings.output_dir)?;
        let recovery_sessions = count_sessions(&data_dir);
        let e = Self(Arc::new(Mutex::new(Inner {
            settings,
            data_dir,
            media,
            status: Status {
                state: "Stopped".into(),
                recovery_sessions,
                ..Default::default()
            },
            pipeline: None,
            ring: Ring::default(),
            recording: None,
            recovery: None,
            ring_audio_tracks: 0,
            video_path_cache: None,
            denied_paths: Vec::new(),
            activity: Default::default(),
            last_activity_sample: None,
            reserved: Default::default(),
            _instance_lock: instance_lock,
        })));
        let weak = Arc::downgrade(&e.0);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(200));
            let Some(arc) = weak.upgrade() else { break };
            let mut i = arc.lock().unwrap();
            match i.tick() {
                Ok(Some(job)) => {
                    let engine = Engine(arc.clone());
                    thread::spawn(move || {
                        let result = job.run();
                        engine.complete_save(&result);
                    });
                }
                Ok(None) => {}
                Err(e) => i.fail(format!("{e:#}")),
            }
        });
        Ok(e)
    }
    pub fn status(&self) -> Status {
        self.0.lock().unwrap().status.clone()
    }
    pub fn settings(&self) -> Settings {
        self.0.lock().unwrap().settings.clone()
    }
    pub fn set_settings(&self, s: Settings) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        if i.pipeline.is_some() || i.recovery.is_some() || i.status.saving > 0 {
            bail!("Stop capture and wait for saves before changing settings")
        };
        fs::create_dir_all(&s.output_dir)?;
        s.save(&i.data_dir.join("settings.json"))?;
        i.settings = s;
        Ok(())
    }
    pub fn start_replay(&self) -> Result<()> {
        self.start(false, false)
    }
    pub fn start_recording(&self) -> Result<()> {
        self.start(true, false)
    }
    pub fn start_synthetic(&self, recording: bool) -> Result<()> {
        self.start(recording, true)
    }
    fn start(&self, recording: bool, synthetic: bool) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        if recording && i.recording.is_some() {
            bail!("Recording is already active")
        }
        i.status.error = None;
        if i.recovery.is_some() {
            bail!("Capture is recovering. Stop all capture to cancel recovery first.")
        }
        if i.recording.is_some() && !i.status.recording {
            bail!("Recover the interrupted recording before starting a new session")
        }
        if i.pipeline.is_none() {
            if let Err(e) = i.start_pipeline(synthetic) {
                i.fail(format!("{e:#}"));
                return Err(e);
            }
        }
        if recording {
            let after = i.ring.segments.back().map(|s| s.end).unwrap_or(0.);
            let dir = i
                .data_dir
                .join("recordings")
                .join(uuid::Uuid::new_v4().to_string());
            fs::create_dir_all(&dir)?;
            i.recording = Some(Recording {
                dir,
                segments: Vec::new(),
                after,
                started: Instant::now(),
            });
            i.status.recording = true;
        } else {
            i.status.replay = true;
        }
        Ok(())
    }
    pub fn stop_replay(&self) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        if let Some(recovery) = &mut i.recovery {
            recovery.replay = false;
            if !recovery.recording {
                i.recovery = None;
                i.status.state = "Stopped".into();
            }
        }
        i.status.replay = false;
        if !i.status.recording {
            i.stop_pipeline()?;
        }
        Ok(())
    }
    pub fn stop_recording(&self) -> Result<SaveJob> {
        let requested = Instant::now();
        let mut i = self.0.lock().unwrap();
        if i.recording.is_none() {
            bail!("No active recording")
        }
        // Finalize current segment without changing codecs mid-recording. Replay is restarted explicitly below.
        let restart = i.status.replay;
        i.stop_pipeline()?;
        let rec = i.recording.take().unwrap();
        i.status.recording = false;
        let expected = i.expected_audio();
        let clip = i.clip_path("Recording", rec.started, requested);
        let job = SaveJob {
            media: i.media.clone(),
            segments: rec.segments,
            path: clip.0,
            expected_audio: expected,
            dir: rec.dir,
            requested,
            tags: clip.1,
        };
        i.status.saving += 1;
        if restart {
            if let Err(e) = i.start_pipeline(false) {
                i.fail(format!(
                    "Recording finalized, but replay restart failed: {e}"
                ));
            }
        }
        Ok(job)
    }
    pub fn save_replay(&self) -> Result<SaveJob> {
        let requested = Instant::now();
        let mut i = self.0.lock().unwrap();
        i.collect_segments()?;
        let dir = i
            .data_dir
            .join("saves")
            .join(uuid::Uuid::new_v4().to_string());
        let segments = i.ring.snapshot(i.settings.replay_seconds as f64, &dir)?;
        if segments.is_empty() {
            let _ = fs::remove_dir(&dir);
            bail!("No finalized footage yet. Wait at least two seconds.")
        }
        i.status.saving += 1;
        // Attribute the game over the footage actually saved, which ends at the
        // last finalized segment, not at the moment save was pressed.
        let covered: f64 = segments.iter().map(|s| s.end - s.start).sum();
        let end = requested
            .checked_sub(Duration::from_millis(i.status.segment_age_ms))
            .unwrap_or(requested);
        let start = end
            .checked_sub(Duration::from_secs_f64(covered))
            .unwrap_or(end);
        let (path, tags) = i.clip_path("Replay", start, end);
        Ok(SaveJob {
            media: i.media.clone(),
            segments,
            path,
            expected_audio: i.expected_audio(),
            dir,
            requested,
            tags,
        })
    }
    pub fn complete_save(&self, result: &Result<(PathBuf, Probe)>) {
        let mut i = self.0.lock().unwrap();
        i.status.saving = i.status.saving.saturating_sub(1);
        match result {
            Ok((p, probe)) => {
                i.status.last_clip = p.file_name().map(|p| p.to_string_lossy().into());
                i.status.notice =
                    Some("Clip ready — streams and playback boundaries checked".into());
                i.status.last_save_timings = probe.save_timings.clone();
            }
            Err(e) => {
                i.status.error = Some(format!("Save failed; source segments preserved: {e:#}"));
            }
        }
    }
    pub fn shutdown(&self) -> Result<()> {
        self.0.lock().unwrap().recovery = None;
        if self.status().recording {
            let job = self.stop_recording()?;
            let result = job.run();
            self.complete_save(&result);
            result?;
        }
        self.stop_replay()?;
        Ok(())
    }
    pub fn set_mix(&self, index: usize, volume: f32, muted: bool) -> Result<()> {
        if !volume.is_finite() || !(0.0..=2.0).contains(&volume) {
            bail!("Volume out of range")
        }
        let mut i = self.0.lock().unwrap();
        let s = i
            .settings
            .audio
            .get_mut(index)
            .context("Unknown audio source")?;
        s.volume = volume;
        s.muted = muted;
        let enabled_index = i.settings.audio[..index]
            .iter()
            .filter(|s| s.enabled)
            .count();
        if i.settings.audio[index].enabled {
            if let Some(p) = &i.pipeline {
                if let Some(f) = p.feeds.get(enabled_index) {
                    f.meter.volume.store(volume.to_bits(), Ordering::Relaxed);
                    f.meter.muted.store(muted, Ordering::Relaxed);
                }
            }
        }
        i.settings.save(&i.data_dir.join("settings.json"))?;
        Ok(())
    }
    pub fn recover(&self) -> Result<Vec<String>> {
        let mut i = self.0.lock().unwrap();
        if i.pipeline.is_some() || i.recovery.is_some() || i.status.saving > 0 {
            bail!("Stop capture before recovery")
        }
        let mut outputs = Vec::new();
        for category in ["buffer", "recordings", "saves"] {
            let base = i.data_dir.join(category);
            if !base.exists() {
                continue;
            }
            for entry in fs::read_dir(&base)? {
                let dir = entry?.path();
                if !dir.is_dir() || !crate::model::owned_session(&base, &dir) {
                    continue;
                }
                let mut files: Vec<_> = fs::read_dir(&dir)?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().is_some_and(|x| x == "mkv") && crate::model::regular_file(p)
                    })
                    .collect();
                files.sort();
                let mut segments = Vec::new();
                let mut end = 0.;
                let mut tracks = 0;
                for path in files {
                    if let Ok(probe) = i.media.probe(&path) {
                        tracks = probe.audio_tracks;
                        let start = end;
                        end += probe.duration;
                        segments.push(Segment {
                            path,
                            start,
                            end,
                            bytes: probe.bytes,
                        });
                    }
                }
                if segments.is_empty() {
                    continue;
                }
                let snapshot_dir = i
                    .data_dir
                    .join("recovery")
                    .join(uuid::Uuid::new_v4().to_string());
                let pinned = snapshot(&segments, &snapshot_dir)?;
                let destination = i.clip_path_flat("Recovered");
                i.media.concat(&pinned, &destination, tracks)?;
                fs::remove_dir_all(snapshot_dir)?;
                // Remove only recovered media; unreadable/torn files remain for inspection.
                for s in &segments {
                    fs::remove_file(&s.path)?;
                }
                if dir.join("session.json").is_file() {
                    fs::remove_file(dir.join("session.json"))?;
                }
                if dir.join("segments.csv").is_file() {
                    fs::remove_file(dir.join("segments.csv"))?;
                }
                if dir.join("concat.txt").is_file() {
                    fs::remove_file(dir.join("concat.txt"))?;
                }
                let _ = fs::remove_dir(&dir);
                outputs.push(destination.file_name().unwrap().to_string_lossy().into());
            }
        }
        i.ring = Ring::default();
        i.recording = None;
        i.status.recording = false;
        i.status.replay = false;
        i.status.buffer_seconds = 0.;
        i.status.buffer_bytes = 0;
        i.status.recovery_sessions = count_sessions(&i.data_dir);
        Ok(outputs)
    }
}
fn quarantine(path: &Path) -> Result<()> {
    let dir = path
        .parent()
        .context("Segment parent missing")?
        .join("quarantine");
    fs::create_dir_all(&dir)?;
    fs::rename(
        path,
        dir.join(path.file_name().context("Segment filename missing")?),
    )?;
    Ok(())
}
fn count_sessions(data: &Path) -> usize {
    ["buffer", "recordings", "saves"]
        .iter()
        .map(|cat| {
            fs::read_dir(data.join(cat))
                .map(|d| d.flatten().filter(|e| e.path().is_dir()).count())
                .unwrap_or(0)
        })
        .sum()
}
impl Inner {
    fn expected_audio(&self) -> usize {
        if let Some(p) = &self.pipeline {
            return p.expected_audio;
        }
        self.ring_audio_tracks
    }
    /// Legacy flat path, used for recovered sessions of unknown content.
    fn clip_path_flat(&self, prefix: &str) -> PathBuf {
        self.settings.output_dir.join(format!(
            "{prefix}-{}-{}.mkv",
            chrono::Local::now().format("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        ))
    }
    /// Path and metadata for a new clip covering [from, to]: the game is
    /// attributed over that interval; the name is collision-safe.
    fn clip_path(
        &mut self,
        kind: &str,
        from: Instant,
        to: Instant,
    ) -> (PathBuf, Vec<(String, String)>) {
        let game = self.activity.attribute(from, to);
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let (dir, label) = if self.settings.organize_by_game {
            let folder = crate::library::sanitize_component(&game);
            (self.settings.output_dir.join(&folder), folder)
        } else {
            (
                self.settings.output_dir.clone(),
                crate::library::sanitize_component(kind),
            )
        };
        let _ = fs::create_dir_all(&dir);
        self.reserved.retain(|p| !p.exists());
        let stem = format!("{label}_{stamp}");
        let mut path = crate::library::unique_path(&dir, &stem, "mkv");
        let mut n = 2;
        while self.reserved.contains(&path) {
            path = crate::library::unique_path(&dir, &format!("{stem}_{n}"), "mkv");
            n += 1;
        }
        self.reserved.insert(path.clone());
        let tags = vec![
            ("REPLAYX_GAME".to_string(), game),
            ("REPLAYX_KIND".to_string(), kind.to_string()),
            ("title".to_string(), format!("REPLAYX {kind}")),
        ];
        (path, tags)
    }
    fn sample_activity(&mut self) {
        let now = Instant::now();
        if self
            .last_activity_sample
            .is_some_and(|t| now - t < Duration::from_secs(1))
        {
            return;
        }
        self.last_activity_sample = Some(now);
        let what = crate::games::sample(&self.settings.game_mappings);
        if let crate::games::Foreground::Game(g) = &what {
            self.status.detected_game = Some(g.clone());
        } else if what == crate::games::Foreground::App {
            self.status.detected_game = None;
        }
        self.activity.record(now, what);
    }
    /// Pick the most efficient scaler/encoder chain whose real probe succeeds.
    /// Returns the scaler, encoder and a summary of rejected faster paths.
    fn select_video_path(
        &mut self,
        synthetic: bool,
        geometry: Option<&DisplayGeometry>,
        plan: Option<&geometry::VideoPlan>,
    ) -> Result<(Scaler, String, Option<String>)> {
        let (Some(g), Some(plan), false) = (geometry, plan, synthetic) else {
            let encoder = self.media.choose_encoder(&self.settings.encoder)?;
            return Ok((Scaler::Cpu, encoder, None));
        };
        let key = format!(
            "{}|{}|{:?}|{}|{}|{}|{}",
            g.adapter_luid,
            self.settings.display,
            plan,
            self.settings.encoder,
            self.settings.fps,
            self.settings.preset,
            self.settings.gpu_scaling
        );
        if let Some((cached, scaler, encoder)) = &self.video_path_cache {
            if *cached == key {
                return Ok((*scaler, encoder.clone(), self.status.scaling_detail.clone()));
            }
        }
        let mut rejected = Vec::new();
        let candidates = if self.settings.gpu_scaling {
            media::scaling_candidates(&self.settings.encoder)
        } else {
            rejected.push("GPU scaling disabled in settings".into());
            Vec::new()
        };
        for (scaler, encoder) in candidates {
            if !scaler.supports(plan) {
                continue;
            }
            let id = format!("{key}|{scaler:?}|{encoder}");
            if let Some((_, reason)) = self.denied_paths.iter().find(|(d, _)| *d == id) {
                rejected.push(format!(
                    "{} + {encoder}: failed during capture startup ({reason})",
                    scaler.label()
                ));
                continue;
            }
            match self
                .media
                .probe_video(&self.settings, scaler, encoder, Some(plan))
            {
                Ok(()) => {
                    let detail = (!rejected.is_empty()).then(|| rejected.join("; "));
                    self.video_path_cache = Some((key, scaler, encoder.into()));
                    return Ok((scaler, encoder.into(), detail));
                }
                Err(e) => rejected.push(format!("{} + {encoder}: {e}", scaler.label())),
            }
        }
        let encoder = self.media.choose_encoder(&self.settings.encoder)?;
        self.media
            .probe_video(&self.settings, Scaler::Cpu, &encoder, Some(plan))
            .map_err(|e| anyhow::anyhow!("Capture could not start with {encoder}: {e}"))?;
        let detail = (!rejected.is_empty()).then(|| rejected.join("; "));
        self.video_path_cache = Some((key, Scaler::Cpu, encoder.clone()));
        Ok((Scaler::Cpu, encoder, detail))
    }
    fn start_pipeline(&mut self, synthetic: bool) -> Result<()> {
        self.status.state = "Starting".into();
        self.settings.validate()?;
        health::storage_ready(
            fs2::available_space(&self.data_dir)?,
            fs2::available_space(&self.settings.output_dir)?,
            true,
        )?;
        self.media.preflight()?;
        let geometry = if !synthetic && self.settings.capture == "desktop-duplication" {
            let g = geometry::display(self.settings.display)?;
            g.region(self.settings.source_rect)?;
            Some(g)
        } else {
            None
        };
        let plan = match &geometry {
            Some(g) => Some(geometry::plan(
                g.region(self.settings.source_rect)?,
                self.settings.height,
                &self.settings.canvas_mode,
            )?),
            None => None,
        };
        let (scaler, encoder, scaling_detail) =
            self.select_video_path(synthetic, geometry.as_ref(), plan.as_ref())?;
        if scaler == Scaler::Cpu && plan.is_some() {
            self.status.notice = Some(
                "GPU scaling is unavailable on this display adapter; using the CPU fallback (see Performance for details).".into(),
            );
        }
        let dir = self
            .data_dir
            .join("buffer")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("session.json"),
            serde_json::to_vec_pretty(&self.settings)?,
        )?;
        let mut feeds = Vec::new();
        if !synthetic {
            for source in self.settings.audio.iter().filter(|s| s.enabled) {
                feeds.push(audio::spawn(source.clone())?);
            }
        }
        let mut c = media::command(&self.media.ffmpeg);
        c.current_dir(&dir).args([
            "-hide_banner",
            "-y",
            "-stats_period",
            "0.5",
            "-filter_complex_threads",
            "2",
            "-filter_buffered_frames",
            "64",
            "-progress",
            "pipe:1",
        ]);
        let filter = media::video_input(&mut c, &self.settings, scaler, synthetic, plan.as_ref())?;
        let source_count = if synthetic { 2 } else { feeds.len() };
        if synthetic {
            for frequency in [440, 880] {
                c.args(["-re","-f","lavfi","-i",&format!("sine=frequency={frequency}:sample_rate=48000,volume='if(lt(mod(t,1),0.1),1,0)':eval=frame")]);
            }
        } else {
            for f in &feeds {
                c.args([
                    // Let FFmpeg's demuxer thread keep draining PCM while the video
                    // graph and encoder initialise.
                    "-thread_queue_size",
                    "4096",
                    "-probesize",
                    "32",
                    "-analyzeduration",
                    "0",
                    "-f",
                    "f32le",
                    "-ar",
                    "48000",
                    "-ac",
                    "2",
                    "-i",
                    &f.url,
                ]);
            }
        }
        let mut graph = format!("[0:v]{filter}[v]");
        if source_count > 0 {
            for n in 0..source_count {
                let input = n + 1;
                graph += &format!(";[{input}:a]aresample=async=1000:first_pts=0");
                if self.settings.separate_tracks {
                    graph += &format!(",asplit=2[m{n}][a{n}]")
                } else {
                    graph += &format!("[m{n}]")
                }
            }
            graph += ';'.to_string().as_str();
            for n in 0..source_count {
                graph += &format!("[m{n}]");
            }
            graph += &format!("amix=inputs={source_count}:normalize=1[mix]");
        }
        c.args(["-filter_complex", &graph, "-map", "[v]"]);
        if source_count > 0 {
            c.args([
                "-map",
                "[mix]",
                "-metadata:s:a:0",
                "title=Playback mix",
                "-disposition:a:0",
                "default",
            ]);
            if self.settings.separate_tracks {
                for n in 0..source_count {
                    c.args([
                        "-map",
                        &format!("[a{n}]"),
                        &format!("-metadata:s:a:{}", n + 1),
                        &format!(
                            "title={}",
                            if synthetic {
                                format!("Tone {}", n + 1)
                            } else {
                                self.settings
                                    .audio
                                    .iter()
                                    .filter(|s| s.enabled)
                                    .nth(n)
                                    .unwrap()
                                    .name
                                    .clone()
                            }
                        ),
                        &format!("-disposition:a:{}", n + 1),
                        "0",
                    ]);
                }
            }
        }
        media::encoding_args(&mut c, &self.settings, &encoder);
        c.args([
            "-fps_mode",
            "cfr",
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
        let mut child = c
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start FFmpeg")?;
        let job = match crate::process_job::CaptureJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.context("Could not protect encoder process with a Windows job"));
            }
        };
        let progress = Arc::new(Progress::default());
        let p = progress.clone();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let started = Instant::now();
        let out_thread = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some((key, value)) = line.trim().split_once('=') {
                    match key {
                        "frame" => {
                            let frame = value.trim().parse().unwrap_or(0);
                            if frame > p.frames.load(Ordering::Relaxed) {
                                p.frames.store(frame, Ordering::Relaxed);
                                p.last_ms
                                    .store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
                            }
                        }
                        "drop_frames" => {
                            p.dropped
                                .store(value.parse().unwrap_or(0), Ordering::Relaxed);
                        }
                        "dup_frames" => {
                            p.duplicated
                                .store(value.parse().unwrap_or(0), Ordering::Relaxed);
                        }
                        "fps" => {
                            p.fps.store(
                                value.parse::<f64>().unwrap_or(0.).to_bits(),
                                Ordering::Relaxed,
                            );
                        }
                        _ => {}
                    }
                }
            }
        });
        let p = progress.clone();
        let err_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut lines = p.lines.lock().unwrap();
                if lines.len() == 30 {
                    lines.remove(0);
                }
                lines.push(line);
            }
        });
        self.status.encoder = encoder;
        self.status.capture = self.settings.capture.clone();
        self.status.frames = 0;
        self.status.elapsed = 0.;
        self.status.geometry = geometry.clone();
        self.status.scaling = if synthetic {
            "Synthetic test source".into()
        } else {
            scaler.label().into()
        };
        self.status.scaling_detail = scaling_detail;
        (self.status.output_width, self.status.output_height) =
            plan.map(|p| (p.width, p.height)).unwrap_or_default();
        self.status.buffer_bytes = 0;
        self.status.buffer_seconds = 0.;
        self.ring = Ring::default();
        let expected_audio = if source_count == 0 {
            0
        } else if self.settings.separate_tracks {
            source_count + 1
        } else {
            1
        };
        self.ring_audio_tracks = expected_audio;
        self.pipeline = Some(Pipeline {
            child,
            _job: job,
            discard_tail: false,
            feeds,
            dir,
            seen: 0,
            progress,
            started,
            expected_audio,
            readers: vec![out_thread, err_thread],
            geometry,
            last_geometry_check: Instant::now(),
            last_segment_ms: 0,
            validated: false,
            expected_size: plan.map(|p| (p.width, p.height)),
            scaler,
            synthetic,
        });
        Ok(())
    }
    fn collect_segments(&mut self) -> Result<()> {
        let Some(p) = &mut self.pipeline else {
            return Ok(());
        };
        if p.discard_tail {
            return Ok(());
        }
        let csv = fs::read_to_string(p.dir.join("segments.csv")).unwrap_or_default();
        // Ignore a concurrently written partial CSV line until its newline is visible.
        let lines: Vec<_> = csv
            .split_inclusive('\n')
            .filter(|s| s.ends_with('\n'))
            .map(str::trim)
            .collect();
        for line in lines.iter().skip(p.seen) {
            let pieces: Vec<_> = line.rsplitn(3, ',').collect();
            if pieces.len() != 3 {
                break;
            }
            let end = pieces[0].parse::<f64>()?;
            let start = pieces[1].parse::<f64>()?;
            if !start.is_finite() || !end.is_finite() || end <= start || start < 0. {
                bail!("Invalid segment timestamps")
            }
            let filename = pieces[2].trim_matches('"');
            if !filename.starts_with("segment-") || filename.contains(['\\', '/']) {
                bail!("Invalid segment manifest")
            };
            let path = p.dir.join(filename);
            let bytes = fs::metadata(&path)?.len();
            if bytes == 0 {
                bail!("Finalized segment is empty")
            }
            if !p.validated {
                let probe = self.media.probe(&path)?;
                if probe.audio_tracks != p.expected_audio {
                    bail!("Finalized segment is missing configured audio tracks")
                };
                // Every display-capture mode has an exact planned frame size; a mismatch
                // would mean stale geometry or a wrong scaler, e.g. desktop leakage.
                if let Some((w, h)) = p.expected_size {
                    if (probe.width, probe.height) != (w as u64, h as u64) {
                        bail!(
                            "Encoded dimensions {}x{} do not match the planned {w}x{h} frame",
                            probe.width,
                            probe.height
                        )
                    }
                }
                p.validated = true;
            }
            let segment = Segment {
                path,
                start,
                end,
                bytes,
            };
            if let Some(r) = &mut self.recording {
                if segment.end > r.after {
                    let destination = r.dir.join(format!("{:08}.mkv", r.segments.len()));
                    if fs::hard_link(&segment.path, &destination).is_err() {
                        fs::copy(&segment.path, &destination)?;
                    }
                    r.segments.push(Segment {
                        path: destination,
                        ..segment.clone()
                    });
                }
            }
            self.ring.add(segment);
            p.seen += 1;
            p.last_segment_ms = p.started.elapsed().as_millis() as u64;
        }
        self.ring.prune(
            self.settings.replay_seconds as f64,
            self.settings.buffer_mb * 1024 * 1024,
        )?;
        self.status.buffer_seconds = self.ring.duration();
        self.status.buffer_bytes = self.ring.total_bytes;
        Ok(())
    }
    fn stop_pipeline(&mut self) -> Result<()> {
        if let Some(p) = &mut self.pipeline {
            if let Some(stdin) = &mut p.child.stdin {
                let _ = stdin.write_all(b"q\n");
            }
            let deadline = Instant::now() + Duration::from_secs(8);
            while p.child.try_wait()?.is_none() {
                if Instant::now() > deadline {
                    p.child.kill()?;
                    p.child.wait()?;
                    self.status.notice=Some("Capture required forced shutdown; recover remaining segments before restarting".into());
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        let collected = self.collect_segments();
        if let Some(mut p) = self.pipeline.take() {
            p.feeds.clear();
            for t in p.readers.drain(..) {
                let _ = t.join();
            }
        }
        self.status.state = "Stopped".into();
        self.status.recovery_sessions = count_sessions(&self.data_dir);
        collected
    }
    fn fail(&mut self, error: String) {
        // A GPU path can pass its short probe yet fail in the full pipeline (for
        // example no usable keyframes). Before any valid footage exists, deny that
        // path for this configuration and restart on the next one.
        let startup_gpu_failure = self.recovery.is_none()
            && self
                .pipeline
                .as_ref()
                .is_some_and(|p| !p.validated && !p.synthetic && p.scaler != Scaler::Cpu)
            // Audio device/transport failures say nothing about the video path.
            && !error.starts_with("Audio");
        if startup_gpu_failure {
            if let Some((key, scaler, encoder)) = self.video_path_cache.take() {
                self.denied_paths
                    .push((format!("{key}|{scaler:?}|{encoder}"), error.clone()));
                let (replay, recording) = (self.status.replay, self.status.recording);
                let _ = self.stop_pipeline();
                match self.start_pipeline(false) {
                    Ok(()) => {
                        self.status.replay = replay;
                        self.status.recording = recording;
                        self.status.notice = Some(format!(
                            "{} + {encoder} failed during startup ({error}). Switched to {}.",
                            scaler.label(),
                            self.status.scaling
                        ));
                        return;
                    }
                    Err(e) => {
                        return self.fail_final(format!(
                            "{error}. Fallback capture path also failed: {e:#}"
                        ))
                    }
                }
            }
        }
        self.fail_final(error)
    }
    fn fail_final(&mut self, error: String) {
        self.recovery = None;
        let _ = self.stop_pipeline();
        self.status.replay = false;
        self.status.recording = false;
        self.status.state = "Error".into();
        self.status.error = Some(error);
    }
    fn tick(&mut self) -> Result<Option<SaveJob>> {
        self.status.free_bytes = fs2::available_space(&self.settings.output_dir).unwrap_or(0);
        if self.recovery.is_some() {
            self.retry_recovery()?;
            return Ok(None);
        }
        if self.pipeline.is_none() {
            if self.status.state != "Error" {
                self.status.state = if self.status.saving > 0 {
                    "Saving"
                } else {
                    "Stopped"
                }
                .into();
            }
            return Ok(None);
        }
        health::storage_ready(
            fs2::available_space(&self.data_dir)?,
            self.status.free_bytes,
            false,
        )?;
        if !self.pipeline.as_ref().is_some_and(|p| p.synthetic) {
            self.sample_activity();
        }
        let p = self.pipeline.as_mut().unwrap();
        if !p.synthetic
            && p.geometry.is_some()
            && p.last_geometry_check.elapsed() > Duration::from_millis(500)
        {
            p.last_geometry_check = Instant::now();
            let current = geometry::display(self.settings.display);
            if current.as_ref().ok() != p.geometry.as_ref() {
                return self.begin_recovery("Display geometry changed or the display disconnected; previous footage is preserved.");
            }
        }
        self.collect_segments()?;
        let p = self.pipeline.as_mut().unwrap();
        if let Some(exit) = p.child.try_wait()? {
            let reason = p
                .progress
                .lines
                .lock()
                .unwrap()
                .iter()
                .rev()
                .take(30)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if !p.synthetic
                && [
                    "access lost",
                    "Access lost",
                    "device removed",
                    "Device removed",
                    "DXGI_ERROR_ACCESS_LOST",
                ]
                .iter()
                .any(|s| reason.contains(s))
            {
                return self.begin_recovery(
                    "Desktop Duplication lost its device; attempting bounded recovery.",
                );
            }
            bail!("Capture exited ({exit}). {reason}")
        }
        for f in &p.feeds {
            if let Some(e) = f.meter.error.lock().unwrap().clone() {
                bail!("Audio capture stopped: {e}")
            }
            if f.microphone
                && f.started.elapsed().as_millis() as u64
                    > f.meter.last_packet_ms.load(Ordering::Relaxed) + 5000
            {
                bail!("Microphone packets stopped. Check Windows microphone permission and the selected device, then restart capture.")
            }
        }
        self.status.frames = p.progress.frames.load(Ordering::Relaxed);
        self.status.dropped_frames = p.progress.dropped.load(Ordering::Relaxed);
        self.status.duplicated_frames = p.progress.duplicated.load(Ordering::Relaxed);
        self.status.fps = f64::from_bits(p.progress.fps.load(Ordering::Relaxed));
        self.status.elapsed = p.started.elapsed().as_secs_f64();
        self.status.audio_peaks = p
            .feeds
            .iter()
            .map(|f| f32::from_bits(f.meter.peak.load(Ordering::Relaxed)))
            .collect();
        self.status.audio_packets = p
            .feeds
            .iter()
            .map(|f| f.meter.packets.load(Ordering::Relaxed))
            .collect();
        self.status.audio_gaps = p
            .feeds
            .iter()
            .map(|f| f.meter.gaps.load(Ordering::Relaxed))
            .collect();
        self.status.audio_discontinuities = p
            .feeds
            .iter()
            .map(|f| {
                f.meter.discontinuities.load(Ordering::Relaxed)
                    + f.meter.device_glitches.load(Ordering::Relaxed)
            })
            .collect();
        self.status.audio_drift_ppm = p
            .feeds
            .iter()
            .map(|f| f32::from_bits(f.meter.correction_ppm.load(Ordering::Relaxed)))
            .collect();
        self.status.audio_written_bytes = p
            .feeds
            .iter()
            .map(|f| f.meter.written_bytes.load(Ordering::Relaxed))
            .collect();
        self.status.write_mbps = if self.status.buffer_seconds > 0. {
            self.status.buffer_bytes as f64 / self.status.buffer_seconds / 1_000_000.
        } else {
            0.
        };
        let last = p.progress.last_ms.load(Ordering::Relaxed);
        let elapsed = p.started.elapsed().as_millis() as u64;
        self.status.frame_age_ms = elapsed.saturating_sub(last);
        self.status.segment_age_ms = elapsed.saturating_sub(p.last_segment_ms);
        let audio_writes: Vec<_> = p
            .feeds
            .iter()
            .map(|f| {
                elapsed.saturating_sub(
                    (f.started.elapsed().as_millis() as u64)
                        .saturating_sub(f.meter.last_write_ms.load(Ordering::Relaxed)),
                )
            })
            .collect();
        health::stream_health(elapsed, last, p.last_segment_ms, &audio_writes)?;
        self.status.state = if self.status.frames == 0 || !p.validated {
            "Starting"
        } else if self.status.recording {
            "Recording"
        } else if self.status.saving > 0 {
            "Saving"
        } else {
            "Buffering"
        }
        .into();
        Ok(None)
    }
    fn begin_recovery(&mut self, reason: &str) -> Result<Option<SaveJob>> {
        let display = self.pipeline.as_ref().and_then(|p| p.geometry.clone());
        let replay = self.status.replay;
        let recording = self.status.recording;
        let expected = self.expected_audio();
        // The in-flight and last completed segment may straddle the mode switch.
        // Preserve them under quarantine, excluded from ordinary save/recovery.
        let old_dir = self.pipeline.as_ref().map(|p| p.dir.clone());
        if let Some(p) = &mut self.pipeline {
            p.discard_tail = true;
        }
        self.stop_pipeline()?;
        if let Some(tail) = self.ring.segments.pop_back() {
            self.ring.total_bytes -= tail.bytes;
            quarantine(&tail.path)?;
        }
        if let Some(rec) = &mut self.recording {
            if let Some(tail) = rec.segments.pop() {
                quarantine(&tail.path)?;
            }
        }
        if let Some(dir) = old_dir {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "mkv")
                    && !self.ring.segments.iter().any(|s| s.path == path)
                {
                    quarantine(&path)?;
                }
            }
        }
        self.status.buffer_seconds = self.ring.duration();
        self.status.buffer_bytes = self.ring.total_bytes;
        let finished = self.recording.take().filter(|r| !r.segments.is_empty());
        let job = finished.map(|r| {
            let part = self.clip_path("Recording", r.started, Instant::now());
            SaveJob {
                media: self.media.clone(),
                segments: r.segments,
                path: part.0,
                expected_audio: expected,
                dir: r.dir,
                requested: Instant::now(),
                tags: part.1,
            }
        });
        if job.is_some() {
            self.status.saving += 1;
        }
        self.status.recording = false;
        self.status.replay = false;
        self.status.state = "Recovering".into();
        self.status.notice=Some(format!("{reason} Recording is split into verified parts across display changes; replay restarts with a fresh buffer."));
        self.recovery = Some(Recovery {
            replay,
            recording,
            display,
            attempts: 0,
            next: Instant::now() + Duration::from_secs(1),
            stable: None,
        });
        Ok(job)
    }
    fn retry_recovery(&mut self) -> Result<()> {
        let mut r = self.recovery.take().unwrap();
        if Instant::now() < r.next {
            self.recovery = Some(r);
            return Ok(());
        }
        if r.attempts >= 3 {
            bail!("Display recovery exhausted three attempts. Select the display/region and restart capture. Previous footage is preserved.")
        }
        r.attempts += 1;
        let current = geometry::display(self.settings.display);
        match current {
            Ok(g) => {
                if r.display.as_ref().is_some_and(|old| !old.same_display(&g)) {
                    bail!("Selected display identity changed. Capture stopped instead of recording a different display.")
                }
                g.region(self.settings.source_rect)?;
                if r.stable.as_ref() != Some(&g) {
                    r.stable = Some(g);
                    r.next = Instant::now() + Duration::from_secs(1);
                    self.recovery = Some(r);
                    return Ok(());
                }
                match self.start_pipeline(false) {
                    Ok(()) => {
                        self.status.error = None;
                        self.status.replay = r.replay;
                        if r.recording {
                            let dir = self
                                .data_dir
                                .join("recordings")
                                .join(uuid::Uuid::new_v4().to_string());
                            fs::create_dir_all(&dir)?;
                            self.recording = Some(Recording {
                                dir,
                                segments: Vec::new(),
                                after: 0.,
                                started: Instant::now(),
                            });
                            self.status.recording = true;
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        self.status.error = Some(format!("Capture restart failed: {e:#}"));
                    }
                }
            }
            Err(e) => {
                self.status.error = Some(format!("Waiting for selected display: {e:#}"));
            }
        }
        self.status.state = "Recovering".into();
        r.next = Instant::now() + Duration::from_secs(1);
        self.recovery = Some(r);
        Ok(())
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.stop_pipeline();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_part_cleanup_preserves_quarantined_media() {
        let root = tempfile::tempdir().unwrap();
        let keep = root.path().join("quarantine");
        fs::create_dir(&keep).unwrap();
        fs::write(keep.join("tail.mkv"), b"uncertain").unwrap();
        let path = root.path().join("00000000.mkv");
        fs::write(&path, b"verified").unwrap();
        fs::write(root.path().join("concat.txt"), []).unwrap();
        cleanup_saved_snapshot(
            root.path(),
            &[Segment {
                path: path.clone(),
                start: 0.,
                end: 2.,
                bytes: 8,
            }],
        )
        .unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(keep.join("tail.mkv")).unwrap(), b"uncertain");
    }
    fn isolated(data: PathBuf, settings: Settings) -> Result<Engine> {
        let media = Media {
            ffmpeg: data.join("missing-ffmpeg.exe"),
            ffprobe: data.join("missing-ffprobe.exe"),
        };
        Engine::with_media(data, settings, media)
    }
    #[test]
    fn missing_encoder_is_error_and_preserves_hotkey_warning() {
        let root = tempfile::tempdir().unwrap();
        let engine = isolated(
            root.path().join("data"),
            Settings {
                output_dir: root.path().join("clips"),
                ..Default::default()
            },
        )
        .unwrap();
        {
            let mut inner = engine.0.lock().unwrap();
            inner.media.ffmpeg = root.path().join("absent-runtime.exe");
            inner.status.hotkey_error = Some("Shortcut conflict".into());
        }
        assert!(engine.start_replay().is_err());
        let status = engine.status();
        assert_eq!(status.state, "Error");
        assert!(!status.replay);
        assert!(!status.recording);
        assert!(status.error.unwrap().contains("Cannot run"));
        assert!(status.hotkey_error.is_some());
    }
    #[test]
    fn instance_lock_rejects_second_engine() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let settings = Settings {
            output_dir: root.path().join("clips"),
            ..Default::default()
        };
        let _first = isolated(data.clone(), settings.clone()).unwrap();
        assert!(isolated(data, settings).is_err());
    }
    #[test]
    fn quarantine_is_excluded_from_top_level_recovery() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("segment-00000001.mkv");
        fs::write(&path, b"transition").unwrap();
        quarantine(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(
            fs::read(root.path().join("quarantine/segment-00000001.mkv")).unwrap(),
            b"transition"
        );
    }
}
