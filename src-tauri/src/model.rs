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
    /// Recording gain for a microphone in dB (0-24), applied by the Voice
    /// Enhancer before its limiter, so a quiet microphone can be made louder
    /// without clipping. Windows' own microphone level is never changed.
    pub boost_db: f32,
    /// Voice Enhancer settings (microphones only). Older files' separate
    /// high_pass/noise_suppression/noise_gate/compressor keys are ignored.
    pub voice: crate::voice::VoiceConfig,
}
/// A user-saved Voice Enhancer preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedVoice {
    pub name: String,
    pub voice: crate::voice::VoiceConfig,
    pub boost_db: f32,
}
impl AudioSource {
    /// The Windows default microphone with the rumble filter on.
    pub fn microphone() -> Self {
        Self {
            kind: "microphone".into(),
            name: "Microphone".into(),
            ..Default::default()
        }
    }
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
            voice: Default::default(),
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
    /// Opens REPLAYX, or hides it to the tray when it's in front.
    pub window_hotkey: String,
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
    /// Display capture API: "dda" (Desktop Duplication, the default) or
    /// "wgc" (Windows Graphics Capture, experimental). If WGC cannot start,
    /// the start falls back to Desktop Duplication and says why.
    pub capture_api: String,
    /// Runtime only (never saved): this session's WGC capture missed output
    /// slots at 1x, so capture runs at 2x the output rate.
    #[serde(skip)]
    pub wgc_boost: bool,
    /// In-game status overlay (replay, recording, microphone).
    pub overlay: crate::status_overlay::OverlaySettings,
    /// Close-to-tray explanation already shown once.
    pub tray_hint_shown: bool,
    /// First-run tour finished or skipped.
    pub tour_done: bool,
    /// Voice Enhancer presets saved by the user.
    pub voice_presets: Vec<SavedVoice>,
    /// Local playtime tracking (opt-in).
    pub playtime: crate::playtime::PlaytimeSettings,
    /// Short bar when a recording starts or a clip is saved.
    pub capture_notices: bool,
    /// Update checks, downloads and installs (see `updates`).
    pub updates: crate::updates::UpdateSettings,
    /// Settings file layout version (see `migrate`); absent in older files.
    #[serde(default)]
    pub settings_version: u32,
}
/// Current settings layout. 2: explicit output width (0.2.1-alpha.3).
/// 3: a microphone source is always present (0.2.2-alpha.1).
/// 4: closing the window minimizes to the tray by default (0.3.1-alpha.1).
pub const SETTINGS_VERSION: u32 = 6;
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
                .join("Videos")
                .join("REPLAYX"),
            audio: vec![AudioSource::default(), AudioSource::microphone()],
            separate_tracks: true,
            replay_hotkey: "Control+Shift+F10".into(),
            recording_hotkey: "Control+Shift+F9".into(),
            mute_hotkey: "Control+Shift+M".into(),
            pause_hotkey: "Control+Shift+F8".into(),
            window_hotkey: "Control+Shift+F11".into(),
            close_to_tray: true,
            diagnostics: false,
            check_updates: false,
            gpu_scaling: true,
            audio_bitrate_kbps: 192,
            launch_at_startup: false,
            organize_by_game: true,
            game_mappings: Vec::new(),
            game_notifications: true,
            scale_policy: "smooth".into(),
            capture_api: "dda".into(),
            wgc_boost: false,
            overlay: Default::default(),
            tray_hint_shown: false,
            tour_done: false,
            voice_presets: Vec::new(),
            playtime: Default::default(),
            updates: Default::default(),
            capture_notices: true,
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
        if self.settings_version < 2 {
            self.width = crate::geometry::canvas(self.height).0;
            if self.canvas_mode == "native" {
                self.canvas_mode = "stretch".into();
            }
        }
        // 3: the microphone is part of every setup (the app has no separate
        // "add microphone" step any more). Its own on/off switch decides
        // whether it is recorded.
        if self.settings_version < 3
            && !self.audio.iter().any(|a| a.kind == "microphone")
            && self.audio.len() < 5
        {
            self.audio.push(AudioSource::microphone());
        }
        // 4: REPLAYX keeps running in the tray when the window closes, so
        // Instant Replay, hotkeys and playtime keep working (once; the user
        // can turn it off again in Settings).
        if self.settings_version < 4 {
            self.close_to_tray = true;
        }
        // 5: playtime tracking is on by default (once; it can be turned off
        // in Settings and stays off).
        if self.settings_version < 5 {
            self.playtime.enabled = true;
        }
        // 6: new Open/hide shortcut; never one an action already uses.
        if self.settings_version < 6 {
            let used = [
                &self.replay_hotkey,
                &self.recording_hotkey,
                &self.mute_hotkey,
                &self.pause_hotkey,
            ]
            .map(|k| k.to_ascii_lowercase());
            if let Some(free) = ["Control+Shift+F11", "Control+Shift+F7", "Control+Alt+F11"]
                .into_iter()
                .find(|k| !used.contains(&k.to_ascii_lowercase()))
            {
                self.window_hotkey = free.into();
            }
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
        // Any length from 30 s: the Replay page slider offers up to 10 min in
        // 15 s steps; 15 and 30 min saved by earlier versions stay valid.
        if !(30..=1800).contains(&self.replay_seconds) {
            bail!("Replay length must be between 30 seconds and 30 minutes")
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
        for a in &self.audio {
            a.voice.validate()?;
        }
        if self.voice_presets.len() > 20
            || self.voice_presets.iter().any(|p| {
                p.name.trim().is_empty() || p.name.len() > 40 || !(0.0..=24.0).contains(&p.boost_db)
            })
        {
            bail!("Invalid saved voice preset")
        }
        for p in &self.voice_presets {
            p.voice.validate()?;
        }
        if self.playtime.idle_minutes > 240 || self.playtime.excluded.len() > 500 {
            bail!("Invalid playtime settings")
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
            &self.window_hotkey,
        ];
        for (i, k) in keys.iter().enumerate() {
            if k.is_empty() || keys[..i].iter().any(|x| x.eq_ignore_ascii_case(k)) {
                bail!("Hotkeys must be nonempty and distinct")
            }
        }
        Ok(())
    }
    /// Windows-style separators in the clips folder. Older versions saved
    /// "C:\Users\Name\Videos/REPLAYX"; Explorer reads the "/REPLAYX" part as a
    /// command-line switch and opened Documents instead of the folder.
    pub fn normalize_paths(&mut self) -> bool {
        let fixed = normalize_dir(&self.output_dir);
        // Compare the text: Path equality treats '/' and '\' as the same.
        let changed = fixed.as_os_str() != self.output_dir.as_os_str();
        self.output_dir = fixed;
        changed
    }
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_checked(path).map_err(|e| match e {
            LoadError::Unreadable(e) | LoadError::Damaged(e, _) => e,
        })
    }
    /// Load, telling a file that couldn't be opened apart from one whose
    /// contents are damaged. A migration that can't be written back is not
    /// an error: the migrated settings are used and saved with the next
    /// change.
    fn load_checked(path: &Path) -> std::result::Result<Self, LoadError> {
        let bytes = match read_settings_bytes(path) {
            Ok(Some(b)) => b,
            Ok(None) => return Ok(Self::default()),
            Err(e) => {
                return Err(LoadError::Unreadable(
                    anyhow::Error::new(e).context("Couldn't open settings.json"),
                ))
            }
        };
        let checked = serde_json::from_slice::<Self>(&bytes)
            .context("Settings are corrupt; restore or remove settings.json")
            .and_then(|mut s| {
                // Migrate only the complete legacy default set, preserving
                // custom keys.
                let keys = s.migrate_legacy_hotkeys();
                let layout = s.migrate();
                let paths = s.normalize_paths();
                s.validate()?;
                Ok((s, keys || layout || paths))
            });
        let (s, migrated) = checked.map_err(|e| LoadError::Damaged(e, bytes))?;
        if migrated {
            if let Err(e) = s.save(path) {
                eprintln!("REPLAYX: updated settings will be saved with the next change: {e:#}");
            }
        }
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
    /// Start-up loading that never leaves REPLAYX unable to open and never
    /// loses the user's settings file:
    ///
    /// - A file that can't be opened (in use by another program, access
    ///   denied) is left untouched, and the problem says REPLAYX must not
    ///   write over it this session.
    /// - A damaged file (not valid JSON, or invalid values) is set aside
    ///   under a new name that is never reused, so no earlier copy is ever
    ///   replaced. If it can only be copied, the original remains protected
    ///   from settings writes until restart because another writer may change it.
    ///
    /// In every case REPLAYX starts with its defaults.
    pub fn load_or_recover(path: &Path) -> (Self, Option<SettingsProblem>) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        Self::recover_at(path, stamp, KEPT_NAMES)
    }
    fn recover_at(path: &Path, stamp: u64, names: u32) -> (Self, Option<SettingsProblem>) {
        let problem = match Self::load_checked(path) {
            Ok(s) => return (s, None),
            Err(LoadError::Unreadable(e)) => SettingsProblem::Unreadable {
                technical: format!("{e:#}"),
            },
            Err(LoadError::Damaged(e, bytes)) => {
                let technical = format!("{e:#}");
                match set_aside(path, &bytes, stamp, names) {
                    Ok(SettingsBackup::Moved(kept)) => SettingsProblem::Damaged {
                        backup: kept
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        technical,
                    },
                    Ok(SettingsBackup::Copied(kept)) => SettingsProblem::DamagedCopied {
                        backup: kept
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        technical,
                    },
                    Err(why) => SettingsProblem::DamagedNotKept {
                        technical: format!("{technical}. Keeping a copy failed: {why}"),
                    },
                }
            }
        };
        (Self::default(), Some(problem))
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

/// What start-up found wrong with the settings file. The UI shows it in plain
/// words; `technical` is kept apart, for the details section only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsProblem {
    /// The file was damaged. It was set aside as `backup` (next to it, in the
    /// same folder); the defaults are in use and saving is safe.
    Damaged { backup: String, technical: String },
    /// A snapshot was copied, but the original remains in place and may
    /// change independently. Keep this session read-only until restart.
    DamagedCopied { backup: String, technical: String },
    /// The file was damaged and couldn't be set aside or copied. It is left
    /// as it is, and REPLAYX doesn't write over it until it restarts.
    DamagedNotKept { technical: String },
    /// The file exists but couldn't be opened (another program is using it,
    /// or access was denied). It is left as it is, and REPLAYX doesn't write
    /// over it until it restarts.
    Unreadable { technical: String },
}
impl SettingsProblem {
    /// Whether REPLAYX may write settings.json in this session. Only when the
    /// original was kept somewhere safe first.
    pub fn allows_saving(&self) -> bool {
        matches!(self, Self::Damaged { .. })
    }
}

