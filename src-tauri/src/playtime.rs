//! Local playtime tracking: how long each game was the foreground window
//! while REPLAYX was running with tracking enabled. Opt-in, never uploaded.
//!
//! * Fed once per second by the desktop loop (outside every engine lock),
//!   from the same foreground sample game detection already takes.
//! * Durations come from the monotonic clock; the wall clock is used only to
//!   place a session on the calendar, so clock changes don't distort time.
//! * A gap of more than `GAP` between samples (sleep, hibernation, a stalled
//!   process) ends the session at the last sample; the gap is never counted.
//! * Without keyboard/mouse input for `idle_minutes`, the idle time is taken
//!   back off and the session ends (controllers may not count as input, so the
//!   limit can be turned off).
//! * Finished sessions are appended to `sessions.jsonl`. The open session is
//!   checkpointed to `current.json` at most once a minute; after a crash it is
//!   recovered once (by id, so it is never counted twice).
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Playtime preferences (part of `Settings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaytimeSettings {
    /// Off until the user opts in on the Games page.
    pub enabled: bool,
    pub paused: bool,
    /// Stop counting after this many minutes without keyboard or mouse input
    /// (0 = never).
    pub idle_minutes: u32,
    /// Games never tracked.
    pub excluded: Vec<String>,
}
impl Default for PlaytimeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            paused: false,
            idle_minutes: 15,
            excluded: Vec::new(),
        }
    }
}
impl PlaytimeSettings {
    pub fn tracks(&self, game: &str) -> bool {
        self.enabled && !self.paused && !self.excluded.iter().any(|g| g.eq_ignore_ascii_case(game))
    }
}

/// One finished stretch of foreground time in one game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub game: String,
    /// Start, Unix milliseconds (UTC).
    pub start: i64,
    /// Foreground seconds (monotonic).
    pub seconds: f64,
    /// How the time was measured.
    pub source: String,
}
impl Session {
    pub fn end(&self) -> i64 {
        self.start + (self.seconds * 1000.0) as i64
    }
}

/// Samples further apart than this mean the PC slept or REPLAYX stalled.
pub const GAP: Duration = Duration::from_secs(10);
/// Sessions shorter than this are alt-tab noise and are not stored.
pub const MIN_SECONDS: f64 = 5.0;
const CHECKPOINT: Duration = Duration::from_secs(60);
const SOURCE: &str = concat!("foreground (REPLAYX ", env!("CARGO_PKG_VERSION"), ")");

struct Open {
    id: String,
    game: String,
    start: i64,
    seconds: f64,
}

pub struct Tracker {
    dir: PathBuf,
    open: Option<Open>,
    last: Option<Instant>,
    saved: Option<Instant>,
}

impl Tracker {
    /// Open the store, recovering a session left open by a crash.
    pub fn open(dir: &Path) -> Self {
        let _ = fs::create_dir_all(dir);
        let t = Self {
            dir: dir.to_path_buf(),
            open: None,
            last: None,
            saved: None,
        };
        let current = t.dir.join("current.json");
        if let Ok(bytes) = fs::read(&current) {
            if let Ok(s) = serde_json::from_slice::<Session>(&bytes) {
                let known = sessions(&t.dir).iter().any(|x| x.id == s.id);
                if !known && s.seconds >= MIN_SECONDS {
                    let _ = append(&t.dir, &s);
                }
            }
            let _ = fs::remove_file(&current);
        }
        t
    }

    /// One sample: the game in front (if any), seconds since the last
    /// keyboard/mouse input, and the settings.
    pub fn tick(
        &mut self,
        now: Instant,
        wall_ms: i64,
        game: Option<&str>,
        idle_secs: u64,
        cfg: &PlaytimeSettings,
    ) {
        let gap = self.last.map(|l| now.saturating_duration_since(l));
        self.last = Some(now);
        let idle_limit = (cfg.idle_minutes > 0).then(|| u64::from(cfg.idle_minutes) * 60);
        let idle = idle_limit.is_some_and(|l| idle_secs >= l);
        let wanted = game.filter(|g| cfg.tracks(g) && !idle);
        match (&mut self.open, gap) {
            // Same game, continuous sampling: count the elapsed time.
            (Some(o), Some(g)) if g <= GAP && wanted == Some(o.game.as_str()) => {
                o.seconds += g.as_secs_f64();
                if self
                    .saved
                    .is_none_or(|s| now.saturating_duration_since(s) >= CHECKPOINT)
                {
                    self.checkpoint();
                    self.saved = Some(now);
                }
                return;
            }
            _ => {}
        }
        // Anything else ends the open session first.
        if let Some(mut o) = self.open.take() {
            let continuous = gap.is_some_and(|g| g <= GAP);
            if continuous && game == Some(o.game.as_str()) && idle {
                // Went idle while still in the game: the idle stretch was
                // counted second by second; take it back.
                o.seconds = (o.seconds - idle_secs as f64).max(0.0);
            } else if continuous && game != Some(o.game.as_str()) {
                // Switched away: the last interval belonged to the old game.
                o.seconds += gap.unwrap().as_secs_f64().min(1.5);
            }
            self.close(o);
        }
        if let Some(g) = wanted {
            self.open = Some(Open {
                id: format!("{:x}-{}", wall_ms, std::process::id()),
                game: g.to_string(),
                start: wall_ms,
                seconds: 0.0,
            });
            self.saved = None;
        }
    }

    /// End tracking (REPLAYX quitting or tracking turned off).
    pub fn stop(&mut self) {
        if let Some(o) = self.open.take() {
            self.close(o);
        }
        self.last = None;
    }

    /// The game being tracked now and its seconds so far.
    pub fn current(&self) -> Option<(String, i64, f64)> {
        self.open
            .as_ref()
            .map(|o| (o.game.clone(), o.start, o.seconds))
    }

    fn close(&mut self, o: Open) {
        let _ = fs::remove_file(self.dir.join("current.json"));
        if o.seconds >= MIN_SECONDS {
            let _ = append(&self.dir, &session(&o));
        }
    }
    fn checkpoint(&self) {
        if let Some(o) = &self.open {
            let tmp = self.dir.join("current.json.tmp");
            if let Ok(bytes) = serde_json::to_vec(&session(o)) {
                if fs::write(&tmp, bytes).is_ok() {
                    let _ = fs::rename(&tmp, self.dir.join("current.json"));
                }
            }
        }
    }
}

fn session(o: &Open) -> Session {
    Session {
        id: o.id.clone(),
        game: o.game.clone(),
        start: o.start,
        seconds: (o.seconds * 10.0).round() / 10.0,
        source: SOURCE.into(),
    }
}

fn append(dir: &Path, s: &Session) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("sessions.jsonl"))?;
    writeln!(f, "{}", serde_json::to_string(s)?)?;
    Ok(())
}

/// All stored sessions (unreadable lines are skipped, never fatal).
pub fn sessions(dir: &Path) -> Vec<Session> {
    fs::read_to_string(dir.join("sessions.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Export as CSV (local times are added by spreadsheet users as needed).
pub fn export_csv(dir: &Path) -> String {
    let mut out = String::from("game,start_utc,end_utc,seconds,source\n");
    for s in sessions(dir) {
        let iso = |ms: i64| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
                .map(|d| d.to_rfc3339())
                .unwrap_or_default()
        };
        out += &format!(
            "\"{}\",{},{},{:.1},\"{}\"\n",
            s.game.replace('"', "\"\""),
            iso(s.start),
            iso(s.end()),
            s.seconds,
            s.source
        );
    }
    out
}

/// Seconds since the last keyboard or mouse input anywhere in Windows.
pub fn idle_seconds() -> u64 {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        let now = windows::Win32::System::SystemInformation::GetTickCount();
        u64::from(now.wrapping_sub(info.dwTime)) / 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> PlaytimeSettings {
        PlaytimeSettings {
            enabled: true,
            ..Default::default()
        }
    }
    /// Drive the tracker with one sample per second.
    fn run(
        t: &mut Tracker,
        start: Instant,
        from: u64,
        to: u64,
        game: Option<&str>,
        idle: u64,
        c: &PlaytimeSettings,
    ) {
        for s in from..to {
            t.tick(
                start + Duration::from_secs(s),
                1_700_000_000_000 + s as i64 * 1000,
                game,
                idle,
                c,
            );
        }
    }
    #[test]
    fn counts_foreground_time_and_switches_games() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Tracker::open(dir.path());
        let t0 = Instant::now();
        run(&mut t, t0, 0, 120, Some("Roblox"), 0, &cfg());
        run(&mut t, t0, 120, 180, Some("Fortnite"), 0, &cfg());
        run(&mut t, t0, 180, 240, None, 0, &cfg()); // Chrome / desktop: not playtime
        let s = sessions(dir.path());
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].game, "Roblox");
        assert!((s[0].seconds - 120.0).abs() <= 1.5, "{}", s[0].seconds);
        assert_eq!(s[1].game, "Fortnite");
        assert!((s[1].seconds - 60.0).abs() <= 1.5, "{}", s[1].seconds);
    }
    #[test]
    fn background_alt_tab_and_short_switches_do_not_count() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Tracker::open(dir.path());
        let t0 = Instant::now();
        run(&mut t, t0, 0, 60, Some("Roblox"), 0, &cfg());
        run(&mut t, t0, 60, 1860, None, 0, &cfg()); // 30 min in another app
        run(&mut t, t0, 1860, 1863, Some("Valorant"), 0, &cfg()); // 3 s: noise
        run(&mut t, t0, 1863, 1870, None, 0, &cfg());
        let total: f64 = sessions(dir.path()).iter().map(|s| s.seconds).sum();
        assert!((total - 60.0).abs() <= 1.5, "{total}");
        assert!(sessions(dir.path()).iter().all(|s| s.game == "Roblox"));
    }
    #[test]
    fn sleep_gaps_are_never_counted() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Tracker::open(dir.path());
        let t0 = Instant::now();
        run(&mut t, t0, 0, 60, Some("Roblox"), 0, &cfg());
        // PC sleeps for an hour with the game in front, then wakes up.
        run(&mut t, t0, 3660, 3720, Some("Roblox"), 0, &cfg());
        t.stop();
        let s = sessions(dir.path());
        assert_eq!(s.len(), 2, "split at the sleep");
        let total: f64 = s.iter().map(|x| x.seconds).sum();
        assert!(total < 125.0, "{total}");
    }
    #[test]
    fn idle_time_is_taken_back_and_can_be_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Tracker::open(dir.path());
        let t0 = Instant::now();
        let c = PlaytimeSettings {
            idle_minutes: 1,
            ..cfg()
        };
        run(&mut t, t0, 0, 100, Some("Roblox"), 0, &c);
        // Then no input: idle seconds climb until the 60 s limit.
        for s in 100..200u64 {
            t.tick(t0 + Duration::from_secs(s), 0, Some("Roblox"), s - 100, &c);
        }
        let total: f64 = sessions(dir.path()).iter().map(|x| x.seconds).sum();
        assert!((total - 100.0).abs() <= 2.0, "idle minute removed: {total}");
        // Limit off: the same idle time counts.
        let dir2 = tempfile::tempdir().unwrap();
        let mut t2 = Tracker::open(dir2.path());
        let off = PlaytimeSettings {
            idle_minutes: 0,
            ..cfg()
        };
        for s in 0..200u64 {
            t2.tick(
                t0 + Duration::from_secs(s),
                0,
                Some("Roblox"),
                s.saturating_sub(100),
                &off,
            );
        }
        t2.stop();
        let total: f64 = sessions(dir2.path()).iter().map(|x| x.seconds).sum();
        assert!(total > 195.0, "{total}");
    }
    #[test]
    fn disabled_paused_and_excluded_games_are_not_tracked() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Tracker::open(dir.path());
        let t0 = Instant::now();
        run(
            &mut t,
            t0,
            0,
            60,
            Some("Roblox"),
            0,
            &PlaytimeSettings {
                enabled: false,
                ..cfg()
            },
        );
        run(
            &mut t,
            t0,
            60,
            120,
            Some("Roblox"),
            0,
            &PlaytimeSettings {
                paused: true,
                ..cfg()
            },
        );
        run(
            &mut t,
            t0,
            120,
            180,
            Some("Roblox"),
            0,
            &PlaytimeSettings {
                excluded: vec!["roblox".into()],
                ..cfg()
            },
        );
        t.stop();
        assert!(sessions(dir.path()).is_empty());
    }
    #[test]
    fn crash_recovery_counts_the_session_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        let t0 = Instant::now();
        {
            let mut t = Tracker::open(dir.path());
            run(&mut t, t0, 0, 150, Some("Roblox"), 0, &cfg());
            // Crash: no stop(); the checkpoint (written at 60 s and 120 s) remains.
            assert!(dir.path().join("current.json").exists());
        }
        let _ = Tracker::open(dir.path());
        let s = sessions(dir.path());
        assert_eq!(s.len(), 1);
        assert!(
            s[0].seconds >= 59.0 && s[0].seconds <= 150.0,
            "{}",
            s[0].seconds
        );
        assert!(!dir.path().join("current.json").exists());
        // A crash between appending and removing the checkpoint: still once.
        fs::write(
            dir.path().join("current.json"),
            serde_json::to_vec(&s[0]).unwrap(),
        )
        .unwrap();
        let _ = Tracker::open(dir.path());
        assert_eq!(sessions(dir.path()).len(), 1);
    }
    #[test]
    fn export_is_csv_with_utc_times() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Tracker::open(dir.path());
        let t0 = Instant::now();
        run(&mut t, t0, 0, 30, Some("Counter-Strike 2"), 0, &cfg());
        t.stop();
        let csv = export_csv(dir.path());
        assert!(csv.starts_with("game,start_utc,end_utc,seconds,source\n"));
        assert!(csv.contains("\"Counter-Strike 2\",2023-11-14T"));
    }
}
