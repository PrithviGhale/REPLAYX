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
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// FFmpeg's CSV list is append-only for this segment configuration. Poll only
/// newly appended bytes, keeping replay maintenance independent of session age.
/// The caller commits complete rows after validation; unfinished rows are retried.
fn manifest_tail(path: &Path, offset: u64) -> Result<String> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && offset == 0 => {
            return Ok(String::new())
        }
        Err(e) => return Err(e).context("Cannot read replay segment manifest"),
    };
    let size = file.metadata()?.len();
    if size < offset {
        bail!("Replay segment manifest was truncated unexpectedly")
    }
    if size == offset {
        return Ok(String::new());
    }
    const MAX_PENDING_BYTES: u64 = 1024 * 1024;
    if size - offset > MAX_PENDING_BYTES {
        bail!("Replay segment manifest backlog exceeded its bound")
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut tail = String::new();
    file.take(MAX_PENDING_BYTES + 1).read_to_string(&mut tail)?;
    if tail.len() as u64 > MAX_PENDING_BYTES {
        bail!("Replay segment manifest backlog exceeded its bound")
    }
    Ok(tail)
}

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
    /// Display capture API in use: "dda" (Desktop Duplication) or "wgc"
    /// (Windows Graphics Capture); empty for other sources.
    pub capture_api: String,
    /// FFmpeg capture filter actually started: "gfxcapture" (WGC),
    /// "ddagrab" (Desktop Duplication), "gdigrab" or "synthetic".
    pub capture_filter: String,
    /// Set when WGC was selected but could not start, so Desktop
    /// Duplication is recording instead: the reason.
    pub capture_fallback: Option<String>,
    /// Why faster scaling paths were not used (probe errors), if any.
    pub scaling_detail: Option<String>,
    pub output_width: u32,
    pub output_height: u32,
    pub segment_age_ms: u64,
    pub frame_age_ms: u64,
    pub audio_written_bytes: Vec<u64>,
    pub audio_queue_ms: Vec<u64>,
    pub hotkey_error: Option<String>,
    /// A recording was requested and capture is still starting.
    pub pending_recording: bool,
    /// Length of the current recording (session time is `elapsed`).
    pub recording_elapsed: f64,
    /// Time from the start request to each startup stage of the last start.
    pub start_timings: Vec<media::StageTiming>,
    /// Encoder throughput in multiples of real time (1.0 = keeping up).
    pub encoder_speed: f64,
    /// How far encoded video trails the wall clock since the first frame.
    pub video_lag_ms: u64,
    /// Video bitrate the encoder targets (Mb/s); with constant quality, the
    /// typical average.
    pub target_bitrate_mbps: u32,
    /// Peak video bitrate (Mb/s).
    pub cap_bitrate_mbps: u32,
    /// NVENC constant-quality level, when used.
    pub constant_quality: Option<u32>,
    /// Codec actually being recorded ("h264", "hevc", "av1"); differs from
    /// the setting when the GPU could not encode the chosen codec.
    pub codec: String,
    /// Encoder in use, as a readable name (e.g. "h264_qsv (low-power VDEnc)").
    pub encoder_label: String,
    /// Game in front right now, also while not capturing (for the notice).
    pub foreground_game: Option<String>,
    /// Size players show when the stream is anamorphic (0 = the encoded size).
    pub play_width: u32,
    pub play_height: u32,
}
/// Snapshot readable without the engine lock, so no caller (in particular the
/// UI thread) ever waits for capture startup, saves or recovery.
#[derive(Default)]
pub struct Published {
    status: Mutex<Status>,
    settings: Mutex<Settings>,
}
/// A capture start in flight. The slow part (probes, device and encoder
/// start) runs without the engine lock; Stop only clears the intent here.
struct Starting {
    recording: bool,
    replay: bool,
    began: Instant,
}
/// What a start needs, copied out of the engine under the lock.
#[derive(Clone)]
struct PipelineRequest {
    settings: Settings,
    media: Media,
    data_dir: PathBuf,
    synthetic: bool,
    cache: Option<(String, Scaler, String)>,
    cached_detail: Option<String>,
    denied: Vec<(String, String)>,
    began: Instant,
}
/// A started encoder, ready to be installed into the engine.
struct Prepared {
    pipeline: Pipeline,
    encoder: String,
    scaling_detail: Option<String>,
    capture_api: &'static str,
    capture_filter: &'static str,
    capture_fallback: Option<String>,
    plan: Option<geometry::VideoPlan>,
    expected_audio: usize,
    cache: Option<(String, Scaler, String)>,
    timings: Vec<media::StageTiming>,
}
/// Encoded video may trail the wall clock this much before capture stops with
/// an overload error (a few seconds of lag are absorbed by FFmpeg's queues).
const VIDEO_LAG_LIMIT_MS: u64 = 8000;
fn overload_message(lag_ms: u64, speed: f64, fps: u32) -> String {
    format!(
        "The video encoder fell {:.1} s behind real time (running at {:.2}x), so capture stopped. \
The GPU could not keep up with {fps} fps alongside the game: choose 60 fps, a lower output \
resolution or the Performance preset in Video settings. Footage recorded so far is saved.",
        lag_ms as f64 / 1000.0,
        speed
    )
}
/// Longest a start may take before it is abandoned with an error.
const START_WATCHDOG: Duration = Duration::from_secs(60);
#[derive(Default)]
struct Progress {
    /// Encoder output time in microseconds (FFmpeg progress out_time_us).
    out_us: AtomicU64,
    /// Milliseconds from the start request to the first encoded frame (0 = none yet).
    first_frame_ms: AtomicU64,
    /// FFmpeg's `speed` (x real time, f64 bits).
    speed: AtomicU64,
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
    manifest_offset: u64,
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
    /// When the start was requested (for first-frame/first-segment timings).
    began: Instant,
    reported_first_frame: bool,
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
    /// Output time (s) at which Stop was pressed; footage after it is not added.
    stop_at: Option<f64>,
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
    /// Background-probed container start times, keyed by buffer file.
    segment_starts: Arc<Mutex<std::collections::HashMap<PathBuf, f64>>>,
    segment_probe_tx: std::sync::mpsc::Sender<PathBuf>,
    starting: Option<Starting>,
    /// Saves of recordings cut short by an error, started by the tick loop.
    orphaned: Vec<SaveJob>,
    _instance_lock: fs::File,
}
#[derive(Clone)]
pub struct Engine(pub Arc<Mutex<Inner>>, Arc<Published>);
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
        let segment_starts: Arc<Mutex<std::collections::HashMap<PathBuf, f64>>> =
            Default::default();
        let (segment_probe_tx, probe_rx) = std::sync::mpsc::channel::<PathBuf>();
        {
            let (starts, media) = (segment_starts.clone(), media.clone());
            thread::spawn(move || {
                for path in probe_rx {
                    if let Ok(info) = media.segment_info_low_priority(&path) {
                        let mut map = starts.lock().unwrap();
                        map.insert(path, info.start_time);
                        if map.len() > 4096 {
                            map.retain(|p, _| p.exists());
                        }
                    }
                }
            });
        }
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
        remove_empty_sessions(&data_dir);
        let recovery_sessions = count_sessions(&data_dir, &[]);
        let published = Arc::new(Published {
            status: Mutex::new(Status {
                state: "Stopped".into(),
                recovery_sessions,
                ..Default::default()
            }),
            settings: Mutex::new(settings.clone()),
        });
        let e = Self(
            Arc::new(Mutex::new(Inner {
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
                segment_starts: segment_starts.clone(),
                segment_probe_tx,
                starting: None,
                orphaned: Vec::new(),
                _instance_lock: instance_lock,
            })),
            published.clone(),
        );
        let weak = Arc::downgrade(&e.0);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(200));
            let Some(arc) = weak.upgrade() else { break };
            // Sample the foreground window without holding the engine lock: the
            // query may involve another thread, which must never wait on us.
            let mappings = arc.lock().unwrap().activity_due();
            let sample = mappings.map(|m| (Instant::now(), crate::games::sample(&m)));
            let mut i = arc.lock().unwrap();
            if let Some((at, what)) = sample {
                i.record_activity(at, what);
            }
            match i.tick() {
                Ok(Some(job)) => {
                    let engine = Engine(arc.clone(), published.clone());
                    thread::spawn(move || {
                        let result = job.run();
                        engine.complete_save(&result);
                    });
                }
                Ok(None) => {}
                Err(e) => i.fail(format!("{e:#}")),
            }
            for job in std::mem::take(&mut i.orphaned) {
                let engine = Engine(arc.clone(), published.clone());
                thread::spawn(move || {
                    let result = job.run();
                    engine.complete_save(&result);
                });
            }
            *published.status.lock().unwrap() = i.status.clone();
        });
        Ok(e)
    }
    /// Current status. Never waits: if the engine is busy (starting capture,
    /// stopping the encoder, recovering) the last published snapshot is
    /// returned, at most one tick (200 ms) old.
    pub fn status(&self) -> Status {
        match self.0.try_lock() {
            Ok(i) => {
                let s = i.status.clone();
                *self.1.status.lock().unwrap() = s.clone();
                s
            }
            Err(std::sync::TryLockError::WouldBlock) => self.1.status.lock().unwrap().clone(),
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner().status.clone(),
        }
    }
    /// Current settings. Never waits for the engine lock.
    pub fn settings(&self) -> Settings {
        self.1.settings.lock().unwrap().clone()
    }
    fn publish(&self, i: &Inner) {
        *self.1.status.lock().unwrap() = i.status.clone();
        *self.1.settings.lock().unwrap() = i.settings.clone();
    }
    pub fn set_settings(&self, s: Settings) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        if i.pipeline.is_some()
            || i.recovery.is_some()
            || i.status.saving > 0
            || i.starting.is_some()
        {
            bail!("Stop capture and wait for saves before changing settings")
        };
        fs::create_dir_all(&s.output_dir)?;
        s.save(&i.data_dir.join("settings.json"))?;
        i.settings = s;
        self.publish(&i);
        Ok(())
    }
    /// Remember that the close-to-tray explanation was shown (saved at once;
    /// works during capture, unlike `set_settings`).
    pub fn mark_tray_hint_shown(&self) {
        let mut i = self.0.lock().unwrap();
        if i.settings.tray_hint_shown {
            return;
        }
        i.settings.tray_hint_shown = true;
        let _ = i.settings.save(&i.data_dir.join("settings.json"));
        self.publish(&i);
    }
    /// Remember that the first-run tour was finished or skipped (saved at
    /// once, also during capture).
    pub fn mark_tour_done(&self) {
        let mut i = self.0.lock().unwrap();
        if i.settings.tour_done {
            return;
        }
        i.settings.tour_done = true;
        let _ = i.settings.save(&i.data_dir.join("settings.json"));
        self.publish(&i);
    }
    /// Change playtime settings at once (also during capture: they don't
    /// touch the recording pipeline).
    pub fn set_playtime(&self, p: crate::playtime::PlaytimeSettings) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        let mut next = i.settings.clone();
        next.playtime = p;
        next.validate()?;
        next.save(&i.data_dir.join("settings.json"))?;
        i.settings = next;
        self.publish(&i);
        Ok(())
    }
    /// Update preferences apply at once, also during capture.
    pub fn set_updates(&self, u: crate::updates::UpdateSettings) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        let mut next = i.settings.clone();
        next.updates = u.normalized();
        next.save(&i.data_dir.join("settings.json"))?;
        i.settings = next;
        self.publish(&i);
        Ok(())
    }
    /// One switch for "is my microphone heard in recordings". Off mutes it
    /// (silence on its track, the timeline and other sources untouched), also
    /// in the middle of a recording; On unmutes it. A microphone that was
    /// taken out of the capture entirely (Audio page) is added back when
    /// capture is stopped; a running capture can't gain a track.
    /// Device, volume and Voice Enhancer settings are never changed.
    pub fn set_microphone(&self, on: bool) -> Result<Settings> {
        let mut i = self.0.lock().unwrap();
        if i.starting.is_some() || i.recovery.is_some() {
            bail!("Capture is starting or reconnecting. Retry the microphone switch when it is ready.")
        }
        let capturing = i.pipeline.is_some() || i.starting.is_some() || i.recovery.is_some();
        let mut next = i.settings.clone();
        let mics: Vec<usize> = next
            .audio
            .iter()
            .enumerate()
            .filter(|(_, a)| a.kind == "microphone")
            .map(|(n, _)| n)
            .collect();
        if mics.is_empty() {
            bail!("No microphone is set up. Add one on the Audio page.")
        }
        for &n in &mics {
            let a = &mut next.audio[n];
            if on && !a.enabled {
                if capturing {
                    bail!("The microphone isn't part of the capture that is running. Stop Instant Replay and recording, then turn it on.")
                }
                a.enabled = true;
            }
            a.muted = !on;
        }
        next.validate()?;
        next.save(&i.data_dir.join("settings.json"))?;
        i.settings = next.clone();
        // Apply to the running capture: the feed multiplies by zero when muted.
        if let Some(p) = &i.pipeline {
            let mut feed = 0;
            for a in &i.settings.audio {
                if !a.enabled {
                    continue;
                }
                if a.kind == "microphone" {
                    if let Some(f) = p.feeds.get(feed) {
                        f.meter.muted.store(a.muted, Ordering::Relaxed);
                    }
                }
                feed += 1;
            }
        }
        self.publish(&i);
        Ok(next)
    }
    /// Mutate engine state with the lock held, then publish the result.
    pub fn update(&self, f: impl FnOnce(&mut Inner)) {
        let mut i = self.0.lock().unwrap();
        f(&mut i);
        self.publish(&i);
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
    /// Start replay or recording. If capture already runs (replay on), a
    /// recording attaches to the running encoder: no restart, no gap. Otherwise
    /// the encoder is started *without* holding the engine lock, so status,
    /// Stop and every other command stay responsive during startup.
    fn start(&self, recording: bool, synthetic: bool) -> Result<()> {
        let began = Instant::now();
        let request = {
            let mut i = self.0.lock().unwrap();
            if recording
                && (i.recording.is_some() || i.starting.as_ref().is_some_and(|s| s.recording))
            {
                bail!("Recording is already active")
            }
            if i.recovery.is_some() {
                bail!("Capture is recovering. Stop all capture to cancel recovery first.")
            }
            if i.recording.is_some() && !i.status.recording {
                bail!("Recover the interrupted recording before starting a new session")
            }
            i.status.error = None;
            if i.starting.is_some() {
                // Capture is already starting (e.g. Record pressed while replay
                // starts): join that start instead of launching a second encoder.
                i.set_intent(recording);
                self.publish(&i);
                return Ok(());
            }
            if i.pipeline.is_some() {
                let result = i.attach(recording);
                self.publish(&i);
                return result;
            }
            i.starting = Some(Starting {
                recording: false,
                replay: false,
                began,
            });
            i.set_intent(recording);
            i.status.state = "Starting".into();
            i.status.start_timings = Vec::new();
            match i.pipeline_request(synthetic, began) {
                Ok(r) => {
                    self.publish(&i);
                    r
                }
                Err(e) => {
                    i.starting = None;
                    i.status.pending_recording = false;
                    i.fail(format!("{e:#}"));
                    self.publish(&i);
                    return Err(e);
                }
            }
        };
        let prepared = prepare_pipeline(request);
        let mut i = self.0.lock().unwrap();
        let intent = i.starting.take();
        i.status.pending_recording = false;
        let result = match (intent, prepared) {
            (None, Ok(p)) => {
                // The watchdog gave up on this start; nothing was recorded yet.
                abandon(p);
                Err(anyhow::anyhow!("Capture start was abandoned"))
            }
            (None, Err(e)) => Err(e),
            (Some(_), Err(e)) => {
                i.fail(format!("{e:#}"));
                Err(e)
            }
            (Some(intent), Ok(p)) if !intent.recording && !intent.replay => {
                // Stopped while starting: discard the encoder, keep nothing.
                abandon(p);
                i.status.replay = false;
                i.status.state = "Stopped".into();
                Ok(())
            }
            (Some(intent), Ok(p)) => {
                i.install_pipeline(p);
                i.status.replay = intent.replay;
                if intent.recording {
                    i.attach(true)
                } else {
                    Ok(())
                }
            }
        };
        i.refresh_session_count();
        self.publish(&i);
        result
    }
    /// Cancel a recording that was requested but has not started yet.
    /// Returns true if there was one.
    pub fn cancel_pending_recording(&self) -> bool {
        let mut i = self.0.lock().unwrap();
        let cancelled = i.recording.is_none()
            && i.starting.as_mut().is_some_and(|s| {
                let had = s.recording;
                s.recording = false;
                had
            });
        if cancelled {
            i.status.pending_recording = false;
            self.publish(&i);
        }
        cancelled
    }
    pub fn stop_replay(&self) -> Result<()> {
        let mut i = self.0.lock().unwrap();
        if let Some(s) = &mut i.starting {
            s.replay = false;
        }
        if let Some(recovery) = &mut i.recovery {
            recovery.replay = false;
            if !recovery.recording {
                i.recovery = None;
                i.status.state = "Stopped".into();
            }
        }
        i.status.replay = false;
        if !i.status.recording && i.pipeline.is_some() {
            let result = i.stop_clean();
            self.publish(&i);
            result?;
        }
        self.publish(&i);
        Ok(())
    }
    /// Stop recording without stopping capture: the recording ends with the
    /// segment containing the moment Stop was pressed, which the running
    /// encoder finalizes within about two seconds. Replay keeps buffering and
    /// nothing is restarted. Falls back to finalizing by stopping the encoder.
    pub fn stop_recording(&self) -> Result<SaveJob> {
        let requested = Instant::now();
        let stop_at = {
            let mut i = self.0.lock().unwrap();
            if i.recording.is_none() {
                bail!("No active recording")
            }
            let out = i
                .pipeline
                .as_ref()
                .map(|p| p.progress.out_us.load(Ordering::Relaxed) as f64 / 1e6)
                .filter(|t| *t > 0.);
            if let (Some(rec), Some(t)) = (i.recording.as_mut(), out) {
                rec.stop_at = Some(t);
            }
            i.status.recording = false;
            i.status.saving += 1;
            self.publish(&i);
            out
        };
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut finalized = false;
        while let Some(stop) = stop_at {
            {
                let i = self.0.lock().unwrap();
                if i.pipeline.is_none() || i.recording.is_none() {
                    break;
                }
                if i.recording
                    .as_ref()
                    .is_some_and(|r| r.segments.last().is_some_and(|s| s.end >= stop))
                    || i.ring.segments.back().is_some_and(|s| s.end >= stop)
                {
                    finalized = true;
                    break;
                }
            }
            if Instant::now() > deadline {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let mut i = self.0.lock().unwrap();
        if !finalized && i.recording.is_some() && i.pipeline.is_some() {
            // Fallback: finalize the in-flight segment by stopping the encoder.
            let restart = i.status.replay;
            i.stop_clean()?;
            if restart {
                if let Err(e) = i.start_pipeline(false) {
                    i.fail(format!(
                        "Recording finalized, but replay restart failed: {e}"
                    ));
                }
            }
        }
        let Some(mut rec) = i.recording.take() else {
            i.status.saving = i.status.saving.saturating_sub(1);
            bail!("The recording ended unexpectedly; recover its segments from Instant Replay")
        };
        if rec.segments.is_empty() {
            i.status.saving = i.status.saving.saturating_sub(1);
            bail!("The recording has no finished footage yet. Record for at least two seconds.")
        }
        let expected = i.expected_audio();
        i.fill_start_times(&mut rec.segments);
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
        if !i.status.replay && i.pipeline.is_some() {
            // Recording only: capture is no longer needed. The recording holds
            // its own links to every segment, so the replay buffer can go.
            i.stop_clean()?;
        }
        self.publish(&i);
        Ok(job)
    }
    pub fn save_replay(&self) -> Result<SaveJob> {
        let requested = Instant::now();
        let mut i = self.0.lock().unwrap();
        if i.pipeline.is_none() && i.ring.segments.is_empty() {
            bail!("Instant Replay is off. Turn it on, then save after something happens.")
        }
        i.collect_segments()?;
        let dir = i
            .data_dir
            .join("saves")
            .join(uuid::Uuid::new_v4().to_string());
        let mut segments = i.ring.snapshot(i.settings.replay_seconds as f64, &dir)?;
        i.fill_start_times(&mut segments);
        if segments.is_empty() {
            let _ = fs::remove_dir(&dir);
            bail!("No finalized footage yet. Wait at least two seconds.")
        }
        i.status.saving += 1;
        self.publish(&i);
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
        i.refresh_session_count();
        self.publish(&i);
    }
    pub fn shutdown(&self) -> Result<()> {
        let recording = {
            let mut i = self.0.lock().unwrap();
            i.recovery = None;
            // A start still in flight is discarded when it completes.
            if let Some(s) = &mut i.starting {
                s.recording = false;
                s.replay = false;
            }
            i.status.pending_recording = false;
            i.status.recording
        };
        if recording {
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
        *self.1.settings.lock().unwrap() = i.settings.clone();
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
        if i.pipeline.is_some()
            || i.recovery.is_some()
            || i.status.saving > 0
            || i.starting.is_some()
        {
            bail!("Stop capture before recovery")
        }
        i.status.state = "Recovering sessions".into();
        self.publish(&i);
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
                let mut unreadable = Vec::new();
                let mut end = 0.;
                let mut tracks = 0;
                for path in files {
                    let probed = match fs::metadata(&path) {
                        Ok(m) if m.len() > 0 => i.media.probe(&path).ok(),
                        _ => None,
                    };
                    if probed.is_none() {
                        unreadable.push(path.clone());
                    }
                    if let Some(probe) = probed {
                        tracks = probe.audio_tracks;
                        let start = end;
                        end += probe.duration;
                        segments.push(Segment {
                            path,
                            start,
                            end,
                            bytes: probe.bytes,
                            origin: None,
                            start_time: Some(probe.start_time),
                        });
                    }
                }
                // Torn or empty files (a segment cut off by a crash) are kept for
                // inspection, moved aside so the session is not offered again.
                for path in &unreadable {
                    let _ = quarantine(path);
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
        i.status.state = "Stopped".into();
        i.refresh_session_count();
        self.publish(&i);
        Ok(outputs)
    }
    /// Move interrupted sessions the user chose not to recover to the Recycle
    /// Bin (restorable), instead of deleting them. Returns how many were moved.
    pub fn discard_sessions(&self) -> Result<usize> {
        let mut i = self.0.lock().unwrap();
        if i.pipeline.is_some()
            || i.recovery.is_some()
            || i.status.saving > 0
            || i.starting.is_some()
        {
            bail!("Stop capture before discarding sessions")
        }
        let mut moved = 0;
        for dir in recoverable_sessions(&i.data_dir, &[]) {
            crate::winutil::recycle(&dir)
                .with_context(|| format!("Could not move {} to the Recycle Bin", dir.display()))?;
            moved += 1;
        }
        i.ring = Ring::default();
        i.status.buffer_seconds = 0.;
        i.status.buffer_bytes = 0;
        i.refresh_session_count();
        self.publish(&i);
        Ok(moved)
    }
}
fn mark(timings: &mut Vec<media::StageTiming>, began: Instant, stage: impl Into<String>) {
    timings.push(media::StageTiming {
        stage: stage.into(),
        elapsed_ms: began.elapsed().as_millis() as u64,
    });
}
/// Check the media runtime once per process instead of on every start (it
/// costs two process launches).
fn preflight_once(media: &Media) -> Result<()> {
    static PASSED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    if PASSED.lock().unwrap().contains(&media.ffmpeg) {
        return Ok(());
    }
    media.preflight()?;
    PASSED.lock().unwrap().push(media.ffmpeg.clone());
    Ok(())
}
/// The GPU capture/encoder chain that last passed its probe for this display
/// and these settings, kept across launches so a start does not re-run the
/// probes (about a second per rejected candidate). If it fails during a real
/// start, the existing startup fallback denies it, forgets it and probes again.
#[derive(Serialize, serde::Deserialize)]
struct RememberedPath {
    key: String,
    scaler: Scaler,
    encoder: String,
    detail: Option<String>,
}
const REMEMBERED_PATH: &str = "video-path.json";
fn remembered_key(
    req: &PipelineRequest,
    g: &DisplayGeometry,
    plan: &geometry::VideoPlan,
) -> String {
    // The adapter LUID changes on every boot, so it is not part of this key.
    let s = &req.settings;
    format!(
        "v6|{:04x}|{}|{}x{}@{}|{}|{:?}|{}|{}|{}|{}|{}|{}|{}",
        g.adapter_vendor,
        g.device,
        g.desktop_width,
        g.desktop_height,
        g.refresh_hz,
        s.display,
        plan,
        s.encoder,
        s.codec,
        s.fps,
        s.preset,
        s.gpu_scaling,
        media::capture_api(s),
        req.media.ffmpeg.display()
    )
}
/// Remembered paths, one per display mode and settings (newest first). A hybrid
/// laptop alternates between modes (desktop on the Intel GPU, full-screen game
/// on the NVIDIA GPU), and each must start without probing.
#[derive(Serialize, serde::Deserialize, Default)]
struct RememberedPaths {
    paths: Vec<RememberedPath>,
}
const REMEMBERED_MAX: usize = 8;
fn remembered_paths(data_dir: &Path) -> RememberedPaths {
    fs::read(data_dir.join(REMEMBERED_PATH))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn write_remembered(data_dir: &Path, all: &RememberedPaths) {
    let file = data_dir.join(REMEMBERED_PATH);
    let tmp = data_dir.join("video-path.json.tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(all) {
        if fs::write(&tmp, bytes).is_ok() {
            let _ = fs::rename(&tmp, &file);
        }
    }
}
fn remember_video_path(data_dir: &Path, path: RememberedPath) {
    let mut all = remembered_paths(data_dir);
    all.paths.retain(|p| p.key != path.key);
    all.paths.insert(0, path);
    all.paths.truncate(REMEMBERED_MAX);
    write_remembered(data_dir, &all);
}
/// Forget one remembered path (it failed during a real start).
fn forget_video_path(data_dir: &Path, scaler: Scaler, encoder: &str) {
    let mut all = remembered_paths(data_dir);
    all.paths
        .retain(|p| !(p.scaler == scaler && p.encoder == encoder));
    write_remembered(data_dir, &all);
}
struct VideoPath {
    scaler: Scaler,
    encoder: String,
    detail: Option<String>,
    /// New in-memory cache entry, if this selection was not already cached.
    cache: Option<(String, Scaler, String)>,
    how: &'static str,
}
/// Pick the most efficient scaler/encoder chain whose real probe succeeds.
fn select_video_path(
    req: &PipelineRequest,
    geometry: Option<&DisplayGeometry>,
    plan: Option<&geometry::VideoPlan>,
) -> Result<VideoPath> {
    let s = &req.settings;
    let (Some(g), Some(plan), false) = (geometry, plan, req.synthetic) else {
        let key = format!("no-geometry|{}|{}|{}", s.encoder, s.codec, req.synthetic);
        if let Some((cached, scaler, encoder)) = &req.cache {
            if *cached == key {
                return Ok(VideoPath {
                    scaler: *scaler,
                    encoder: encoder.clone(),
                    detail: None,
                    cache: None,
                    how: "cached",
                });
            }
        }
        let encoder = req
            .media
            .choose_encoder(&s.encoder, media::requested_codec(s))?;
        return Ok(VideoPath {
            scaler: Scaler::Cpu,
            encoder: encoder.clone(),
            detail: None,
            cache: Some((key, Scaler::Cpu, encoder)),
            how: "probed",
        });
    };
    let key = format!(
        "{}|{}|{:?}|{}|{}|{}|{}|{}|{}",
        g.adapter_luid,
        s.display,
        plan,
        s.encoder,
        s.codec,
        s.fps,
        s.preset,
        s.gpu_scaling,
        media::capture_api(s)
    );
    if let Some((cached, scaler, encoder)) = &req.cache {
        if *cached == key {
            return Ok(VideoPath {
                scaler: *scaler,
                encoder: encoder.clone(),
                detail: req.cached_detail.clone(),
                cache: None,
                how: "cached",
            });
        }
    }
    let denied = |scaler: Scaler, encoder: &str| {
        let id = format!("{key}|{scaler:?}|{encoder}");
        req.denied
            .iter()
            .find(|(d, _)| *d == id)
            .map(|(_, r)| r.clone())
    };
    let stable = remembered_key(req, g, plan);
    let remembered = remembered_paths(&req.data_dir)
        .paths
        .into_iter()
        .find(|r| r.key == stable);
    if let Some(r) = remembered {
        if r.key == stable
            && s.gpu_scaling
            && r.scaler != Scaler::Cpu
            && r.scaler.supports(plan)
            && denied(r.scaler, &r.encoder).is_none()
        {
            return Ok(VideoPath {
                scaler: r.scaler,
                cache: Some((key, r.scaler, r.encoder.clone())),
                encoder: r.encoder,
                detail: r.detail,
                how: "remembered",
            });
        }
    }
    let mut rejected = Vec::new();
    let candidates = if s.gpu_scaling {
        media::codec_candidates(
            &media::scaling_candidates(&s.encoder, g.adapter_vendor),
            media::requested_codec(s),
        )
    } else {
        rejected.push("GPU scaling disabled in settings".into());
        Vec::new()
    };
    for (scaler, encoder) in candidates {
        let encoder = encoder.as_str();
        // WgcGpu is a Windows Graphics Capture path only.
        if !scaler.supports(plan)
            || (scaler == Scaler::WgcGpu
                && (media::capture_api(s) != "wgc" || !media::tuning().wgc_gpu))
        {
            continue;
        }
        if let Some(reason) = denied(scaler, encoder) {
            rejected.push(format!(
                "{} + {encoder}: failed during capture startup ({reason})",
                scaler.label()
            ));
            continue;
        }
        match req.media.probe_video(s, scaler, encoder, Some(plan)) {
            Ok(()) => {
                let detail = (!rejected.is_empty()).then(|| rejected.join("; "));
                // Only GPU paths are remembered: a CPU fallback may be caused by
                // something temporary and must be re-checked on the next start.
                remember_video_path(
                    &req.data_dir,
                    RememberedPath {
                        key: stable,
                        scaler,
                        encoder: encoder.into(),
                        detail: detail.clone(),
                    },
                );
                return Ok(VideoPath {
                    scaler,
                    encoder: encoder.into(),
                    detail,
                    cache: Some((key, scaler, encoder.into())),
                    how: "probed",
                });
            }
            Err(e) => rejected.push(format!("{} + {encoder}: {e}", scaler.label())),
        }
    }
    let encoder = req
        .media
        .choose_encoder(&s.encoder, media::requested_codec(s))?;
    req.media
        .probe_video(s, Scaler::Cpu, &encoder, Some(plan))
        .map_err(|e| anyhow::anyhow!("Capture could not start with {encoder}: {e}"))?;
    let detail = (!rejected.is_empty()).then(|| rejected.join("; "));
    Ok(VideoPath {
        scaler: Scaler::Cpu,
        encoder: encoder.clone(),
        detail,
        cache: Some((key, Scaler::Cpu, encoder)),
        how: "probed",
    })
}
/// Everything slow about starting capture: runtime check, display query,
/// capture-path probes, audio devices and the encoder process. Runs without
/// the engine lock. Each stage is timed from the start request.
fn prepare_pipeline(req: PipelineRequest) -> Result<Prepared> {
    // Experimental Windows Graphics Capture: when none of its capture paths
    // starts, record with Desktop Duplication instead and say why.
    let fallback = (!req.synthetic
        && req.settings.capture == "desktop-duplication"
        && media::capture_api(&req.settings) == "wgc")
        .then(|| {
            let mut r = req.clone();
            r.settings.capture_api = "dda".into();
            r
        });
    let began = req.began;
    let s = &req.settings;
    let synthetic = req.synthetic;
    let mut timings = Vec::new();
    preflight_once(&req.media)?;
    mark(&mut timings, began, "Media runtime checked");
    let geometry = if !synthetic && s.capture == "desktop-duplication" {
        let g = geometry::display(s.display)?;
        g.region(s.source_rect)?;
        Some(g)
    } else {
        None
    };
    let plan = match &geometry {
        Some(g) => Some(geometry::plan(
            g.region(s.source_rect)?,
            s.output_width(),
            s.height,
            &s.canvas_mode,
        )?),
        None => None,
    };
    // Displays driven by the NVIDIA (or AMD) GPU have no GPU scaler for Desktop
    // Duplication frames in the bundled FFmpeg (scale_d3d11 cannot create its
    // output texture; D3D11 frames cannot be mapped to Vulkan or D3D12). A CPU
    // round trip repeated 13-86% of frames under a game load in measurements,
    // the zero-copy path 0-0.14%. "smooth" therefore records the display's
    // pixels and marks the stream to play at the chosen shape.
    let anamorphic = match (&geometry, plan) {
        (Some(g), Some(p))
            if s.gpu_scaling
                && s.scale_policy == "smooth"
                && [media::VENDOR_NVIDIA, media::VENDOR_AMD].contains(&g.adapter_vendor)
                && p.needs_scaling()
                && !p.padded() =>
        {
            Some(p)
        }
        _ => None,
    };
    let plan = anamorphic.map(|p| p.anamorphic()).or(plan);
    mark(&mut timings, began, "Display measured");
    let (path, req, capture_fallback) = match (
        select_video_path(&req, geometry.as_ref(), plan.as_ref()),
        &fallback,
    ) {
        (Ok(path), _) => (path, &req, None),
        (Err(e), Some(dda)) => {
            let mut path = select_video_path(dda, geometry.as_ref(), plan.as_ref())?;
            let note = format!(
                "Windows Graphics Capture could not start ({e}); using Desktop Duplication"
            );
            path.detail = Some(match path.detail {
                Some(d) => format!("{note}; {d}"),
                None => note.clone(),
            });
            (path, dda, Some(note))
        }
        (Err(e), None) => return Err(e),
    };
    let s = &req.settings;
    let capture_api = media::capture_api(s);
    let (scaler, encoder) = (path.scaler, path.encoder.clone());
    mark(
        &mut timings,
        began,
        format!("Video path {} ({} + {encoder})", path.how, scaler.label()),
    );
    let dir = req
        .data_dir
        .join("buffer")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&dir)?;
    let started = (|| -> Result<_> {
        fs::write(dir.join("session.json"), serde_json::to_vec_pretty(s)?)?;
        let mut feeds = Vec::new();
        if !synthetic {
            for source in s.audio.iter().filter(|s| s.enabled) {
                feeds.push(audio::spawn(source.clone())?);
            }
        }
        mark(&mut timings, began, "Audio sources opened");
        let mut c = media::command(&req.media.ffmpeg);
        c.current_dir(&dir).args([
            "-hide_banner",
            "-y",
            // 0.5 s as in 0.2.1-alpha.1 (0.25 s was tried for finer start timing;
            // kept at the measured value so the encoder loop is unchanged).
            "-stats_period",
            "0.5",
            "-filter_complex_threads",
            "2",
            "-filter_buffered_frames",
            "64",
            "-progress",
            "pipe:1",
        ]);
        let filter = media::video_input(&mut c, s, scaler, synthetic, plan.as_ref())?;
        let source_count = if synthetic { 2 } else { feeds.len() };
        if synthetic {
            for frequency in [440, 880] {
                c.args(["-re","-f","lavfi","-i",&format!("sine=frequency={frequency}:sample_rate=48000,volume='if(lt(mod(t,1),0.1),1,0)':eval=frame")]);
            }
        } else {
            for f in &feeds {
                if media::tuning().audio_wallclock {
                    c.args(["-use_wallclock_as_timestamps", "1"]);
                }
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
        let chains: Vec<String> = if synthetic {
            vec![String::new(); source_count]
        } else {
            s.audio
                .iter()
                .filter(|a| a.enabled)
                .map(media::source_chain)
                .collect()
        };
        for graph in media::filter_graphs(
            &filter,
            &chains,
            s.separate_tracks,
            media::tuning().split_graphs,
        ) {
            c.args(["-filter_complex", &graph]);
        }
        c.args(["-map", "[v]"]);
        if source_count > 0 {
            c.args([
                "-map",
                "[mix]",
                "-metadata:s:a:0",
                "title=Playback mix",
                "-disposition:a:0",
                "default",
            ]);
            if s.separate_tracks {
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
                                s.audio
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
        let size = plan
            .map(|p| (p.width, p.height))
            .unwrap_or((s.output_width(), s.height));
        media::encoding_args(&mut c, s, &encoder, size, media::converts_bt709(scaler));
        c.args(["-fps_mode", "cfr", "-max_muxing_queue_size", "512"]);
        if let Some(d) = media::tuning().interleave_delta {
            c.args(["-max_interleave_delta", &d.to_string()]);
        }
        c.args([
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
        let cpu_raised =
            crate::media::tuning().cpu_priority && crate::process_job::raise_cpu_priority(&child);
        mark(
            &mut timings,
            began,
            if cpu_raised {
                "Encoder process started (CPU priority above normal)"
            } else {
                "Encoder process started"
            },
        );
        Ok((child, job, feeds, source_count))
    })();
    let (mut child, job, feeds, source_count) = match started {
        Ok(v) => v,
        Err(e) => {
            discard_buffer_dir(&dir, &[]);
            return Err(e);
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
                            let _ = p.first_frame_ms.compare_exchange(
                                0,
                                (began.elapsed().as_millis() as u64).max(1),
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            );
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
                    "out_time_us" => {
                        if let Ok(us) = value.trim().parse::<u64>() {
                            p.out_us.store(us, Ordering::Relaxed);
                        }
                    }
                    "fps" => {
                        p.fps.store(
                            value.parse::<f64>().unwrap_or(0.).to_bits(),
                            Ordering::Relaxed,
                        );
                    }
                    "speed" => {
                        if let Ok(x) = value.trim().trim_end_matches('x').parse::<f64>() {
                            p.speed.store(x.to_bits(), Ordering::Relaxed);
                        }
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
    let expected_audio = if source_count == 0 {
        0
    } else if s.separate_tracks {
        source_count + 1
    } else {
        1
    };
    Ok(Prepared {
        pipeline: Pipeline {
            child,
            _job: job,
            discard_tail: false,
            feeds,
            dir,
            manifest_offset: 0,
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
            began,
            reported_first_frame: false,
        },
        encoder,
        scaling_detail: path.detail,
        capture_api: if synthetic || s.capture != "desktop-duplication" {
            ""
        } else {
            capture_api
        },
        capture_filter: if synthetic {
            "synthetic"
        } else if s.capture != "desktop-duplication" {
            "gdigrab"
        } else if capture_api == "wgc" {
            "gfxcapture"
        } else {
            "ddagrab"
        },
        capture_fallback,
        plan,
        expected_audio,
        cache: path.cache,
        timings,
    })
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
/// Session folders that hold footage recovery could use: at least one
/// non-empty top-level segment. Empty folders (a start that never produced a
/// segment) and folders whose leftovers were moved to `quarantine` are not
/// offered; neither is anything in `active` (the running capture).
fn recoverable_sessions(data: &Path, active: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for cat in ["buffer", "recordings", "saves"] {
        let base = data.join(cat);
        let Ok(entries) = fs::read_dir(&base) else {
            continue;
        };
        for dir in entries.flatten().map(|e| e.path()) {
            if active.contains(&dir)
                || dir.join(DISCARDED).exists()
                || !crate::model::owned_session(&base, &dir)
            {
                continue;
            }
            let has_footage = fs::read_dir(&dir).is_ok_and(|files| {
                files.flatten().any(|f| {
                    let p = f.path();
                    p.extension().is_some_and(|x| x == "mkv")
                        && f.metadata().is_ok_and(|m| m.is_file() && m.len() > 0)
                })
            });
            if has_footage {
                found.push(dir);
            }
        }
    }
    found
}
fn count_sessions(data: &Path, active: &[PathBuf]) -> usize {
    recoverable_sessions(data, active).len()
}
/// Remove session folders that are completely empty (never received a file),
/// and finish deleting stopped replay buffers whose deletion was cut short.
/// Anything else containing a file is left alone.
fn remove_empty_sessions(data: &Path) {
    for cat in ["buffer", "recordings", "saves"] {
        let base = data.join(cat);
        if let Ok(entries) = fs::read_dir(&base) {
            for dir in entries.flatten().map(|e| e.path()) {
                if !crate::model::owned_session(&base, &dir) {
                    continue;
                }
                if cat == "buffer" && dir.join(DISCARDED).is_file() {
                    try_discard(&dir, &[]);
                } else if fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_none()) {
                    let _ = fs::remove_dir(&dir);
                }
            }
        }
    }
}
/// Throw away a start that was cancelled or timed out before any footage.
fn abandon(p: Prepared) {
    let mut pl = p.pipeline;
    let _ = pl.child.kill();
    let _ = pl.child.wait();
    pl.feeds.clear();
    for t in pl.readers.drain(..) {
        let _ = t.join();
    }
    discard_buffer_dir(&pl.dir, &[]);
}
/// Marks a buffer folder whose capture was stopped by the user: its footage
/// was not asked for, so it is deleted rather than offered for recovery.
const DISCARDED: &str = "discarded.marker";
/// Delete a cleanly stopped replay buffer: its segments (which the user did
/// not save; recordings and saves hold their own hard links), manifest and
/// settings copy. Unknown files and subfolders are left in place. The newest
/// segment can still be open for a moment (start-time probe, a save reading
/// its hard link), so deletion is retried in the background; the marker keeps
/// the folder out of the interrupted-session count meanwhile and lets the
/// next launch finish the job.
fn discard_buffer_dir(dir: &Path, ring: &[Segment]) {
    if !dir.is_dir() {
        return;
    }
    let _ = fs::write(
        dir.join(DISCARDED),
        b"Replay buffer stopped by the user; REPLAYX deletes this folder.",
    );
    if try_discard(dir, ring) {
        return;
    }
    let (dir, ring) = (dir.to_path_buf(), ring.to_vec());
    thread::spawn(move || {
        for _ in 0..120 {
            thread::sleep(Duration::from_millis(500));
            if try_discard(&dir, &ring) {
                break;
            }
        }
    });
}
/// One deletion pass; true once the folder is gone.
fn try_discard(dir: &Path, ring: &[Segment]) -> bool {
    for s in ring {
        let _ = fs::remove_file(&s.path);
    }
    let mut left = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for p in entries.flatten().map(|e| e.path()) {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let ours = (name.starts_with("segment-") && name.ends_with(".mkv"))
                || name == "segments.csv"
                || name == "session.json";
            if !(ours && p.is_file() && fs::remove_file(&p).is_ok()) && name != DISCARDED {
                left += 1;
            }
        }
    }
    if left == 0 {
        let _ = fs::remove_file(dir.join(DISCARDED));
        let _ = fs::remove_dir(dir);
    }
    !dir.exists()
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
    fn fill_start_times(&self, segments: &mut [Segment]) {
        let map = self.segment_starts.lock().unwrap();
        for s in segments.iter_mut().filter(|s| s.start_time.is_none()) {
            let key = s.origin.as_ref().unwrap_or(&s.path);
            s.start_time = map.get(key).copied();
        }
    }
    /// If a foreground sample is due (once a second while capturing a real
    /// source), the mappings to classify it with. The caller samples *after*
    /// releasing the lock.
    pub fn activity_due(&mut self) -> Option<Vec<crate::games::GameMapping>> {
        let now = Instant::now();
        let capturing = self.pipeline.as_ref().is_some_and(|p| !p.synthetic);
        // While idle, the game-detected notice and playtime tracking need to
        // know what is in front.
        let idle_watch = self.pipeline.is_none()
            && (self.settings.game_notifications || self.settings.playtime.enabled);
        if !(capturing || idle_watch) {
            // Nobody samples: never leave a stale game behind (it would keep
            // the overlay up after leaving the game).
            self.status.foreground_game = None;
            return None;
        }
        if self
            .last_activity_sample
            .is_some_and(|t| now - t < Duration::from_secs(1))
        {
            return None;
        }
        self.last_activity_sample = Some(now);
        Some(self.settings.game_mappings.clone())
    }
    pub fn record_activity(&mut self, at: Instant, what: crate::games::Foreground) {
        if let crate::games::Foreground::Game(g) = &what {
            self.status.foreground_game = Some(g.clone());
        } else if what == crate::games::Foreground::App {
            self.status.foreground_game = None;
        }
        // Clip attribution only uses time spent capturing.
        if self.pipeline.as_ref().is_some_and(|p| !p.synthetic) {
            self.status.detected_game = self.status.foreground_game.clone();
            self.activity.record(at, what);
        } else {
            self.status.detected_game = None;
        }
    }
    /// Record what a pending start should do once the encoder runs.
    fn set_intent(&mut self, recording: bool) {
        if let Some(s) = &mut self.starting {
            if recording {
                s.recording = true;
                self.status.pending_recording = true;
            } else {
                s.replay = true;
                self.status.replay = true;
            }
        }
    }
    /// Begin recording (or mark replay on) on the running encoder.
    fn attach(&mut self, recording: bool) -> Result<()> {
        if !recording {
            self.status.replay = true;
            return Ok(());
        }
        let after = self.ring.segments.back().map(|s| s.end).unwrap_or(0.);
        let dir = self
            .data_dir
            .join("recordings")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir)?;
        self.recording = Some(Recording {
            dir,
            segments: Vec::new(),
            after,
            started: Instant::now(),
            stop_at: None,
        });
        self.status.recording = true;
        self.status.recording_elapsed = 0.;
        if self.status.frames > 0 && self.status.state == "Buffering" {
            // The encoder is already writing frames: recording starts now.
            self.status.state = "Recording".into();
        }
        Ok(())
    }
    fn refresh_session_count(&mut self) {
        let mut active = Vec::new();
        if let Some(p) = &self.pipeline {
            active.push(p.dir.clone());
        }
        if let Some(r) = &self.recording {
            active.push(r.dir.clone());
        }
        self.status.recovery_sessions = count_sessions(&self.data_dir, &active);
    }
    /// Stop the encoder after a user stop and delete the replay buffer it
    /// leaves behind (0.2.1-alpha.1 kept every stopped buffer on disk, and
    /// the next launch offered each one as an "interrupted session").
    /// Footage is only kept after crashes and errors, where it is recoverable.
    fn stop_clean(&mut self) -> Result<()> {
        let dir = self
            .pipeline
            .as_ref()
            .filter(|p| !p.discard_tail)
            .map(|p| p.dir.clone());
        self.stop_pipeline()?;
        if let Some(dir) = dir {
            let ring: Vec<_> = self.ring.segments.drain(..).collect();
            discard_buffer_dir(&dir, &ring);
            self.ring = Ring::default();
            self.status.buffer_seconds = 0.;
            self.status.buffer_bytes = 0;
        }
        self.refresh_session_count();
        Ok(())
    }
    /// Copy what a start needs out of the engine (cheap checks only).
    fn pipeline_request(&self, synthetic: bool, began: Instant) -> Result<PipelineRequest> {
        self.settings.validate()?;
        health::storage_ready(
            fs2::available_space(&self.data_dir)?,
            fs2::available_space(&self.settings.output_dir)?,
            true,
        )?;
        Ok(PipelineRequest {
            settings: self.settings.clone(),
            media: self.media.clone(),
            data_dir: self.data_dir.clone(),
            synthetic,
            cache: self.video_path_cache.clone(),
            cached_detail: self.status.scaling_detail.clone(),
            denied: self.denied_paths.clone(),
            began,
        })
    }
    /// Start the encoder while holding the lock. Only used for automatic
    /// restarts (GPU-path fallback, display recovery, recording finalize
    /// fallback); user starts go through `Engine::start`, which does not lock.
    fn start_pipeline(&mut self, synthetic: bool) -> Result<()> {
        self.status.state = "Starting".into();
        let request = self.pipeline_request(synthetic, Instant::now())?;
        let prepared = prepare_pipeline(request)?;
        self.install_pipeline(prepared);
        Ok(())
    }
    fn install_pipeline(&mut self, p: Prepared) {
        let synthetic = p.pipeline.synthetic;
        let scaler = p.pipeline.scaler;
        if scaler == Scaler::Cpu && p.plan.is_some() {
            self.status.notice = Some(
                "GPU scaling is unavailable on this display adapter; using the CPU fallback (see Performance for details).".into(),
            );
        }
        (self.status.play_width, self.status.play_height) = match p.plan {
            Some(plan) if plan.sar != (1, 1) => {
                let (w, h) = plan.display_size();
                self.status.notice = Some(format!(
                    "Recording your display's {}×{} pixels instead of resizing to {}×{}, marked to play at {w}×{h}. Keeping every frame on the GPU is what makes the recording smooth: this GPU can only resize through a CPU copy, which stutters in games. Players and editors show it at {w}×{h}. For a file that is exactly {}×{}, choose Exact size in Video settings.",
                    plan.width,
                    plan.height,
                    self.settings.output_width(),
                    self.settings.height,
                    self.settings.output_width(),
                    self.settings.height
                ));
                (w, h)
            }
            _ => (0, 0),
        };
        if let Some(cache) = p.cache {
            self.video_path_cache = Some(cache);
        }
        self.status.encoder_label = media::encoder_label(&p.encoder);
        self.status.encoder = p.encoder;
        self.status.capture = self.settings.capture.clone();
        self.status.frames = 0;
        self.status.elapsed = 0.;
        self.status.geometry = p.pipeline.geometry.clone();
        self.status.scaling = if synthetic {
            "Synthetic test source".into()
        } else {
            scaler.label().into()
        };
        self.status.scaling_detail = p.scaling_detail;
        self.status.capture_api = p.capture_api.into();
        self.status.capture_filter = p.capture_filter.into();
        self.status.capture_fallback = p.capture_fallback;
        (self.status.output_width, self.status.output_height) =
            p.plan.map(|p| (p.width, p.height)).unwrap_or_default();
        let rate = media::rate_plan(
            &self.settings,
            &self.status.encoder,
            if self.status.output_width > 0 {
                self.status.output_width
            } else {
                self.settings.output_width()
            },
            if self.status.output_height > 0 {
                self.status.output_height
            } else {
                self.settings.height
            },
        );
        self.status.target_bitrate_mbps = rate.target_mbps;
        self.status.cap_bitrate_mbps = rate.cap_mbps;
        self.status.constant_quality = rate.cq;
        self.status.codec = media::codec_of(&self.status.encoder).into();
        self.status.buffer_bytes = 0;
        self.status.buffer_seconds = 0.;
        self.status.start_timings = p.timings;
        self.ring = Ring::default();
        self.ring_audio_tracks = p.expected_audio;
        self.last_activity_sample = None;
        self.pipeline = Some(p.pipeline);
        // Controls may change while a worker is initializing a capture path.
        if let Some(p) = &self.pipeline {
            for (feed, source) in p
                .feeds
                .iter()
                .zip(self.settings.audio.iter().filter(|a| a.enabled))
            {
                feed.meter.muted.store(source.muted, Ordering::Relaxed);
                feed.meter
                    .volume
                    .store(source.volume.to_bits(), Ordering::Relaxed);
            }
        }
    }
    fn collect_segments(&mut self) -> Result<()> {
        let Some(p) = &mut self.pipeline else {
            return Ok(());
        };
        if p.discard_tail {
            return Ok(());
        }
        let csv = manifest_tail(&p.dir.join("segments.csv"), p.manifest_offset)?;
        // Ignore a concurrently written partial CSV line until its newline is visible.
        let lines: Vec<_> = csv
            .split_inclusive('\n')
            .filter(|s| s.ends_with('\n'))
            .collect();
        for raw in lines {
            let line = raw.trim();
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
                self.status.start_timings.push(media::StageTiming {
                    stage: "First segment finalized and validated".into(),
                    elapsed_ms: p.began.elapsed().as_millis() as u64,
                });
            }
            // Probe the container start time now, in the background at low
            // priority, so saves need not probe every segment.
            let _ = self.segment_probe_tx.send(path.clone());
            let segment = Segment {
                origin: Some(path.clone()),
                path,
                start,
                end,
                bytes,
                start_time: None,
            };
            if let Some(r) = &mut self.recording {
                if segment.end > r.after && r.stop_at.is_none_or(|stop| segment.start < stop) {
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
            // Commit only after the segment was validated and retained. A
            // partial row or failed segment never advances the cursor.
            p.manifest_offset += raw.len() as u64;
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
        self.refresh_session_count();
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
                forget_video_path(&self.data_dir, scaler, &encoder);
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
        // 0.2.1-alpha.2 left the recording attached after a failure: the UI
        // said "Capture is off" while Record answered "Recording is already
        // active" until a restart. Save what was recorded and clear it.
        if let Some(mut rec) = self.recording.take() {
            if rec.segments.is_empty() {
                let _ = fs::remove_dir(&rec.dir);
            } else {
                self.fill_start_times(&mut rec.segments);
                let (path, tags) = self.clip_path("Recording", rec.started, Instant::now());
                self.orphaned.push(SaveJob {
                    media: self.media.clone(),
                    segments: rec.segments,
                    path,
                    expected_audio: self.expected_audio(),
                    dir: rec.dir,
                    requested: Instant::now(),
                    tags,
                });
                self.status.saving += 1;
            }
        }
        self.status.pending_recording = false;
        self.status.replay = false;
        self.status.recording = false;
        self.status.state = "Error".into();
        self.status.error = Some(error);
    }
    fn tick(&mut self) -> Result<Option<SaveJob>> {
        self.status.free_bytes = fs2::available_space(&self.settings.output_dir).unwrap_or(0);
        if let Some(s) = &self.starting {
            if s.began.elapsed() > START_WATCHDOG {
                // Bounded failure: the start thread discards its encoder if it
                // ever returns; the UI gets control and a reason now.
                self.starting = None;
                self.status.pending_recording = false;
                self.status.replay = false;
                self.status.state = "Error".into();
                self.status.error = Some(format!(
                    "Capture did not start within {} s (display, GPU encoder or audio device did not respond). Nothing was recorded; try again or choose another encoder in Video settings.",
                    START_WATCHDOG.as_secs()
                ));
            } else {
                self.status.state = if s.recording || s.replay {
                    "Starting"
                } else {
                    "Stopping"
                }
                .into();
            }
            return Ok(None);
        }
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
        let warming_up = self
            .pipeline
            .as_ref()
            .is_some_and(|p| p.progress.frames.load(Ordering::Relaxed) == 0);
        if let Some(r) = self.recording.as_mut().filter(|_| warming_up) {
            // The recording (and its timer) begins with the first encoded frame.
            r.started = Instant::now();
        }
        self.status.recording_elapsed = match &self.recording {
            Some(r) if self.status.recording => r.started.elapsed().as_secs_f64(),
            _ => 0.,
        };
        let p = self.pipeline.as_mut().unwrap();
        if !p.reported_first_frame {
            let ms = p.progress.first_frame_ms.load(Ordering::Relaxed);
            if ms > 0 {
                p.reported_first_frame = true;
                if media::tuning().gpu_priority {
                    let class = media::tuning().gpu_priority_class;
                    let status = crate::process_job::set_gpu_priority(&p.child, class);
                    self.status.start_timings.push(media::StageTiming {
                        stage: if status == 0 {
                            format!(
                                "Encoder GPU priority raised ({})",
                                match class {
                                    3 => "above normal",
                                    2 => "normal",
                                    _ => "high",
                                }
                            )
                        } else {
                            format!("Encoder GPU priority unchanged (status {status:#x})")
                        },
                        elapsed_ms: p.began.elapsed().as_millis() as u64,
                    });
                }
                self.status.start_timings.push(media::StageTiming {
                    stage: "First frame encoded".into(),
                    elapsed_ms: ms,
                });
            }
        }
        // Checked before FFmpeg's exit status: when an audio device goes away,
        // FFmpeg may also stop because that audio input ended.
        // An audio device that changed during capture (headset plugged in or
        // removed, output switched, Bluetooth profile change) reconnects
        // through the bounded recovery instead of stopping capture.
        let device_changed = p.feeds.iter().find_map(|f| {
            f.meter
                .error
                .lock()
                .unwrap()
                .clone()
                .filter(|e| audio::is_device_change(e))
        });
        if let Some(e) = device_changed.filter(|_| !p.synthetic) {
            return self.begin_recovery(&format!(
                "An audio device changed ({e}); reconnecting audio."
            ));
        }
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
        self.status.audio_queue_ms = p
            .feeds
            .iter()
            .map(|f| f.meter.queued_chunks.load(Ordering::Relaxed) * 10)
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
        self.status.encoder_speed = f64::from_bits(p.progress.speed.load(Ordering::Relaxed));
        let first_frame = p.progress.first_frame_ms.load(Ordering::Relaxed);
        self.status.video_lag_ms = if first_frame > 0 {
            let since_first = (p.began.elapsed().as_millis() as u64).saturating_sub(first_frame);
            since_first.saturating_sub(p.progress.out_us.load(Ordering::Relaxed) / 1000)
        } else {
            0
        };
        // The real cause behind "FFmpeg stopped reading audio": the video side
        // fell behind real time (encoder or capture overloaded), and FFmpeg
        // pauses inputs that are ahead of the slowest output.
        if self.status.video_lag_ms > VIDEO_LAG_LIMIT_MS {
            bail!(
                "{}",
                overload_message(
                    self.status.video_lag_ms,
                    self.status.encoder_speed,
                    self.settings.fps
                )
            )
        }
        for f in &p.feeds {
            if let Some(e) = f.meter.error.lock().unwrap().clone() {
                if e.contains("backpressure") {
                    bail!(
                        "{e}. {}",
                        overload_message(
                            self.status.video_lag_ms,
                            self.status.encoder_speed,
                            self.settings.fps
                        )
                    )
                }
            }
        }
        health::stream_health(elapsed, last, p.last_segment_ms, &audio_writes)?;
        // "Recording" only once the encoder is writing frames to the output; the
        // first finished segment is still validated (stream layout and size)
        // and a failure there falls back to the next capture path.
        self.status.state = if self.status.frames == 0 {
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
        self.status.notice=Some(format!("{reason} A recording continues as a new verified part; replay restarts with a fresh buffer. Footage saved so far is kept."));
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
            bail!("Capture couldn't restart after three attempts. Check the display and Windows Sound settings, then start capture again. Previous footage is preserved.")
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
                                stop_at: None,
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
    #[ignore = "Explicit manifest polling microbenchmark, not a timing assertion"]
    fn manifest_poll_cost() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("segments.csv");
        let history = "segment-00000001.mkv,0,2\n".repeat(20_000);
        fs::write(&path, &history).unwrap();
        let started = Instant::now();
        for _ in 0..1000 {
            let text = fs::read_to_string(&path).unwrap();
            std::hint::black_box(
                text.split_inclusive('\n')
                    .filter(|s| s.ends_with('\n'))
                    .map(str::trim)
                    .collect::<Vec<_>>(),
            );
        }
        let old = started.elapsed();
        let started = Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(manifest_tail(&path, history.len() as u64).unwrap());
        }
        println!(
            "MANIFEST_BENCH bytes={} polls=1000 full_scan_ms={:.3} incremental_ms={:.3}",
            history.len(),
            old.as_secs_f64() * 1000.,
            started.elapsed().as_secs_f64() * 1000.
        );
    }
    #[test]
    fn manifest_cursor_retains_partial_rows_and_reads_only_new_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("segments.csv");
        assert_eq!(manifest_tail(&path, 0).unwrap(), "");
        let history = "segment-00000001.mkv,0,2\n".repeat(20_000);
        fs::write(&path, &history).unwrap();
        let offset = history.len() as u64;
        assert_eq!(
            manifest_tail(&path, offset).unwrap(),
            "",
            "old history is not read or reparsed"
        );
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"segment-00020001.mkv,2,").unwrap();
        let partial = manifest_tail(&path, offset).unwrap();
        assert_eq!(partial.lines().count(), 1);
        assert!(!partial.ends_with('\n'));
        file.write_all(b"4\nsegment-00020002.mkv,4,6\n").unwrap();
        let tail = manifest_tail(&path, offset).unwrap();
        assert_eq!(tail, "segment-00020001.mkv,2,4\nsegment-00020002.mkv,4,6\n");
        assert!(manifest_tail(&path, offset + tail.len() as u64)
            .unwrap()
            .is_empty());
        fs::write(&path, b"").unwrap();
        assert!(manifest_tail(&path, offset)
            .unwrap_err()
            .to_string()
            .contains("truncated"));
        assert!(manifest_tail(&dir.path().join("missing.csv"), 1).is_err());
        fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();
        assert!(manifest_tail(&path, 0)
            .unwrap_err()
            .to_string()
            .contains("backlog"));
    }
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
                origin: None,
                start_time: None,
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
    fn tour_and_tray_hint_are_remembered_across_launches() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let settings = Settings {
            output_dir: root.path().join("clips"),
            audio: Vec::new(),
            ..Default::default()
        };
        let engine = isolated(data.clone(), settings).unwrap();
        assert!(!engine.settings().tour_done);
        engine.mark_tour_done();
        engine.mark_tray_hint_shown();
        let reloaded = Settings::load(&data.join("settings.json")).unwrap();
        assert!(
            reloaded.tour_done && reloaded.tray_hint_shown,
            "saved at once"
        );
        engine.mark_tour_done();
        assert!(engine.settings().tour_done);
    }
    #[test]
    fn missing_encoder_is_error_and_preserves_hotkey_warning() {
        let root = tempfile::tempdir().unwrap();
        let engine = isolated(
            root.path().join("data"),
            Settings {
                output_dir: root.path().join("clips"),
                // Never a real device in tests (the default includes the mic).
                audio: Vec::new(),
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

    // ---- 0.2.1-alpha.2: startup responsiveness and session lifecycle ----
    // Tests that start an encoder use the synthetic source (generated pattern
    // and tones). Nothing is captured from the screen, microphone or speakers.

    fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        f()
    }
    /// Engine with the real media runtime, or None where it is not installed.
    fn real(root: &Path) -> Option<Engine> {
        let media = Media::discover().ok()?;
        let settings = Settings {
            output_dir: root.join("clips"),
            ..Default::default()
        };
        Some(Engine::with_media(root.join("data"), settings, media).unwrap())
    }
    fn session_dirs(data: &Path, cat: &str) -> usize {
        fs::read_dir(data.join(cat))
            .map(|d| d.flatten().filter(|e| e.path().is_dir()).count())
            .unwrap_or(0)
    }
    #[test]
    fn status_and_settings_never_wait_for_a_busy_engine() {
        // The 0.2.1-alpha.1 freeze: the UI thread's status poll waited for the
        // engine lock. A held lock must not delay either read.
        let root = tempfile::tempdir().unwrap();
        let engine = isolated(
            root.path().join("data"),
            Settings {
                output_dir: root.path().join("clips"),
                ..Default::default()
            },
        )
        .unwrap();
        let busy = engine.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let holder = thread::spawn(move || {
            let _guard = busy.0.lock().unwrap();
            tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(800));
        });
        rx.recv().unwrap();
        let t = Instant::now();
        for _ in 0..20 {
            assert_eq!(engine.status().state, "Stopped");
            let _ = engine.settings();
        }
        assert!(
            t.elapsed() < Duration::from_millis(100),
            "{:?}",
            t.elapsed()
        );
        holder.join().unwrap();
    }
    #[test]
    fn failed_start_returns_control_and_is_never_left_starting() {
        let root = tempfile::tempdir().unwrap();
        let engine = isolated(
            root.path().join("data"),
            Settings {
                output_dir: root.path().join("clips"),
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..3 {
            // Missing runtime: the error is reported, the next attempt is allowed.
            let error = engine.start_recording().unwrap_err().to_string();
            assert!(error.contains("Cannot run"), "{error}");
            let s = engine.status();
            assert_eq!(s.state, "Error");
            assert!(!s.recording && !s.pending_recording && !s.replay);
            assert!(engine.0.lock().unwrap().starting.is_none());
        }
        assert_eq!(session_dirs(&root.path().join("data"), "buffer"), 0);
    }
    #[test]
    fn start_watchdog_gives_control_back_with_a_reason() {
        let root = tempfile::tempdir().unwrap();
        let engine = isolated(
            root.path().join("data"),
            Settings {
                output_dir: root.path().join("clips"),
                ..Default::default()
            },
        )
        .unwrap();
        let mut i = engine.0.lock().unwrap();
        i.starting = Some(Starting {
            recording: true,
            replay: false,
            began: Instant::now() - START_WATCHDOG - Duration::from_secs(1),
        });
        i.status.pending_recording = true;
        i.tick().unwrap();
        assert!(i.starting.is_none());
        assert_eq!(i.status.state, "Error");
        assert!(!i.status.pending_recording);
        assert!(i.status.error.as_ref().unwrap().contains("did not start"));
    }
    #[test]
    fn stop_during_start_cancels_only_a_pending_recording() {
        let root = tempfile::tempdir().unwrap();
        let engine = isolated(
            root.path().join("data"),
            Settings {
                output_dir: root.path().join("clips"),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!engine.cancel_pending_recording());
        {
            let mut i = engine.0.lock().unwrap();
            i.starting = Some(Starting {
                recording: true,
                replay: true,
                began: Instant::now(),
            });
            i.status.pending_recording = true;
        }
        assert!(engine.cancel_pending_recording());
        assert!(!engine.cancel_pending_recording());
        let i = engine.0.lock().unwrap();
        let s = i.starting.as_ref().unwrap();
        assert!(!s.recording && s.replay, "replay intent is kept");
        assert!(!i.status.pending_recording);
    }
    #[test]
    fn interrupted_sessions_count_only_real_footage() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let id = || uuid::Uuid::new_v4().to_string();
        let crashed = data.join("buffer").join(id());
        let torn = data.join("buffer").join(id());
        let empty = data.join("recordings").join(id());
        let recovered = data.join("buffer").join(id());
        for d in [&crashed, &torn, &empty, &recovered] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(crashed.join("segment-00000000.mkv"), b"footage").unwrap();
        fs::write(torn.join("segment-00000001.mkv"), b"").unwrap();
        fs::create_dir_all(recovered.join("quarantine")).unwrap();
        fs::write(recovered.join("quarantine/segment-00000002.mkv"), b"torn").unwrap();
        assert_eq!(count_sessions(&data, &[]), 1);
        assert_eq!(count_sessions(&data, std::slice::from_ref(&crashed)), 0);
        remove_empty_sessions(&data);
        assert!(!empty.exists(), "a folder without any file is removed");
        assert!(crashed.exists() && torn.exists() && recovered.exists());
        assert_eq!(
            fs::read(crashed.join("segment-00000000.mkv")).unwrap(),
            b"footage"
        );
        let engine = isolated(
            data.clone(),
            Settings {
                output_dir: root.path().join("clips"),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(engine.status().recovery_sessions, 1);
    }
    #[test]
    fn stop_while_starting_discards_the_encoder_and_leaves_nothing() {
        let root = tempfile::tempdir().unwrap();
        let Some(engine) = real(root.path()) else {
            return;
        };
        let starter = engine.clone();
        let start = thread::spawn(move || starter.start_synthetic(true));
        assert!(wait_until(Duration::from_secs(10), || engine
            .status()
            .pending_recording));
        assert!(engine.cancel_pending_recording());
        start.join().unwrap().unwrap();
        let s = engine.status();
        assert!(
            !s.recording && !s.pending_recording && !s.replay,
            "{}",
            s.state
        );
        assert_eq!(s.state, "Stopped");
        assert!(engine.0.lock().unwrap().pipeline.is_none());
        let data = root.path().join("data");
        assert_eq!(session_dirs(&data, "buffer"), 0);
        assert_eq!(session_dirs(&data, "recordings"), 0);
        assert_eq!(s.recovery_sessions, 0);
    }
    #[test]
    fn rapid_start_stop_never_leaves_an_encoder_or_session() {
        let root = tempfile::tempdir().unwrap();
        let Some(engine) = real(root.path()) else {
            return;
        };
        for delay in [0u64, 30, 200, 700] {
            let starter = engine.clone();
            let start = thread::spawn(move || starter.start_synthetic(false));
            thread::sleep(Duration::from_millis(delay));
            engine.stop_replay().unwrap();
            start.join().unwrap().unwrap();
            if engine.status().replay {
                // The start completed before the stop arrived: stop it now.
                engine.stop_replay().unwrap();
            }
            assert!(engine.0.lock().unwrap().pipeline.is_none());
            assert!(engine.0.lock().unwrap().starting.is_none());
        }
        let data = root.path().join("data");
        assert_eq!(session_dirs(&data, "buffer"), 0);
        assert_eq!(engine.status().recovery_sessions, 0);
    }
    #[test]
    fn output_folder_failure_reports_error_without_hanging() {
        let root = tempfile::tempdir().unwrap();
        let Some(engine) = real(root.path()) else {
            return;
        };
        // Make the buffer folder impossible to create.
        fs::write(root.path().join("data").join("buffer"), b"not a folder").unwrap();
        let t = Instant::now();
        assert!(engine.start_synthetic(true).is_err());
        assert!(t.elapsed() < Duration::from_secs(20));
        let s = engine.status();
        assert_eq!(s.state, "Error");
        assert!(!s.recording && !s.pending_recording);
        assert!(engine.0.lock().unwrap().starting.is_none());
    }
    #[test]
    fn audio_start_failure_reports_error_and_cleans_up() {
        let root = tempfile::tempdir().unwrap();
        let Some(media) = Media::discover().ok() else {
            return;
        };
        let mut source = Settings::default().audio[0].clone();
        source.kind = "application".into();
        source.pid = 4_000_000_000;
        source.process_started = 1;
        source.name = "Exited app".into();
        let settings = Settings {
            output_dir: root.path().join("clips"),
            // Window capture: no display probe; audio fails before FFmpeg starts,
            // so nothing is captured.
            capture: "desktop".into(),
            audio: vec![source],
            ..Default::default()
        };
        let data = root.path().join("data");
        let engine = Engine::with_media(data.clone(), settings, media).unwrap();
        let error = engine.start_recording().unwrap_err().to_string();
        assert!(error.contains("application"), "{error}");
        let s = engine.status();
        assert_eq!(s.state, "Error");
        assert!(!s.recording && !s.pending_recording);
        assert_eq!(session_dirs(&data, "buffer"), 0);
        assert_eq!(session_dirs(&data, "recordings"), 0);
    }
    #[test]
    fn one_microphone_switch_mutes_live_and_touches_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let Some(engine) = real(root.path()) else {
            return;
        };
        let source = |s: &Settings, kind: &str| {
            let mut v =
                serde_json::to_value(s.audio.iter().find(|a| a.kind == kind).unwrap()).unwrap();
            let state = (v["enabled"].take(), v["muted"].take());
            (v, state.0 == true, state.1 == true)
        };
        let before = engine.settings();
        // Off = muted: still in the capture layout, device/volume/voice untouched.
        let off = engine.set_microphone(false).unwrap();
        let (mic, enabled, muted) = source(&off, "microphone");
        assert!(enabled && muted);
        assert_eq!(
            mic,
            source(&before, "microphone").0,
            "device, volume and Voice Enhancer kept"
        );
        assert_eq!(
            source(&off, "system"),
            source(&before, "system"),
            "system audio unaffected"
        );
        let saved = Settings::load(&root.path().join("data").join("settings.json")).unwrap();
        assert!(source(&saved, "microphone").2, "survives a restart");
        // During capture: switching is live, repeated fast switching is safe,
        // and capture is never restarted (same encoder process).
        let pid = |e: &Engine| e.0.lock().unwrap().pipeline.as_ref().map(|p| p.child.id());
        engine.start_synthetic(false).unwrap();
        assert!(wait_until(Duration::from_secs(15), || engine
            .status()
            .state
            == "Buffering"));
        let encoder = pid(&engine).unwrap();
        for _ in 0..25 {
            assert!(!source(&engine.set_microphone(true).unwrap(), "microphone").2);
            assert!(source(&engine.set_microphone(false).unwrap(), "microphone").2);
        }
        engine.set_microphone(true).unwrap();
        assert_eq!(pid(&engine), Some(encoder));
        assert_eq!(engine.status().state, "Buffering");
        assert!(engine.status().error.is_none());
        engine.stop_replay().unwrap();
        // A microphone taken out of the capture (Audio page) comes back with
        // the switch while idle, but can't join a capture that is running.
        let mut without = engine.settings();
        for a in without.audio.iter_mut().filter(|a| a.kind == "microphone") {
            a.enabled = false;
        }
        engine.set_settings(without.clone()).unwrap();
        let on = engine.set_microphone(true).unwrap();
        assert!(source(&on, "microphone").1 && !source(&on, "microphone").2);
        engine.set_settings(without).unwrap();
        engine.start_synthetic(false).unwrap();
        assert!(wait_until(Duration::from_secs(15), || engine
            .status()
            .state
            == "Buffering"));
        let refused = engine.set_microphone(true).unwrap_err().to_string();
        assert!(refused.contains("isn't part of the capture"), "{refused}");
        assert!(
            !source(&engine.settings(), "microphone").1,
            "nothing changed on refusal"
        );
        engine.stop_replay().unwrap();
        // No microphone at all: a clear message.
        let mut none = engine.settings();
        none.audio.retain(|a| a.kind != "microphone");
        engine.set_settings(none).unwrap();
        assert!(engine
            .set_microphone(true)
            .unwrap_err()
            .to_string()
            .contains("No microphone"));
    }
    #[test]
    fn recording_joins_running_replay_and_stops_cleanly() {
        let root = tempfile::tempdir().unwrap();
        let Some(engine) = real(root.path()) else {
            return;
        };
        let pid = |e: &Engine| e.0.lock().unwrap().pipeline.as_ref().map(|p| p.child.id());
        engine.start_synthetic(false).unwrap();
        assert!(wait_until(Duration::from_secs(15), || engine
            .status()
            .state
            == "Buffering"));
        let encoder = pid(&engine).unwrap();
        // Record joins the running encoder: no restart, no capture gap. The
        // same encoder process (below) is the proof; the time bound only
        // catches a restart (about 3 s) and allows a busy test machine.
        let t = Instant::now();
        engine.start_synthetic(true).unwrap();
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        assert_eq!(pid(&engine), Some(encoder));
        assert!(engine.status().recording);
        assert!(
            engine.start_synthetic(true).is_err(),
            "a second Record is refused"
        );
        assert!(wait_until(Duration::from_secs(10), || engine
            .0
            .lock()
            .unwrap()
            .recording
            .as_ref()
            .is_some_and(|r| !r.segments.is_empty())));
        let job = engine.stop_recording().unwrap();
        let result = job.run();
        engine.complete_save(&result);
        let (clip, probe) = result.unwrap();
        assert!(clip.is_file());
        assert!(probe.duration > 1.0, "{}", probe.duration);
        // Replay kept buffering on the same encoder.
        assert_eq!(pid(&engine), Some(encoder));
        assert!(engine.status().replay);
        let buffer = engine
            .0
            .lock()
            .unwrap()
            .pipeline
            .as_ref()
            .unwrap()
            .dir
            .clone();
        engine.stop_replay().unwrap();
        let s = engine.status();
        assert_eq!(s.state, "Stopped");
        assert!(!buffer.exists(), "a stopped replay buffer is not kept");
        assert_eq!(s.recovery_sessions, 0);
        assert_eq!(session_dirs(&root.path().join("data"), "recordings"), 0);
    }

    #[test]
    fn stopped_buffer_with_a_busy_segment_is_deleted_later_and_never_counted() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let dir = data.join("buffer").join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        for f in [
            "segment-00000000.mkv",
            "segment-00000001.mkv",
            "segments.csv",
            "session.json",
        ] {
            fs::write(dir.join(f), b"x").unwrap();
        }
        // Like the background start-time probe holding the newest segment.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(dir.join("segment-00000001.mkv"))
            .unwrap();
        discard_buffer_dir(&dir, &[]);
        assert!(!dir.join("segment-00000000.mkv").exists());
        assert!(dir.join("segment-00000001.mkv").exists());
        assert_eq!(
            count_sessions(&data, &[]),
            0,
            "a stopped buffer is not an interrupted session"
        );
        drop(held);
        assert!(wait_until(Duration::from_secs(5), || !dir.exists()));
        // Cut short by an exit: the next launch finishes the deletion.
        let dir = data.join("buffer").join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("segment-00000007.mkv"), b"x").unwrap();
        fs::write(dir.join(DISCARDED), b"").unwrap();
        remove_empty_sessions(&data);
        assert!(!dir.exists());
    }
    #[test]
    fn failed_recording_is_saved_and_never_blocks_the_next_one() {
        // 0.2.1-alpha.2: after an audio/encoder failure the UI showed "Capture is
        // off" while Record answered "Recording is already active".
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
            let mut i = engine.0.lock().unwrap();
            let dir = root.path().join("data/recordings/x");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("00000000.mkv"), b"part").unwrap();
            i.recording = Some(Recording {
                dir: dir.clone(),
                segments: vec![Segment {
                    path: dir.join("00000000.mkv"),
                    start: 0.,
                    end: 2.,
                    bytes: 4,
                    origin: None,
                    start_time: Some(0.),
                }],
                after: 0.,
                started: Instant::now(),
                stop_at: None,
            });
            i.status.recording = true;
            i.fail_final("Audio capture stopped: test".into());
            assert!(i.recording.is_none());
            assert_eq!(i.orphaned.len(), 1, "the footage goes to a save, not lost");
            assert_eq!(i.status.saving, 1);
            assert!(!i.status.recording && i.status.state == "Error");
            i.orphaned.clear();
            i.status.saving = 0;
        }
        let error = engine.start_recording().unwrap_err().to_string();
        assert!(!error.contains("already active"), "{error}");
        assert!(!error.contains("Recover the interrupted"), "{error}");
    }
    #[test]
    fn overload_message_names_the_cause_and_a_fix() {
        let m = overload_message(9200, 0.83, 120);
        assert!(m.contains("9.2 s behind") && m.contains("0.83x") && m.contains("120 fps"));
        assert!(m.contains("60 fps") && m.contains("Performance"));
    }
}
