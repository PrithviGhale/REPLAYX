use crate::{
    buffer::Segment,
    geometry::{self, VideoPlan},
    model::Settings,
};
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
    pub bytes: u64,
    pub video_codec: String,
    pub audio_tracks: usize,
    pub width: u64,
    pub height: u64,
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
            width: video["width"].as_u64().unwrap_or(0),
            height: video["height"].as_u64().unwrap_or(0),
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
        c.args([
            "-vf",
            &filter,
            "-c:v",
            encoder,
            "-frames:v",
            "5",
            "-f",
            "null",
            "-",
        ]);
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
        let deadline = Instant::now() + Duration::from_secs(8);
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
        if segments.is_empty() {
            bail!("No finalized footage yet. Wait for the first 2-second segment.")
        }
        let first = self.probe(&segments[0].path)?;
        let mut starts = vec![first.start_time];
        for s in segments.iter().skip(1) {
            let p = self.probe(&s.path)?;
            starts.push(p.start_time);
            if p.width != first.width
                || p.height != first.height
                || p.video_codec != first.video_codec
                || p.audio_tracks != first.audio_tracks
            {
                bail!("Capture format changed between segments. Recover the separate sessions instead of merging incompatible media.")
            }
        }
        let parent = segments[0].path.parent().context("No snapshot directory")?;
        let list = parent.join("concat.txt");
        // Snapshot names are generated locally; user-controlled paths never enter concat syntax.
        let spans: Vec<f64> = segments.iter().map(|s| s.end - s.start).collect();
        let lines = segments
            .iter()
            .zip(concat_durations(&spans, &starts))
            .map(|(s, duration)| {
                format!(
                    "file '{}'\nduration {:.6}\n",
                    s.path.file_name().unwrap().to_string_lossy(),
                    duration
                )
            })
            .collect::<String>();
        fs::write(&list, lines)?;
        let part = destination.with_extension("partial.mkv");
        let output = command(&self.ffmpeg)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-nostdin",
                "-f",
                "concat",
                "-safe",
                "1",
                "-i",
            ])
            .arg(&list)
            .args([
                "-map",
                "0",
                "-c",
                "copy",
                "-avoid_negative_ts",
                "make_zero",
                "-n",
            ])
            .arg(&part)
            .output()?;
        if !output.status.success() {
            bail!(
                "Replay remux failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        }
        let verified = self.verify(&part, expected_audio)?;
        let expected: f64 = segments.iter().map(|s| s.end - s.start).sum();
        if (verified.duration - expected).abs() > 0.75 + (segments.len() as f64 * 0.05) {
            bail!(
                "Replay duration failed verification: expected about {expected:.2}s, got {:.2}s",
                verified.duration
            )
        }
        fs::rename(&part, destination)?;
        Ok(verified)
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
    /// Direct3D 11 video processor, frames stay on the display GPU (zero copy).
    D3d11,
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
            Scaler::D3d11 => "GPU zero-copy (Direct3D 11)",
            Scaler::QsvZeroCopy => "GPU zero-copy (Intel Quick Sync)",
            Scaler::QsvHybrid => "GPU scaling (Intel VPP) + scaled-frame transfer",
            Scaler::Cpu => "CPU fallback",
        }
    }
    /// Whether the scaler can produce this plan's frame (padding needs system memory).
    pub fn supports(self, plan: &VideoPlan) -> bool {
        !matches!(self, Scaler::D3d11 | Scaler::QsvZeroCopy) || !plan.padded()
    }
}

