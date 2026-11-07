use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSource {
    pub kind: String,
    pub device: String,
    pub pid: u32,
    pub process_started: u64,
    pub name: String,
    pub enabled: bool,
    pub volume: f32,
    pub muted: bool,
}
impl Default for AudioSource {
    fn default() -> Self {
        Self {
            kind: "system".into(),
            device: String::new(),
            pid: 0,
            process_started: 0,
            name: "System audio".into(),
            enabled: true,
            volume: 1.0,
            muted: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub capture: String,
    pub display: u32,
    pub window_title: String,
    pub height: u32,
    pub canvas_mode: String,
    pub source_rect: Option<crate::geometry::CaptureRect>,
    pub fps: u32,
    pub encoder: String,
    pub preset: String,
    pub bitrate_mbps: u32,
    pub replay_seconds: u32,
    pub buffer_mb: u64,
    pub output_dir: PathBuf,
    pub audio: Vec<AudioSource>,
    pub separate_tracks: bool,
    pub replay_hotkey: String,
    pub recording_hotkey: String,
    pub mute_hotkey: String,
    pub pause_hotkey: String,
    pub close_to_tray: bool,
    pub diagnostics: bool,
    pub check_updates: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            capture: "desktop-duplication".into(),
            display: 0,
            window_title: String::new(),
            height: 1080,
            canvas_mode: "native".into(),
            source_rect: None,
            fps: 30,
            encoder: "auto".into(),
            preset: "balanced".into(),
            bitrate_mbps: 12,
            replay_seconds: 60,
            buffer_mb: 2048,
            output_dir: PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
                .join("Videos/REPLAYX"),
            audio: vec![AudioSource::default()],
            separate_tracks: true,
            replay_hotkey: "Control+Shift+F10".into(),
            recording_hotkey: "Control+Shift+F9".into(),
            mute_hotkey: "Control+Shift+M".into(),
            pause_hotkey: "Control+Shift+F8".into(),
            close_to_tray: false,
            diagnostics: false,
            check_updates: false,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !["native", "stretch"].contains(&self.canvas_mode.as_str()) {
            bail!("Unknown canvas mode")
        }
        if let Some(rect) = self.source_rect {
            if self.capture != "desktop-duplication" {
                bail!("A manual region requires display capture")
            };
            rect.validate(32768, 32768)?;
        }
        if ![480, 720, 1080, 1440, 2160].contains(&self.height) || ![30, 60].contains(&self.fps) {
            bail!("Unsupported resolution or frame rate")
        }
        if ![30, 60, 180, 300, 600, 900, 1800].contains(&self.replay_seconds) {
            bail!("Invalid replay duration")
        }
        if !(128..=102400).contains(&self.buffer_mb) || !(1..=100).contains(&self.bitrate_mbps) {
            bail!("Invalid disk budget or bitrate")
        }
        if !["desktop-duplication", "desktop", "window"].contains(&self.capture.as_str()) {
            bail!("Unsupported capture mode")
        }
        if ![
            "auto",
            "h264_nvenc",
            "h264_amf",
            "h264_qsv",
            "libopenh264",
            "mpeg4",
        ]
        .contains(&self.encoder.as_str())
        {
            bail!("Unsupported encoder")
        }
        if !["balanced", "quality", "performance"].contains(&self.preset.as_str()) {
            bail!("Invalid preset")
        }
        if !self.output_dir.is_absolute() {
            bail!("Recording directory must be absolute")
        }
        if self.window_title.contains(['\n', '\r']) || self.window_title.len() > 512 {
            bail!("Invalid window title")
        }
        if self.audio.len() > 5 {
            bail!("At most five audio sources are supported")
        }
        let enabled: Vec<_> = self.audio.iter().filter(|s| s.enabled).collect();
        if enabled
            .iter()
            .any(|s| s.kind == "system" || s.kind == "exclude")
            && enabled.iter().any(|s| s.kind == "application")
        {
            bail!("System/exclusion and application tracks cannot be combined: this would duplicate app audio. Use application tracks plus microphone.")
        }
        for s in &self.audio {
            if !["system", "microphone", "application", "exclude"].contains(&s.kind.as_str())
                || !s.volume.is_finite()
                || !(0.0..=2.0).contains(&s.volume)
            {
                bail!("Invalid audio source")
            }
            if ["application", "exclude"].contains(&s.kind.as_str())
                && (s.pid == 0 || s.process_started == 0)
            {
                bail!("Choose a running process with a validated start time")
            }
        }
        let keys = [
            &self.replay_hotkey,
            &self.recording_hotkey,
            &self.mute_hotkey,
            &self.pause_hotkey,
        ];
        for (i, k) in keys.iter().enumerate() {
            if k.is_empty() || keys[..i].iter().any(|x| x.eq_ignore_ascii_case(k)) {
                bail!("Hotkeys must be nonempty and distinct")
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut s: Self = serde_json::from_slice(&fs::read(path)?)
            .context("Settings are corrupt; restore or remove settings.json")?;
        // Migrate only the complete legacy default set, preserving custom keys.
        if s.migrate_legacy_hotkeys() {
            s.save(path)?;
        }
        s.validate()?;
        Ok(s)
    }
    fn migrate_legacy_hotkeys(&mut self) -> bool {
        let current = [
            self.replay_hotkey.as_str(),
            self.recording_hotkey.as_str(),
            self.mute_hotkey.as_str(),
            self.pause_hotkey.as_str(),
        ];
        if current != ["Alt+F10", "Alt+F9", "Alt+M", "Alt+F8"] {
            return false;
        }
        let defaults = Self::default();
        self.replay_hotkey = defaults.replay_hotkey;
        self.recording_hotkey = defaults.recording_hotkey;
        self.mute_hotkey = defaults.mute_hotkey;
        self.pause_hotkey = defaults.pause_hotkey;
        true
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        fs::create_dir_all(path.parent().context("Invalid settings path")?)?;
        let tmp = path.with_extension("tmp");
        let file = fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&file, self)?;
        file.sync_all()?;
        // MoveFileExW atomically replaces an existing file on Windows.
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            },
        };
        let a: Vec<u16> = tmp
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let b: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            MoveFileExW(
                PCWSTR(a.as_ptr()),
                PCWSTR(b.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )?;
        }
        Ok(())
    }
}

