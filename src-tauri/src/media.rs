use crate::{buffer::Segment, geometry::VideoPlan, model::Settings};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn command(exe: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
/// Helper processes after capture (probes, saving, checks, thumbnails,
/// export) run below normal priority, so a save or a thumbnail never takes
/// CPU time from the game being played. Capture itself uses `command`.
pub fn helper(exe: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    c
}
#[derive(Clone)]
pub struct Media {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub duration: f64,
    /// Container start time; the concat demuxer re-bases each file by this.
    pub start_time: f64,
    /// Audio track titles in stream order (empty string when untitled).
    #[serde(default)]
    pub audio_titles: Vec<String>,
    #[serde(default)]
    pub fps: f64,
    pub bytes: u64,
    pub video_codec: String,
    pub audio_tracks: usize,
    pub width: u64,
    pub height: u64,
    #[serde(default)]
    pub save_timings: Vec<StageTiming>,
    /// Game attributed when the clip was saved (REPLAYX_GAME tag), if any.
    #[serde(default)]
    pub game: Option<String>,
    /// "Replay" or "Recording" (REPLAYX_KIND tag), if any.
    #[serde(default)]
    pub kind: Option<String>,
}
/// Facts needed to concatenate a segment: stream layout and start time.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentInfo {
    pub start_time: f64,
    pub layout: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageTiming {
    pub stage: String,
    pub elapsed_ms: u64,
}
#[derive(Serialize)]
pub struct Encoder {
    pub name: String,
    pub available: bool,
    pub reason: String,
}
impl Media {
    pub fn preflight(&self) -> Result<()> {
        for exe in [&self.ffmpeg, &self.ffprobe] {
            let output=command(exe).args(["-version"]).output().with_context(||format!("Cannot run {}. Windows may have blocked the media runtime; review the security notification without disabling protection.",exe.file_name().unwrap_or_default().to_string_lossy()))?;
            if !output.status.success() {
                bail!("Media runtime version check failed")
            }
        }
        Ok(())
    }
    pub fn discover() -> Result<Self> {
        let mut dirs = Vec::new();
        if let Some(p) = std::env::var_os("REPLAYX_FFMPEG_DIR") {
            dirs.push(PathBuf::from(p));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(p) = exe.parent() {
                dirs.push(p.join("media"));
                dirs.push(p.to_path_buf());
            }
        }
        // Use the reviewed lockfile, never an arbitrary old cache or PATH copy.
        let lock: serde_json::Value = serde_json::from_str(
            include_str!("../../tools/ffmpeg-lock.json").trim_start_matches('\u{feff}'),
        )?;
        let runtime = lock["runtime_dir"]
            .as_str()
            .context("Media lock is missing runtime_dir")?;
        dirs.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../tools")
                .join(runtime),
        );
        for d in dirs {
            if d.join("ffmpeg.exe").is_file() && d.join("ffprobe.exe").is_file() {
                return Ok(Self {
                    ffmpeg: d.join("ffmpeg.exe"),
                    ffprobe: d.join("ffprobe.exe"),
                });
            }
        }
        bail!("FFmpeg not found. Run scripts/setup-media.ps1 or set REPLAYX_FFMPEG_DIR to a trusted FFmpeg bin directory.")
    }
    pub fn probe(&self, path: &Path) -> Result<Probe> {
        let out = helper(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(path)
            .output()?;
        if !out.status.success() {
            bail!("ffprobe could not read the output")
        }
        let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        let streams = json["streams"].as_array().context("No streams in output")?;
        let video = streams
            .iter()
            .find(|s| s["codec_type"] == "video")
            .context("Output contains no video")?;
        let duration = json["format"]["duration"]
            .as_str()
            .unwrap_or("0")
            .parse::<f64>()?;
        if !duration.is_finite() || duration <= 0.0 {
            bail!("Output has invalid duration")
        }
        Ok(Probe {
            duration,
            start_time: json["format"]["start_time"]
                .as_str()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(0.),
            bytes: fs::metadata(path)?.len(),
            video_codec: video["codec_name"].as_str().unwrap_or("unknown").into(),
            audio_tracks: streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .count(),
            audio_titles: streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .map(|s| s["tags"]["title"].as_str().unwrap_or("").to_owned())
                .collect(),
            fps: video["avg_frame_rate"]
                .as_str()
                .and_then(|r| r.split_once('/'))
                .and_then(|(n, d)| {
                    Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok().filter(|d| *d > 0.)?)
                })
                .unwrap_or(0.),
            width: video["width"].as_u64().unwrap_or(0),
            height: video["height"].as_u64().unwrap_or(0),
            save_timings: Vec::new(),
            game: format_tag(&json, "REPLAYX_GAME"),
            kind: format_tag(&json, "REPLAYX_KIND"),
        })
    }
    pub fn verify(&self, path: &Path, expected_audio: usize) -> Result<Probe> {
        let p = self.probe(path)?;
        if p.audio_tracks != expected_audio {
            bail!(
                "Audio verification failed: expected {expected_audio} tracks, found {}",
                p.audio_tracks
            )
        }
        let out = helper(&self.ffmpeg)
            .args(["-v", "error", "-xerror", "-i"])
            .arg(path)
            .args(["-map", "0:v", "-map", "0:a?", "-f", "null", "-"])
            .output()?;
        if !out.status.success() {
            bail!(
                "Saved media failed a complete decode check: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        }
        Ok(p)
    }
    pub fn encoder_probe(&self, name: &str) -> bool {
        let mut child = match command(&self.ffmpeg)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=320x180:rate=30",
                "-frames:v",
                "3",
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                name,
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return false,
        };
        let start = Instant::now();
        loop {
            if let Ok(Some(s)) = child.try_wait() {
                return s.success();
            }
            if start.elapsed() > Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    /// Encode-probe several encoders at once (each is a short FFmpeg run).
    fn probe_all(&self, names: &[String]) -> Vec<bool> {
        thread::scope(|scope| {
            let jobs: Vec<_> = names
                .iter()
                .map(|n| scope.spawn(move || self.encoder_probe(n)))
                .collect();
            jobs.into_iter()
                .map(|j| j.join().unwrap_or(false))
                .collect()
        })
    }
    pub fn encoders(&self) -> Vec<Encoder> {
        let names: Vec<String> = H264_ENCODERS.iter().map(|n| n.to_string()).collect();
        let ok = self.probe_all(&names);
        names
            .into_iter()
            .zip(ok)
            .map(|(name, ok)| Encoder {
                name,
                available: ok,
                reason: if ok {
                    "3-frame encode probe passed"
                } else {
                    "Encoder probe unavailable or failed"
                }
                .into(),
            })
            .collect()
    }
    /// Which codecs this PC can record, and with which hardware encoders.
    /// H.264 always has a software fallback; HEVC and AV1 need a GPU encoder
    /// (NVIDIA RTX 40 and newer, Intel Arc/Core Ultra and AMD RX 7000 for AV1).
    pub fn codecs(&self) -> Vec<CodecSupport> {
        let names: Vec<String> = ["hevc", "av1"]
            .iter()
            .flat_map(|c| HW_FAMILIES.iter().map(move |f| format!("{c}_{f}")))
            .collect();
        let ok = self.probe_all(&names);
        let mut out = vec![CodecSupport {
            codec: "h264".into(),
            encoders: H264_ENCODERS.iter().map(|n| n.to_string()).collect(),
        }];
        for codec in ["hevc", "av1"] {
            out.push(CodecSupport {
                codec: codec.into(),
                encoders: names
                    .iter()
                    .zip(&ok)
                    .filter(|(n, ok)| **ok && codec_of(n) == codec)
                    .map(|(n, _)| n.clone())
                    .collect(),
            });
        }
        out
    }
    /// First encoder that passes its probe: the requested codec on each
    /// encoder, then H.264 (see `codec_candidates`).
    pub fn choose_encoder(&self, requested: &str, codec: &str) -> Result<String> {
        let names: Vec<&str> = if requested == "auto" {
            H264_ENCODERS.to_vec()
        } else {
            vec![requested]
        };
        let mut order: Vec<String> = names.iter().filter_map(|n| with_codec(n, codec)).collect();
        if codec != "h264" {
            order.extend(names.iter().map(|n| n.to_string()));
        }
        for name in order {
            if self.encoder_probe(&name) {
                return Ok(name);
            }
        }
        if requested != "auto" {
            bail!("Selected encoder failed its encode probe")
        }
        bail!("No working video encoder")
    }
    /// Run the exact capture -> filter -> encoder chain for a few frames.
    /// Returns FFmpeg's last error lines when it fails, for diagnostics.
    pub fn probe_video(
        &self,
        s: &Settings,
        scaler: Scaler,
        encoder: &str,
        plan: Option<&VideoPlan>,
    ) -> std::result::Result<(), String> {
        let mut c = command(&self.ffmpeg);
        c.args(["-hide_banner", "-v", "error", "-nostdin"]);
        let filter = video_input(&mut c, s, scaler, false, plan).map_err(|e| e.to_string())?;
        let (ffmpeg_encoder, low_power) = ffmpeg_encoder(encoder);
        if source_in_graph(&filter) {
            c.args(["-filter_complex", &format!("{filter}[v]"), "-map", "[v]"]);
        } else {
            c.args(["-vf", &filter]);
        }
        c.args(["-c:v", ffmpeg_encoder]);
        if low_power {
            c.args(["-low_power", "1"]);
        }
        c.args(["-frames:v", "5", "-f", "null", "-"]);
        let mut child = c
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let stderr = child.stderr.take();
        let reader = thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut e) = stderr {
                use std::io::Read;
                let _ = e.read_to_string(&mut text);
            }
            text
        });
        // Five frames normally take well under a second, first driver init ~1-2 s.
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Ok(Some(status)) = child.try_wait() {
                break Some(status);
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            thread::sleep(Duration::from_millis(20));
        };
        let text = reader.join().unwrap_or_default();
        match status {
            Some(s) if s.success() => Ok(()),
            Some(_) => Err(summarize_error(&text)),
            None => Err("Probe timed out".into()),
        }
    }
    pub fn concat(
        &self,
        segments: &[Segment],
        destination: &Path,
        expected_audio: usize,
    ) -> Result<Probe> {
        self.concat_progress(
            segments,
            destination,
            expected_audio,
            true,
            &[],
            &mut |_| {},
        )
    }
    pub fn concat_progress(
        &self,
        segments: &[Segment],
        destination: &Path,
        expected_audio: usize,
        full_decode: bool,
        tags: &[(String, String)],
        progress: &mut impl FnMut(&str),
    ) -> Result<Probe> {
        let started = Instant::now();
        let mut timings = Vec::new();
        let mut mark = |stage: &str| {
            timings.push(StageTiming {
                stage: stage.into(),
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
            progress(stage);
        };
        mark("Inspecting segments");
        if segments.is_empty() {
            bail!("No finalized footage yet. Wait for the first 2-second segment.")
        }
        // Every segment is still checked (format and container start time),
        // with a minimal probe on up to 8 threads instead of full sequential
        // probes, which dominated save time (about 60 ms per 2 s segment).
        let unknown: Vec<Segment> = segments
            .iter()
            .filter(|s| s.start_time.is_none())
            .cloned()
            .collect();
        let infos = self.segment_infos(&unknown)?;
        if let Some(first) = infos.first() {
            if infos.iter().any(|i| i.layout != first.layout) {
                bail!("Capture format changed between segments. Recover the separate sessions instead of merging incompatible media.")
            }
        }
        let mut probed = infos.into_iter();
        let starts: Vec<f64> = segments
            .iter()
            .map(|s| {
                s.start_time
                    .unwrap_or_else(|| probed.next().map(|i| i.start_time).unwrap_or(0.))
            })
            .collect();
        let parent = segments[0].path.parent().context("No snapshot directory")?;
        let list = parent.join("concat.txt");
        // Snapshot names are generated locally; user-controlled paths never enter concat syntax.
        let spans: Vec<f64> = segments.iter().map(|s| s.end - s.start).collect();
        fs::write(
            &list,
            concat_list(segments, &concat_durations(&spans, &starts))?,
        )?;
        mark("Remuxing");
        let part = destination.with_extension("partial.mkv");
        let output = helper(&self.ffmpeg)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-nostdin",
                "-xerror",
                "-f",
                "concat",
                // Per-file probe options need safe=0; concat_list only writes
                // bare generated names from the snapshot directory.
                "-safe",
                "0",
                "-i",
            ])
            .arg(&list)
            .args(["-map", "0", "-c", "copy", "-avoid_negative_ts", "make_zero"])
            .args(
                tags.iter()
                    .flat_map(|(k, v)| ["-metadata".to_string(), format!("{k}={v}")]),
            )
            .arg("-n")
            .arg(&part)
            .output()?;
        if !output.status.success() {
            bail!(
                "Replay remux failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        }
        mark("Validating");
        let mut verified = if full_decode {
            self.verify(&part, expected_audio)?
        } else {
            self.verify_playable(&part, expected_audio)?
        };
        let expected: f64 = segments.iter().map(|s| s.end - s.start).sum();
        if (verified.duration - expected).abs() > 0.75 + (segments.len() as f64 * 0.05) {
            bail!(
                "Replay duration failed verification: expected about {expected:.2}s, got {:.2}s",
                verified.duration
            )
        }
        fs::rename(&part, destination)?;
        mark("Ready");
        verified.save_timings = timings;
        Ok(verified)
    }
    /// Minimal per-segment facts for concatenation, probed in parallel.
    pub fn segment_infos(&self, segments: &[Segment]) -> Result<Vec<SegmentInfo>> {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let results = std::sync::Mutex::new((0..segments.len()).map(|_| None).collect::<Vec<_>>());
        std::thread::scope(|scope| {
            for _ in 0..segments.len().clamp(1, 8) {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(segment) = segments.get(i) else {
                        break;
                    };
                    let info = self.segment_info(&segment.path);
                    results.lock().unwrap()[i] = Some(info);
                });
            }
        });
        results
            .into_inner()
            .unwrap()
            .into_iter()
            .map(|r| r.unwrap_or_else(|| Err(anyhow::anyhow!("Segment was not probed"))))
            .collect()
    }
    /// Same probe as a below-normal-priority process, for background use
    /// while capture runs.
    pub fn segment_info_low_priority(&self, path: &Path) -> Result<SegmentInfo> {
        self.segment_info_with(path, CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS)
    }
    fn segment_info(&self, path: &Path) -> Result<SegmentInfo> {
        self.segment_info_with(path, CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS)
    }
    fn segment_info_with(&self, path: &Path, flags: u32) -> Result<SegmentInfo> {
        let mut c = Command::new(&self.ffprobe);
        c.creation_flags(flags);
        let out = c
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=start_time:stream=codec_type,codec_name,width,height",
                "-of",
                "json",
            ])
            .arg(path)
            .output()?;
        if !out.status.success() {
            bail!("A replay segment could not be read: {}", path.display())
        }
        let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        let streams = json["streams"]
            .as_array()
            .context("No streams in segment")?;
        let layout = streams
            .iter()
            .map(|s| {
                format!(
                    "{}:{}:{}x{}",
                    s["codec_type"].as_str().unwrap_or(""),
                    s["codec_name"].as_str().unwrap_or(""),
                    s["width"].as_u64().unwrap_or(0),
                    s["height"].as_u64().unwrap_or(0)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        Ok(SegmentInfo {
            start_time: json["format"]["start_time"]
                .as_str()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(0.),
            layout,
        })
    }
    /// The remux has already read every packet. Validate stream layout, duration
    /// and decode both ends before publication. Full decode remains available
    /// for recovery, export and offline acceptance tests, not every live save.
    fn verify_playable(&self, path: &Path, expected_audio: usize) -> Result<Probe> {
        let p = self.probe(path)?;
        if p.audio_tracks != expected_audio || p.width == 0 || p.height == 0 {
            bail!("Saved clip has an unexpected stream layout")
        }
        for at in [0., (p.duration - 1.).max(0.)] {
            let out = helper(&self.ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-xerror",
                    "-threads",
                    "2",
                    "-ss",
                    &format!("{at:.3}"),
                    "-i",
                ])
                .arg(path)
                .args([
                    "-t", "1", "-map", "0:v", "-map", "0:a?", "-threads", "1", "-f", "null", "-",
                ])
                .output()?;
            if !out.status.success() {
                bail!(
                    "Saved clip failed playback validation: {}",
                    String::from_utf8_lossy(&out.stderr)
                )
            }
        }
        Ok(p)
    }
    /// JPEG still from roughly a third of the way into the clip.
    pub fn thumbnail(&self, path: &Path, duration: f64, width: u32) -> Result<Vec<u8>> {
        // Start about a third in (never the black first frames), at most a
        // minute in, then let FFmpeg's `thumbnail` filter pick the most
        // representative of the next 30 frames.
        let at = if duration > 1.0 {
            (duration * 0.35).min(60.0).min(duration - 0.5)
        } else {
            0.0
        };
        let out = helper(&self.ffmpeg)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-nostdin",
                "-ss",
                &format!("{at:.3}"),
                "-i",
            ])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                // Square pixels first (Smooth-mode files carry a pixel aspect
                // ratio), so the still is never squashed.
                &format!("thumbnail=n=30,scale=trunc(iw*sar/2)*2:ih,setsar=1,scale={width}:-2:flags=lanczos"),
                "-q:v",
                "3",
                "-f",
                "image2pipe",
                "-c:v",
                "mjpeg",
                "-",
            ])
            .output()?;
        if !out.status.success() || out.stdout.len() < 100 {
            bail!(
                "Thumbnail failed: {}",
                summarize_error(&String::from_utf8_lossy(&out.stderr))
            )
        }
        Ok(out.stdout)
    }
    pub fn export_mp4(&self, input: &Path, output: &Path) -> Result<Probe> {
        let original = self.probe(input)?;
        let part = output.with_extension("partial.mp4");
        let r = helper(&self.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-nostdin", "-i"])
            .arg(input)
            .args(["-map", "0", "-c", "copy", "-movflags", "+faststart", "-n"])
            // HEVC in MP4 must be tagged hvc1 for Apple devices and some players.
            .args(if original.video_codec == "hevc" {
                &["-tag:v", "hvc1"][..]
            } else {
                &[][..]
            })
            .arg(&part)
            .output()?;
        if !r.status.success() {
            bail!("MP4 remux failed; keep the MKV source")
        }
        let p = self.verify(&part, original.audio_tracks)?;
        fs::rename(part, output)?;
        Ok(p)
    }
}

