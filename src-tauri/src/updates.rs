//! Update preferences, state and policy. Pure logic: no network, no Tauri.
//! The flow that talks to GitHub and the installer lives in `update_flow`.
//!
//! Updates come only from published GitHub releases, are verified against the
//! publisher's public key before anything is installed, and never interrupt a
//! recording or a save.
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Where signed updates are published. `latest` resolves to the newest
/// published, non-draft release that is not marked pre-release.
pub const ENDPOINT: &str =
    "https://github.com/PrithviGhale/REPLAYX/releases/latest/download/latest.json";
/// Update packages may only come from this repository's release assets.
pub const DOWNLOAD_PREFIX: &str = "https://github.com/PrithviGhale/REPLAYX/releases/download/";
pub const RELEASES_PAGE: &str = "https://github.com/PrithviGhale/REPLAYX/releases";
pub const RELEASES_API: &str =
    "https://api.github.com/repos/PrithviGhale/REPLAYX/releases?per_page=100";

#[derive(Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
}
#[derive(Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

/// Stable builds only offer stable versions. Prerelease builds follow their
/// named channel or graduate to stable, never rely on GitHub's Latest badge.
pub fn compatible(current: &str, candidate: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim_start_matches('v'));
    let (Ok(c), Ok(n)) = (parse(current), parse(candidate)) else {
        return false;
    };
    n > c
        && (n.pre.is_empty()
            || (!c.pre.is_empty()
                && n.pre.as_str().split('.').next() == c.pre.as_str().split('.').next()))
}

pub fn release_endpoint(current: &str, releases: &[Release]) -> Option<String> {
    releases
        .iter()
        .filter_map(|r| {
            let version = semver::Version::parse(r.tag_name.strip_prefix('v')?).ok()?;
            if r.draft
                || r.prerelease == version.pre.is_empty()
                || !compatible(current, &version.to_string())
            {
                return None;
            }
            let expected = format!("{DOWNLOAD_PREFIX}{}/latest.json", r.tag_name);
            r.assets
                .iter()
                .any(|a| a.name == "latest.json" && a.browser_download_url == expected)
                .then_some((version, expected))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, url)| url)
}