pub fn safe_clip(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains(['/', '\\', ':'])
        || name.starts_with('.')
        || !["mkv", "mp4"].contains(
            &Path::new(name)
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or(""),
        )
    {
        bail!("Invalid clip filename")
    }
    let path = root.join(name).canonicalize()?;
    if path.parent() != Some(root.canonicalize()?.as_path()) {
        bail!("Clip is outside the recording directory")
    }
    Ok(path)
}
pub fn regular_file(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.file_attributes() & 0x400 == 0)
}
pub fn owned_session(root: &Path, dir: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    let Ok(meta) = fs::symlink_metadata(dir) else {
        return false;
    };
    if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
        return false;
    }
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if uuid::Uuid::parse_str(name).is_err() {
        return false;
    }
    let (Ok(root), Ok(path)) = (root.canonicalize(), dir.canonicalize()) else {
        return false;
    };
    path.parent() == Some(root.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_default_shortcuts_migrate_but_custom_keys_stay() {
        let mut settings = Settings {
            replay_hotkey: "Alt+F10".into(),
            recording_hotkey: "Alt+F9".into(),
            mute_hotkey: "Alt+M".into(),
            pause_hotkey: "Alt+F8".into(),
            ..Default::default()
        };
        let mut custom = settings.clone();
        custom.replay_hotkey = "Control+F6".into();
        assert!(!custom.migrate_legacy_hotkeys());
        assert_eq!(custom.replay_hotkey, "Control+F6");
        assert!(settings.migrate_legacy_hotkeys());
        assert_eq!(settings.replay_hotkey, "Control+Shift+F10");
        assert!(!settings.migrate_legacy_hotkeys());
    }
    #[test]
    fn alpha_settings_preserve_aspect_ratio() {
        let old = serde_json::json!({"height":1080,"fps":30});
        let settings: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(settings.canvas_mode, "native");
        assert!(settings.source_rect.is_none());
        settings.validate().unwrap();
    }
    #[test]
    fn recovery_only_accepts_owned_session_directories() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let name = uuid::Uuid::new_v4().to_string();
        let own = root.path().join(&name);
        fs::create_dir(&own).unwrap();
        let foreign = other.path().join(&name);
        fs::create_dir(&foreign).unwrap();
        assert!(owned_session(root.path(), &own));
        assert!(!owned_session(root.path(), &foreign));
        let file = root.path().join(uuid::Uuid::new_v4().to_string());
        fs::write(&file, []).unwrap();
        assert!(!owned_session(root.path(), &file));
        assert!(!regular_file(&own));
        assert!(regular_file(&file));
    }
    #[test]
    fn settings_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        let s = Settings::default();
        s.save(&p).unwrap();
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap().fps, 30);
    }
    #[test]
    fn reject_bad_settings() {
        let mut s = Settings {
            fps: 0,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        s.fps = 30;
        s.audio[0].volume = f32::NAN;
        assert!(s.validate().is_err());
    }
    #[test]
    fn traversal_blocked() {
        let d = tempfile::tempdir().unwrap();
        for p in [
            "../x.mkv",
            "C:\\x.mkv",
            "x:stream.mkv",
            ".hidden.mkv",
            "x.exe",
        ] {
            assert!(safe_clip(d.path(), p).is_err());
        }
    }
    #[test]
    fn duplicate_audio_blocked() {
        let mut s = Settings::default();
        s.audio.push(AudioSource {
            kind: "application".into(),
            pid: 1,
            process_started: 10,
            ..Default::default()
        });
        assert!(s.validate().is_err());
    }
}