/// How captured frames are scaled before encoding. Desktop Duplication frames
/// live on the GPU that drives the display; on hybrid laptops that is often the
/// integrated GPU while NVENC is on the discrete GPU, so a D3D11 texture cannot
/// be handed to NVENC directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scaler {
    /// No scaling: Direct3D 11 frames go straight to the encoder, which converts
    /// BGRA to YUV itself (zero copy). Only when the frame needs no resize.
    D3d11,
    /// One frame copy into CUDA, scaled there (BGRA), encoded by NVENC. For
    /// displays driven by the NVIDIA GPU. (FFmpeg's scale_d3d11 cannot create
    /// its output texture in the pinned build, on Intel or NVIDIA.)
    Cuda,
    /// Intel VPP + Quick Sync encode, frames stay on the Intel GPU (zero copy).
    QsvZeroCopy,
    /// Intel VPP scaling, then only the scaled NV12 frame is copied to the encoder.
    QsvHybrid,
    /// Full-resolution download and software scaling.
    Cpu,
    /// Windows Graphics Capture delivers frames to a Direct3D 11 device on
    /// the encoder's GPU (Windows copies across adapters on hybrid laptops),
    /// FFmpeg's gfxcapture scales them there, NVENC encodes them: no CPU copy.
    WgcGpu,
}
impl Scaler {
    pub fn label(self) -> &'static str {
        match self {
            Scaler::D3d11 => "GPU zero-copy (Direct3D 11 → encoder)",
            Scaler::Cuda => "GPU scaling (CUDA) + one frame transfer",
            Scaler::QsvZeroCopy => "GPU zero-copy (Intel Quick Sync)",
            Scaler::QsvHybrid => "GPU scaling (Intel VPP) + scaled-frame transfer",
            Scaler::Cpu => "CPU fallback",
            Scaler::WgcGpu => "GPU zero-copy (Windows Graphics Capture → NVENC)",
        }
    }
    /// Whether the scaler can produce this plan's frame (padding needs system
    /// memory; the direct path cannot resize at all).
    pub fn supports(self, plan: &VideoPlan) -> bool {
        match self {
            Scaler::D3d11 => {
                !plan.padded()
                    && (plan.source.width, plan.source.height) == (plan.width, plan.height)
            }
            Scaler::QsvZeroCopy | Scaler::Cuda | Scaler::WgcGpu => !plan.padded(),
            Scaler::QsvHybrid | Scaler::Cpu => true,
        }
    }
}