enum LoadError {
    /// The file exists but couldn't be read.
    Unreadable(anyhow::Error),
    /// The file was read, but isn't valid settings. Carries its bytes.
    Damaged(anyhow::Error, Vec<u8>),
}

/// How many names `settings.unreadable-<time>[-n].json` are tried in one
/// second before giving up.
const KEPT_NAMES: u32 = 1000;

/// Read the settings file; `None` when there is none yet (first start).
/// Antivirus, backup and sync tools can hold a file for a moment, so a
/// sharing or lock violation, or access denied, is retried for about half a
/// second before it counts.
fn read_settings_bytes(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut tries = 0;
    loop {
        match fs::read(path) {
            Ok(b) => return Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            // 5 access denied, 32 sharing violation, 33 lock violation.
            Err(e) if tries < 5 && matches!(e.raw_os_error(), Some(5 | 32 | 33)) => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
}

fn kept_name(stamp: u64, n: u32) -> String {
    if n == 0 {
        format!("settings.unreadable-{stamp}.json")
    } else {
        format!("settings.unreadable-{stamp}-{n}.json")
    }
}

fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

enum SettingsBackup {
    Moved(PathBuf),
    Copied(PathBuf),
}

/// Set a damaged settings file aside without ever replacing another file.
///
/// 1. Move it to the first free `settings.unreadable-<time>[-n].json`. The
///    move refuses to replace an existing file, so an earlier copy is never
///    lost, even when two recoveries happen in the same second.
/// 2. If it can't be moved (another program holds it open, or another
///    REPLAYX start moved it first), keep an exact copy of the bytes that
///    were read instead. A copy with the same bytes that is already there is
///    reused. A new copy is created only under a name that doesn't exist
///    yet, and is read back to check it. Copy outcomes require read-only recovery.
fn set_aside(path: &Path, bytes: &[u8], stamp: u64, names: u32) -> std::io::Result<SettingsBackup> {
    use std::io::{Error, ErrorKind, Write};
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH},
    };
    let dir = path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "settings path has no folder"))?;
    let from = wide(path);
    let mut move_failed = None;
    for n in 0..names {
        let kept = dir.join(kept_name(stamp, n));
        // No MOVEFILE_REPLACE_EXISTING: fails if `kept` exists.
        match unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(wide(&kept).as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        } {
            Ok(()) => return Ok(SettingsBackup::Moved(kept)),
            // 80 file exists, 183 already exists: try the next name.
            Err(e) if matches!(e.code().0 as u32 & 0xFFFF, 80 | 183) && kept.exists() => continue,
            Err(e) => {
                move_failed = Some(e);
                break;
            }
        }
    }
    let existing = fs::read_dir(dir)?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("settings.unreadable-") && name.ends_with(".json")
        })
        .find(|e| {
            e.metadata()
                .is_ok_and(|m| m.is_file() && m.len() == bytes.len() as u64)
                && fs::read(e.path()).is_ok_and(|b| b == bytes)
        });
    if let Some(e) = existing {
        return Ok(SettingsBackup::Copied(e.path()));
    }
    for n in 0..names {
        let kept = dir.join(kept_name(stamp, n));
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&kept)
        {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let written = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        if written.is_ok() && fs::read(&kept).is_ok_and(|b| b == bytes) {
            return Ok(SettingsBackup::Copied(kept));
        }
        // Only ever our own, just-created, incomplete copy.
        let _ = fs::remove_file(&kept);
        return Err(written
            .err()
            .unwrap_or_else(|| Error::other("the copy didn't match the original")));
    }
    Err(Error::other(match move_failed {
        Some(e) => format!("couldn't move it ({e}) and no free name for a copy"),
        None => "no free name to keep it under".to_owned(),
    }))
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

