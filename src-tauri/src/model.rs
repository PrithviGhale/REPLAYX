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
    /// Extra gain for this source in dB (0-24), applied in FFmpeg and always
    /// followed by a limiter, so a quiet microphone can be made louder without
    /// harsh clipping. Windows' own microphone level is never changed.
    pub boost_db: f32,
    /// Remove rumble and handling noise below ~80 Hz (voice is unaffected).
    pub high_pass: bool,
    /// Light FFT noise reduction for steady background noise (fans, hum).
    pub noise_suppression: bool,
    /// Gentle expander that lowers the level between words.
    pub noise_gate: bool,
    /// Gentle compressor that evens out loud and quiet speech.
    pub compressor: bool,
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
            boost_db: 0.0,
            high_pass: false,
            noise_suppression: false,
            noise_gate: false,
            compressor: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub capture: String,
    pub display: u32,
    pub window_title: String,
    /// Encoded frame width. Independent of the display: 1920 with height 1080
    /// is always a 1920×1080 file. Ignored by `native` (display shape).
    pub width: u32,
    pub height: u32,
    pub canvas_mode: String,
    pub source_rect: Option<crate::geometry::CaptureRect>,
    pub fps: u32,
    pub encoder: String,
    /// Video codec: "auto" (H.264), "h264", "hevc" or "av1". Never changed
    /// automatically; if the GPU cannot encode it, capture uses H.264 and the
    /// app shows a warning.
    pub codec: String,
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
    /// Try GPU scaling paths before the CPU fallback.
    pub gpu_scaling: bool,
    /// AAC bitrate per audio track.
    pub audio_bitrate_kbps: u32,
    /// Start REPLAYX in the tray at Windows sign-in. Capture never auto-starts.
    pub launch_at_startup: bool,
    /// Save clips into a folder per detected game (`Fortnite/Fortnite_….mkv`).
    pub organize_by_game: bool,
    /// User corrections for game detection (executable → game name).
    pub game_mappings: Vec<crate::games::GameMapping>,
    /// Show a small "<game> detected" notice when a game comes to the front.
    pub game_notifications: bool,
    /// When the display's GPU cannot resize frames on the GPU (NVIDIA/AMD-driven
    /// displays with the bundled FFmpeg): "smooth" records the display's pixels
    /// marked to play at the chosen shape; "exact" resizes through a CPU copy,
    /// which can repeat frames while a demanding game runs.
    pub scale_policy: String,
    /// In-game status overlay (replay, recording, microphone).
    pub overlay: crate::status_overlay::OverlaySettings,
    /// Close-to-tray explanation already shown once.
    pub tray_hint_shown: bool,
    /// Settings file layout version (see `migrate`); absent in older files.
    #[serde(default)]
    pub settings_version: u32,
}
/// Current settings layout. 2: explicit output width (0.2.1-alpha.3).
pub const SETTINGS_VERSION: u32 = 2;
impl Default for Settings {
    fn default() -> Self {
        Self {
            capture: "desktop-duplication".into(),
            display: 0,
            window_title: String::new(),
            width: 1920,
            height: 1080,
            canvas_mode: "stretch".into(),
            source_rect: None,
            fps: 60,
            encoder: "auto".into(),
            codec: "auto".into(),
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
            gpu_scaling: true,
            audio_bitrate_kbps: 192,
            launch_at_startup: false,
            organize_by_game: true,
            game_mappings: Vec::new(),
            game_notifications: true,
            scale_policy: "smooth".into(),
            overlay: Default::default(),
            tray_hint_shown: false,
            settings_version: SETTINGS_VERSION,
        }
    }
}
impl Settings {
    /// Output width for the selected mode (`native` derives it at plan time).
    pub fn output_width(&self) -> u32 {
        if self.width == 0 {
            crate::geometry::canvas(self.height).0
        } else {
            self.width
        }
    }
    /// Bring an older settings file to the current layout. Version 1 had no
    /// width: the frame was the 16:9 canvas for the height, and the default
    /// mode `native` kept the display's shape, so "1080p" on a 2560×1600
    /// display recorded 1728×1080. Users asked for 1080p get 1920×1080: native
    /// becomes Stretch to fill (the full-GPU path); fit and crop keep their mode.
    pub fn migrate(&mut self) -> bool {
        if self.settings_version >= SETTINGS_VERSION {
            return false;
        }
        self.width = crate::geometry::canvas(self.height).0;
        if self.canvas_mode == "native" {
            self.canvas_mode = "stretch".into();
        }
        self.settings_version = SETTINGS_VERSION;
        true
    }
    pub fn validate(&self) -> Result<()> {
        if !crate::geometry::CANVAS_MODES.contains(&self.canvas_mode.as_str()) {
            bail!("Unknown canvas mode")
        }
        if let Some(rect) = self.source_rect {
            if self.capture != "desktop-duplication" {
                bail!("A manual region requires display capture")
            };
            rect.validate(32768, 32768)?;
        }
        // H.264 hardware encoders accept up to 4096×2304; sizes must be even.
        if !(240..=2304).contains(&self.height)
            || !(320..=4096).contains(&self.output_width())
            || !self.height.is_multiple_of(2)
            || !self.output_width().is_multiple_of(2)
            || ![30, 60, 120, 144].contains(&self.fps)
        {
            bail!("Unsupported resolution or frame rate")
        }
        if ![30, 60, 180, 300, 600, 900, 1800].contains(&self.replay_seconds) {
            bail!("Invalid replay duration")
        }
        if !(128..=102400).contains(&self.buffer_mb) || !(1..=150).contains(&self.bitrate_mbps) {
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
        if !crate::status_overlay::POSITIONS.contains(&self.overlay.position.as_str())
            || !(30..=100).contains(&self.overlay.opacity)
        {
            bail!("Invalid overlay position or opacity")
        }
        if !["auto", "h264", "hevc", "av1"].contains(&self.codec.as_str()) {
            bail!("Unsupported video codec")
        }
        if ![128, 160, 192, 256, 320].contains(&self.audio_bitrate_kbps) {
            bail!("Unsupported audio bitrate")
        }
        if !["smooth", "exact"].contains(&self.scale_policy.as_str()) {
            bail!("Invalid scaling policy")
        }
        if !["balanced", "quality", "performance", "custom"].contains(&self.preset.as_str()) {
            bail!("Invalid preset")
        }
        if !self.output_dir.is_absolute() {
            bail!("Recording directory must be absolute")
        }
        if self.window_title.contains(['\n', '\r']) || self.window_title.len() > 512 {
            bail!("Invalid window title")
        }
        if self.game_mappings.len() > 200
            || self.game_mappings.iter().any(|m| {
                m.exe.is_empty()
                    || m.exe.len() > 260
                    || m.exe.contains(['\\', '/'])
                    || m.name.len() > 64
            })
        {
            bail!("Invalid game mapping: use an executable file name such as game.exe")
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
                || !s.boost_db.is_finite()
                || !(0.0..=24.0).contains(&s.boost_db)
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
        let layout = s.migrate();
        if s.migrate_legacy_hotkeys() || layout {
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
    fn older_settings_get_a_real_16_9_output_size() {
        // 0.2.1-alpha.2 and earlier: no width, "native" = display shape
        // (1728x1080 on a 2560x1600 display). 1080p now means 1920x1080.
        let old = serde_json::json!({"height":1080,"fps":120,"canvas_mode":"native"});
        let mut s: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(s.settings_version, 0);
        assert!(s.migrate());
        assert_eq!(
            (s.output_width(), s.height, s.canvas_mode.as_str()),
            (1920, 1080, "stretch")
        );
        assert_eq!(s.fps, 120, "frame rate is kept");
        s.validate().unwrap();
        assert!(!s.migrate(), "runs once");
        let old = serde_json::json!({"height":720,"canvas_mode":"fit"});
        let mut s: Settings = serde_json::from_value(old).unwrap();
        s.migrate();
        assert_eq!(
            (s.output_width(), s.height, s.canvas_mode.as_str()),
            (1280, 720, "fit")
        );
        // A current file with a custom size is left alone.
        let new = serde_json::json!({"width":2560,"height":1080,"canvas_mode":"crop","settings_version":2});
        let mut s: Settings = serde_json::from_value(new).unwrap();
        assert!(!s.migrate());
        assert_eq!(s.output_width(), 2560);
        s.validate().unwrap();
    }
    #[test]
    fn output_size_is_validated_for_hardware_encoders() {
        let ok = |w: u32, h: u32| {
            Settings {
                width: w,
                height: h,
                ..Default::default()
            }
            .validate()
            .is_ok()
        };
        assert!(ok(1920, 1080) && ok(1280, 720) && ok(3840, 2160) && ok(1440, 1080));
        assert!(!ok(1921, 1080), "odd width");
        assert!(!ok(5120, 2880), "beyond H.264 hardware limits");
        assert!(!ok(200, 100), "too small");
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
        assert_eq!(Settings::load(&p).unwrap().fps, 60);
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