/// Ordered scaler/encoder candidates for a requested encoder. The first chain
/// whose real probe succeeds is used; the CPU path is always last.
/// Engine encoder ids map to an FFmpeg encoder plus options. `h264_qsv_lp`
/// is Quick Sync on Intel's fixed-function VDEnc path (`-low_power 1`): it
/// leaves the GPU's shader cores (the "3D" engine, which on hybrid laptops
/// also composites every game frame) free. The default Quick Sync path runs
/// motion estimation on those cores. It is probed first and falls back.
pub fn ffmpeg_encoder(id: &str) -> (&str, bool) {
    match id.strip_suffix("_lp") {
        Some(base) => (base, true),
        None => (id, false),
    }
}
/// Human-readable encoder name for status and diagnostics.
pub fn encoder_label(id: &str) -> String {
    match id.strip_suffix("_lp") {
        Some(base) => format!("{base} (low-power VDEnc)"),
        None => id.into(),
    }
}
/// H.264 encoders in automatic preference order.
pub const H264_ENCODERS: [&str; 5] = ["h264_nvenc", "h264_amf", "h264_qsv", "libopenh264", "mpeg4"];
/// Hardware encoder families that also offer HEVC and AV1.
const HW_FAMILIES: [&str; 3] = ["nvenc", "amf", "qsv"];
#[derive(Debug, Clone, Serialize)]
pub struct CodecSupport {
    pub codec: String,
    /// Encoders that passed an encode probe (H.264 lists every candidate).
    pub encoders: Vec<String>,
}
/// The codec the user asked for. "auto" records H.264, which plays
/// everywhere (Discord, browsers, editors, phones, older TVs); HEVC and AV1
/// are explicit choices, so the codec never changes behind the user's back.
pub fn requested_codec(s: &Settings) -> &'static str {
    match s.codec.as_str() {
        "hevc" => "hevc",
        "av1" => "av1",
        _ => "h264",
    }
}
/// Codec an encoder id produces ("av1_nvenc" -> "av1").
pub fn codec_of(id: &str) -> &'static str {
    if id.starts_with("hevc_") {
        "hevc"
    } else if id.starts_with("av1_") {
        "av1"
    } else if id == "mpeg4" {
        "mpeg4"
    } else {
        "h264"
    }
}
/// The `codec` encoder on the same hardware as an H.264 encoder id
/// (`h264_qsv_lp` -> `hevc_qsv_lp`). The CPU encoders only do H.264.
pub fn with_codec(id: &str, codec: &str) -> Option<String> {
    if codec == "h264" {
        return Some(id.into());
    }
    let hw = id.strip_prefix("h264_")?;
    matches!(hw, "nvenc" | "amf" | "qsv" | "qsv_lp").then(|| format!("{codec}_{hw}"))
}
/// Probe order for a codec: the requested codec on every path first, then
/// H.264 on the same paths. A codec the display's GPU cannot encode falls
/// back to H.264 instead of failing capture; the status reports the codec
/// actually used and the app shows a warning, so it is never silent.
pub fn codec_candidates(paths: &[(Scaler, &str)], codec: &str) -> Vec<(Scaler, String)> {
    let mut out: Vec<(Scaler, String)> = paths
        .iter()
        .filter_map(|(s, e)| with_codec(e, codec).map(|e| (*s, e)))
        .collect();
    if codec != "h264" {
        out.extend(paths.iter().map(|(s, e)| (*s, e.to_string())));
    }
    out
}
pub const VENDOR_INTEL: u32 = 0x8086;
pub const VENDOR_NVIDIA: u32 = 0x10de;
pub const VENDOR_AMD: u32 = 0x1002;
/// GPU paths worth probing, fastest first, for the GPU that drives the
/// display. Paths that cannot work on that GPU are not tried: each failed
/// probe costs a capture start, and Quick Sync probes on a display owned by
/// another GPU could run into their timeout (0.2.1-alpha.3 took ~30 s to start
/// on an NVIDIA-driven display). Unknown vendors get every candidate.
pub fn scaling_candidates(requested: &str, vendor: u32) -> Vec<(Scaler, &'static str)> {
    let all = [
        (VENDOR_NVIDIA, Scaler::D3d11, "h264_nvenc"),
        (VENDOR_NVIDIA, Scaler::WgcGpu, "h264_nvenc"),
        (VENDOR_NVIDIA, Scaler::Cuda, "h264_nvenc"),
        (VENDOR_AMD, Scaler::D3d11, "h264_amf"),
        (VENDOR_INTEL, Scaler::QsvZeroCopy, "h264_qsv_lp"),
        (VENDOR_INTEL, Scaler::QsvZeroCopy, "h264_qsv"),
        (VENDOR_INTEL, Scaler::WgcGpu, "h264_nvenc"),
        (VENDOR_INTEL, Scaler::QsvHybrid, "h264_nvenc"),
        (VENDOR_INTEL, Scaler::QsvHybrid, "h264_amf"),
    ];
    let known = [VENDOR_INTEL, VENDOR_NVIDIA, VENDOR_AMD].contains(&vendor);
    all.into_iter()
        .filter(|(v, _, _)| !known || *v == vendor)
        .filter(|(_, _, e)| requested == "auto" || ffmpeg_encoder(e).0 == requested)
        .map(|(_, s, e)| (s, e))
        .collect()
}

fn summarize_error(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains("non monotonically increasing dts"))
        .collect();
    let first = lines.first().copied().unwrap_or("Probe failed");
    first.chars().take(240).collect()
}