/// Ordered scaler/encoder candidates for a requested encoder. The first chain
/// whose real probe succeeds is used; the CPU path is always last.
pub fn scaling_candidates(requested: &str) -> Vec<(Scaler, &'static str)> {
    match requested {
        "auto" => vec![
            (Scaler::D3d11, "h264_nvenc"),
            (Scaler::D3d11, "h264_amf"),
            (Scaler::QsvZeroCopy, "h264_qsv"),
            (Scaler::QsvHybrid, "h264_nvenc"),
            (Scaler::QsvHybrid, "h264_amf"),
        ],
        "h264_nvenc" => vec![
            (Scaler::D3d11, "h264_nvenc"),
            (Scaler::QsvHybrid, "h264_nvenc"),
        ],
        "h264_amf" => vec![(Scaler::D3d11, "h264_amf"), (Scaler::QsvHybrid, "h264_amf")],
        "h264_qsv" => vec![(Scaler::QsvZeroCopy, "h264_qsv")],
        _ => vec![],
    }
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
pub fn video_filter(plan: &VideoPlan, scaler: Scaler) -> String {
    let (iw, ih) = (plan.image_width, plan.image_height);
    let pad = plan
        .pad_filter()
        .map(|p| format!(",{p}"))
        .unwrap_or_default();
    match scaler {
        Scaler::D3d11 => format!("scale_d3d11=width={iw}:height={ih}:format=nv12,setsar=1"),
        Scaler::QsvZeroCopy => {
            format!("hwmap=derive_device=qsv,format=qsv,vpp_qsv=w={iw}:h={ih}:format=nv12,setsar=1")
        }
        Scaler::QsvHybrid => format!(
            "hwmap=derive_device=qsv,format=qsv,vpp_qsv=w={iw}:h={ih}:format=nv12,hwdownload,format=nv12{pad},setsar=1"
        ),
        Scaler::Cpu => format!(
            "hwdownload,format=bgra,scale={iw}:{ih}:flags=bilinear{pad},setsar=1,format=yuv420p"
        ),
    }
}

/// Desktop Duplication stamps frames with DWM present times. On high-refresh
/// displays two frames often fall in one output slot and the next slot is
/// empty, so CFR conversion drops one and duplicates another (visible judder).
/// Snap each frame to its nearest slot, never earlier than one slot after the
/// previous frame: every captured frame is kept and output stays on real time.
pub fn regrid_filter(fps: u32) -> String {
    let slot = format!("round(PTS*TB*{fps})/(TB*{fps})");
    format!("setpts='if(isnan(PREV_OUTPTS),{slot},max(PREV_OUTPTS+1/(TB*{fps}),{slot}))'")
}

/// CPU filter for sources without a precomputed plan (GDI capture, synthetic).
pub fn expression_filter(height: u32, mode: &str) -> String {
    let (w, h) = geometry::canvas(height);
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
        return Ok(expression_filter(s.height, &s.canvas_mode));
    }
    if s.capture == "desktop-duplication" {
        let plan = plan.context("Missing validated display geometry")?;
        let rect = plan.source;
        c.args([
            "-f",
            "lavfi",
            "-i",
            &format!(
                "ddagrab=output_idx={}:framerate={}:draw_mouse=1:video_size={}x{}:offset_x={}:offset_y={}",
                s.display, s.fps, rect.width, rect.height, rect.x, rect.y
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
    Ok(expression_filter(s.height, &s.canvas_mode))
}
pub fn encoding_args(c: &mut Command, s: &Settings, encoder: &str) {
    c.args([
        "-c:v",
        encoder,
        "-b:v",
        &format!("{}M", s.bitrate_mbps),
        "-g",
        &(s.fps * 2).to_string(),
        "-bf",
        "0",
    ]);
    // Segments must start with IDR frames so each 2s file decodes on its own and
    // the segment muxer can split; QSV otherwise emits only one IDR per stream.
    match encoder {
        "h264_nvenc" => {
            c.args(["-forced-idr", "1"]);
        }
        "h264_qsv" | "h264_amf" => {
            c.args(["-forced_idr", "1"]);
        }
        _ => {}
    }
    if encoder == "h264_nvenc" {
        c.args([
            "-preset",
            match s.preset.as_str() {
                "performance" => "p1",
                "quality" => "p6",
                _ => "p4",
            },
            "-tune",
            "ll",
        ]);
    }
    if encoder == "h264_amf" {
        c.args([
            "-quality",
            match s.preset.as_str() {
                "performance" => "speed",
                "quality" => "quality",
                _ => "balanced",
            },
        ]);
    }
    if encoder == "h264_qsv" {
        c.args([
            "-preset",
            match s.preset.as_str() {
                "performance" => "veryfast",
                "quality" => "slow",
                _ => "medium",
            },
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
        encoding_args(&mut c, &Settings::default(), encoder);
        c.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn display_capture_regrids_timestamps_before_scaling() {
        let source = CaptureRect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1600,
        };
        let p = plan(source, 1080, "stretch").unwrap();
        let mut c = Command::new("ffmpeg");
        let s = Settings {
            fps: 60,
            ..Settings::default()
        };
        let f = video_input(&mut c, &s, Scaler::QsvZeroCopy, false, Some(&p)).unwrap();
        assert!(f.starts_with(
            "setpts='if(isnan(PREV_OUTPTS),round(PTS*TB*60)/(TB*60),max(PREV_OUTPTS+1/(TB*60)"
        ));
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
    fn zero_copy_paths_are_tried_before_transfers_and_cpu_is_never_a_candidate() {
        let auto = scaling_candidates("auto");
        assert_eq!(auto[0], (Scaler::D3d11, "h264_nvenc"));
        let qsv = auto
            .iter()
            .position(|c| c.0 == Scaler::QsvZeroCopy)
            .unwrap();
        let hybrid = auto.iter().position(|c| c.0 == Scaler::QsvHybrid).unwrap();
        assert!(qsv < hybrid);
        assert!(scaling_candidates("h264_nvenc")
            .iter()
            .all(|c| c.1 == "h264_nvenc"));
        for r in [
            "auto",
            "h264_nvenc",
            "h264_qsv",
            "h264_amf",
            "libopenh264",
            "mpeg4",
        ] {
            assert!(scaling_candidates(r).iter().all(|c| c.0 != Scaler::Cpu));
        }
    }

    #[test]
    fn letterboxed_plans_skip_paths_that_cannot_pad_on_the_gpu() {
        let source = CaptureRect {
            x: 0,
            y: 0,
            width: 1440,
            height: 1080,
        };
        let fit = plan(source, 1080, "fit").unwrap();
        assert!(!Scaler::D3d11.supports(&fit) && !Scaler::QsvZeroCopy.supports(&fit));
        assert!(Scaler::QsvHybrid.supports(&fit) && Scaler::Cpu.supports(&fit));
        assert!(video_filter(&fit, Scaler::QsvHybrid).contains("pad=1920:1080:240:0:black"));
        assert!(video_filter(&fit, Scaler::Cpu).contains("pad=1920:1080:240:0:black"));
        let stretch = plan(source, 1080, "stretch").unwrap();
        assert!(Scaler::QsvZeroCopy.supports(&stretch));
        let f = video_filter(&stretch, Scaler::QsvZeroCopy);
        assert!(f.contains("vpp_qsv=w=1920:h=1080") && !f.contains("hwdownload"));
        assert!(video_filter(&stretch, Scaler::Cpu)
            .starts_with("hwdownload,format=bgra,scale=1920:1080"));
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
}