/// Backslashes only, no trailing separator (except a drive root such as `C:\`).
pub fn normalize_dir(dir: &Path) -> PathBuf {
    let s = dir.to_string_lossy().replace('/', "\\");
    let trimmed = s.trim_end_matches('\\');
    if trimmed.len() <= 2 && trimmed.ends_with(':') {
        PathBuf::from(format!("{trimmed}\\"))
    } else {
        PathBuf::from(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clips_folder_paths_use_windows_separators() {
        assert_eq!(
            normalize_dir(Path::new(r"C:\Users\A\Videos/REPLAYX")),
            PathBuf::from(r"C:\Users\A\Videos\REPLAYX")
        );
        assert_eq!(
            normalize_dir(Path::new(r"D:/Clips/")),
            PathBuf::from(r"D:\Clips")
        );
        assert_eq!(normalize_dir(Path::new("E:/")), PathBuf::from(r"E:\"));
        let mut s = Settings {
            output_dir: r"C:\Users\A\Videos/REPLAYX".into(),
            ..Settings::default()
        };
        assert!(s.normalize_paths(), "an old mixed path is repaired once");
        assert!(!s.normalize_paths());
        assert!(
            !Settings::default()
                .output_dir
                .to_string_lossy()
                .contains('/'),
            "the default has no forward slash"
        );
    }
    #[test]
    fn open_hide_shortcut_never_collides() {
        let mut old = Settings {
            settings_version: 5,
            pause_hotkey: "Control+Shift+F11".into(),
            ..Settings::default()
        };
        old.migrate();
        assert_eq!(old.window_hotkey, "Control+Shift+F7");
        assert!(old.validate().is_ok());
        assert_eq!(Settings::default().window_hotkey, "Control+Shift+F11");
    }
    #[test]
    fn playtime_is_switched_on_once() {
        let mut old = Settings {
            settings_version: 4,
            ..Settings::default()
        };
        old.playtime.enabled = false;
        assert!(old.migrate());
        assert!(old.playtime.enabled, "on by default after the upgrade");
        old.playtime.enabled = false;
        assert!(!old.migrate(), "a later choice to turn it off is kept");
        assert!(!old.playtime.enabled);
        assert!(Settings::default().playtime.enabled);
    }
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
        // A version 2 file keeps its custom size and gains the microphone.
        let new = serde_json::json!({"width":2560,"height":1080,"canvas_mode":"crop","settings_version":2,
            "audio":[{"kind":"system","name":"System audio"}]});
        let mut s: Settings = serde_json::from_value(new).unwrap();
        assert!(s.migrate());
        assert_eq!(s.output_width(), 2560);
        assert_eq!(s.audio.iter().filter(|a| a.kind == "microphone").count(), 1);
        assert!(s.audio[1].voice.preset == "clear" && s.audio[1].device.is_empty());
        s.validate().unwrap();
        assert!(!s.migrate(), "runs once");
        // Version 4: minimize to tray on close, once.
        let mut v3 = Settings {
            settings_version: 3,
            close_to_tray: false,
            ..Default::default()
        };
        assert!(v3.migrate());
        assert!(v3.close_to_tray);
        let mut chose_off = Settings {
            close_to_tray: false,
            ..Default::default()
        };
        assert!(
            !chose_off.migrate(),
            "a current file keeps the user's choice"
        );
        assert!(!chose_off.close_to_tray);
        // An existing microphone is not duplicated.
        let mut s = Settings {
            settings_version: 2,
            ..Default::default()
        };
        s.migrate();
        assert_eq!(s.audio.iter().filter(|a| a.kind == "microphone").count(), 1);
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
    fn kept_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("settings.unreadable-"))
            .map(|n| {
                let b = fs::read(dir.join(&n)).unwrap_or_default();
                (n, b)
            })
            .collect();
        v.sort();
        v
    }
    /// Open the file the way another program holding it would: with the
    /// given sharing (0 = nothing, 1 = read only).
    fn hold(path: &Path, share: u32) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(share)
            .open(path)
            .unwrap()
    }
    const TORN: &[u8] = b"{ \"fps\": 60, \"capt"; // cut off mid-write
    #[test]
    fn damaged_settings_are_set_aside_byte_for_byte_and_defaults_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, TORN).unwrap();
        let (s, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        assert_eq!(s.fps, Settings::default().fps);
        let problem = problem.expect("the user is told");
        assert!(matches!(
            &problem,
            SettingsProblem::Damaged { backup, technical }
                if backup == "settings.unreadable-1000.json" && technical.contains("corrupt")
        ));
        assert!(problem.allows_saving());
        assert_eq!(
            kept_files(dir.path()),
            vec![("settings.unreadable-1000.json".into(), TORN.to_vec())]
        );
        // Saving now can't lose anything, and a healthy file loads as before.
        Settings::default().save(&path).unwrap();
        assert!(Settings::load_or_recover(&path).1.is_none());
        assert_eq!(kept_files(dir.path()).len(), 1);
    }
    #[test]
    fn invalid_values_count_as_damaged_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, br#"{ "fps": 0 }"#).unwrap();
        let (_, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        assert!(matches!(problem, Some(SettingsProblem::Damaged { .. })));
        assert_eq!(kept_files(dir.path())[0].1, br#"{ "fps": 0 }"#);
    }
    #[test]
    fn two_recoveries_in_the_same_second_keep_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"first damaged").unwrap();
        Settings::recover_at(&path, 1000, KEPT_NAMES);
        fs::write(&path, b"second damaged").unwrap();
        Settings::recover_at(&path, 1000, KEPT_NAMES);
        assert_eq!(
            kept_files(dir.path()),
            vec![
                (
                    "settings.unreadable-1000-1.json".into(),
                    b"second damaged".to_vec()
                ),
                (
                    "settings.unreadable-1000.json".into(),
                    b"first damaged".to_vec()
                ),
            ]
        );
    }
    #[test]
    fn an_existing_backup_is_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(dir.path().join("settings.unreadable-1000.json"), b"older").unwrap();
        fs::write(&path, TORN).unwrap();
        let (_, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        assert!(matches!(
            problem,
            Some(SettingsProblem::Damaged { ref backup, .. }) if backup == "settings.unreadable-1000-1.json"
        ));
        assert_eq!(
            kept_files(dir.path()),
            vec![
                ("settings.unreadable-1000-1.json".into(), TORN.to_vec()),
                ("settings.unreadable-1000.json".into(), b"older".to_vec()),
            ]
        );
    }
    #[test]
    fn a_settings_file_in_use_is_left_alone_and_must_not_be_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let custom = Settings {
            fps: 30,
            ..Default::default()
        };
        custom.save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        let held = hold(&path, 0); // sharing violation for everyone else
        let (s, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        drop(held);
        assert_eq!(s.fps, Settings::default().fps);
        let problem = problem.unwrap();
        assert!(matches!(problem, SettingsProblem::Unreadable { .. }));
        assert!(!problem.allows_saving(), "a later save would lose fps 30");
        assert!(kept_files(dir.path()).is_empty());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn a_damaged_file_that_cannot_be_moved_is_copied_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, TORN).unwrap();
        let held = hold(&path, 1); // readable, but can't be moved or deleted
        let (_, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        drop(held);
        let problem = problem.unwrap();
        assert!(matches!(problem, SettingsProblem::DamagedCopied { .. }));
        assert!(
            !problem.allows_saving(),
            "the original can still change independently"
        );
        assert_eq!(
            kept_files(dir.path()),
            vec![("settings.unreadable-1000.json".into(), TORN.to_vec())]
        );
        assert_eq!(fs::read(&path).unwrap(), TORN, "the original is untouched");
        // The next start finds the same damage: the copy is reused, not
        // duplicated.
        let held = hold(&path, 1);
        Settings::recover_at(&path, 1001, KEPT_NAMES);
        drop(held);
        assert_eq!(kept_files(dir.path()).len(), 1);
    }
    #[test]
    fn copied_damage_can_finish_recovery_after_the_file_is_released() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, TORN).unwrap();
        let held = hold(&path, 1);
        let (_, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        assert!(!problem.unwrap().allows_saving());
        drop(held);
        let (settings, problem) = Settings::recover_at(&path, 1001, KEPT_NAMES);
        assert!(matches!(problem, Some(SettingsProblem::Damaged { .. })));
        assert!(problem.unwrap().allows_saving());
        settings.save(&path).unwrap();
        assert!(Settings::load_or_recover(&path).1.is_none());
        assert!(kept_files(dir.path())
            .iter()
            .all(|(_, bytes)| bytes == TORN));
    }
    #[test]
    fn a_damaged_file_with_no_copy_possible_must_not_be_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, TORN).unwrap();
        // Every name is taken (by folders), and the file can't be moved.
        for n in 0..3 {
            fs::create_dir(dir.path().join(kept_name(1000, n))).unwrap();
        }
        let held = hold(&path, 1);
        let (_, problem) = Settings::recover_at(&path, 1000, 3);
        drop(held);
        let problem = problem.unwrap();
        assert!(matches!(problem, SettingsProblem::DamagedNotKept { .. }));
        assert!(!problem.allows_saving());
        assert_eq!(fs::read(&path).unwrap(), TORN);
    }
    #[test]
    fn a_migration_that_cannot_be_written_back_keeps_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let legacy = Settings {
            fps: 30,
            replay_hotkey: "Alt+F10".into(),
            recording_hotkey: "Alt+F9".into(),
            mute_hotkey: "Alt+M".into(),
            pause_hotkey: "Alt+F8".into(),
            ..Default::default()
        };
        fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        let original = fs::read(&path).unwrap();
        // The save's temporary file can't be created.
        fs::create_dir(path.with_extension("tmp")).unwrap();
        let (s, problem) = Settings::recover_at(&path, 1000, KEPT_NAMES);
        assert!(problem.is_none(), "the settings themselves are fine");
        assert_eq!(s.fps, 30, "not replaced by the defaults");
        assert_eq!(
            s.replay_hotkey,
            Settings::default().replay_hotkey,
            "migrated"
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(kept_files(dir.path()).is_empty());
    }
    #[test]
    fn a_first_start_has_nothing_to_report() {
        let dir = tempfile::tempdir().unwrap();
        let (s, problem) = Settings::load_or_recover(&dir.path().join("settings.json"));
        assert!(problem.is_none());
        assert_eq!(s.fps, Settings::default().fps);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
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
    fn any_replay_length_from_30_seconds_is_valid() {
        // 2 min (the Balanced preset) was refused: "Invalid replay duration".
        for seconds in [30, 45, 120, 135, 600, 900, 1800] {
            let s = Settings {
                replay_seconds: seconds,
                ..Default::default()
            };
            assert!(s.validate().is_ok(), "{seconds}");
        }
        for seconds in [0, 29, 1801] {
            let s = Settings {
                replay_seconds: seconds,
                ..Default::default()
            };
            assert!(s.validate().is_err(), "{seconds}");
        }
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
