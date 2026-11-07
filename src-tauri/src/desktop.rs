use crate::{
    audio,
    engine::{Engine, Status},
    media::{command, Media, Probe},
    model::{safe_clip, AudioSource, Settings},
    winutil,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;
type Reply<T> = Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone)]
struct AppState {
    engine: Engine,
    updating: Arc<Mutex<bool>>,
    icons: Arc<Mutex<HashMap<String, Option<String>>>>,
    testing_audio: Arc<Mutex<bool>>,
}
impl AppState {
    fn data_dir(&self) -> PathBuf {
        self.engine.0.lock().unwrap().data_dir.clone()
    }
}
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
    let updating = state.updating.lock().unwrap();
    if *updating {
        return Err("Update installation is in progress".into());
    }
    let engine = &state.engine;
    let result = match name {
        "start_replay" => engine.start_replay(),
        "stop_replay" => engine.stop_replay(),
        "start_recording" => engine.start_recording(),
        "save_replay" | "stop_recording" => {
            let job = if name == "save_replay" {
                engine.save_replay()
            } else {
                engine.stop_recording()
            }
            .map_err(err)?;
            drop(updating);
            let result = job.run();
            engine.complete_save(&result);
            return result.map(|(p,probe)|serde_json::json!({"file":p.file_name().unwrap().to_string_lossy(),"probe":probe})).map_err(err);
        }
        "stop_all" => engine.shutdown(),
        "recover" => return engine.recover().map(|v| serde_json::json!(v)).map_err(err),
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
fn save_settings(settings: Settings, app: tauri::AppHandle, state: State<AppState>) -> Reply<()> {
    settings.validate().map_err(err)?;
    let previous = state.engine.settings();
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
    let warning = bind_hotkeys(&app, &settings)?;
    if let Err(e) = state.engine.set_settings(settings) {
        let _ = bind_hotkeys(&app, &previous);
        return Err(err(e));
    }
    state.engine.0.lock().unwrap().status.hotkey_error = warning;
    Ok(())
}
#[tauri::command]
fn set_mix(index: usize, volume: f32, muted: bool, state: State<AppState>) -> Reply<()> {
    state.engine.set_mix(index, volume, muted).map_err(err)
}
#[tauri::command]
async fn capabilities() -> Reply<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(||{
    let devices=crate::audio::devices().map_err(err)?;
    let available=Media::discover().and_then(|m|{m.preflight()?;Ok(m)});
    let (encoders,media_error)=match available {Ok(m)=>(m.encoders(),None),Err(e)=>(Vec::new(),Some(format!("{e:#}")))};
    Ok(serde_json::json!({"encoders":encoders,"media_error":media_error,"displays":crate::geometry::displays(),"devices":devices,"wgc":false,"pause_recording":false,"monitoring":false,"updates_configured":update_configured()}))
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
/// Cached JPEG thumbnail of a clip as a data URL.
#[tauri::command]
async fn clip_thumbnail(name: String, state: State<'_, AppState>) -> Reply<String> {
    let root = state.engine.settings().output_dir;
    let cache = state.data_dir().join("thumbnails");
    tauri::async_runtime::spawn_blocking(move || {
        let path = safe_clip(&root, &name).map_err(err)?;
        let meta = fs::metadata(&path).map_err(err)?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stem: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .take(80)
            .collect();
        let file = cache.join(format!("{stem}-{modified}-{}.jpg", meta.len()));
        let jpeg = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(_) => {
                let media = Media::discover().map_err(err)?;
                let duration = media.probe(&path).map(|p| p.duration).unwrap_or(0.);
                let bytes = media.thumbnail(&path, duration, 480).map_err(err)?;
                fs::create_dir_all(&cache).map_err(err)?;
                prune_cache(&cache, 400);
                fs::write(&file, &bytes).map_err(err)?;
                bytes
            }
        };
        Ok(format!("data:image/jpeg;base64,{}", winutil::base64(&jpeg)))
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
    let file = state.data_dir().join("tests").join("audio-test.wav");
    let flag = state.testing_audio.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let capture = audio::record_test(source, seconds.clamp(2, 15), |peak, elapsed| {
            let _ = app.emit("audio-test-level", (peak, elapsed));
        })
        .map_err(err)?;
        fs::create_dir_all(file.parent().unwrap()).map_err(err)?;
        audio::write_wav16(&file, &capture.pcm).map_err(err)?;
        Ok(capture)
    })
    .await
    .map_err(err);
    *flag.lock().unwrap() = false;
    result?
}
#[tauri::command]
fn play_audio_test(state: State<AppState>) -> Reply<()> {
    let file = state.data_dir().join("tests").join("audio-test.wav");
    if !file.is_file() {
        return Err("Run a test recording first".into());
    }
    open_local(&file)
}
#[tauri::command]
fn open_folder(state: State<AppState>) -> Reply<()> {
    let root = state.engine.settings().output_dir;
    fs::create_dir_all(&root).map_err(err)?;
    command("explorer.exe").arg(&root).spawn().map_err(err)?;
    Ok(())
}
#[derive(Serialize)]
struct Clip {
    name: String,
    modified: u64,
    probe: Option<Probe>,
}
#[tauri::command]
async fn clips(state: State<'_, AppState>) -> Reply<Vec<Clip>> {
    let root = state.engine.settings().output_dir;
    tauri::async_runtime::spawn_blocking(move || {
        let media = Media::discover().map_err(err)?;
        let mut out = Vec::new();
        for entry in fs::read_dir(&root).map_err(err)? {
            let entry = entry.map_err(err)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(".partial.") {
                continue;
            }
            let Ok(path) = safe_clip(&root, &name) else {
                continue;
            };
            let meta = fs::metadata(&path).map_err(err)?;
            out.push(Clip {
                name,
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                probe: media.probe(&path).ok(),
            });
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.modified));
        Ok(out)
    })
    .await
    .map_err(err)?
}
#[tauri::command]
async fn clip_action(
    name: String,
    operation: String,
    new_name: Option<String>,
    state: State<'_, AppState>,
) -> Reply<String> {
    let root = state.engine.settings().output_dir;
    tauri::async_runtime::spawn_blocking(move || {
        let path = safe_clip(&root, &name).map_err(err)?;
        match operation.as_str() {
            "delete" => {
                winutil::recycle(&path).map_err(err)?;
            }
            "rename" => {
                let n = new_name.ok_or("New name required")?;
                if n.len() > 160
                    || n.trim() != n
                    || n.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|'])
                    || n.starts_with('.')
                    || n.ends_with('.')
                    || std::path::Path::new(&n).extension() != path.extension()
                {
                    return Err("Use a valid filename with the same extension".into());
                }
                let to = root.join(n);
                if to.exists() {
                    return Err("A clip with that name already exists".into());
                }
                fs::rename(path, to).map_err(err)?;
            }
            "folder" => {
                command("explorer.exe")
                    .arg("/select,")
                    .arg(&path)
                    .spawn()
                    .map_err(err)?;
            }
            "play" => {
                open_local(&path)?;
            }
            "mp4" => {
                let output = root.join(format!(
                    "{}-export-{}.mp4",
                    path.file_stem().unwrap().to_string_lossy(),
                    &uuid::Uuid::new_v4().to_string()[..8]
                ));
                Media::discover()
                    .map_err(err)?
                    .export_mp4(&path, &output)
                    .map_err(err)?;
                return Ok(output.file_name().unwrap().to_string_lossy().into());
            }
            _ => return Err("Unknown clip action".into()),
        }
        Ok("Done".into())
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
            || ["Starting", "Recovering"].contains(&s.state.as_str())
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
#[tauri::command]
fn exit_app(app: tauri::AppHandle, state: State<AppState>) -> Reply<()> {
    if state.engine.status().saving > 0 {
        return Err("Wait for clip verification to finish".into());
    }
    state.engine.shutdown().map_err(err)?;
    app.exit(0);
    Ok(())
}
fn background_action(app: tauri::AppHandle, name: &'static str) {
    thread::spawn(move || {
        let state = app.state::<AppState>();
        if let Err(e) = perform(&state, name) {
            state.engine.0.lock().unwrap().status.error = Some(e);
        }
    });
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
                        background_action(
                            app.clone(),
                            if state.engine.status().recording {
                                "stop_recording"
                            } else {
                                "start_recording"
                            },
                        );
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
                        for (index, s) in settings
                            .audio
                            .iter()
                            .enumerate()
                            .filter(|(_, s)| s.kind == "microphone")
                        {
                            if let Err(e) = state.engine.set_mix(index, s.volume, !s.muted) {
                                state.engine.0.lock().unwrap().status.error = Some(err(e));
                            }
                        }
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
            let engine = Engine::new(data, settings.clone())?;
            app.manage(AppState {
                engine,
                updating: Arc::new(Mutex::new(false)),
                icons: Arc::new(Mutex::new(HashMap::new())),
                testing_audio: Arc::new(Mutex::new(false)),
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
            app.state::<AppState>()
                .engine
                .0
                .lock()
                .unwrap()
                .status
                .hotkey_error = match bind_hotkeys(app.handle(), &settings) {
                Ok(warning) => warning,
                Err(error) => Some(error),
            };
            let show = MenuItem::with_id(app, "show", "Open REPLAYX", true, None::<&str>)?;
            let save = MenuItem::with_id(app, "save", "Save instant replay", true, None::<&str>)?;
            let replay =
                MenuItem::with_id(app, "replay", "Start / stop replay", true, None::<&str>)?;
            let recording = MenuItem::with_id(
                app,
                "recording",
                "Start / stop recording",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                "Exit and finalize recording",
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&show, &save, &replay, &recording, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("REPLAYX — Stopped")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
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
                        if app.state::<AppState>().engine.status().recording {
                            "stop_recording"
                        } else {
                            "start_recording"
                        },
                    ),
                    "quit" => {
                        let app = app.clone();
                        thread::spawn(move || {
                            let state = app.state::<AppState>();
                            if state.engine.status().saving == 0 {
                                match state.engine.shutdown() {
                                    Ok(()) => app.exit(0),
                                    Err(e) => {
                                        state.engine.0.lock().unwrap().status.error = Some(err(e))
                                    }
                                }
                            }
                        });
                    }
                    _ => {}
                })
                .build(app)?;
            let handle = app.handle().clone();
            thread::spawn(move || {
                let mut sys = sysinfo::System::new();
                let own = sysinfo::Pid::from_u32(std::process::id());
                let mut family = vec![own];
                let mut tick = 0u64;
                loop {
                    thread::sleep(std::time::Duration::from_secs(1));
                    let state = handle.state::<AppState>();
                    let status = state.engine.status();
                    if let Some(tray) = handle.tray_by_id("main-tray") {
                        let _ = tray.set_tooltip(Some(format!("REPLAYX — {}", status.state)));
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
                    let mut inner = state.engine.0.lock().unwrap();
                    inner.status.memory_mb = memory as f64 / 1048576.;
                    inner.status.cpu_percent = cpu as f64
                        / std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(1) as f64;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.engine.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    api.prevent_close();
                    let app = window.app_handle().clone();
                    thread::spawn(move || {
                        let state = app.state::<AppState>();
                        if state.engine.status().saving > 0 {
                            state.engine.0.lock().unwrap().status.error =
                                Some("Wait for saving to finish before exiting".into());
                            return;
                        }
                        match state.engine.shutdown() {
                            Ok(()) => app.exit(0),
                            Err(e) => state.engine.0.lock().unwrap().status.error = Some(err(e)),
                        }
                    });
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
            play_audio_test,
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
