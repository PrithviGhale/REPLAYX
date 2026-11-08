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
    c.creation_flags(0x08000000);
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
        let out = command(&self.ffprobe)
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
        let out = command(&self.ffmpeg)
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
    pub fn encoders(&self) -> Vec<Encoder> {
        ["h264_nvenc", "h264_amf", "h264_qsv", "libopenh264", "mpeg4"]
            .iter()
            .map(|n| {
                let ok = self.encoder_probe(n);
                Encoder {
                    name: n.to_string(),
                    available: ok,
                    reason: if ok {
                        "3-frame encode probe passed"
                    } else {
                        "Encoder probe unavailable or failed"
                    }
                    .into(),
                }
            })
            .collect()
    }
    pub fn choose_encoder(&self, requested: &str) -> Result<String> {
        if requested != "auto" {
            if self.encoder_probe(requested) {
                return Ok(requested.into());
            }
            bail!("Selected encoder failed its encode probe")
        }
        for name in ["h264_nvenc", "h264_amf", "h264_qsv", "libopenh264", "mpeg4"] {
            if self.encoder_probe(name) {
                return Ok(name.into());
            }
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
        c.args(["-vf", &filter, "-c:v", ffmpeg_encoder]);
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
        let output = command(&self.ffmpeg)
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
        self.segment_info_with(path, 0x0800_0000 | 0x0000_4000)
    }
    fn segment_info(&self, path: &Path) -> Result<SegmentInfo> {
        self.segment_info_with(path, 0x0800_0000)
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
            let out = command(&self.ffmpeg)
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
        let at = (duration / 3.).clamp(0., 5.);
        let out = command(&self.ffmpeg)
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
                &format!("scale={width}:-2"),
                "-q:v",
                "4",
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
        let r = command(&self.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-nostdin", "-i"])
            .arg(input)
            .args(["-map", "0", "-c", "copy", "-movflags", "+faststart", "-n"])
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
}
impl Scaler {
    pub fn label(self) -> &'static str {
        match self {
            Scaler::D3d11 => "GPU zero-copy (Direct3D 11 → encoder)",
            Scaler::Cuda => "GPU scaling (CUDA) + one frame transfer",
            Scaler::QsvZeroCopy => "GPU zero-copy (Intel Quick Sync)",
            Scaler::QsvHybrid => "GPU scaling (Intel VPP) + scaled-frame transfer",
            Scaler::Cpu => "CPU fallback",
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
            Scaler::QsvZeroCopy | Scaler::Cuda => !plan.padded(),
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
    match id {
        "h264_qsv_lp" => ("h264_qsv", true),
        other => (other, false),
    }
}
/// Human-readable encoder name for status and diagnostics.
pub fn encoder_label(id: &str) -> String {
    match id {
        "h264_qsv_lp" => "h264_qsv (low-power VDEnc)".into(),
        other => other.into(),
    }
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
        (VENDOR_NVIDIA, Scaler::Cuda, "h264_nvenc"),
        (VENDOR_AMD, Scaler::D3d11, "h264_amf"),
        (VENDOR_INTEL, Scaler::QsvZeroCopy, "h264_qsv_lp"),
        (VENDOR_INTEL, Scaler::QsvZeroCopy, "h264_qsv"),
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
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            qsv_low_power: None,
            vpp_scale_mode: None,
            draw_mouse: true,
            qsv_async_depth: None,
        }
    }
}
pub fn tuning() -> Tuning {
    let mut t = Tuning::default();
    if let Ok(spec) = std::env::var("REPLAYX_VIDEO_TUNING") {
        for (k, v) in spec.split(',').filter_map(|kv| kv.split_once('=')) {
            match k.trim() {
                "qsv_low_power" => t.qsv_low_power = v.parse::<u8>().ok().map(|v| v == 1),
                "vpp_scale_mode" => t.vpp_scale_mode = v.parse().ok(),
                "draw_mouse" => t.draw_mouse = v != "0",
                "qsv_async_depth" => t.qsv_async_depth = v.parse().ok(),
                _ => {}
            }
        }
    }
    t
}
/// Quick Sync VPP converts RGB to limited-range BT.709 YUV (the matching tags
/// are written by `encoding_args`), instead of a driver-chosen matrix.
const QSV_709: &str = ":out_color_matrix=bt709:out_range=1";
/// Whether this scaler's RGB→YUV conversion is BT.709 limited range.
pub fn converts_bt709(scaler: Scaler) -> bool {
    !matches!(scaler, Scaler::D3d11 | Scaler::Cuda)
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
        Scaler::D3d11 => "setsar=1".to_string(),
        Scaler::Cuda => format!(
            "hwdownload,format=bgra,hwupload_cuda,scale_cuda=w={iw}:h={ih}:interp_algo=bilinear,setsar=1"
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
        c.args([
            "-f",
            "lavfi",
            "-i",
            &format!(
                "ddagrab=output_idx={}:framerate={}:draw_mouse={}:video_size={}x{}:offset_x={}:offset_y={}",
                s.display, s.fps, u8::from(tuning().draw_mouse), rect.width, rect.height, rect.x, rect.y
            ),
        ]);
        return Ok(format!(
            "{},{}",
            regrid_filter(s.fps),
            video_filter(plan, scaler)
        ));
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
/// Video bitrate for a preset at an output size and frame rate. Anchored at
/// 1080p60: Performance 16, Balanced 28, High Quality 45 Mb/s (NVIDIA's own
/// recorder defaults to 50 Mb/s for 1080p60 "High"), scaled by pixel count
/// and sub-linearly by frame rate (more frames per second means each frame
/// differs less). Custom uses the user's bitrate as is.
pub fn target_bitrate_mbps(s: &Settings, width: u32, height: u32) -> u32 {
    if s.preset == "custom" {
        return s.bitrate_mbps;
    }
    let base = match s.preset.as_str() {
        "performance" => 16.0,
        "quality" => 45.0,
        _ => 28.0,
    };
    let pixels = (width as f64 * height as f64) / (1920.0 * 1080.0);
    let rate = (s.fps as f64 / 60.0).powf(0.75);
    (base * pixels.powf(0.9) * rate).round().clamp(4.0, 150.0) as u32
}
/// Encoder arguments. `size` is the encoded frame size; `bt709` says the
/// scaler converted with BT.709 limited range, so the stream is tagged so.
pub fn encoding_args(c: &mut Command, s: &Settings, encoder: &str, size: (u32, u32), bt709: bool) {
    let (ffmpeg_encoder, low_power) = ffmpeg_encoder(encoder);
    let mbps = target_bitrate_mbps(s, size.0, size.1);
    // VBR around the target with a 1.5x peak: action scenes get more bits than
    // menus without unbounded replay-buffer growth.
    c.args([
        "-c:v",
        ffmpeg_encoder,
        "-r",
        &s.fps.to_string(),
        "-enc_time_base",
        &format!("1:{}", s.fps),
        "-b:v",
        &format!("{mbps}M"),
        "-maxrate",
        &format!("{}k", mbps * 1500),
        "-bufsize",
        &format!("{}k", mbps * 2000),
        "-g",
        &(s.fps * 2).to_string(),
        "-bf",
        "0",
    ]);
    // Segments must start with IDR frames so each 2s file decodes on its own and
    // the segment muxer can split; QSV otherwise emits only one IDR per stream.
    match ffmpeg_encoder {
        "h264_nvenc" => {
            c.args(["-forced-idr", "1"]);
        }
        "h264_qsv" | "h264_amf" => {
            c.args(["-forced_idr", "1"]);
        }
        _ => {}
    }
    if ffmpeg_encoder == "h264_nvenc" {
        // NVENC runs on its own engine; presets trade encoder time, not game GPU time.
        c.args([
            "-preset",
            match s.preset.as_str() {
                "performance" => "p2",
                "quality" => "p5",
                _ => "p4",
            },
            "-tune",
            if s.preset == "quality" { "hq" } else { "ll" },
        ]);
        if s.preset == "quality" {
            c.args(["-spatial-aq", "1"]);
        }
    }
    if ffmpeg_encoder == "h264_amf" {
        c.args([
            "-quality",
            match s.preset.as_str() {
                "performance" => "speed",
                "quality" => "quality",
                _ => "balanced",
            },
        ]);
    }
    if ffmpeg_encoder == "h264_qsv" {
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
        let nvidia = scaling_candidates("auto", VENDOR_NVIDIA);
        assert_eq!(
            nvidia,
            vec![(Scaler::D3d11, "h264_nvenc"), (Scaler::Cuda, "h264_nvenc")]
        );
        assert_eq!(
            scaling_candidates("auto", VENDOR_AMD),
            vec![(Scaler::D3d11, "h264_amf")]
        );
        // Unknown vendor: everything; an explicit encoder filters the list.
        assert_eq!(scaling_candidates("auto", 0).len(), 7);
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
        assert_eq!(video_filter(&same, Scaler::D3d11), "setsar=1");
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
        assert!(pair("-b:v", "28M") && pair("-maxrate", "42000k"));
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
        assert_eq!(at("performance", 60, 1920, 1080), 16);
        assert_eq!(at("balanced", 60, 1920, 1080), 28);
        assert_eq!(at("quality", 60, 1920, 1080), 45);
        assert!(at("balanced", 120, 1920, 1080) > 28 && at("balanced", 120, 1920, 1080) < 56);
        assert!(at("balanced", 60, 1280, 720) < 16);
        assert!(at("quality", 60, 3840, 2160) > 100);
        let custom = Settings {
            preset: "custom".into(),
            bitrate_mbps: 33,
            ..Default::default()
        };
        assert_eq!(target_bitrate_mbps(&custom, 1920, 1080), 33);
    }
}
