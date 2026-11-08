use crate::{
    audio,
    engine::{Engine, Status},
    library,
    media::{command, Media},
    model::{AudioSource, Settings},
    winutil,
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;
type Reply<T> = Result<T, String>;
/// Persistent clip index plus background-probe bookkeeping.
struct Library {
    index: library::Index,
    path: PathBuf,
    probing: bool,
    last_index_ms: u64,
}
impl Library {
    fn save(&mut self) -> anyhow::Result<()> {
        self.index.save(&self.path)
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone)]
struct AppState {
    engine: Engine,
    updating: Arc<Mutex<bool>>,
    icons: Arc<Mutex<HashMap<String, Option<String>>>>,
    testing_audio: Arc<Mutex<bool>>,
    /// Set by "Stop test" to end a microphone/source test early.
    test_stop: Arc<std::sync::atomic::AtomicBool>,
    /// Set to end live microphone monitoring ("hear yourself").
    monitor_stop: Arc<std::sync::atomic::AtomicBool>,
    /// Live monitoring gain (f32 bits), adjustable while listening.
    monitor_gain: Arc<std::sync::atomic::AtomicU32>,
    app: tauri::AppHandle,
    library: Arc<Mutex<Library>>,
    thumbnail_gate: Arc<Mutex<()>>,
    data_dir: PathBuf,
}
impl AppState {
    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }
    /// Record an error for the UI (the engine lock is held only for the assignment).
    fn report(&self, error: String) {
        self.engine.update(|i| i.status.error = Some(error));
    }
}
// Synchronous Tauri commands run on the window's UI thread. `status` and
// `settings` never wait for the engine lock (published snapshots), so a busy
// engine can no longer freeze the window. Every command that may lock the
// engine is async and runs on a worker thread.
#[tauri::command]
fn status(state: State<AppState>) -> Status {
    state.engine.status()
}
#[tauri::command]
fn settings(state: State<AppState>) -> Settings {
    state.engine.settings()
}
#[tauri::command]
async fn action(name: String, state: State<'_, AppState>) -> Reply<serde_json::Value> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || perform(&state, &name))
        .await
        .map_err(err)?
}
fn perform(state: &AppState, name: &str) -> Reply<serde_json::Value> {
    // Check, don't hold: holding this guard for a whole action serialized every
    // action, so Stop/Cancel pressed during a slow start waited for the start.
    if *state.updating.lock().unwrap() {
        return Err("Update installation is in progress".into());
    }
    let engine = &state.engine;
    let result = match name {
        "start_replay" => engine.start_replay(),
        "stop_replay" => engine.stop_replay(),
        "start_recording" => engine.start_recording(),
        "stop_recording" if engine.cancel_pending_recording() => {
            // Stop pressed while capture was still starting: nothing recorded.
            return Ok(serde_json::json!({"cancelled": true}));
        }
        "save_replay" | "stop_recording" => {
            // Acknowledge at once: progress and the finished clip arrive as
            // clip-save events, so saves never block the UI or each other.
            let id = uuid::Uuid::new_v4().to_string();
            let began = std::time::Instant::now();
            let emitter = state.clone();
            let save_id = id.clone();
            let emit = move |stage: &str, clip: Option<library::Clip>, error: Option<String>| {
                let _ = emitter.app.emit(
                    "clip-save",
                    serde_json::json!({"id":save_id,"stage":stage,"clip":clip,"error":error,"elapsed_ms":began.elapsed().as_millis() as u64}),
                );
            };
            emit(
                if name == "save_replay" {
                    "Saving"
                } else {
                    "Finishing recording"
                },
                None,
                None,
            );
            let job = if name == "save_replay" {
                engine.save_replay()
            } else {
                engine.stop_recording()
            };
            let job = match job {
                Ok(job) => job,
                Err(e) => {
                    emit("Failed", None, Some(e.to_string()));
                    return Err(err(e));
                }
            };
            let worker = state.clone();
            thread::spawn(move || {
                let result = job.run_progress(|stage| {
                    if stage != "Ready" {
                        emit(stage, None, None);
                    }
                });
                worker.engine.complete_save(&result);
                match &result {
                    Ok((path, probe)) => {
                        let root = worker.engine.settings().output_dir;
                        let clip = library::id_for(&root, path).and_then(|id| {
                            let meta = fs::metadata(path).ok()?;
                            let mut lib = worker.library.lock().unwrap();
                            lib.index.insert(&id, &meta, probe.clone());
                            let _ = lib.save();
                            lib.index.clip(&root, &id)
                        });
                        emit("Ready", clip, None);
                    }
                    Err(e) => emit("Failed", None, Some(format!("{e:#}"))),
                }
            });
            return Ok(serde_json::json!({"id": id, "accepted": true}));
        }
        "stop_all" => engine.shutdown(),
        "recover" => return engine.recover().map(|v| serde_json::json!(v)).map_err(err),
        "discard_sessions" => {
            return engine
                .discard_sessions()
                .map(|n| serde_json::json!({"moved": n}))
                .map_err(err)
        }
        _ => return Err("Unknown action".into()),
    };
    result.map(|_| serde_json::json!({"ok":true})).map_err(err)
}
fn bind_hotkeys(app: &tauri::AppHandle, s: &Settings) -> Reply<Option<String>> {
    let keys = [
        &s.replay_hotkey,
        &s.recording_hotkey,
        &s.mute_hotkey,
        &s.pause_hotkey,
    ];
    let shortcuts: Vec<_> = keys
        .iter()
        .map(|key| Shortcut::from_str(key).map_err(err))
        .collect::<Reply<_>>()?;
    for (index, shortcut) in shortcuts.iter().enumerate() {
        if shortcuts[..index].contains(shortcut) {
            return Err(
                "Hotkeys resolve to the same shortcut; choose distinct combinations".into(),
            );
        }
    }
    app.global_shortcut().unregister_all().map_err(err)?;
    let labels = [
        "Save replay",
        "Start/stop recording",
        "Mute microphone",
        "Start/stop replay",
    ];
    let mut conflicts = Vec::new();
    for (index, shortcut) in shortcuts.into_iter().enumerate() {
        if app.global_shortcut().register(shortcut).is_err() {
            conflicts.push(format!("{} ({})", labels[index], keys[index]));
        }
    }
    Ok(if conflicts.is_empty() {
        None
    } else {
        Some(format!("{} unavailable. Other shortcuts and the on-screen buttons still work. Change the conflicting keys in Settings.", conflicts.join(", ")))
    })
}
#[tauri::command]
async fn save_settings(
    settings: Settings,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Reply<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || save_settings_blocking(settings, &app, &state))
        .await
        .map_err(err)?
}
fn save_settings_blocking(
    settings: Settings,
    app: &tauri::AppHandle,
    state: &AppState,
) -> Reply<()> {
    settings.validate().map_err(err)?;
    let previous = state.engine.settings();
    let mut settings = settings;
    settings.tray_hint_shown |= previous.tray_hint_shown;
    if state.engine.status().replay
        || state.engine.status().recording
        || state.engine.status().saving > 0
    {
        return Err("Stop capture before changing settings".into());
    }
    if settings.launch_at_startup != previous.launch_at_startup
        || (settings.launch_at_startup && !winutil::autostart_enabled())
    {
        winutil::set_autostart(settings.launch_at_startup).map_err(err)?;
    }
    let warning = bind_hotkeys(app, &settings)?;
    if let Err(e) = state.engine.set_settings(settings) {
        let _ = bind_hotkeys(app, &previous);
        return Err(err(e));
    }
    state.engine.update(|i| i.status.hotkey_error = warning);
    Ok(())
}
#[tauri::command]
async fn set_mix(index: usize, volume: f32, muted: bool, state: State<'_, AppState>) -> Reply<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.engine.set_mix(index, volume, muted))
        .await
        .map_err(err)?
        .map_err(err)
}
#[tauri::command]
async fn capabilities() -> Reply<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(||{
    let devices=crate::audio::devices().map_err(err)?;
    let available=Media::discover().and_then(|m|{m.preflight()?;Ok(m)});
    let (encoders,codecs,media_error)=match available {Ok(m)=>(m.encoders(),m.codecs(),None),Err(e)=>(Vec::new(),Vec::new(),Some(format!("{e:#}")))};
    Ok(serde_json::json!({"encoders":encoders,"codecs":codecs,"media_error":media_error,"displays":crate::geometry::displays(),"devices":devices,"wgc":false,"pause_recording":false,"monitoring":false,"updates_configured":update_configured()}))
}).await.map_err(err)?
}
#[tauri::command]
async fn processes() -> Reply<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(||{
    let sys=sysinfo::System::new_all();let mut list:Vec<_>=sys.processes().iter().filter(|(pid,_)|pid.as_u32()>4).map(|(pid,p)|serde_json::json!({"pid":pid.as_u32(),"name":p.name().to_string_lossy(),"started":p.start_time()})).collect();list.sort_by_key(|p|p["name"].as_str().unwrap_or("").to_lowercase());Ok(serde_json::json!(list))
}).await.map_err(err)?
}
#[tauri::command]
async fn audio_sessions() -> Reply<Vec<winutil::AudioSession>> {
    tauri::async_runtime::spawn_blocking(|| winutil::audio_sessions().map_err(err))
        .await
        .map_err(err)?
}
/// PNG data URL of an executable's icon. Only existing local .exe files.
#[tauri::command]
async fn app_icon(path: String, state: State<'_, AppState>) -> Reply<Option<String>> {
    if let Some(hit) = state.icons.lock().unwrap().get(&path) {
        return Ok(hit.clone());
    }
    let icons = state.icons.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        let url = (p.is_absolute()
            && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            && p.is_file())
        .then(|| winutil::exe_icon_png(p))
        .flatten()
        .map(|png| format!("data:image/png;base64,{}", winutil::base64(&png)));
        icons.lock().unwrap().insert(path, url.clone());
        Ok(url)
    })
    .await
    .map_err(err)?
}
/// Cached JPEG thumbnail of a clip as a data URL. Cache hits never wait for
/// another thumbnail being generated; only generation is serialized.
#[tauri::command]
async fn clip_thumbnail(id: String, state: State<'_, AppState>) -> Reply<String> {
    let root = state.engine.settings().output_dir;
    let cache = state.data_dir().join("thumbnails");
    let gate = state.thumbnail_gate.clone();
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = library::resolve(&root, &id).map_err(err)?;
        let meta = fs::metadata(&path).map_err(err)?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stem: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .take(100)
            .collect();
        let file = cache.join(format!("{stem}-{modified}-{}.jpg", meta.len()));
        if let Ok(bytes) = fs::read(&file) {
            return Ok(format!(
                "data:image/jpeg;base64,{}",
                winutil::base64(&bytes)
            ));
        }
        // A grid of visible cards must not start dozens of decoders at once.
        let _permit = gate.lock().unwrap();
        if let Ok(bytes) = fs::read(&file) {
            return Ok(format!(
                "data:image/jpeg;base64,{}",
                winutil::base64(&bytes)
            ));
        }
        let media = Media::discover().map_err(err)?;
        let duration = library
            .lock()
            .unwrap()
            .index
            .clip(&root, &id)
            .and_then(|c| c.probe)
            .map(|p| p.duration)
            .unwrap_or(0.);
        let bytes = media.thumbnail(&path, duration, 480).map_err(err)?;
        fs::create_dir_all(&cache).map_err(err)?;
        prune_cache(&cache, 400);
        fs::write(&file, &bytes).map_err(err)?;
        Ok(format!(
            "data:image/jpeg;base64,{}",
            winutil::base64(&bytes)
        ))
    })
    .await
    .map_err(err)?
}
/// Keep at most `keep` files, removing the least recently modified.
fn prune_cache(dir: &std::path::Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    for (_, path) in files.iter().take(files.len() - keep) {
        let _ = fs::remove_file(path);
    }
}
/// Record a few seconds from one source through the real capture path,
/// emitting `audio-test-level` events, and keep the result as a WAV file.
#[tauri::command]
async fn audio_test(
    source: AudioSource,
    seconds: u64,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Reply<audio::TestCapture> {
    {
        let mut busy = state.testing_audio.lock().unwrap();
        if *busy {
            return Err("An audio test is already running".into());
        }
        *busy = true;
    }
    let dir = state.data_dir().join("tests");
    let file = dir.join("audio-test.wav");
    let flag = state.testing_audio.clone();
    let stop = state.test_stop.clone();
    stop.store(false, std::sync::atomic::Ordering::Relaxed);
    let ffmpeg = Media::discover().ok().map(|m| m.ffmpeg);
    let result = tauri::async_runtime::spawn_blocking(move || {
        // The same processing the recording will use (boost, filters, limiter),
        // so what the user hears is what gets recorded.
        let chain = crate::media::source_chain(&source);
        let capture = audio::record_test(source, seconds.clamp(2, 15), &stop, |peak, elapsed| {
            let _ = app.emit("audio-test-level", (peak, elapsed));
        })
        .map_err(err)?;
        fs::create_dir_all(&dir).map_err(err)?;
        let raw = dir.join("audio-test-raw.wav");
        audio::write_wav16(&raw, &capture.pcm).map_err(err)?;
        let _ = fs::remove_file(&file);
        match (&ffmpeg, chain.is_empty()) {
            (Some(ffmpeg), false) => {
                let status = command(ffmpeg)
                    .args(["-hide_banner", "-v", "error", "-y", "-i"])
                    .arg(&raw)
                    .args(["-af", &chain, "-c:a", "pcm_s16le"])
                    .arg(&file)
                    .status()
                    .map_err(err)?;
                let _ = fs::remove_file(&raw);
                if !status.success() {
                    return Err("Could not apply the microphone processing to the test".into());
                }
            }
            _ => fs::rename(&raw, &file).map_err(err)?,
        }
        Ok(capture)
    })
    .await
    .map_err(err);
    *flag.lock().unwrap() = false;
    result?
}
/// Audio devices right now (cheap; no encoder probes).
#[tauri::command]
async fn audio_devices() -> Reply<Vec<audio::AudioDevice>> {
    tauri::async_runtime::spawn_blocking(|| audio::devices().map_err(err))
        .await
        .map_err(err)?
}
/// Start "hear yourself": the microphone played live on `output` ("" = the
/// Windows default). Emits `mic-monitor-level` and, when it ends,
/// `mic-monitor-ended` (with the error, if any). Nothing is recorded.
#[tauri::command]
fn mic_monitor_start(
    source: AudioSource,
    output: String,
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Reply<()> {
    let status = state.engine.status();
    if (status.replay || status.recording) && audio::monitor_leaks(&state.engine.settings().audio) {
        return Err("While capture records System audio, hearing yourself would end up in the clip. Turn off Instant Replay to test your mic.".into());
    }
    {
        let mut busy = state.testing_audio.lock().unwrap();
        if *busy {
            return Err("A microphone test or monitor is already running".into());
        }
        *busy = true;
    }
    let flag = state.testing_audio.clone();
    let stop = state.monitor_stop.clone();
    stop.store(false, std::sync::atomic::Ordering::Relaxed);
    let gain = state.monitor_gain.clone();
    gain.store(
        audio::monitor_level(source.volume, source.boost_db).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
    thread::spawn(move || {
        let level_app = app.clone();
        let result = audio::monitor(source, &output, &stop, &gain, |peak| {
            let _ = level_app.emit("mic-monitor-level", peak);
        });
        *flag.lock().unwrap() = false;
        let _ = app.emit(
            "mic-monitor-ended",
            serde_json::json!({"error": result.err().map(|e| format!("{e:#}"))}),
        );
    });
    Ok(())
}
/// Change volume/boost while listening.
#[tauri::command]
fn mic_monitor_set(volume: f32, boost_db: f32, state: State<AppState>) {
    state.monitor_gain.store(
        audio::monitor_level(volume, boost_db).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
}
#[tauri::command]
fn mic_monitor_stop(state: State<AppState>) {
    state
        .monitor_stop
        .store(true, std::sync::atomic::Ordering::Relaxed);
}
/// End a running audio test early ("Stop test").
#[tauri::command]
fn audio_test_stop(state: State<AppState>) {
    state
        .test_stop
        .store(true, std::sync::atomic::Ordering::Relaxed);
}
/// The last test recording as a WAV data URL for in-app playback.
#[tauri::command]
async fn audio_test_data(state: State<'_, AppState>) -> Reply<String> {
    let file = state.data_dir().join("tests").join("audio-test.wav");
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = fs::read(&file).map_err(|_| "Run a test recording first".to_string())?;
        Ok(format!("data:audio/wav;base64,{}", winutil::base64(&bytes)))
    })
    .await
    .map_err(err)?
}
/// Delete test recordings (closing the test, and at start-up): microphone
/// tests are never kept.
#[tauri::command]
fn audio_test_discard(state: State<AppState>) {
    discard_audio_tests(&state.data_dir());
}
fn discard_audio_tests(data: &std::path::Path) {
    for name in ["audio-test.wav", "audio-test-raw.wav"] {
        let _ = fs::remove_file(data.join("tests").join(name));
    }
}
#[tauri::command]
fn open_folder(state: State<AppState>) -> Reply<()> {
    let root = state.engine.settings().output_dir;
    fs::create_dir_all(&root).map_err(err)?;
    command("explorer.exe").arg(&root).spawn().map_err(err)?;
    Ok(())
}
/// Clip list from the persistent index. Returns immediately; clips whose
/// metadata is missing or stale are probed in the background and the whole
/// list is re-sent as a `library-updated` event when they are done.
#[tauri::command]
async fn clips(state: State<'_, AppState>) -> Reply<Vec<library::Clip>> {
    let root = state.engine.settings().output_dir;
    let library = state.library.clone();
    let app = state.app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        fs::create_dir_all(&root).map_err(err)?;
        let found = library::scan(&root).map_err(err)?;
        let (clips, stale) = {
            let mut lib = library.lock().unwrap();
            let stale = lib.index.stale(&found);
            (lib.index.clips(&found), stale)
        };
        library.lock().unwrap().last_index_ms = started.elapsed().as_millis() as u64;
        let busy = !stale.is_empty() && !library.lock().unwrap().probing;
        if busy {
            library.lock().unwrap().probing = true;
            thread::spawn(move || {
                let started = std::time::Instant::now();
                let results = Media::discover()
                    .map(|media| {
                        library::probe_parallel(&found, &stale, 4, |p| media.probe(p).ok())
                    })
                    .unwrap_or_default();
                let mut lib = library.lock().unwrap();
                lib.index.apply(&found, results);
                let clips = lib.index.clips(&found);
                let _ = lib.save();
                lib.probing = false;
                lib.last_index_ms = started.elapsed().as_millis() as u64;
                drop(lib);
                let _ = app.emit("library-updated", clips);
            });
        } else {
            let _ = library.lock().unwrap().save();
        }
        Ok(clips)
    })
    .await
    .map_err(err)?
}
#[tauri::command]
async fn clip_action(
    id: String,
    operation: String,
    value: Option<String>,
    state: State<'_, AppState>,
) -> Reply<String> {
    let root = state.engine.settings().output_dir;
    let library = state.library.clone();
    let gate = state.thumbnail_gate.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match operation.as_str() {
            "delete" => {
                // Wait for any thumbnail decode that may be reading this file.
                let _permit = gate.lock().unwrap();
                library::delete_clip(&root, &id, winutil::recycle).map_err(err)?;
                let mut lib = library.lock().unwrap();
                lib.index.remove(&id);
                let _ = lib.save();
                Ok(id)
            }
            "rename" => {
                let path = library::resolve(&root, &id).map_err(err)?;
                let wanted = value.ok_or("New name required")?;
                let ext = path.extension().unwrap().to_string_lossy().to_string();
                let stem = wanted.trim().trim_end_matches(&format!(".{ext}")).to_string();
                let clean = library::sanitize_component(&stem);
                if stem.is_empty() || clean != stem || stem.len() > 150 {
                    return Err("Use letters, numbers, spaces and dashes; these are not allowed: \\ / : * ? \" < > |".into());
                }
                let to = path.with_file_name(format!("{clean}.{ext}"));
                if to.exists() {
                    return Err("A clip with that name already exists in this folder".into());
                }
                if library::in_use(&path) {
                    return Err("The clip is open in another program. Close it and try again.".into());
                }
                fs::rename(&path, &to).map_err(err)?;
                let new_id = match id.split_once('/') {
                    Some((folder, _)) => format!("{folder}/{clean}.{ext}"),
                    None => format!("{clean}.{ext}"),
                };
                library.lock().unwrap().index.remove(&id);
                Ok(new_id)
            }
            "move" => {
                let category = value.ok_or("Choose a game or category")?;
                let _permit = gate.lock().unwrap();
                let new_id = library::move_to_category(&root, &id, &category).map_err(err)?;
                library.lock().unwrap().index.remove(&id);
                Ok(new_id)
            }
            "folder" => {
                let path = library::resolve(&root, &id).map_err(err)?;
                command("explorer.exe")
                    .arg(format!("/select,{}", path.display()))
                    .spawn()
                    .map_err(err)?;
                Ok(id)
            }
            "play" => {
                let path = library::resolve(&root, &id).map_err(err)?;
                open_local(&path)?;
                Ok(id)
            }
            "mp4" => {
                let path = library::resolve(&root, &id).map_err(err)?;
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let dir = path.parent().unwrap();
                let output = library::unique_path(dir, &format!("{stem}-export"), "mp4");
                Media::discover()
                    .map_err(err)?
                    .export_mp4(&path, &output)
                    .map_err(err)?;
                let name = output.file_name().unwrap().to_string_lossy().to_string();
                Ok(match id.split_once('/') {
                    Some((folder, _)) => format!("{folder}/{name}"),
                    None => name,
                })
            }
            _ => Err("Unknown clip action".into()),
        }
    })
    .await
    .map_err(err)?
}
fn open_local(path: &std::path::Path) -> Reply<()> {
    use windows::{
        core::PCWSTR,
        Win32::{UI::Shell::ShellExecuteW, UI::WindowsAndMessaging::SW_SHOWNORMAL},
    };
    let file: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("Windows could not open the clip. Install a player supporting MKV.".into());
    }
    Ok(())
}
fn update_configured() -> bool {
    option_env!("REPLAYX_UPDATE_URL").is_some_and(validate_update_url)
        && option_env!("REPLAYX_UPDATE_PUBKEY").is_some_and(|key| !key.trim().is_empty())
}
pub fn validate_update_url(url: &str) -> bool {
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
#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Reply<serde_json::Value> {
    let url = option_env!("REPLAYX_UPDATE_URL").ok_or(
        "Updates are unavailable until the publisher configures a signed GitHub release endpoint",
    )?;
    if !validate_update_url(url) {
        return Err("Untrusted update endpoint".into());
    }
    let update = app
        .updater_builder()
        .pubkey(option_env!("REPLAYX_UPDATE_PUBKEY").unwrap_or(""))
        .endpoints(vec![url.parse().map_err(err)?])
        .map_err(err)?
        .build()
        .map_err(err)?
        .check()
        .await
        .map_err(err)?;
    Ok(match update {
        Some(u) => serde_json::json!({"available":true,"version":u.version,"notes":u.body}),
        None => serde_json::json!({"available":false}),
    })
}
#[tauri::command]
async fn install_update(app: tauri::AppHandle, state: State<'_, AppState>) -> Reply<()> {
    {
        let mut gate = state.updating.lock().unwrap();
        let s = state.engine.status();
        if *gate
            || s.replay
            || s.recording
            || s.saving > 0
            || s.pending_recording
            || ["Starting", "Stopping", "Recovering", "Recovering sessions"]
                .contains(&s.state.as_str())
        {
            return Err("Stop capture and wait for all saves before installing".into());
        }
        *gate = true;
    }
    let result = async {
        let url = option_env!("REPLAYX_UPDATE_URL").ok_or("Updates are not configured")?;
        if !validate_update_url(url) {
            return Err("Untrusted endpoint".into());
        }
        let update = app
            .updater_builder()
            .pubkey(option_env!("REPLAYX_UPDATE_PUBKEY").unwrap_or(""))
            .endpoints(vec![url.parse().map_err(err)?])
            .map_err(err)?
            .build()
            .map_err(err)?
            .check()
            .await
            .map_err(err)?;
        if let Some(u) = update {
            if !u.download_url.as_str().starts_with("https://github.com/") {
                return Err("Update artifact must use HTTPS on github.com".into());
            }
            u.download_and_install(|_, _| {}, || {})
                .await
                .map_err(err)?;
        }
        Ok(())
    }
    .await;
    *state.updating.lock().unwrap() = false;
    result
}
/// Bring the main window forward, optionally on a page.
fn show_main(app: &tauri::AppHandle, page: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(page) = page {
        let _ = app.emit("navigate", page);
    }
}
/// Tray texts: (replay on, replay label, save enabled, recording label, mic label).
fn tray_labels(status: &Status, settings: &Settings) -> (bool, String, bool, String, String) {
    let view = crate::status_overlay::view(status, settings);
    use crate::status_overlay::{MicState, ReplayState};
    let replay = match view.replay {
        ReplayState::Off => "Instant Replay: off",
        ReplayState::Starting => "Instant Replay: starting…",
        ReplayState::On => "Instant Replay: on",
        ReplayState::Saving => "Instant Replay: saving…",
        ReplayState::Error => "Instant Replay: error",
    };
    let recording = match (status.recording, view.recording) {
        (true, Some(s)) => format!("Stop recording ({}:{:02})", s / 60, s % 60),
        (true, None) => "Stop recording (starting…)".into(),
        _ => "Start recording".into(),
    };
    let mic = match view.mic {
        MicState::None => "Microphone: not added",
        MicState::Off => "Microphone: off",
        MicState::Muted => "Microphone: muted",
        MicState::Ready => "Microphone: ready",
        MicState::Live => "Microphone: recording",
        MicState::Error => "Microphone: error",
    };
    (
        status.replay,
        replay.into(),
        status.replay && matches!(status.state.as_str(), "Buffering" | "Recording"),
        recording,
        mic.into(),
    )
}
/// Quit from the tray or the close button. While a recording or a save is in
/// progress the window asks first (the frontend calls `exit_app`); otherwise
/// capture is stopped, the recording finalized and the encoder ended.
fn request_quit(app: tauri::AppHandle) {
    thread::spawn(move || {
        let state = app.state::<AppState>();
        let status = state.engine.status();
        if status.recording || status.saving > 0 {
            show_main(&app, None);
            let _ = app.emit(
                "confirm-quit",
                serde_json::json!({"recording": status.recording, "saving": status.saving}),
            );
            return;
        }
        match state.engine.shutdown() {
            Ok(()) => app.exit(0),
            Err(e) => state.report(err(e)),
        }
    });
}
#[tauri::command]
async fn exit_app(app: tauri::AppHandle, state: State<'_, AppState>) -> Reply<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if state.engine.status().saving > 0 {
            return Err("Wait for clip verification to finish".into());
        }
        // Finalizing a recording can take seconds: never on the UI thread.
        state.engine.shutdown().map_err(err)?;
        app.exit(0);
        Ok(())
    })
    .await
    .map_err(err)?
}
fn background_action(app: tauri::AppHandle, name: &'static str) {
    thread::spawn(move || {
        let state = app.state::<AppState>();
        if let Err(e) = perform(&state, name) {
            state.report(e);
        }
    });
}
/// Recording toggles treat a recording that is still starting as on, so a
/// second press cancels it instead of failing with "already active".
fn recording_toggle(s: &Status) -> &'static str {
    if s.recording || s.pending_recording {
        "stop_recording"
    } else {
        "start_recording"
    }
}
pub fn run() {
    tauri::Builder::default()
        // A second launch focuses the running instance instead of failing on
        // the data-directory lock.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let state = app.state::<AppState>();
                    let settings = state.engine.settings();
                    let matches = |s: &str| Shortcut::from_str(s).is_ok_and(|s| s == *shortcut);
                    if matches(&settings.replay_hotkey) {
                        background_action(app.clone(), "save_replay");
                    } else if matches(&settings.recording_hotkey) {
                        background_action(app.clone(), recording_toggle(&state.engine.status()));
                    } else if matches(&settings.pause_hotkey) {
                        background_action(
                            app.clone(),
                            if state.engine.status().replay {
                                "stop_replay"
                            } else {
                                "start_replay"
                            },
                        );
                    } else if matches(&settings.mute_hotkey) {
                        // This handler runs on the UI thread: change the mix on a worker.
                        let app = app.clone();
                        thread::spawn(move || {
                            let state = app.state::<AppState>();
                            for (index, s) in settings
                                .audio
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| s.kind == "microphone")
                            {
                                if let Err(e) = state.engine.set_mix(index, s.volume, !s.muted) {
                                    state.report(err(e));
                                }
                            }
                        });
                    }
                })
                .build(),
        )
        .setup(|app| {
            let data = if let Some(dir) = std::env::var_os("REPLAYX_DATA_DIR") {
                PathBuf::from(dir)
            } else {
                app.path().app_local_data_dir()?
            };
            let settings = Settings::load(&data.join("settings.json"))?;
            discard_audio_tests(&data);
            let engine = Engine::new(data.clone(), settings.clone())?;
            app.manage(AppState {
                engine,
                updating: Arc::new(Mutex::new(false)),
                icons: Arc::new(Mutex::new(HashMap::new())),
                testing_audio: Arc::new(Mutex::new(false)),
                test_stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                monitor_stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                monitor_gain: Arc::new(std::sync::atomic::AtomicU32::new(1f32.to_bits())),
                app: app.handle().clone(),
                library: Arc::new(Mutex::new(Library {
                    index: library::Index::load(&data.join("library-index.json")),
                    path: data.join("library-index.json"),
                    probing: false,
                    last_index_ms: 0,
                })),
                thumbnail_gate: Arc::new(Mutex::new(())),
                data_dir: data.clone(),
            });
            // Keep the startup entry pointing at this executable if enabled.
            if settings.launch_at_startup {
                let _ = winutil::set_autostart(true);
            }
            // Sign-in launches start in the tray; everything else shows the window.
            if !std::env::args().any(|a| a == "--background") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                }
            }
            let hotkeys = match bind_hotkeys(app.handle(), &settings) {
                Ok(warning) => warning,
                Err(error) => Some(error),
            };
            app.state::<AppState>()
                .engine
                .update(|i| i.status.hotkey_error = hotkeys);
            let show = MenuItem::with_id(app, "show", "Open REPLAYX", true, None::<&str>)?;
            let replay =
                CheckMenuItem::with_id(app, "replay", "Instant Replay", true, false, None::<&str>)?;
            let save = MenuItem::with_id(app, "save", "Save replay", false, None::<&str>)?;
            let recording =
                MenuItem::with_id(app, "recording", "Start recording", true, None::<&str>)?;
            let mic = MenuItem::with_id(app, "mic", "Microphone: not added", false, None::<&str>)?;
            let clips = MenuItem::with_id(app, "clips", "Open clips folder", true, None::<&str>)?;
            let open_settings =
                MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit REPLAYX", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show,
                    &PredefinedMenuItem::separator(app)?,
                    &replay,
                    &save,
                    &recording,
                    &mic,
                    &PredefinedMenuItem::separator(app)?,
                    &clips,
                    &open_settings,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?;
            let tray_items = (replay.clone(), save.clone(), recording.clone(), mic.clone());
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("REPLAYX — Stopped")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app, None),
                    "settings" => show_main(app, Some("settings")),
                    "clips" => {
                        let root = app.state::<AppState>().engine.settings().output_dir;
                        if fs::create_dir_all(&root).is_ok() {
                            let _ = command("explorer.exe").arg(&root).spawn();
                        }
                    }
                    "save" => background_action(app.clone(), "save_replay"),
                    "replay" => background_action(
                        app.clone(),
                        if app.state::<AppState>().engine.status().replay {
                            "stop_replay"
                        } else {
                            "start_replay"
                        },
                    ),
                    "recording" => background_action(
                        app.clone(),
                        recording_toggle(&app.state::<AppState>().engine.status()),
                    ),
                    "quit" => request_quit(app.clone()),
                    _ => {}
                })
                .build(app)?;
            let handle = app.handle().clone();
            thread::spawn(move || {
                let mut sys = sysinfo::System::new();
                let own = sysinfo::Pid::from_u32(std::process::id());
                let mut family = vec![own];
                let mut tick = 0u64;
                let notices = crate::overlay::Overlay::spawn();
                let status_overlay = crate::status_overlay::StatusOverlay::spawn();
                // Audio device watcher: an endpoint list check every 2 s (a few
                // ms of COM calls); the window gets the new list only on change.
                {
                    let handle = handle.clone();
                    thread::spawn(move || {
                        let mut last: Option<Vec<(String, String, String)>> = None;
                        loop {
                            if let Ok(list) = audio::devices() {
                                let key: Vec<_> = list
                                    .iter()
                                    .map(|d| (d.id.clone(), d.name.clone(), d.kind.clone()))
                                    .collect();
                                if last.as_ref().is_some_and(|l| *l != key) {
                                    let _ = handle.emit("audio-devices", &list);
                                }
                                last = Some(key);
                            }
                            thread::sleep(std::time::Duration::from_secs(2));
                        }
                    });
                }
                let mut tray_shown = Default::default();
                let mut overlay_frame: Option<crate::status_overlay::Frame> = None;
                let mut announcer = crate::overlay::Announcer::default();
                loop {
                    thread::sleep(std::time::Duration::from_secs(1));
                    let state = handle.state::<AppState>();
                    let status = state.engine.status();
                    if let Some(tray) = handle.tray_by_id("main-tray") {
                        let _ = tray.set_tooltip(Some(format!("REPLAYX — {}", status.state)));
                    }
                    // Tray menu mirrors the real state (texts only change on news).
                    {
                        let settings = state.engine.settings();
                        let labels = tray_labels(&status, &settings);
                        if labels != tray_shown {
                            let (replay, save, recording, mic) = &tray_items;
                            let _ = replay.set_checked(labels.0);
                            let _ = replay.set_text(&labels.1);
                            let _ = save.set_enabled(labels.2);
                            let _ = recording.set_text(&labels.3);
                            let _ = mic.set_text(&labels.4);
                            tray_shown = labels;
                        }
                    }
                    // "Game detected" notice, from the same detector that files
                    // clips (sampled by the engine without its lock).
                    let settings = state.engine.settings();
                    // In-game status overlay: only sent when what it shows changes.
                    let frame = crate::status_overlay::visible(&settings.overlay)
                        .then(|| crate::status_overlay::Frame {
                            tiles: crate::status_overlay::tiles(
                                &crate::status_overlay::view(&status, &settings),
                                &settings.overlay,
                            ),
                            position: settings.overlay.position.clone(),
                            opacity: settings.overlay.opacity,
                            capturable: false,
                        })
                        .filter(|f| !f.tiles.is_empty());
                    if frame != overlay_frame || frame.is_some() {
                        // Re-sent while visible so it follows the active monitor
                        // and stays on top; the overlay thread skips repaints.
                        status_overlay.update(frame.clone());
                        overlay_frame = frame;
                    }
                    let news = announcer.observe(
                        status
                            .foreground_game
                            .as_deref()
                            .filter(|_| settings.game_notifications),
                        std::time::Instant::now(),
                    );
                    if let Some(game) = news {
                        // The process in front is the game just classified; its icon
                        // decorates the notice. Never queries REPLAYX's own windows.
                        let icon_exe = crate::games::foreground_process()
                            .map(|(_, path, _)| PathBuf::from(path));
                        notices.show(crate::overlay::Notice {
                            title: format!("{game} detected"),
                            subtitle: crate::overlay::capture_line(
                                status.replay,
                                status.recording,
                                &status.state,
                                &settings.replay_hotkey,
                            ),
                            active: (status.replay && status.state == "Buffering")
                                || (status.recording && status.state == "Recording"),
                            icon_exe,
                        });
                        let _ = handle.emit("game-detected", serde_json::json!({"game": game}));
                    }
                    // Rediscover child processes (encoder, WebView) every 5s; refresh
                    // only REPLAYX's own processes every second.
                    if tick.is_multiple_of(5) {
                        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                        family = vec![own];
                        loop {
                            let before = family.len();
                            for (pid, process) in sys.processes() {
                                if !family.contains(pid)
                                    && process.parent().is_some_and(|p| family.contains(&p))
                                {
                                    family.push(*pid);
                                }
                            }
                            if family.len() == before {
                                break;
                            }
                        }
                    } else {
                        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&family), true);
                    }
                    tick += 1;
                    let mut memory = 0;
                    let mut cpu = 0.;
                    for (pid, p) in sys.processes() {
                        if family.contains(pid) {
                            memory += p.memory();
                            cpu += p.cpu_usage();
                        }
                    }
                    let cores = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(1) as f64;
                    state.engine.update(|inner| {
                        inner.status.memory_mb = memory as f64 / 1048576.;
                        inner.status.cpu_percent = cpu as f64 / cores;
                    });
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                api.prevent_close();
                if state.engine.settings().close_to_tray {
                    let _ = window.hide();
                    if !state.engine.settings().tray_hint_shown {
                        state.engine.mark_tray_hint_shown();
                        // One-time explanation; the notice thread ends after it.
                        let notice = crate::overlay::Overlay::spawn();
                        notice.show(crate::overlay::Notice {
                            title: "REPLAYX is still running".into(),
                            subtitle: "In the tray. Right-click its icon to quit.".into(),
                            active: true,
                            icon_exe: None,
                        });
                    }
                } else {
                    request_quit(window.app_handle().clone());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            status,
            settings,
            action,
            save_settings,
            set_mix,
            capabilities,
            processes,
            clips,
            clip_action,
            check_update,
            install_update,
            exit_app,
            audio_sessions,
            app_icon,
            clip_thumbnail,
            audio_test,
            audio_test_stop,
            audio_test_data,
            audio_devices,
            mic_monitor_start,
            mic_monitor_set,
            mic_monitor_stop,
            audio_test_discard,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("REPLAYX could not start; check FFmpeg installation and settings.json");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recommended_shortcuts_parse_and_are_distinct() {
        let s = Settings::default();
        let keys = [
            s.replay_hotkey,
            s.recording_hotkey,
            s.mute_hotkey,
            s.pause_hotkey,
        ];
        let parsed: Vec<_> = keys
            .iter()
            .map(|k| Shortcut::from_str(k).unwrap())
            .collect();
        for (n, key) in parsed.iter().enumerate() {
            assert!(!parsed[..n].contains(key));
        }
    }
    #[test]
    fn updater_hosts() {
        assert!(validate_update_url(
            "https://github.com/example/replayx/releases/latest/download/latest.json"
        ));
        for url in [
            "http://github.com/x/releases/latest/download/latest.json",
            "https://github.com.evil.test/x/releases/latest/download/latest.json",
            "https://github.com/user@evil.test/releases/latest/download/latest.json",
            "https://github.com/../repo/releases/latest/download/latest.json",
            "https://github.com/owner/repo/releases/latest/download/latest.json?redirect=1",
            "https://github.com//repo/releases/latest/download/latest.json",
        ] {
            assert!(!validate_update_url(url));
        }
    }
}