/// An update endpoint is trusted only if it is exactly
/// `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`.
pub fn valid_endpoint(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts: Vec<_> = path.split('/').collect();
    parts.len() == 6
        && parts[2..] == ["releases", "latest", "download", "latest.json"]
        && parts[..2].iter().all(|part| {
            !part.is_empty()
                && ![".", ".."].contains(part)
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ['-', '_', '.'].contains(&c))
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// Check for updates after start and every `interval_hours`.
    pub auto_check: bool,
    pub interval_hours: u32,
    /// Download a found update without asking.
    pub auto_download: bool,
    /// Install a downloaded update without asking, when it is safe.
    pub auto_install: bool,
    /// Show the pop-up and the tray notice when an update is found.
    pub notify: bool,
    /// Automatic installs wait for this many minutes without keyboard or mouse input.
    pub idle_minutes: u32,
}
impl Default for UpdateSettings {
    /// "Notify me": check automatically, ask before downloading or installing.
    fn default() -> Self {
        Self {
            auto_check: true,
            interval_hours: 6,
            auto_download: false,
            auto_install: false,
            notify: true,
            idle_minutes: 5,
        }
    }
}
impl UpdateSettings {
    /// Keep the switches consistent: installing needs downloading, which needs checking.
    pub fn normalized(mut self) -> Self {
        self.interval_hours = self.interval_hours.clamp(1, 168);
        self.idle_minutes = self.idle_minutes.clamp(1, 120);
        if !self.auto_check {
            self.auto_download = false;
        }
        if !self.auto_download {
            self.auto_install = false;
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Verifying,
    Ready,
    Installing,
    Failed,
}

/// What the interface shows. Sent as the `update-status` event.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct UpdateStatus {
    pub stage: Stage,
    pub current: String,
    pub latest: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    /// Extra plain-language line (for example why an install is waiting).
    pub detail: Option<String>,
    /// Unix milliseconds of the last completed check.
    pub last_checked: Option<i64>,
    /// Test data from the development preview, never a real release.
    pub simulated: bool,
    /// A public key and endpoint are built in.
    pub configured: bool,
    /// This copy can install updates (the installed app, not a development build).
    pub installable: bool,
    /// Downloaded and verified; waiting for a safe moment to install.
    pub waiting: bool,
    /// Show the "update available" pop-up now.
    pub prompt: bool,
    pub release_url: Option<String>,
}
impl UpdateStatus {
    /// A newer version is known (drives the Settings badge).
    pub fn has_update(&self) -> bool {
        self.latest.is_some()
            && matches!(
                self.stage,
                Stage::Available
                    | Stage::Downloading
                    | Stage::Verifying
                    | Stage::Ready
                    | Stage::Installing
            )
    }
}

/// Remembered between runs (`updates.json`): never contains personal data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Memory {
    pub last_checked: Option<i64>,
    /// Failed automatic checks in a row (for back-off).
    pub failures: u32,
    pub latest_seen: Option<String>,
    pub dismissed_version: Option<String>,
    pub dismissed_at: Option<i64>,
    /// The tray notice was shown for this version.
    pub notified_version: Option<String>,
    /// Instant Replay was paused for an update at this time: resume it once.
    pub resume_replay_at: Option<i64>,
}
impl Memory {
    pub fn load(data: &Path) -> Self {
        fs::read(data.join("updates.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, data: &Path) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let tmp = data.join("updates.json.tmp");
            if fs::write(&tmp, bytes).is_ok() {
                let _ = fs::rename(&tmp, data.join("updates.json"));
            }
        }
    }
}

const HOUR_MS: i64 = 3_600_000;
/// After "Not now", the same version is offered again after this long.
pub const REMIND_AFTER_MS: i64 = 3 * 24 * HOUR_MS;
/// Instant Replay is resumed after an update only if the app restarts this soon.
pub const RESUME_WINDOW_MS: i64 = 10 * 60_000;

/// Wait after failed automatic checks: 5 min, 15 min, 1 h, then the normal interval.
pub fn backoff_ms(failures: u32, interval_hours: u32) -> i64 {
    let interval = interval_hours.max(1) as i64 * HOUR_MS;
    match failures {
        0 => interval,
        1 => 5 * 60_000,
        2 => 15 * 60_000,
        3 => HOUR_MS,
        _ => interval,
    }
    .min(interval)
}

/// Is an automatic check due? Never more often than the interval (or the
/// back-off after failures), and never when automatic checks are off.
pub fn check_due(now: i64, memory: &Memory, settings: &UpdateSettings) -> bool {
    if !settings.auto_check {
        return false;
    }
    match memory.last_checked {
        None => true,
        // A clock set backwards must not block checks forever.
        Some(last) if last > now => true,
        Some(last) => now - last >= backoff_ms(memory.failures, settings.interval_hours),
    }
}

/// Show the pop-up for `version`? Not when notifications are off, and not
/// again for a version dismissed less than three days ago.
pub fn should_prompt(now: i64, version: &str, memory: &Memory, settings: &UpdateSettings) -> bool {
    if !settings.notify {
        return false;
    }
    match (&memory.dismissed_version, memory.dismissed_at) {
        (Some(v), Some(at)) if v == version => now < at || now - at >= REMIND_AFTER_MS,
        _ => true,
    }
}

/// Is `candidate` a newer version than `current`? Semantic versioning:
/// 0.3.1-alpha.7 > 0.3.1-alpha.6, beta > alpha, 0.3.1 > 0.3.1-beta.2.
/// Anything that isn't a valid version is never "newer".
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v'));
    match (parse(current), parse(candidate)) {
        (Ok(c), Ok(n)) => n > c,
        _ => false,
    }
}

/// The parts of the capture state that decide whether installing is safe.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaptureState {
    pub recording: bool,
    pub pending_recording: bool,
    pub saving: u32,
    pub replay: bool,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Safety {
    /// Nothing is running: install now.
    Safe,
    /// Only Instant Replay is buffering: it can be paused, installed, resumed.
    ReplayOnly,
    /// A recording, save, export or recovery is in progress: never interrupt.
    Blocked(&'static str),
}

pub fn safety(c: &CaptureState) -> Safety {
    if c.recording || c.pending_recording || c.state == "Recording" {
        return Safety::Blocked("A recording is in progress.");
    }
    if c.saving > 0 || c.state == "Saving" {
        return Safety::Blocked("A clip is being saved.");
    }
    if ["Starting", "Stopping", "Recovering", "Recovering sessions"].contains(&c.state.as_str()) {
        return Safety::Blocked("Capture is starting, stopping or recovering.");
    }
    if c.replay {
        return Safety::ReplayOnly;
    }
    Safety::Safe
}

/// May an automatic install start right now? Only with nothing to interrupt,
/// no game in front, and the user away from keyboard and mouse long enough.
pub fn auto_install_ok(
    capture: &CaptureState,
    game_in_front: bool,
    idle_seconds: u64,
    settings: &UpdateSettings,
) -> bool {
    settings.auto_install
        && !game_in_front
        && idle_seconds >= settings.idle_minutes as u64 * 60
        && safety(capture) == Safety::Safe
}

/// Should Instant Replay be switched back on after an update restart?
pub fn resume_replay(now: i64, memory: &Memory) -> bool {
    memory
        .resume_replay_at
        .is_some_and(|at| now >= at && now - at <= RESUME_WINDOW_MS)
}

/// Plain-language text for a failed check or download.
pub fn friendly_error(raw: &str) -> String {
    let r = raw.to_ascii_lowercase();
    let text = if r.contains("signature")
        || r.contains("minisign")
        || r.contains("signed version")
        || r.contains("signed for version")
    {
        "The update's signature is not valid, so it was not installed. Nothing on your PC was changed."
    } else if r.contains("timed out") || r.contains("timeout") {
        "The update server took too long to answer. Check your connection and try again."
    } else if r.contains("dns")
        || r.contains("connect")
        || r.contains("network")
        || r.contains("sending request")
        || r.contains("os error")
    {
        "Couldn't reach GitHub. Check your internet connection and try again."
    } else if r.contains("platform") || r.contains("target") || r.contains("arch") {
        "The newest release has no update for this kind of PC (64-bit Windows)."
    } else if r.contains("json") || r.contains("deserialize") || r.contains("missing field") {
        "The update information on GitHub could not be read. Try again later."
    } else {
        "The update could not be completed. Try again later."
    };
    text.to_string()
}

/// "What's new" lines for the pop-up: the first few bullet points of the
/// release notes, without Markdown markup.
pub fn highlights(notes: &str, max: usize) -> Vec<String> {
    notes
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")))
        .map(|l| l.replace("**", "").replace('`', ""))
        .filter(|l| !l.is_empty())
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_select_highest_semver_and_reject_untrusted_or_mislabeled_releases() {
        assert!(!compatible("0.3.1", "0.4.0-alpha.1"));
        assert!(!compatible("0.3.1-beta.1", "0.4.0-alpha.1"));
        assert!(compatible("0.3.1-alpha.6", "0.3.1-alpha.10"));
        assert!(compatible("0.3.1-alpha.6", "0.3.1"));
        let release = |v: &str| Release {
            tag_name: format!("v{v}"),
            draft: false,
            prerelease: v.contains('-'),
            assets: vec![ReleaseAsset {
                name: "latest.json".into(),
                browser_download_url: format!("{DOWNLOAD_PREFIX}v{v}/latest.json"),
            }],
        };
        let mut releases = vec![release("0.3.1-alpha.9"), release("0.3.1-alpha.10")];
        assert!(release_endpoint("0.3.1-alpha.6", &releases)
            .unwrap()
            .contains("alpha.10/"));
        releases[1].draft = true;
        assert!(release_endpoint("0.3.1-alpha.6", &releases)
            .unwrap()
            .contains("alpha.9/"));
        releases[0].prerelease = false;
        assert!(release_endpoint("0.3.1-alpha.6", &releases).is_none());
        releases[0].prerelease = true;
        releases[0].assets[0].browser_download_url = "https://example.com/latest.json".into();
        assert!(release_endpoint("0.3.1-alpha.6", &releases).is_none());
    }
    const DAY: i64 = 24 * HOUR_MS;
    fn capture(replay: bool, recording: bool, saving: u32, state: &str) -> CaptureState {
        CaptureState {
            recording,
            pending_recording: false,
            saving,
            replay,
            state: state.into(),
        }
    }
    #[test]
    fn versions_compare_as_semantic_versions() {
        assert!(is_newer("0.3.1-alpha.6", "0.3.1-alpha.7"));
        assert!(
            is_newer("0.3.1-alpha.9", "0.3.1-alpha.10"),
            "numeric, not text"
        );
        assert!(is_newer("0.3.1-alpha.6", "0.3.1-beta.1"));
        assert!(is_newer("0.3.1-beta.2", "0.3.1"));
        assert!(is_newer("0.3.1", "0.4.0-alpha.1"));
        assert!(is_newer("0.3.1-alpha.6", "v0.3.2"));
        // Never a downgrade, never the same version, never garbage.
        assert!(!is_newer("0.3.1-alpha.6", "0.3.1-alpha.6"));
        assert!(!is_newer("0.3.1-alpha.6", "0.3.1-alpha.5"));
        assert!(!is_newer("0.3.1", "0.3.1-beta.9"));
        assert!(!is_newer("0.3.1-alpha.6", "latest"));
        assert!(!is_newer("0.3.1-alpha.6", ""));
    }
    #[test]
    fn automatic_checks_follow_the_interval_and_back_off() {
        let s = UpdateSettings::default();
        let mut m = Memory::default();
        assert!(check_due(1_000, &m, &s), "first run");
        m.last_checked = Some(0);
        assert!(!check_due(5 * HOUR_MS, &m, &s), "not before 6 hours");
        assert!(check_due(6 * HOUR_MS, &m, &s));
        // Failures retry sooner, but never hammer the server.
        m.failures = 1;
        assert!(!check_due(4 * 60_000, &m, &s));
        assert!(check_due(5 * 60_000, &m, &s));
        m.failures = 2;
        assert!(!check_due(10 * 60_000, &m, &s));
        m.failures = 3;
        assert!(check_due(HOUR_MS, &m, &s));
        m.failures = 9;
        assert!(!check_due(HOUR_MS, &m, &s), "back to the normal interval");
        // Manual mode never checks by itself.
        let manual = UpdateSettings {
            auto_check: false,
            ..s.clone()
        };
        assert!(!check_due(100 * DAY, &m, &manual));
        // A clock moved backwards doesn't block checks.
        m.failures = 0;
        m.last_checked = Some(10 * DAY);
        assert!(check_due(DAY, &m, &s));
        // A one-hour interval can't back off for longer than an hour.
        assert_eq!(backoff_ms(3, 1), HOUR_MS);
        assert_eq!(backoff_ms(1, 1), 5 * 60_000);
    }
    #[test]
    fn a_dismissed_update_is_not_shown_again_for_three_days() {
        let s = UpdateSettings::default();
        let mut m = Memory::default();
        assert!(should_prompt(0, "0.3.1-alpha.7", &m, &s));
        m.dismissed_version = Some("0.3.1-alpha.7".into());
        m.dismissed_at = Some(DAY);
        assert!(!should_prompt(2 * DAY, "0.3.1-alpha.7", &m, &s));
        assert!(should_prompt(4 * DAY, "0.3.1-alpha.7", &m, &s), "reminder");
        assert!(
            should_prompt(2 * DAY, "0.3.1-alpha.8", &m, &s),
            "a newer one asks at once"
        );
        let quiet = UpdateSettings { notify: false, ..s };
        assert!(!should_prompt(9 * DAY, "0.3.1-alpha.8", &m, &quiet));
    }
    #[test]
    fn installing_never_interrupts_recording_saving_or_recovery() {
        assert_eq!(safety(&capture(false, false, 0, "Stopped")), Safety::Safe);
        assert_eq!(
            safety(&capture(true, false, 0, "Buffering")),
            Safety::ReplayOnly
        );
        for c in [
            capture(true, true, 0, "Recording"),
            capture(false, true, 0, "Recording"),
            capture(true, false, 1, "Saving"),
            capture(false, false, 2, "Stopped"),
            capture(false, false, 0, "Starting"),
            capture(false, false, 0, "Stopping"),
            capture(true, false, 0, "Recovering"),
            capture(false, false, 0, "Recovering sessions"),
            CaptureState {
                pending_recording: true,
                ..capture(false, false, 0, "Starting")
            },
        ] {
            assert!(matches!(safety(&c), Safety::Blocked(_)), "{c:?}");
        }
    }
    #[test]
    fn automatic_installs_wait_for_an_idle_pc_and_no_game() {
        let auto = UpdateSettings {
            auto_download: true,
            auto_install: true,
            ..Default::default()
        };
        let idle = capture(false, false, 0, "Stopped");
        assert!(auto_install_ok(&idle, false, 300, &auto));
        assert!(
            !auto_install_ok(&idle, false, 299, &auto),
            "user still active"
        );
        assert!(
            !auto_install_ok(&idle, true, 3600, &auto),
            "a game is in front"
        );
        // Even an idle PC must keep its active replay buffer intact.
        assert!(!auto_install_ok(
            &capture(true, false, 0, "Buffering"),
            false,
            300,
            &auto
        ));
        assert!(!auto_install_ok(
            &capture(true, true, 0, "Recording"),
            false,
            9999,
            &auto
        ));
        assert!(!auto_install_ok(
            &capture(true, false, 1, "Saving"),
            false,
            9999,
            &auto
        ));
        // Never without the user's explicit switch.
        assert!(!auto_install_ok(
            &idle,
            false,
            9999,
            &UpdateSettings::default()
        ));
    }
    #[test]
    fn settings_stay_consistent_and_default_to_notify() {
        let d = UpdateSettings::default();
        assert!(d.auto_check && d.notify && !d.auto_download && !d.auto_install);
        let odd = UpdateSettings {
            auto_check: false,
            auto_download: true,
            auto_install: true,
            interval_hours: 0,
            idle_minutes: 9999,
            ..d
        }
        .normalized();
        assert!(!odd.auto_download && !odd.auto_install);
        assert_eq!((odd.interval_hours, odd.idle_minutes), (1, 120));
    }
    #[test]
    fn replay_resumes_only_right_after_an_update() {
        let mut m = Memory::default();
        assert!(!resume_replay(1_000, &m));
        m.resume_replay_at = Some(1_000);
        assert!(resume_replay(60_000, &m));
        assert!(
            !resume_replay(1_000 + RESUME_WINDOW_MS + 1, &m),
            "too late: stay off"
        );
        assert!(!resume_replay(500, &m));
    }
    #[test]
    fn memory_round_trips_and_survives_a_bad_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Memory::load(dir.path()), Memory::default());
        let m = Memory {
            last_checked: Some(42),
            dismissed_version: Some("0.3.1-alpha.7".into()),
            dismissed_at: Some(7),
            ..Default::default()
        };
        m.save(dir.path());
        assert_eq!(Memory::load(dir.path()), m);
        std::fs::write(dir.path().join("updates.json"), b"{not json").unwrap();
        assert_eq!(Memory::load(dir.path()), Memory::default());
    }
    #[test]
    fn errors_and_notes_are_shown_in_plain_language() {
        assert!(friendly_error("minisign: Invalid signature").contains("signature is not valid"));
        assert!(friendly_error("error sending request for url").contains("Couldn't reach GitHub"));
        assert!(friendly_error("operation timed out").contains("too long"));
        assert!(
            friendly_error("the platform `windows-x86_64` was not found")
                .contains("64-bit Windows")
        );
        assert!(friendly_error("something odd").contains("could not be completed"));
        let notes = "## What's New\n\n- **Audio** that survives `device` changes\n- Fix two\n* Fix three\ntext\n- Four\n- Five";
        assert_eq!(
            highlights(notes, 3),
            ["Audio that survives device changes", "Fix two", "Fix three"]
        );
        assert!(highlights("no bullets here", 3).is_empty());
    }
    #[test]
    fn only_this_repository_serves_updates() {
        assert!(valid_endpoint(ENDPOINT));
        for url in [
            "http://github.com/x/releases/latest/download/latest.json",
            "https://github.com.evil.test/x/releases/latest/download/latest.json",
            "https://github.com/user@evil.test/releases/latest/download/latest.json",
            "https://github.com/../repo/releases/latest/download/latest.json",
            "https://github.com/owner/repo/releases/latest/download/latest.json?redirect=1",
            "https://github.com//repo/releases/latest/download/latest.json",
        ] {
            assert!(!valid_endpoint(url), "{url}");
        }
        // The address compiled into the app is the one in tauri.conf.json.
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &conf["plugins"]["updater"];
        assert_eq!(updater["endpoints"], serde_json::json!([ENDPOINT]));
        assert_eq!(
            updater["requireSignedVersion"], true,
            "a signature must name its version"
        );
        assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
        assert!(updater.get("dangerousAcceptInvalidCerts").is_none());
        assert!(
            updater["pubkey"].as_str().is_some_and(|k| k.len() > 80),
            "the public key is built in"
        );
        assert!(crate::updates::ENDPOINT.starts_with("https://github.com/PrithviGhale/REPLAYX/"));
        assert!(ENDPOINT.ends_with("/releases/latest/download/latest.json"));
        assert!(DOWNLOAD_PREFIX.starts_with("https://github.com/PrithviGhale/REPLAYX/"));
    }
}
