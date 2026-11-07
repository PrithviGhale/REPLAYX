use crate::{
    buffer::Segment,
    geometry::{self, DisplayGeometry},
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
    pub fn gpu_scaling_works(
        &self,
        s: &Settings,
        encoder: &str,
        geometry: Option<&DisplayGeometry>,
    ) -> bool {
        if s.capture != "desktop-duplication" || encoder != "h264_nvenc" {
            return false;
        }
        let mut c = command(&self.ffmpeg);
        c.args(["-v", "error"]);
        let Ok(filter) = video_input(&mut c, s, encoder, false, true, geometry) else {
            return false;
        };
        let Ok(mut child) = c
            .args([
                "-vf",
                &filter,
                "-c:v",
                encoder,
                "-frames:v",
                "3",
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(Some(result)) = child.try_wait() {
                return result.success();
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            thread::sleep(Duration::from_millis(20));
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

pub fn video_input(
    c: &mut Command,
    s: &Settings,
    encoder: &str,
    synthetic: bool,
    gpu_scaling: bool,
    geometry: Option<&DisplayGeometry>,
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
        return Ok(geometry::scale_filter(s.height, &s.canvas_mode, false));
    }
    if s.capture == "desktop-duplication" {
        let geometry = geometry.context("Missing validated display geometry")?;
        let rect = geometry.region(s.source_rect)?;
        c.args([
            "-f",
            "lavfi",
            "-i",
            &format!(
                "ddagrab=output_idx={}:framerate={}:draw_mouse=1:video_size={}x{}:offset_x={}:offset_y={}",
                s.display, s.fps,rect.width,rect.height,rect.x,rect.y
            ),
        ]);
        if encoder == "h264_nvenc" && gpu_scaling {
            return Ok(geometry::scale_filter(s.height, &s.canvas_mode, true));
        }
        return Ok(format!(
            "hwdownload,format=bgra,{}",
            geometry::scale_filter(s.height, &s.canvas_mode, false)
        ));
    }
    c.args(["-f", "gdigrab", "-framerate", &s.fps.to_string(), "-i"]);
    c.arg(if s.capture == "window" {
        format!("title={}", s.window_title)
    } else {
        "desktop".into()
    });
    Ok(geometry::scale_filter(s.height, &s.canvas_mode, false))
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
        "192k",
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