/// Build the filter chain for a scaler. `plan` is required for Desktop Duplication.
/// FFmpeg filter graphs for a capture: video and, when there is audio, a
/// second graph for the audio sources (per-source chain, then a mix and one
/// track per source).
///
/// The video and audio graphs MUST stay separate (`split`, the default): in
/// one shared graph FFmpeg runs both on one thread, and waiting for real-time
/// audio stalled the video chain, which blocked the desktop grabber. With
/// audio, 57% of frames repeated (0.2.1-alpha.3); separate graphs plus NVENC
/// lookahead brought it to 0.0-0.28%. See docs/PERFORMANCE.md.
/// Whether a video filter string starts with its own capture source (no
/// `-i` input for video).
pub fn source_in_graph(video_filter: &str) -> bool {
    video_filter.starts_with("gfxcapture=")
}
pub fn filter_graphs(
    video_filter: &str,
    chains: &[String],
    separate: bool,
    split: bool,
) -> Vec<String> {
    // A capture source inside the graph (WgcGpu) is not input 0.
    let in_graph = source_in_graph(video_filter);
    let first_audio = if in_graph { 0 } else { 1 };
    let video = if in_graph {
        format!("{video_filter}[v]")
    } else {
        format!("[0:v]{video_filter}[v]")
    };
    if chains.is_empty() {
        return vec![video];
    }
    let mut audio = String::new();
    for (n, chain) in chains.iter().enumerate() {
        if n > 0 {
            audio += ";";
        }
        audio += &format!("[{}:a]aresample=async=1000:first_pts=0", n + first_audio);
        if !chain.is_empty() {
            audio += &format!(",{chain}");
        }
        audio += &if separate {
            format!(",asplit=2[m{n}][a{n}]")
        } else {
            format!("[m{n}]")
        };
    }
    audio += ";";
    for n in 0..chains.len() {
        audio += &format!("[m{n}]");
    }
    audio += &format!("amix=inputs={}:normalize=1[mix]", chains.len());
    if split {
        vec![video, audio]
    } else {
        vec![format!("{video};{audio}")]
    }
}
/// Per-source audio processing in FFmpeg, in this order: high-pass, noise
/// reduction, gate, compressor, boost, limiter. Settings are conservative so
/// speech never sounds metallic or pumping; every option is off unless the
/// user turns it on, except the limiter, which runs whenever the source is
/// louder than unity (boost, or volume above 100%) and keeps peaks under
/// -0.5 dBFS instead of clipping. Measured cost: ~0.4% of one core.
pub fn source_chain(source: &crate::model::AudioSource) -> String {
    // Microphone processing (filters, noise suppression, gain, limiter) runs
    // natively in the Voice Enhancer (crate::voice) before samples reach
    // FFmpeg. FFmpeg only limits a source turned above 100% volume.
    if source.volume > 1.0 {
        "alimiter=limit=0.944:attack=4:release=60:level=0".into()
    } else {
        String::new()
    }
}
/// Encoder/capture tuning. `REPLAYX_VIDEO_TUNING` ("key=value,...") overrides
/// the defaults for benchmarking only (see scripts/overhead-bench.py).
#[derive(Debug, Clone)]
pub struct Tuning {
    /// h264_qsv `low_power`: None = FFmpeg default (auto).
    pub qsv_low_power: Option<bool>,
    /// vpp_qsv `scale_mode`: None = auto.
    pub vpp_scale_mode: Option<u8>,
    pub draw_mouse: bool,
    pub qsv_async_depth: Option<u32>,
    /// Raise the encoder process's CPU scheduling class.
    pub cpu_priority: bool,
    /// Raise the encoder process's GPU scheduling class.
    pub gpu_priority: bool,
    /// Profiling-only scheduling experiment; never permits realtime priority.
    pub gpu_priority_class: i32,
    /// Capture at this multiple of the output rate and keep the frame nearest
    /// each output slot (1 = capture at the output rate).
    pub oversample: u32,
    /// Audio input timestamps from arrival (wall clock) instead of sample count.
    pub audio_wallclock: bool,
    /// Muxer interleaving window in microseconds (None = FFmpeg default).
    pub interleave_delta: Option<u64>,
    /// Separate filter graphs (threads) for video and audio.
    pub split_graphs: bool,
    /// Frames queued between the desktop grabber and the video filter graph.
    pub video_queue: Option<u32>,
    /// NVENC rate-control lookahead in frames.
    pub nvenc_lookahead: Option<u32>,
    /// Profiling control only; production keeps spatial AQ enabled.
    pub nvenc_aq: bool,
    /// Experimental capture API for the display path: false = Desktop
    /// Duplication (ddagrab, production), true = Windows Graphics Capture
    /// (FFmpeg gfxcapture). Development/benchmark only.
    pub wgc: bool,
    /// WGC window capture instead of the display: executable-name pattern.
    pub wgc_window: Option<String>,
    /// Keep Windows' capture border (its privacy indicator). Default on.
    pub wgc_border: bool,
    /// WGC capture rate as a multiple of the output rate (the engine raises a
    /// session to 2x when 1x misses slots; see `wgc_rate_multiple`).
    pub wgc_oversample: u32,
    /// Development switch for the WgcGpu path (WGC frames delivered to the
    /// NVIDIA GPU, scaled there, NVENC zero-copy). Off: on the Intel-driven
    /// laptop panel it cost 29.9% game FPS against 5.6% for Intel scaling +
    /// a scaled-frame transfer (Windows copies every full-size frame across
    /// adapters onto the game's GPU). Unmeasured on NVIDIA-driven displays.
    pub wgc_gpu: bool,
    /// WGC minimum update interval in microseconds at 1x. 16 ms, slightly
    /// under one 60 FPS slot: on a 240 Hz display updates land on refresh
    /// boundaries, and a 16.7 ms floor would sometimes slip a refresh.
    pub wgc_floor_us: Option<u32>,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            qsv_low_power: None,
            vpp_scale_mode: None,
            draw_mouse: true,
            qsv_async_depth: None,
            cpu_priority: false,
            gpu_priority: true,
            gpu_priority_class: 4,
            oversample: 1,
            audio_wallclock: false,
            interleave_delta: None,
            split_graphs: true,
            video_queue: None,
            nvenc_lookahead: Some(20),
            nvenc_aq: true,
            wgc: false,
            wgc_window: None,
            wgc_border: true,
            wgc_oversample: 1,
            wgc_gpu: false,
            wgc_floor_us: Some(16_000),
        }
    }
}
pub fn tuning() -> Tuning {
    parse_tuning(&std::env::var("REPLAYX_VIDEO_TUNING").unwrap_or_default())
}
fn parse_tuning(spec: &str) -> Tuning {
    let mut t = Tuning::default();
    {
        for (k, v) in spec.split(',').filter_map(|kv| kv.split_once('=')) {
            match k.trim() {
                "qsv_low_power" => t.qsv_low_power = v.parse::<u8>().ok().map(|v| v == 1),
                "vpp_scale_mode" => t.vpp_scale_mode = v.parse().ok(),
                "draw_mouse" => t.draw_mouse = v != "0",
                "qsv_async_depth" => t.qsv_async_depth = v.parse().ok(),
                "cpu_priority" => t.cpu_priority = v != "0",
                "gpu_priority" => t.gpu_priority = v != "0",
                "gpu_priority_class" => {
                    t.gpu_priority_class = v.parse::<i32>().unwrap_or(4).clamp(2, 4)
                }
                "oversample" => t.oversample = v.parse::<u32>().unwrap_or(1).clamp(1, 8),
                "audio_wallclock" => t.audio_wallclock = v != "0",
                "interleave_delta" => t.interleave_delta = v.parse().ok(),
                "split_graphs" => t.split_graphs = v != "0",
                "video_queue" => t.video_queue = v.parse().ok(),
                "nvenc_lookahead" => t.nvenc_lookahead = v.parse().ok(),
                "nvenc_aq" => t.nvenc_aq = v != "0",
                "capture_api" => t.wgc = v.trim() == "wgc",
                // Executable names only: no characters that end the filter option.
                "wgc_window"
                    if !v.is_empty()
                        && v.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) =>
                {
                    t.wgc_window = Some(v.trim().to_string())
                }
                "wgc_border" => t.wgc_border = v != "0",
                "wgc_gpu" => t.wgc_gpu = v != "0",
                "wgc_floor_us" => {
                    t.wgc_floor_us = v
                        .parse::<u32>()
                        .ok()
                        .filter(|&us| (4000..=33334).contains(&us))
                }
                "wgc_oversample" => t.wgc_oversample = v.parse::<u32>().unwrap_or(2).clamp(1, 4),
                _ => {}
            }
        }
    }
    t
}
/// Quick Sync VPP converts RGB to limited-range BT.709 YUV (the matching tags
/// are written by `encoding_args`), instead of a driver-chosen matrix.
const QSV_709: &str = ":out_color_matrix=bt709:out_range=1";
/// Tags for BGRA frames handed to the encoder. NVENC converts RGB input with
/// the BT.601 matrix into limited range, whatever the frame says (measured on
/// this laptop for H.264, HEVC and AV1, from system memory, CUDA and D3D11
/// frames: red -> Y 81, Cb 90, Cr 239; BT.709 would be 63/102/240). Decoding
/// that as BT.709 turned pure green 192 into 159. The colours are sRGB, so
/// primaries and transfer stay BT.709; only the matrix is BT.601 (SMPTE 170M).
/// AMF follows the frame's matrix tag, so the same tags are correct there.
const BGRA_TAGS: &str =
    "setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=smpte170m";
/// Whether this scaler's RGB→YUV conversion is BT.709 limited range.
pub fn converts_bt709(scaler: Scaler) -> bool {
    !matches!(scaler, Scaler::D3d11 | Scaler::Cuda | Scaler::WgcGpu)
}
pub fn video_filter(plan: &VideoPlan, scaler: Scaler) -> String {
    let t = tuning();
    let vpp_mode = t
        .vpp_scale_mode
        .map(|m| format!(":scale_mode={m}"))
        .unwrap_or_default();
    let (iw, ih) = (plan.image_width, plan.image_height);
    let pad = plan
        .pad_filter()
        .map(|p| format!(",{p}"))
        .unwrap_or_default();
    match scaler {
        // BGRA frames go to the encoder, which converts them to limited-range
        // YUV (see BGRA_TAGS): say so on the frames so the stream and
        // container are tagged correctly.
        Scaler::D3d11 | Scaler::WgcGpu => {
            format!("setsar={}/{},{BGRA_TAGS}", plan.sar.0, plan.sar.1)
        }
        Scaler::Cuda => format!(
            "hwdownload,format=bgra,hwupload_cuda,scale_cuda=w={iw}:h={ih}:interp_algo=bilinear,setsar=1,{BGRA_TAGS}"
        ),
        Scaler::QsvZeroCopy => {
            format!("hwmap=derive_device=qsv,format=qsv,vpp_qsv=w={iw}:h={ih}:format=nv12{vpp_mode}{QSV_709},setsar=1")
        }
        Scaler::QsvHybrid => format!(
            "hwmap=derive_device=qsv,format=qsv,vpp_qsv=w={iw}:h={ih}:format=nv12{vpp_mode}{QSV_709},hwdownload,format=nv12{pad},setsar=1"
        ),
        Scaler::Cpu => format!(
            "hwdownload,format=bgra,scale={iw}:{ih}:flags=bilinear:out_color_matrix=bt709:out_range=tv{pad},setsar=1,format=yuv420p"
        ),
    }
}

/// Quantize acquisition time without pushing a burst into future slots.
/// CFR selects/drops colliding frames and repeats missing slots. The previous
/// max(previous+slot, timestamp) accumulated phase error after stalls/bursts,
/// concealing drops while moving video ahead of its monotonic capture clock.
pub fn regrid_filter(fps: u32) -> String {
    format!("setpts=round(PTS*TB*{fps})/(TB*{fps})")
}

