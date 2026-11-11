//! The update flow of the desktop app: check GitHub Releases, download,
//! verify the publisher's signature, install when it is safe.
//!
//! Rules (policy in `crate::updates`, tested there):
//! - Only this repository's published releases are consulted and only its
//!   release assets are downloaded. The Tauri updater verifies the package
//!   against the built-in public key before anything runs.
//! - One check or download at a time.
//! - A recording, a save, an export or a recovery is never interrupted.
//! - Development builds check and download like the real app but never
//!   install: they must not overwrite the installed REPLAYX.
//! - The simulated flow (development builds only) uses test data, downloads
//!   and installs nothing, and is labelled as such.
use crate::{
    engine::Engine,
    overlay::{Notice, Overlay},
    updates::{self, CaptureState, Memory, Safety, Stage, UpdateStatus},
    winutil,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

type Hook = Arc<dyn Fn() + Send + Sync>;

enum Pending {
    Real {
        update: Box<Update>,
        bytes: Option<Vec<u8>>,
    },
    /// Development preview: test data only.
    Simulated { bad_signature: bool, ready: bool },
}

pub struct UpdateRuntime {
    data: PathBuf,
    engine: Engine,
    status: Mutex<UpdateStatus>,
    memory: Mutex<Memory>,
    pending: Mutex<Option<Pending>>,
    /// A check or a download is running.
    busy: AtomicBool,
    /// "Update now": install as soon as the download is verified.
    install_after: AtomicBool,
    sim_dismissed: AtomicBool,
    /// Finalizes capture before the installer replaces the app.
    before_exit: Mutex<Option<Hook>>,
    /// Blocks capture actions while the installer is being started.
    installing: Arc<RwLock<bool>>,
}

struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn user_error(error: &tauri_plugin_updater::Error) -> String {
    match error {
        tauri_plugin_updater::Error::Reqwest(e) if e.is_timeout() => {
            updates::friendly_error("timeout")
        }
        tauri_plugin_updater::Error::ReleaseNotFound => {
            "The update service did not return a usable release. Try again later.".into()
        }
        _ => updates::friendly_error(&error.to_string()),
    }
}

/// The update source: GitHub, or in development builds a local test server
/// (`REPLAYX_UPDATE_TEST_ENDPOINT=http://127.0.0.1:PORT/latest.json`).
fn endpoint() -> (String, bool) {
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("REPLAYX_UPDATE_TEST_ENDPOINT") {
        if url.starts_with("http://127.0.0.1:") {
            return (url, true);
        }
    }
    (updates::ENDPOINT.to_string(), false)
}

async fn published_endpoint(current: &str) -> Result<Option<String>, tauri_plugin_updater::Error> {
    let request = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("REPLAYX/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let mut response = client
            .get(updates::RELEASES_API)
            .send()
            .await
            .and_then(|r| r.error_for_status())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 2_000_000 {
                return Err(tauri_plugin_updater::Error::Network(
                    "Update release listing exceeds its size limit".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let releases: Vec<updates::Release> = serde_json::from_slice(&bytes)?;
        Ok(updates::release_endpoint(current, &releases))
    };
    // Preserve transport detail only for classification, never return URLs or credentials.
    request.await
}

fn capture_state(engine: &Engine) -> CaptureState {
    let s = engine.status();
    CaptureState {
        recording: s.recording,
        pending_recording: s.pending_recording,
        saving: s.saving,
        replay: s.replay,
        state: s.state,
    }
}

fn game_in_front() -> bool {
    crate::games::foreground_process().is_some_and(|(_, path, title)| {
        matches!(
            crate::games::classify(&path, &title, &[]),
            crate::games::Foreground::Game(_)
        )
    })
}

impl UpdateRuntime {
    pub fn new(
        data: PathBuf,
        engine: Engine,
        current: String,
        configured: bool,
        installing: Arc<RwLock<bool>>,
    ) -> Arc<Self> {
        let memory = Memory::load(&data);
        let installable = std::env::current_exe()
            .map(|exe| winutil::is_installed_build(&exe))
            .unwrap_or(false);
        Arc::new(Self {
            status: Mutex::new(UpdateStatus {
                current,
                configured,
                installable,
                last_checked: memory.last_checked,
                ..Default::default()
            }),
            memory: Mutex::new(memory),
            pending: Mutex::new(None),
            busy: AtomicBool::new(false),
            install_after: AtomicBool::new(false),
            sim_dismissed: AtomicBool::new(false),
            before_exit: Mutex::new(None),
            installing,
            data,
            engine,
        })
    }
    pub fn set_before_exit(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.before_exit.lock().unwrap() = Some(Arc::new(hook));
    }
    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap().clone()
    }
    /// Change the status, decide whether the pop-up shows, tell the interface.
    fn set(&self, app: &AppHandle, change: impl FnOnce(&mut UpdateStatus)) -> UpdateStatus {
        let settings = self.engine.settings().updates;
        let preview = matches!(
            *self.pending.lock().unwrap(),
            Some(Pending::Simulated { .. })
        );
        let status = {
            let mut s = self.status.lock().unwrap();
            change(&mut s);
            let offered = matches!(s.stage, Stage::Available | Stage::Ready);
            s.prompt = match (&s.latest, offered) {
                (Some(_), true) if preview => {
                    settings.notify && !self.sim_dismissed.load(Ordering::Relaxed)
                }
                (Some(v), true) => {
                    updates::should_prompt(now_ms(), v, &self.memory.lock().unwrap(), &settings)
                }
                _ => false,
            };
            s.clone()
        };
        let _ = app.emit("update-status", &status);
        status
    }
    /// Re-evaluate the pop-up after preferences changed.
    pub fn refresh(&self, app: &AppHandle) -> UpdateStatus {
        self.set(app, |_| {})
    }
    fn remember(&self, change: impl FnOnce(&mut Memory)) {
        let mut m = self.memory.lock().unwrap();
        change(&mut m);
        m.save(&self.data);
    }
}

/// Ask GitHub whether a newer signed release exists. One check at a time: a
/// second call while one runs just returns the current status.
pub async fn check(app: AppHandle, rt: Arc<UpdateRuntime>) -> UpdateStatus {
    let stage = rt.status().stage;
    if matches!(
        stage,
        Stage::Downloading | Stage::Verifying | Stage::Installing
    ) || rt.busy.swap(true, Ordering::SeqCst)
    {
        return rt.status();
    }
    let busy = Busy(&rt.busy);
    if !rt.status().configured {
        return rt.set(&app, |s| {
            s.stage = Stage::Failed;
            s.error = Some("This build has no update channel configured.".into());
        });
    }
    rt.set(&app, |s| {
        s.stage = Stage::Checking;
        s.error = None;
        s.detail = None;
        s.waiting = false;
    });
    let (url, test) = endpoint();
    let result = async {
        let url = if test {
            url
        } else {
            let Some(url) = published_endpoint(&rt.status().current).await? else {
                return Ok(None);
            };
            url
        };
        let mut builder = app.updater_builder().timeout(Duration::from_secs(20));
        if let Some(hook) = rt.before_exit.lock().unwrap().clone() {
            builder = builder.on_before_exit(move || hook());
        }
        builder
            .endpoints(vec![url.parse().map_err(|_| {
                tauri_plugin_updater::Error::Network("invalid update address".into())
            })?])?
            .build()?
            .check()
            .await
    }
    .await;
    let now = now_ms();
    let current = rt.status().current;
    let status = match result {
        Ok(Some(update)) => {
            let source_ok = if test {
                update
                    .download_url
                    .as_str()
                    .starts_with("http://127.0.0.1:")
            } else {
                update
                    .download_url
                    .as_str()
                    .starts_with(updates::DOWNLOAD_PREFIX)
            };
            if !source_ok {
                rt.remember(|m| {
                    m.last_checked = Some(now);
                    m.failures += 1;
                });
                rt.set(&app, |s| {
                    s.stage = Stage::Failed;
                    s.last_checked = Some(now);
                    s.error = Some(
                        "The update pointed to an unexpected download address and was ignored."
                            .into(),
                    );
                })
            } else if !updates::compatible(&current, &update.version) {
                // The updater already refuses downgrades; this is a second guard.
                *rt.pending.lock().unwrap() = None;
                rt.remember(|m| {
                    m.last_checked = Some(now);
                    m.failures = 0;
                });
                rt.set(&app, |s| {
                    s.stage = Stage::UpToDate;
                    s.latest = None;
                    s.last_checked = Some(now);
                    s.simulated = test;
                })
            } else {
                let (version, notes) = (update.version.clone(), update.body.clone());
                let date = update.date.map(|d| d.date().to_string());
                *rt.pending.lock().unwrap() = Some(Pending::Real {
                    update: Box::new(update),
                    bytes: None,
                });
                rt.remember(|m| {
                    m.last_checked = Some(now);
                    m.failures = 0;
                    m.latest_seen = Some(version.clone());
                });
                rt.set(&app, |s| {
                    s.stage = Stage::Available;
                    s.release_url = Some(format!("{}/tag/v{version}", updates::RELEASES_PAGE));
                    s.latest = Some(version);
                    s.notes = notes;
                    s.date = date;
                    s.downloaded = 0;
                    s.total = None;
                    s.last_checked = Some(now);
                    s.simulated = test;
                    s.detail = test.then(|| "Test data from a local test server.".into());
                })
            }
        }
        Ok(None) => {
            *rt.pending.lock().unwrap() = None;
            rt.remember(|m| {
                m.last_checked = Some(now);
                m.failures = 0;
            });
            rt.set(&app, |s| {
                s.stage = Stage::UpToDate;
                s.latest = None;
                s.notes = None;
                s.last_checked = Some(now);
                s.simulated = test;
                s.detail =
                    Some("No newer signed release was found on this build's channel.".into());
            })
        }
        Err(e) => {
            rt.remember(|m| {
                m.last_checked = Some(now);
                m.failures += 1;
            });
            let text = user_error(&e);
            rt.set(&app, |s| {
                s.stage = Stage::Failed;
                s.error = Some(text);
                s.last_checked = Some(now);
                s.simulated = test;
            })
        }
    };
    drop(busy);
    if status.stage == Stage::Available && rt.engine.settings().updates.auto_download {
        let (app, rt) = (app.clone(), rt.clone());
        tauri::async_runtime::spawn(async move {
            let _ = download(app, rt).await;
        });
    }
    status
}

/// Download the found update and verify its signature. Progress comes from
/// the real transfer. One download at a time.
pub async fn download(app: AppHandle, rt: Arc<UpdateRuntime>) -> Result<(), String> {
    if rt.busy.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let _busy = Busy(&rt.busy);
    let pending = rt.pending.lock().unwrap().take();
    match pending {
        Some(Pending::Real {
            mut update,
            bytes: None,
        }) => {
            rt.set(&app, |s| {
                s.stage = Stage::Downloading;
                s.downloaded = 0;
                s.total = None;
                s.error = None;
                s.detail = None;
            });
            // The whole transfer may take a while on a slow connection.
            update.timeout = Some(Duration::from_secs(30 * 60));
            let (progress_app, progress_rt) = (app.clone(), rt.clone());
            let (finish_app, finish_rt) = (app.clone(), rt.clone());
            let mut received = 0u64;
            let mut shown = Instant::now() - Duration::from_secs(1);
            let result = update
                .download(
                    move |chunk, total| {
                        received += chunk as u64;
                        // At most ten interface updates a second.
                        if shown.elapsed() >= Duration::from_millis(100) || Some(received) == total
                        {
                            shown = Instant::now();
                            progress_rt.set(&progress_app, |s| {
                                s.downloaded = received;
                                s.total = total;
                            });
                        }
                    },
                    move || {
                        finish_rt.set(&finish_app, |s| s.stage = Stage::Verifying);
                    },
                )
                .await;
            match result {
                Ok(bytes) => {
                    let size = bytes.len() as u64;
                    *rt.pending.lock().unwrap() = Some(Pending::Real {
                        update,
                        bytes: Some(bytes),
                    });
                    rt.set(&app, |s| {
                        s.stage = Stage::Ready;
                        s.downloaded = size;
                        s.total = Some(size);
                    });
                }
                Err(e) => {
                    // Keep the offer so "Try again" can download it again.
                    *rt.pending.lock().unwrap() = Some(Pending::Real {
                        update,
                        bytes: None,
                    });
                    let text = user_error(&e);
                    rt.install_after.store(false, Ordering::Relaxed);
                    rt.set(&app, |s| {
                        s.stage = Stage::Failed;
                        s.error = Some(text.clone());
                    });
                    return Err(text);
                }
            }
        }
        Some(Pending::Simulated { bad_signature, .. }) => {
            // Test data: paced numbers, nothing is transferred.
            const TOTAL: u64 = 61_138_175;
            rt.set(&app, |s| {
                s.stage = Stage::Downloading;
                s.downloaded = 0;
                s.total = Some(TOTAL);
                s.error = None;
            });
            for step in 1..=40u64 {
                tokio_sleep(Duration::from_millis(90)).await;
                rt.set(&app, |s| s.downloaded = TOTAL * step / 40);
            }
            rt.set(&app, |s| s.stage = Stage::Verifying);
            tokio_sleep(Duration::from_millis(700)).await;
            if bad_signature {
                *rt.pending.lock().unwrap() = Some(Pending::Simulated {
                    bad_signature,
                    ready: false,
                });
                rt.install_after.store(false, Ordering::Relaxed);
                let text = updates::friendly_error("invalid signature");
                rt.set(&app, |s| {
                    s.stage = Stage::Failed;
                    s.error = Some(text.clone());
                });
                return Err(text);
            }
            *rt.pending.lock().unwrap() = Some(Pending::Simulated {
                bad_signature,
                ready: true,
            });
            rt.set(&app, |s| s.stage = Stage::Ready);
        }
        other => {
            *rt.pending.lock().unwrap() = other;
            return Ok(());
        }
    }
    if rt.install_after.swap(false, Ordering::Relaxed) {
        let (app, rt) = (app.clone(), rt.clone());
        // The installer start blocks; keep it off the async workers.
        let _ = tauri::async_runtime::spawn_blocking(move || install(&app, &rt, true)).await;
    }
    Ok(())
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || thread::sleep(d)).await;
}

/// "Update now": download (if needed) and install as soon as it is verified.
pub fn start(app: AppHandle, rt: Arc<UpdateRuntime>, install_when_ready: bool) {
    rt.install_after
        .store(install_when_ready, Ordering::Relaxed);
    if rt.status().stage == Stage::Ready {
        if install_when_ready {
            rt.install_after.store(false, Ordering::Relaxed);
            thread::spawn(move || {
                let _ = install(&app, &rt, true);
            });
        }
        return;
    }
    tauri::async_runtime::spawn(async move {
        let _ = download(app, rt).await;
    });
}

/// Install the downloaded, verified update. Refused while anything would be
/// interrupted; Instant Replay alone is paused (with `pause_replay`) and
/// switched back on after the restart. On success the app exits and the
/// installer starts the new version.
pub fn install(app: &AppHandle, rt: &Arc<UpdateRuntime>, pause_replay: bool) -> Result<(), String> {
    if rt.status().stage != Stage::Ready {
        return Err("No downloaded update is ready to install.".into());
    }
    let simulated = matches!(
        *rt.pending.lock().unwrap(),
        Some(Pending::Simulated { ready: true, .. })
    );
    if simulated {
        rt.set(app, |s| {
            s.stage = Stage::Installing;
            s.waiting = false;
            s.detail = Some("Test data: nothing is being installed.".into());
        });
        thread::sleep(Duration::from_millis(1800));
        *rt.pending.lock().unwrap() = None;
        rt.set(app, |s| {
            s.stage = Stage::Idle;
            s.latest = None;
            s.notes = None;
            s.detail = Some(
                "Simulation finished. Nothing was downloaded or installed; this copy is unchanged."
                    .into(),
            );
        });
        return Ok(());
    }
    if !rt.status().installable {
        let text = "This is a development build: the update was downloaded and its signature verified, but it is never installed over your installed REPLAYX.";
        rt.set(app, |s| s.detail = Some(text.into()));
        return Err(text.into());
    }
    let wait = |why: &str| {
        let text = format!("{why} The update is ready and will wait.");
        rt.set(app, |s| {
            s.waiting = true;
            s.detail = Some(text.clone());
        });
        text
    };
    let mut installing = rt.installing.write().unwrap();
    let mut resumed_replay = false;
    match updates::safety(&capture_state(&rt.engine)) {
        Safety::Blocked(why) => return Err(wait(why)),
        Safety::ReplayOnly if !pause_replay => return Err(wait("Instant Replay is on.")),
        Safety::ReplayOnly => {
            // Safe hand-off: finish buffering cleanly, install, switch it back on.
            rt.engine.stop_replay().map_err(|e| e.to_string())?;
            resumed_replay = true;
            rt.remember(|m| m.resume_replay_at = Some(now_ms()));
        }
        Safety::Safe => {}
    }
    // Stopping replay must leave nothing running (a recording may have started).
    if updates::safety(&capture_state(&rt.engine)) != Safety::Safe {
        if resumed_replay {
            rt.remember(|m| m.resume_replay_at = None);
            let _ = rt.engine.start_replay();
        }
        return Err(wait("Capture is busy."));
    }
    *installing = true;
    rt.set(app, |s| {
        s.stage = Stage::Installing;
        s.waiting = false;
        s.detail = None;
    });
    let pending = rt.pending.lock().unwrap().take();
    let result = match &pending {
        Some(Pending::Real {
            update,
            bytes: Some(bytes),
        }) => update.install(bytes).map_err(|e| e.to_string()),
        _ => Err("The downloaded update is no longer available.".into()),
    };
    // Only reached when the installer could not be started.
    *rt.pending.lock().unwrap() = pending;
    *installing = false;
    if resumed_replay {
        rt.remember(|m| m.resume_replay_at = None);
        let _ = rt.engine.start_replay();
    }
    let text = match result {
        Err(e) if e.to_ascii_lowercase().contains("signature") => updates::friendly_error(&e),
        _ => "The installer could not be started. Nothing was changed; try again, or download the installer from GitHub.".to_string(),
    };
    rt.set(app, |s| {
        s.stage = Stage::Failed;
        s.error = Some(text.clone());
    });
    Err(text)
}

/// "Not now": hide the pop-up for this version for a few days.
pub fn dismiss(app: &AppHandle, rt: &Arc<UpdateRuntime>) -> UpdateStatus {
    let status = rt.status();
    if matches!(*rt.pending.lock().unwrap(), Some(Pending::Simulated { .. })) {
        rt.sim_dismissed.store(true, Ordering::Relaxed);
    } else if let Some(v) = status.latest {
        rt.remember(|m| {
            m.dismissed_version = Some(v);
            m.dismissed_at = Some(now_ms());
        });
    }
    rt.refresh(app)
}

/// Development preview of the update interface with test data. Available in
/// development builds only; never touches the network or the installed app.
pub fn simulate(
    app: &AppHandle,
    rt: &Arc<UpdateRuntime>,
    scenario: &str,
) -> Result<UpdateStatus, String> {
    if !cfg!(debug_assertions) {
        return Err("The update preview exists in development builds only.".into());
    }
    if rt.busy.load(Ordering::SeqCst) {
        return Err("Wait for the running check or download to finish.".into());
    }
    let current = rt.status().current;
    // The next version after this one, e.g. 0.3.1-alpha.6 -> 0.3.1-alpha.7.
    let next = match current.rsplit_once('.') {
        Some((head, tail)) if tail.parse::<u64>().is_ok() => {
            format!("{head}.{}", tail.parse::<u64>().unwrap() + 1)
        }
        _ => format!("{current}-test.1"),
    };
    rt.sim_dismissed.store(false, Ordering::Relaxed);
    rt.install_after.store(false, Ordering::Relaxed);
    let now = now_ms();
    Ok(match scenario {
        "available" | "invalid_signature" => {
            *rt.pending.lock().unwrap() = Some(Pending::Simulated {
                bad_signature: scenario == "invalid_signature",
                ready: false,
            });
            rt.set(app, |s| {
                s.stage = Stage::Available;
                s.latest = Some(next.clone());
                s.notes = Some(
                    "## What's New\n\n- TEST DATA: this is a simulated update, not a real release\n- Example: smoother clip saving\n- Example: fixes for audio devices\n- Example: smaller downloads".into(),
                );
                s.date = Some(chrono::Local::now().format("%Y-%m-%d").to_string());
                s.release_url = Some(updates::RELEASES_PAGE.into());
                s.downloaded = 0;
                s.total = None;
                s.error = None;
                s.detail = Some("Test data: a simulated update. Nothing is downloaded or installed.".into());
                s.simulated = true;
                s.waiting = false;
                s.last_checked = Some(now);
            })
        }
        "up_to_date" => {
            *rt.pending.lock().unwrap() = None;
            rt.set(app, |s| {
                s.stage = Stage::UpToDate;
                s.latest = None;
                s.notes = None;
                s.error = None;
                s.detail = Some("Test data: simulated result.".into());
                s.simulated = true;
                s.last_checked = Some(now);
            })
        }
        "failed" => {
            *rt.pending.lock().unwrap() = None;
            rt.set(app, |s| {
                s.stage = Stage::Failed;
                s.latest = None;
                s.error = Some(updates::friendly_error("error sending request (dns)"));
                s.detail = Some("Test data: simulated failure.".into());
                s.simulated = true;
                s.last_checked = Some(now);
            })
        }
        "reset" => {
            *rt.pending.lock().unwrap() = None;
            let last = rt.memory.lock().unwrap().last_checked;
            rt.set(app, |s| {
                *s = UpdateStatus {
                    current: s.current.clone(),
                    configured: s.configured,
                    installable: s.installable,
                    last_checked: last,
                    ..Default::default()
                };
            })
        }
        _ => return Err("Unknown preview scenario".into()),
    })
}

/// Background work: resume Instant Replay after an update restart, check on
/// schedule, show the tray notice once, install automatically when allowed.
/// Sleeps between steps; never blocks start-up, capture or the interface.
pub fn spawn_loop(app: AppHandle, rt: Arc<UpdateRuntime>, notices: Overlay) {
    thread::spawn(move || {
        // Instant Replay was paused for the update that just installed.
        let resume = {
            let mut m = rt.memory.lock().unwrap();
            let resume = updates::resume_replay(now_ms(), &m);
            if m.resume_replay_at.take().is_some() {
                m.save(&rt.data);
            }
            resume
        };
        if resume && !rt.engine.status().replay {
            thread::sleep(Duration::from_secs(3));
            if let Err(e) = rt.engine.start_replay() {
                rt.engine.update(|i| {
                    i.status.error = Some(format!(
                        "Instant Replay could not be switched back on after the update: {e:#}"
                    ))
                });
            }
        }
        // Well after the window is up: the first check never slows start-up.
        thread::sleep(Duration::from_secs(20));
        loop {
            let settings = rt.engine.settings().updates;
            let status = rt.status();
            let idle_stage = matches!(
                status.stage,
                Stage::Idle | Stage::UpToDate | Stage::Failed | Stage::Available
            );
            let due = updates::check_due(now_ms(), &rt.memory.lock().unwrap(), &settings);
            if status.configured && idle_stage && !status.simulated && due {
                tauri::async_runtime::block_on(check(app.clone(), rt.clone()));
            }
            let status = rt.status();
            // A discreet notice when REPLAYX is in the tray: once per version,
            // never over a game, never taking focus.
            if settings.notify && status.stage == Stage::Available && !status.simulated {
                if let Some(version) = status.latest.clone() {
                    let hidden = app.get_webview_window("main").is_none_or(|w| {
                        !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false)
                    });
                    let new = rt.memory.lock().unwrap().notified_version.as_deref()
                        != Some(version.as_str());
                    if hidden && new && !game_in_front() {
                        notices.show(Notice {
                            title: "REPLAYX update available".into(),
                            subtitle: format!("Version {version} · open REPLAYX to update"),
                            active: true,
                            hold_ms: 5000,
                            ..Default::default()
                        });
                        rt.remember(|m| m.notified_version = Some(version));
                    }
                }
            }
            // Automatic install: only with the user's switch, a verified
            // download, nothing to interrupt, no game in front and an idle PC.
            if status.stage == Stage::Ready && !status.simulated && settings.auto_install {
                let ok = status.installable
                    && updates::auto_install_ok(
                        &capture_state(&rt.engine),
                        game_in_front(),
                        crate::playtime::idle_seconds(),
                        &settings,
                    );
                if ok {
                    let _ = install(&app, &rt, false);
                } else if status.installable && !status.waiting {
                    rt.set(&app, |s| {
                        s.waiting = true;
                        s.detail = Some("Waiting until recording, Instant Replay and saves are stopped, no game is in front, and the PC is idle.".into());
                    });
                }
            }
            thread::sleep(Duration::from_secs(30));
        }
    });
}