/// CPU filter for sources without a precomputed plan (GDI capture, synthetic).
pub fn expression_filter(width: u32, height: u32, mode: &str) -> String {
    let (w, h) = (width, height);
    match mode {
        "stretch" => format!("scale={w}:{h}:flags=bilinear,setsar=1,format=yuv420p"),
        "fit" => format!(
            "scale={w}:{h}:flags=bilinear:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black,setsar=1,format=yuv420p"
        ),
        "crop" => format!(
            "scale={w}:{h}:flags=bilinear:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1,format=yuv420p"
        ),
        _ => format!("scale=-2:{h}:flags=bilinear,setsar=1,format=yuv420p"),
    }
}

pub fn video_input(
    c: &mut Command,
    s: &Settings,
    scaler: Scaler,
    synthetic: bool,
    plan: Option<&VideoPlan>,
) -> Result<String> {
    if synthetic {
        c.args([
            "-re",
            "-f",
            "lavfi",
            "-i",
            &format!(
                "color=c=black:s=854x480:r={},drawbox=color=white:t=fill:enable='lt(mod(t,1),0.1)'",
                s.fps
            ),
        ]);
        return Ok(expression_filter(
            s.output_width(),
            s.height,
            &s.canvas_mode,
        ));
    }
    if s.capture == "desktop-duplication" {
        let plan = plan.context("Missing validated display geometry")?;
        let rect = plan.source;
        if let Some(q) = tuning().video_queue {
            c.args(["-thread_queue_size", &q.to_string()]);
        }
        let t = tuning();
        let wgc = capture_api(s) == "wgc";
        let oversample = if wgc {
            wgc_rate_multiple(s, &t)
        } else {
            t.oversample
        };
        let timing = if oversample > 1 {
            // Several captures per output slot: keep the one nearest each slot.
            format!("fps={}:round=near", s.fps)
        } else {
            regrid_filter(s.fps)
        };
        if wgc && scaler == Scaler::WgcGpu {
            // The source lives in the filter graph so it can use a device on
            // the encoder's GPU (a lavfi input cannot be given one).
            let index = crate::geometry::adapter_index(VENDOR_NVIDIA)
                .context("No NVIDIA adapter for Windows Graphics Capture → NVENC")?;
            c.args([
                "-init_hw_device",
                &format!("d3d11va=wgc:{index}"),
                "-filter_hw_device",
                "wgc",
            ]);
            let source = wgc_source(s, &t, rect, Some((plan.width, plan.height)))?;
            return Ok(format!("{source},{timing},{}", video_filter(plan, scaler)));
        }
        let source = if wgc {
            wgc_source(s, &t, rect, None)?
        } else {
            format!(
                "ddagrab=output_idx={}:framerate={}:draw_mouse={}:video_size={}x{}:offset_x={}:offset_y={}",
                s.display, s.fps * t.oversample, u8::from(t.draw_mouse), rect.width, rect.height, rect.x, rect.y
            )
        };
        c.args(["-f", "lavfi", "-i", &source]);
        return Ok(format!("{},{}", timing, video_filter(plan, scaler)));
    }
    c.args(["-f", "gdigrab", "-framerate", &s.fps.to_string(), "-i"]);
    c.arg(if s.capture == "window" {
        format!("title={}", s.window_title)
    } else {
        "desktop".into()
    });
    Ok(expression_filter(
        s.output_width(),
        s.height,
        &s.canvas_mode,
    ))
}
/// The display capture API a start uses: the setting, or the benchmark
/// override `REPLAYX_VIDEO_TUNING=capture_api=wgc`.
pub fn capture_api(s: &Settings) -> &'static str {
    if s.capture_api == "wgc" || tuning().wgc {
        "wgc"
    } else {
        "dda"
    }
}
/// WGC capture rate as a multiple of the output rate. Each delivered frame
/// costs a full-screen copy in Windows and in gfxcapture: at about 550 game
/// FPS, 2x cost 7.0-8.0% game FPS and 1x 3.6-3.9% (synthetic, 0% repeated
/// frames either way). But Windows' update floor makes a game between 60 and
/// 120 FPS arrive below 60/s at 1x (18% repeated frames at ~95 FPS), so the
/// engine switches a session to 2x when it sees that (`Settings::wgc_boost`).
pub fn wgc_rate_multiple(s: &Settings, t: &Tuning) -> u32 {
    if s.wgc_boost {
        t.oversample.max(2)
    } else {
        t.oversample.max(t.wgc_oversample)
    }
}
/// Windows Graphics Capture source (experimental). Frames are
/// Direct3D 11 BGRA textures like ddagrab's, so the same filters and encoders
/// apply. WGC delivers a frame only when the content changes, at most
/// `max_framerate` per second; the output's CFR mode repeats frames to fill
/// slots, as it does for Desktop Duplication stalls.
fn wgc_source(
    s: &Settings,
    t: &Tuning,
    rect: crate::geometry::CaptureRect,
    scale_to: Option<(u32, u32)>,
) -> Result<String> {
    // At 1x, an update floor slightly under one output slot (wgc_floor_us)
    // keeps Windows from slipping a whole display refresh past the slot.
    let rate = match (t.wgc_floor_us, wgc_rate_multiple(s, t)) {
        (Some(us), 1) => format!("1000000/{us}"),
        (_, n) => (s.fps * n).to_string(),
    };
    let mut common = format!(
        "max_framerate={}:capture_cursor={}:display_border={}",
        rate,
        u8::from(t.draw_mouse),
        u8::from(t.wgc_border)
    );
    if let Some((w, h)) = scale_to {
        // gfxcapture scales on the GPU to the encoded size (stretch).
        common += &format!(":width={w}:height={h}:resize_mode=scale");
    }
    if let Some(exe) = &t.wgc_window {
        return Ok(format!(
            "gfxcapture=window_exe=(?i){exe}:capture_border=0:{common}"
        ));
    }
    let display = crate::geometry::display(s.display)?;
    let (w, h) = (display.desktop_width, display.desktop_height);
    Ok(format!(
        "gfxcapture=hmonitor={}:crop_left={}:crop_top={}:crop_right={}:crop_bottom={}:{common}",
        crate::geometry::monitor_handle(s.display)?,
        rect.x,
        rect.y,
        w.saturating_sub(rect.x + rect.width),
        h.saturating_sub(rect.y + rect.height)
    ))
}
/// Video bitrate for a preset at an output size and frame rate. Anchored at
/// 1080p60: Performance 16, Balanced 28, High Quality 45 Mb/s (NVIDIA's own
/// recorder defaults to 50 Mb/s for 1080p60 "High"), scaled by pixel count
/// and sub-linearly by frame rate (more frames per second means each frame
/// differs less). Custom uses the user's bitrate as is.
pub fn target_bitrate_mbps(s: &Settings, width: u32, height: u32) -> u32 {
    if s.preset == "custom" {
        return s.bitrate_mbps;
    }
    // Matched to what NVIDIA's own recorder produced on the same laptop
    // (about 16.5 Mb/s for 1440x1080 at 60 fps); NVENC lookahead and adaptive
    // quantization spend those bits better than the previous 28 Mb/s did.
    let base = match s.preset.as_str() {
        "performance" => 10.0,
        "quality" => 25.0,
        _ => 16.0,
    };
    let pixels = (width as f64 * height as f64) / (1920.0 * 1080.0);
    let rate = (s.fps as f64 / 60.0).powf(0.75);
    (base * pixels.powf(0.9) * rate).round().clamp(4.0, 150.0) as u32
}
/// How a preset spends bits on one encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RatePlan {
    /// NVENC constant-quality level (lower is better); None for plain VBR.
    pub cq: Option<u32>,
    /// Average for VBR; for constant quality the expected average in typical
    /// gameplay (measured), used for size estimates.
    pub target_mbps: u32,
    /// Hard peak: bounds file size and the replay buffer in complex scenes.
    pub cap_mbps: u32,
}
/// Bits a codec needs relative to H.264 for the same picture quality,
/// measured with VMAF on NVENC (RTX 4070 Laptop) on real Roblox gameplay
/// and a synthetic game-like scene: AV1 about 25% fewer; HEVC within +-3%
/// of H.264 with these settings, so it gets no discount (docs/PERFORMANCE.md).
fn codec_factor(codec: &str) -> f64 {
    if codec == "av1" {
        0.75
    } else {
        1.0
    }
}
/// NVENC constant-quality levels per preset. H.264 CQ 28 on real gameplay
/// averaged 14.7 Mb/s at VMAF 97.6, matching the old 16 Mb/s VBR (97.9) at
/// 12% less; AV1 CQ 34 gave VMAF 97.7 at 12.5 Mb/s (docs/PERFORMANCE.md).
fn nvenc_cq(codec: &str, preset: &str) -> u32 {
    match (codec, preset) {
        ("av1", "performance") => 38,
        ("av1", "quality") => 31,
        ("av1", _) => 34,
        (_, "performance") => 31,
        (_, "quality") => 25,
        _ => 28,
    }
}
/// Rate control for a preset on an encoder at an output size. NVENC uses
/// constant quality capped at 1.5x the preset rate: simple scenes cost
/// less, complex ones get bits up to the cap. Other encoders use VBR around
/// the preset rate. Custom is the user's rate, whatever the codec.
pub fn rate_plan(s: &Settings, encoder: &str, width: u32, height: u32) -> RatePlan {
    let h264 = target_bitrate_mbps(s, width, height);
    if s.preset == "custom" {
        return RatePlan {
            cq: None,
            target_mbps: h264,
            cap_mbps: h264 * 3 / 2,
        };
    }
    let codec = codec_of(encoder);
    let k = codec_factor(codec);
    RatePlan {
        cq: ffmpeg_encoder(encoder)
            .0
            .ends_with("_nvenc")
            .then(|| nvenc_cq(codec, &s.preset)),
        target_mbps: ((h264 as f64) * k).round().max(2.0) as u32,
        cap_mbps: ((h264 as f64) * 1.5 * k).round().max(3.0) as u32,
    }
}
/// Encoder arguments. `size` is the encoded frame size; `bt709` says the
/// scaler converted with BT.709 limited range, so the stream is tagged so.
pub fn encoding_args(c: &mut Command, s: &Settings, encoder: &str, size: (u32, u32), bt709: bool) {
    let (ffmpeg_encoder, low_power) = ffmpeg_encoder(encoder);
    let codec = codec_of(encoder);
    let hw = ffmpeg_encoder.rsplit('_').next().unwrap_or_default();
    let nvenc = hw == "nvenc";
    let rate = rate_plan(s, encoder, size.0, size.1);
    c.args([
        "-c:v",
        ffmpeg_encoder,
        "-r",
        &s.fps.to_string(),
        "-enc_time_base",
        &format!("1:{}", s.fps),
    ]);
    match rate.cq {
        // Constant quality with a peak cap (as OBS recommends for recordings).
        Some(cq) => c.args([
            "-rc",
            "vbr",
            "-cq",
            &cq.to_string(),
            "-b:v",
            "0",
            "-maxrate",
            &format!("{}k", rate.cap_mbps * 1000),
            "-bufsize",
            &format!("{}k", rate.cap_mbps * 2000),
        ]),
        // VBR around the target with a 1.5x peak: action scenes get more bits
        // than menus without unbounded replay-buffer growth.
        None => c.args([
            "-b:v",
            &format!("{}M", rate.target_mbps),
            "-maxrate",
            &format!("{}k", rate.cap_mbps * 1000),
            "-bufsize",
            &format!("{}k", rate.target_mbps * 2000),
        ]),
    };
    c.args(["-g", &(s.fps * 2).to_string(), "-bf", "0"]);
    // Segments must start with IDR frames so each 2s file decodes on its own and
    // the segment muxer can split; QSV otherwise emits only one IDR per stream.
    match hw {
        "nvenc" => {
            c.args(["-forced-idr", "1"]);
        }
        "qsv" | "amf" => {
            c.args(["-forced_idr", "1"]);
        }
        _ => {}
    }
    if nvenc {
        // NVENC is dedicated hardware, but RGB conversion, AQ and lookahead
        // can also consume GPU resources shared with rendering.
        c.args([
            "-preset",
            match s.preset.as_str() {
                "performance" => "p3",
                "quality" => "p6",
                _ => "p5",
            },
            "-tune",
            "hq",
            "-spatial-aq",
            if tuning().nvenc_aq { "1" } else { "0" },
        ]);
        // Rate-control lookahead: better bit allocation, and the encoder holds
        // ~1/3 s of video, so video never runs ahead of the (deliberately
        // held-back) audio. Ahead by more than FFmpeg's scheduling tolerance,
        // the desktop grabber was paused and 57% of frames repeated.
        if let Some(n) = tuning().nvenc_lookahead {
            c.args(["-rc-lookahead", &n.to_string()]);
        }
    }
    if hw == "amf" {
        c.args([
            "-quality",
            match s.preset.as_str() {
                "performance" => "speed",
                "quality" => "quality",
                _ => "balanced",
            },
        ]);
    }
    if hw == "qsv" {
        let t = tuning();
        match t.qsv_low_power {
            Some(lp) => {
                c.args(["-low_power", if lp { "1" } else { "0" }]);
            }
            None if low_power => {
                c.args(["-low_power", "1"]);
            }
            None => {}
        }
        if let Some(d) = t.qsv_async_depth {
            c.args(["-async_depth", &d.to_string()]);
        }
        c.args([
            "-preset",
            match s.preset.as_str() {
                "performance" => "veryfast",
                "quality" => "slow",
                _ => "medium",
            },
        ]);
    }
    if !bt709 && nvenc {
        // NVENC converts BGRA input to limited-range BT.601-matrix YUV (see
        // BGRA_TAGS) and may describe it as full-range RGB. Write the real
        // colour description into the bitstream.
        c.args([
            "-bsf:v",
            &match codec {
                "av1" => "av1_metadata=color_range=tv:color_primaries=1:transfer_characteristics=1:matrix_coefficients=6".to_string(),
                c => format!("{c}_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=6"),
            },
        ]);
    }
    if bt709 {
        c.args([
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_range",
            "tv",
        ]);
    }
    c.args([
        "-force_key_frames",
        "expr:gte(t,n_forced*2)",
        "-c:a",
        "aac",
        "-b:a",
        &format!("{}k", s.audio_bitrate_kbps),
        "-ar",
        "48000",
    ]);
}

/// Container-level tag, matched case-insensitively (Matroska may uppercase).
fn format_tag(json: &serde_json::Value, key: &str) -> Option<String> {
    json["format"]["tags"]
        .as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_str())
        .map(str::to_owned)
        .filter(|v| !v.is_empty())
}

/// Concat script for segments in one directory. Each file gets minimal
/// probing: the concat demuxer otherwise runs full stream detection on every
/// 2 s segment (about 30 ms each), which dominated long saves (10 min: 10.0 s
/// -> 3.9 s remux, identical packets).
pub fn concat_list(segments: &[Segment], durations: &[f64]) -> Result<String> {
    let mut out = String::new();
    for (s, duration) in segments.iter().zip(durations) {
        let name = s
            .path
            .file_name()
            .context("Segment has no file name")?
            .to_string_lossy();
        if name.contains(['\'', '\\', '/', ':', '\n', '\r']) || name.starts_with('.') {
            bail!("Unexpected segment file name")
        }
        out += &format!(
            "file '{name}'\noption probesize 32\noption analyzeduration 0\nduration {duration:.6}\n"
        );
    }
    Ok(out)
}

/// Concat-list durations that keep segment seams gapless. The concat demuxer
/// shifts each file by its own container start time, so a segment whose start
/// is not zero (the first segment of a session starts after AAC priming) must
/// declare `span + next_start - start`; declaring only the span leaves a hole.
pub fn concat_durations(spans: &[f64], starts: &[f64]) -> Vec<f64> {
    spans
        .iter()
        .enumerate()
        .map(|(i, span)| span + starts.get(i + 1).copied().unwrap_or(0.) - starts[i])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{plan, CaptureRect};

    fn args_for(encoder: &str) -> Vec<String> {
        let mut c = Command::new("ffmpeg");
        encoding_args(&mut c, &Settings::default(), encoder, (1920, 1080), true);
        c.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn media_lock_is_pinned_to_an_immutable_reviewed_release() {
        let lock: serde_json::Value = serde_json::from_str(
            include_str!("../../tools/ffmpeg-lock.json").trim_start_matches('\u{feff}'),
        )
        .unwrap();
        let url = lock["browser_download_url"].as_str().unwrap();
        assert!(url.starts_with("https://github.com/"));
        assert!(!url.contains("/download/latest/"), "floating asset: {url}");
        assert!(url.contains("/download/autobuild-"));
        let digest = lock["digest"].as_str().unwrap();
        assert!(digest.starts_with("sha256:") && digest.len() == 7 + 64);
        assert!(lock["runtime_dir"].as_str().unwrap().ends_with("/bin"));
        assert!(!lock["reviewed"]
            .as_str()
            .unwrap_or("NOT REVIEWED")
            .starts_with("NOT REVIEWED"));
    }

    #[test]
    fn display_capture_regrids_timestamps_before_scaling() {
        let source = CaptureRect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1600,
        };
        let p = plan(source, 1920, 1080, "stretch").unwrap();
        let mut c = Command::new("ffmpeg");
        let s = Settings {
            fps: 60,
            ..Settings::default()
        };
        let f = video_input(&mut c, &s, Scaler::QsvZeroCopy, false, Some(&p)).unwrap();
        assert!(f.starts_with("setpts=round(PTS*TB*60)/(TB*60),"));
        assert!(f.contains(",hwmap=derive_device=qsv"));
        let input: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(input
            .iter()
            .any(|a| a.contains("ddagrab=output_idx=0:framerate=60")
                && a.contains("video_size=2560x1600")));
    }

    #[test]
    fn hardware_encoders_force_idr_segment_starts() {
        // Regression: h264_qsv emitted a single IDR, so 2s segments never closed.
        for (encoder, flag) in [
            ("h264_qsv", "-forced_idr"),
            ("h264_amf", "-forced_idr"),
            ("h264_nvenc", "-forced-idr"),
        ] {
            let args = args_for(encoder);
            let i = args.iter().position(|a| a == flag).expect(encoder);
            assert_eq!(args[i + 1], "1");
            assert!(args.iter().any(|a| a == "expr:gte(t,n_forced*2)"));
        }
    }

    #[test]
    fn candidates_follow_the_gpu_that_drives_the_display() {
        // Intel-driven display (hybrid laptop desktop): Quick Sync first,
        // low-power before shader-assisted, then a transfer to NVENC/AMF.
        let intel = scaling_candidates("auto", VENDOR_INTEL);
        assert_eq!(intel[0], (Scaler::QsvZeroCopy, "h264_qsv_lp"));
        let qsv = intel.iter().position(|c| c.1 == "h264_qsv").unwrap();
        let hybrid = intel.iter().position(|c| c.0 == Scaler::QsvHybrid).unwrap();
        assert!(qsv < hybrid);
        assert!(intel
            .iter()
            .all(|c| c.0 != Scaler::D3d11 && c.0 != Scaler::Cuda));
        // NVIDIA-driven display (desktop PC, or Advanced Optimus during a
        // full-screen game): NVENC only, zero copy first; no Quick Sync probes.
        // WgcGpu (GPU scaling inside Windows Graphics Capture) is offered
        // only when WGC is the capture method (select_video_path skips it).
        let nvidia = scaling_candidates("auto", VENDOR_NVIDIA);
        assert_eq!(
            nvidia,
            vec![
                (Scaler::D3d11, "h264_nvenc"),
                (Scaler::WgcGpu, "h264_nvenc"),
                (Scaler::Cuda, "h264_nvenc")
            ]
        );
        assert_eq!(
            scaling_candidates("auto", VENDOR_AMD),
            vec![(Scaler::D3d11, "h264_amf")]
        );
        // Unknown vendor: everything; an explicit encoder filters the list.
        assert_eq!(scaling_candidates("auto", 0).len(), 9);
        assert!(scaling_candidates("h264_nvenc", 0)
            .iter()
            .all(|c| c.1 == "h264_nvenc"));
        assert!(scaling_candidates("h264_qsv", VENDOR_NVIDIA).is_empty());
        for r in [
            "auto",
            "h264_nvenc",
            "h264_qsv",
            "h264_amf",
            "libopenh264",
            "mpeg4",
        ] {
            for v in [0, VENDOR_INTEL, VENDOR_NVIDIA, VENDOR_AMD] {
                assert!(scaling_candidates(r, v).iter().all(|c| c.0 != Scaler::Cpu));
            }
        }
    }
    #[test]
    fn direct_encoding_only_without_resizing() {
        let full = |w, h| CaptureRect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        // 1440x1080 stretched gameplay recorded as 1920x1080 needs scaling.
        let stretched = plan(full(1440, 1080), 1920, 1080, "stretch").unwrap();
        assert!(!Scaler::D3d11.supports(&stretched));
        assert!(Scaler::Cuda.supports(&stretched));
        // A 1920x1080 display recorded at 1920x1080 goes straight to NVENC.
        let same = plan(full(1920, 1080), 1920, 1080, "stretch").unwrap();
        assert!(Scaler::D3d11.supports(&same));
        let fit = plan(full(2560, 1600), 1920, 1080, "fit").unwrap();
        assert!(!Scaler::Cuda.supports(&fit) && Scaler::Cpu.supports(&fit));
        assert!(video_filter(&same, Scaler::D3d11).starts_with("setsar=1/1,setparams=range=tv"));
        assert!(video_filter(&stretched.anamorphic(), Scaler::D3d11).starts_with("setsar=4/3,"));
        assert!(video_filter(&stretched, Scaler::Cuda).contains("scale_cuda=w=1920:h=1080"));
        assert!(!converts_bt709(Scaler::Cuda) && converts_bt709(Scaler::QsvZeroCopy));
    }

    #[test]
    fn letterboxed_plans_skip_paths_that_cannot_pad_on_the_gpu() {
        let source = CaptureRect {
            x: 0,
            y: 0,
            width: 1440,
            height: 1080,
        };
        let fit = plan(source, 1920, 1080, "fit").unwrap();
        assert!(!Scaler::D3d11.supports(&fit) && !Scaler::QsvZeroCopy.supports(&fit));
        assert!(Scaler::QsvHybrid.supports(&fit) && Scaler::Cpu.supports(&fit));
        assert!(video_filter(&fit, Scaler::QsvHybrid).contains("pad=1920:1080:240:0:black"));
        assert!(video_filter(&fit, Scaler::Cpu).contains("pad=1920:1080:240:0:black"));
        let stretch = plan(source, 1920, 1080, "stretch").unwrap();
        assert!(Scaler::QsvZeroCopy.supports(&stretch));
        let f = video_filter(&stretch, Scaler::QsvZeroCopy);
        assert!(f.contains("vpp_qsv=w=1920:h=1080") && !f.contains("hwdownload"));
        assert!(video_filter(&stretch, Scaler::Cpu)
            .starts_with("hwdownload,format=bgra,scale=1920:1080"));
    }

    #[test]
    fn concat_list_uses_only_bare_generated_names() {
        let seg = |p: &str| Segment {
            path: PathBuf::from(p),
            start: 0.,
            end: 2.,
            bytes: 1,
            origin: None,
            start_time: Some(0.),
        };
        let list = concat_list(
            &[
                seg(r"C:\saves\x\00000000.mkv"),
                seg(r"C:\saves\x\00000001.mkv"),
            ],
            &[2.0, 2.0],
        )
        .unwrap();
        assert_eq!(list.matches("file '").count(), 2);
        assert!(list.contains(
            "file '00000000.mkv'\noption probesize 32\noption analyzeduration 0\nduration 2.000000\n"
        ));
        assert!(!list.contains("saves"));
        assert!(concat_list(&[seg(r"C:\x\a'b.mkv")], &[2.0]).is_err());
    }

    #[test]
    fn first_segment_priming_offset_leaves_no_seam_gap() {
        // Observed: session segment 0 starts at 0.021s (AAC priming), others at 0.
        let spans = [2.021, 2.0, 2.0];
        let starts = [0.021, 0.0, 0.0];
        let durations = concat_durations(&spans, &starts);
        assert!((durations[0] - 2.0).abs() < 1e-9);
        // Segment 0 audio ends at 2.026s in its own timeline, i.e. 2.005s after the
        // demuxer shift; segment 1 audio begins 0.005s into its file.
        let segment0_audio_end = 2.026 - starts[0];
        let segment1_audio_start = durations[0] + 0.005 - starts[1];
        assert!((segment0_audio_end - segment1_audio_start).abs() < 1e-9);
        assert_eq!(&durations[1..], &[2.0, 2.0]);
    }
    #[test]
    fn low_power_quick_sync_is_tried_first() {
        let auto = scaling_candidates("auto", VENDOR_INTEL);
        let lp = auto.iter().position(|c| c.1 == "h264_qsv_lp").unwrap();
        let normal = auto.iter().position(|c| c.1 == "h264_qsv").unwrap();
        assert!(lp < normal);
        assert_eq!(ffmpeg_encoder("h264_qsv_lp"), ("h264_qsv", true));
        assert_eq!(ffmpeg_encoder("h264_nvenc"), ("h264_nvenc", false));
        let mut c = command(Path::new("ffmpeg"));
        encoding_args(
            &mut c,
            &Settings::default(),
            "h264_qsv_lp",
            (1920, 1080),
            true,
        );
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into()).collect();
        let pair = |k: &str, v: &str| args.windows(2).any(|w| w[0] == k && w[1] == v);
        assert!(pair("-c:v", "h264_qsv") && pair("-low_power", "1"));
        assert!(pair("-colorspace", "bt709") && pair("-color_range", "tv"));
        assert!(pair("-b:v", "16M") && pair("-maxrate", "24000k"));
        assert!(!pair("-cq", "28"), "Quick Sync keeps VBR");
        let mut c = command(Path::new("ffmpeg"));
        encoding_args(
            &mut c,
            &Settings::default(),
            "h264_qsv",
            (1920, 1080),
            false,
        );
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into()).collect();
        assert!(!args.iter().any(|a| a == "-low_power" || a == "-colorspace"));
    }
    #[test]
    fn preset_bitrates_follow_resolution_and_frame_rate() {
        let at = |preset: &str, fps: u32, w: u32, h: u32| {
            target_bitrate_mbps(
                &Settings {
                    preset: preset.into(),
                    fps,
                    ..Default::default()
                },
                w,
                h,
            )
        };
        assert_eq!(at("performance", 60, 1920, 1080), 10);
        assert_eq!(at("balanced", 60, 1920, 1080), 16);
        assert_eq!(at("quality", 60, 1920, 1080), 25);
        assert!(at("balanced", 120, 1920, 1080) > 16 && at("balanced", 120, 1920, 1080) < 32);
        assert!(at("balanced", 60, 1280, 720) < 10);
        assert!(at("quality", 60, 3840, 2160) > 60);
        let custom = Settings {
            preset: "custom".into(),
            bitrate_mbps: 33,
            ..Default::default()
        };
        assert_eq!(target_bitrate_mbps(&custom, 1920, 1080), 33);
    }
    #[test]
    fn windows_graphics_capture_is_opt_in_and_window_names_are_sanitized() {
        // Production stays on Desktop Duplication; WGC is opt-in (setting or
        // benchmark switch).
        assert_eq!(Settings::default().capture_api, "dda");
        if std::env::var_os("REPLAYX_VIDEO_TUNING").is_none() {
            assert_eq!(capture_api(&Settings::default()), "dda");
            let opted = Settings {
                capture_api: "wgc".into(),
                ..Default::default()
            };
            assert_eq!(capture_api(&opted), "wgc");
        }
        let d = Tuning::default();
        assert!(!d.wgc && d.wgc_window.is_none() && d.wgc_border);
        let t = parse_tuning("capture_api=wgc,wgc_window=replayx-gameproxy.exe,wgc_border=0");
        assert!(t.wgc && !t.wgc_border);
        assert_eq!(t.wgc_window.as_deref(), Some("replayx-gameproxy.exe"));
        // Characters that would end or extend the filter option are refused.
        for bad in ["a:b", "x'y", "a[b]", "a;b", r"a\b"] {
            assert!(
                parse_tuning(&format!("wgc_window={bad}"))
                    .wgc_window
                    .is_none(),
                "{bad}"
            );
        }
        let s = Settings {
            fps: 60,
            ..Default::default()
        };
        let rect = crate::geometry::CaptureRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let f = wgc_source(&s, &t, rect, None).unwrap();
        assert!(f.starts_with("gfxcapture=window_exe=(?i)replayx-gameproxy.exe:"));
        // 1x with a 16 ms update floor by default; 2x once a session is boosted.
        assert!(f.contains("max_framerate=1000000/16000") && f.contains("display_border=0"));
        let boosted = Settings {
            fps: 60,
            wgc_boost: true,
            ..Default::default()
        };
        assert!(wgc_source(&boosted, &t, rect, None)
            .unwrap()
            .contains("max_framerate=120"));
        let forced = parse_tuning("capture_api=wgc,wgc_window=game.exe,wgc_oversample=2");
        assert!(wgc_source(&s, &forced, rect, None)
            .unwrap()
            .contains("max_framerate=120"));
        assert!(f.contains("capture_cursor=1"));
    }

    #[test]
    fn capture_source_inside_the_graph_numbers_audio_from_zero() {
        let src = "gfxcapture=hmonitor=1:width=1920:height=1080:resize_mode=scale,fps=60:round=near,setsar=1/1";
        assert!(source_in_graph(src) && !source_in_graph("setsar=1/1"));
        let g = filter_graphs(src, &["".into(), "".into()], true, true);
        assert_eq!(g[0], format!("{src}[v]"));
        assert!(g[1].contains("[0:a]") && g[1].contains("[1:a]") && !g[1].contains("[2:a]"));
        // Ordinary inputs keep video as input 0 and audio from 1.
        let g = filter_graphs("setsar=1", &["".into()], true, true);
        assert!(g[0].starts_with("[0:v]") && g[1].contains("[1:a]"));
        // NVENC on an Intel-driven display: WgcGpu is listed, but
        // select_video_path skips it unless wgc_gpu=1 (measured slower).
        assert!(!Tuning::default().wgc_gpu);
        let intel = scaling_candidates("h264_nvenc", VENDOR_INTEL);
        assert_eq!(
            intel,
            vec![
                (Scaler::WgcGpu, "h264_nvenc"),
                (Scaler::QsvHybrid, "h264_nvenc")
            ]
        );
        assert!(!converts_bt709(Scaler::WgcGpu));
    }

    #[test]
    fn video_and_audio_never_share_a_filter_graph() {
        // Regression guard for the 0.2.1-alpha.3 stutter fix (57% -> 0.28%).
        assert!(Tuning::default().split_graphs);
        let g = filter_graphs("setsar=1", &["".into(), "".into()], true, true);
        assert_eq!(g.len(), 2);
        assert!(g[0].starts_with("[0:v]") && !g[0].contains(":a]"));
        assert!(g[1].contains("[1:a]") && g[1].contains("[2:a]") && g[1].contains("amix=inputs=2"));
        assert!(!g[1].contains("[0:v]"));
        assert_eq!(
            filter_graphs("setsar=1", &[], true, true),
            vec!["[0:v]setsar=1[v]".to_string()]
        );
        // Per-source processing sits before the split, so mix and track agree.
        let g = filter_graphs("x", &["highpass=f=80".into()], true, true);
        assert!(g[1].contains("first_pts=0,highpass=f=80,asplit=2[m0][a0]"));
    }
    #[test]
    fn codecs_map_onto_the_same_hardware_and_fall_back_to_h264() {
        assert_eq!(
            with_codec("h264_qsv_lp", "hevc").as_deref(),
            Some("hevc_qsv_lp")
        );
        assert_eq!(
            with_codec("h264_nvenc", "av1").as_deref(),
            Some("av1_nvenc")
        );
        assert_eq!(with_codec("libopenh264", "av1"), None);
        assert_eq!(ffmpeg_encoder("hevc_qsv_lp"), ("hevc_qsv", true));
        assert_eq!(codec_of("av1_nvenc"), "av1");
        assert_eq!(codec_of("h264_qsv_lp"), "h264");
        let paths = [(Scaler::D3d11, "h264_nvenc"), (Scaler::Cuda, "h264_nvenc")];
        let order = codec_candidates(&paths, "av1");
        let names: Vec<&str> = order.iter().map(|c| c.1.as_str()).collect();
        assert_eq!(
            names,
            ["av1_nvenc", "av1_nvenc", "h264_nvenc", "h264_nvenc"]
        );
        assert_eq!(codec_candidates(&paths, "h264").len(), 2);
        // "auto" never picks anything but H.264.
        let s = Settings::default();
        assert_eq!(s.codec, "auto");
        assert_eq!(requested_codec(&s), "h264");
    }
    #[test]
    fn nvenc_uses_capped_constant_quality_per_codec() {
        let s = Settings::default();
        let h264 = rate_plan(&s, "h264_nvenc", 1920, 1080);
        assert_eq!(
            h264,
            RatePlan {
                cq: Some(28),
                target_mbps: 16,
                cap_mbps: 24
            }
        );
        let av1 = rate_plan(&s, "av1_nvenc", 1920, 1080);
        assert_eq!(
            av1,
            RatePlan {
                cq: Some(34),
                target_mbps: 12,
                cap_mbps: 18
            }
        );
        assert_eq!(rate_plan(&s, "h264_qsv", 1920, 1080).cq, None);
        let custom = Settings {
            preset: "custom".into(),
            bitrate_mbps: 20,
            ..Default::default()
        };
        assert_eq!(
            rate_plan(&custom, "av1_nvenc", 1920, 1080),
            RatePlan {
                cq: None,
                target_mbps: 20,
                cap_mbps: 30
            }
        );
        for (encoder, bsf) in [
            ("h264_nvenc", "h264_metadata="),
            ("hevc_nvenc", "hevc_metadata="),
            ("av1_nvenc", "av1_metadata="),
        ] {
            let mut c = command(Path::new("ffmpeg"));
            encoding_args(&mut c, &s, encoder, (1920, 1080), false);
            let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into()).collect();
            let pair = |k: &str, v: &str| args.windows(2).any(|w| w[0] == k && w[1] == v);
            assert!(
                pair("-c:v", encoder)
                    && pair("-rc", "vbr")
                    && pair("-b:v", "0")
                    && pair("-forced-idr", "1"),
                "{encoder}"
            );
            assert!(
                pair("-rc-lookahead", "20") && pair("-spatial-aq", "1"),
                "{encoder}"
            );
            assert!(
                args.iter()
                    .any(|a| a.starts_with(bsf) && a.contains("matrix_coefficients=6")),
                "{encoder}"
            );
        }
    }
    /// Real encode: NVENC's RGB conversion must match the matrix we tag.
    /// Skipped on PCs without NVENC.
    #[test]
    fn nvenc_colour_tags_match_its_conversion() {
        let Ok(m) = Media::discover() else { return };
        if !m.encoder_probe("h264_nvenc") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("replayx-colour-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for encoder in ["h264_nvenc", "hevc_nvenc", "av1_nvenc"] {
            if !m.encoder_probe(encoder) {
                continue;
            }
            let out = dir.join(format!("{encoder}.mkv"));
            let mut c = command(&m.ffmpeg);
            c.args([
                "-hide_banner",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=0x00C000:s=1920x1080:r=60",
                "-frames:v",
                "3",
                "-vf",
            ]);
            c.arg(format!("format=bgra,{BGRA_TAGS}"));
            encoding_args(&mut c, &Settings::default(), encoder, (1920, 1080), false);
            c.args(["-an"]).arg(&out);
            assert!(c.status().unwrap().success(), "{encoder}");
            // Decode with the tagged matrix: green must come back as green.
            let rgb = command(&m.ffmpeg)
                .args(["-hide_banner", "-v", "error", "-i"])
                .arg(&out)
                .args([
                    "-frames:v",
                    "1",
                    "-vf",
                    "scale=w=16:h=16:in_color_matrix=bt601:in_range=tv,format=rgb24",
                    "-f",
                    "rawvideo",
                    "-",
                ])
                .output()
                .unwrap()
                .stdout;
            let px = &rgb[(8 * 16 + 8) * 3..(8 * 16 + 8) * 3 + 3];
            assert!(
                px[0] < 8 && (184..=200).contains(&px[1]) && px[2] < 8,
                "{encoder}: {px:?}"
            );
            let tags = command(&m.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v",
                    "-show_entries",
                    "stream=color_space,color_range,color_primaries",
                    "-of",
                    "csv=p=0",
                ])
                .arg(&out)
                .output()
                .unwrap()
                .stdout;
            let tags = String::from_utf8_lossy(&tags);
            assert!(
                tags.contains("tv")
                    && (tags.contains("smpte170m") || tags.contains("bt470bg"))
                    && tags.contains("bt709"),
                "{encoder}: {tags}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn nvenc_keeps_lookahead_so_video_never_runs_ahead_of_audio() {
        // Second half of the stutter fix: the encoder holds ~1/3 s of video.
        assert_eq!(Tuning::default().nvenc_lookahead, Some(20));
        let mut c = command(Path::new("ffmpeg"));
        encoding_args(
            &mut c,
            &Settings::default(),
            "h264_nvenc",
            (1920, 1080),
            false,
        );
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into()).collect();
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-rc-lookahead" && w[1] == "20"));
    }
    #[test]
    fn ffmpeg_only_limits_loud_sources_voice_processing_is_native() {
        use crate::model::AudioSource;
        let mic = AudioSource {
            kind: "microphone".into(),
            boost_db: 12.0,
            ..Default::default()
        };
        assert_eq!(
            source_chain(&mic),
            "",
            "boost and processing run in crate::voice"
        );
        let over = AudioSource {
            volume: 1.5,
            ..mic.clone()
        };
        assert!(
            source_chain(&over).starts_with("alimiter"),
            "above 100% is limited, not clipped"
        );
        let bad = crate::model::Settings {
            audio: vec![AudioSource {
                boost_db: 30.0,
                ..mic
            }],
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }
}
