//! Foreground game detection for clip naming and organization.
//!
//! Uses only documented, read-only Windows APIs: the foreground window, its
//! owning process id and that process's executable path (and, for one known
//! case, the window title). Nothing is injected into games, no game memory is
//! read and no files are modified. Sampling runs once per second and only
//! while capture is active.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Clips without a dominant game.
pub const DESKTOP: &str = "Desktop";
/// Clips where several games share the captured time without a clear winner.
pub const MIXED: &str = "Mixed";

/// User override: executable file name → game name ("" = not a game).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameMapping {
    pub exe: String,
    pub name: String,
}

/// One game in the catalog (src/game_catalog.json, compiled in: detection
/// works offline).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogGame {
    /// Stable identifier (artwork, playtime).
    pub id: String,
    /// Display name, also the clip folder name.
    pub name: String,
    #[serde(default)]
    pub category: String,
    /// Lower-case executable file names.
    #[serde(default)]
    pub exe: Vec<String>,
    /// Lower-case path fragments; one must appear in the executable's path.
    #[serde(default)]
    pub path: Vec<String>,
}
#[derive(Deserialize)]
struct CatalogFile {
    games: Vec<CatalogGame>,
    not_games: Vec<String>,
}
pub struct Catalog {
    pub games: Vec<CatalogGame>,
    by_exe: HashMap<String, Vec<usize>>,
    not_games: std::collections::HashSet<String>,
}
/// The built-in game catalog, parsed once.
pub fn catalog() -> &'static Catalog {
    static CATALOG: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let file: CatalogFile = serde_json::from_str(include_str!("game_catalog.json"))
            .expect("game_catalog.json is valid (tested)");
        let mut by_exe: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, g) in file.games.iter().enumerate() {
            for exe in &g.exe {
                by_exe.entry(exe.clone()).or_default().push(i);
            }
        }
        // Entries that also require a folder are tried before plain ones
        // sharing the same executable name (Path of Exile 2 vs 1).
        for list in by_exe.values_mut() {
            list.sort_by_key(|i| file.games[*i].path.is_empty());
        }
        Catalog {
            games: file.games,
            by_exe,
            not_games: file.not_games.into_iter().collect(),
        }
    })
}
impl Catalog {
    /// The catalog game an executable belongs to, if any.
    pub fn lookup(&self, file: &str, lower_path: &str) -> Option<&CatalogGame> {
        self.by_exe
            .get(file)?
            .iter()
            .map(|i| &self.games[*i])
            .find(|g| g.path.is_empty() || g.path.iter().any(|f| lower_path.contains(f.as_str())))
    }
    /// The catalog entry for a display name (clip folder / playtime name).
    pub fn by_name(&self, name: &str) -> Option<&CatalogGame> {
        self.games
            .iter()
            .find(|g| g.name.eq_ignore_ascii_case(name))
    }
}

/// Store install roots: the next path component is the game's folder name.
const STORE_ROOTS: &[&str] = &[
    r"\steamapps\common\",
    r"\epic games\",
    r"\xboxgames\",
    r"\riot games\",
    r"\ea games\",
    r"\ubisoft game launcher\games\",
    r"\gog galaxy\games\",
];
/// Folders under store roots that hold launchers rather than games.
const STORE_TOOLS: &[&str] = &[
    "launcher",
    "epic online services",
    "riot client",
    "steamworks shared",
    "wallpaper_engine",
    "directxredist",
];

/// What the foreground process is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreground {
    /// A recognised game.
    Game(String),
    /// Any other application (counts as Desktop).
    App,
    /// REPLAYX itself; not counted towards any category.
    Ignored,
}

/// Classify a process from its executable path and window title. Pure.
pub fn classify(exe_path: &str, title: &str, overrides: &[GameMapping]) -> Foreground {
    let lower = exe_path.to_lowercase().replace('/', "\\");
    let file = lower.rsplit('\\').next().unwrap_or(&lower).to_string();
    if file == "replayx.exe" {
        return Foreground::Ignored;
    }
    if let Some(m) = overrides.iter().find(|m| m.exe.eq_ignore_ascii_case(&file)) {
        let name = m.name.trim();
        return if name.is_empty() {
            Foreground::App
        } else {
            Foreground::Game(name.to_string())
        };
    }
    let cat = catalog();
    if cat.not_games.contains(&file) {
        return Foreground::App;
    }
    if let Some(game) = cat.lookup(&file, &lower) {
        return Foreground::Game(game.name.clone());
    }
    // Roblox from the Microsoft Store runs as a generic UWP host.
    if file == "windows10universal.exe" && lower.contains("robloxcorporation") {
        return Foreground::Game("Roblox".into());
    }
    // Minecraft Java Edition runs in a generic Java VM; only its window title
    // distinguishes it from other Java programs.
    if (file == "javaw.exe" || file == "java.exe") && title.trim_start().starts_with("Minecraft") {
        return Foreground::Game("Minecraft Java Edition".into());
    }
    for root in STORE_ROOTS {
        if let Some(pos) = lower.find(root) {
            let rest = &exe_path[pos + root.len()..];
            if let Some(folder) = rest.split(['\\', '/']).next().filter(|f| !f.is_empty()) {
                if !STORE_TOOLS
                    .iter()
                    .any(|t| folder.to_lowercase().contains(t))
                {
                    return Foreground::Game(folder.to_string());
                }
            }
        }
    }
    Foreground::App
}

/// Foreground category over time, stored only when it changes.
#[derive(Debug, Default)]
pub struct Activity {
    changes: Vec<(Instant, Foreground)>,
}
impl Activity {
    pub fn record(&mut self, at: Instant, now: Foreground) {
        if self.changes.last().is_none_or(|(_, last)| *last != now) {
            self.changes.push((at, now));
        }
        // Keep at most a day of history; real use needs ≤ the replay window
        // or the current recording.
        if let Some(cut) = at.checked_sub(Duration::from_secs(86_400)) {
            let keep_from = self
                .changes
                .iter()
                .rposition(|(t, _)| *t <= cut)
                .unwrap_or(0);
            self.changes.drain(..keep_from);
        }
    }
    pub fn current(&self) -> Option<&Foreground> {
        self.changes.last().map(|(_, f)| f)
    }
    pub fn clear(&mut self) {
        self.changes.clear();
    }
    /// Seconds per game, plus non-game application seconds, within [from, to].
    pub fn totals(&self, from: Instant, to: Instant) -> (HashMap<String, f64>, f64) {
        let mut games = HashMap::new();
        let mut apps = 0.;
        for (i, (start, what)) in self.changes.iter().enumerate() {
            let end = self.changes.get(i + 1).map(|(t, _)| *t).unwrap_or(to);
            let (a, b) = ((*start).max(from), end.min(to));
            if b <= a {
                continue;
            }
            let secs = (b - a).as_secs_f64();
            match what {
                Foreground::Game(g) => *games.entry(g.clone()).or_insert(0.) += secs,
                Foreground::App => apps += secs,
                Foreground::Ignored => {}
            }
        }
        (games, apps)
    }
    pub fn attribute(&self, from: Instant, to: Instant) -> String {
        let (games, apps) = self.totals(from, to);
        attribute(&games, apps)
    }
}

/// Category for a clip from time spent per game and in other applications.
///
/// - The top game wins if it has at least 30% of the time and at least twice
///   the runner-up's time (so switching to Discord briefly still counts).
/// - Otherwise, if games together fill at least half the time, `Mixed`.
/// - Otherwise `Desktop`.
pub fn attribute(games: &HashMap<String, f64>, app_seconds: f64) -> String {
    let game_total: f64 = games.values().sum();
    let total = game_total + app_seconds;
    if total <= 0. || game_total <= 0. {
        return DESKTOP.into();
    }
    let mut ranked: Vec<_> = games.iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
    let (top, top_secs) = ranked[0];
    let second = ranked.get(1).map(|(_, s)| **s).unwrap_or(0.);
    if *top_secs / total >= 0.30 && *top_secs >= 2. * second {
        return top.clone();
    }
    if game_total / total >= 0.5 && ranked.len() > 1 {
        return MIXED.into();
    }
    DESKTOP.into()
}

/// The current foreground window's process image path and title.
pub fn foreground_process() -> Option<(u32, String, String)> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    window_process(unsafe { GetForegroundWindow() })
}

/// Process image path and title of a top-level window, without ever sending
/// it a message. REPLAYX's own windows are skipped entirely: `GetWindowTextW`
/// on a window owned by the calling process *sends* `WM_GETTEXT` to the
/// window's thread, and when that thread (the UI thread) was itself waiting for
/// the engine lock held by the sampler, the app deadlocked ("Not responding"
/// right after Record, 0.2.1-alpha.1). For other processes Windows returns the
/// cached caption without messaging. REPLAYX is never a game, so nothing is lost.
pub fn window_process(hwnd: windows::Win32::Foundation::HWND) -> Option<(u32, String, String)> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId},
    };
    unsafe {
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let mut title = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        let title = String::from_utf16_lossy(&title[..n]);
        // Limited query rights work for elevated/protected game processes too.
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        ok.ok()?;
        Some((pid, String::from_utf16_lossy(&buf[..len as usize]), title))
    }
}

/// Creation time of a process (100 ns units since 1601), with its id the
/// identity of one game session.
pub fn process_started(pid: u32) -> Option<u64> {
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let ok = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
        let _ = CloseHandle(process);
        ok.ok()?;
        Some(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
    }
}
/// Whether that exact process (id and creation time) is still running.
///
/// A process that can no longer be looked at (a protected or anti-cheat
/// process refusing the query) counts as still running: treating it as ended
/// would announce the game again on every check while it is in front. Only
/// "no such process", a different creation time (the id was reused) or an
/// exit code ends it.
pub fn process_running(pid: u32, started: u64) -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, STILL_ACTIVE},
        System::Threading::{
            GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    unsafe {
        let process = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(p) => p,
            // No process has this id any more.
            Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => return false,
            Err(_) => return true,
        };
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let times = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
        let mut code = 0u32;
        let exit = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        let same =
            ((created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64) == started;
        match (times, exit) {
            (Ok(()), Ok(())) => same && code == STILL_ACTIVE.0 as u32,
            // Opened, but the details were refused: can't tell, keep it.
            _ => true,
        }
    }
}

/// Classify whatever is in the foreground right now.
pub fn sample(overrides: &[GameMapping]) -> Foreground {
    match foreground_process() {
        Some((_, path, title)) => classify(&path, &title, overrides),
        None => Foreground::App,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_identity_follows_the_real_process_lifetime() {
        let me = std::process::id();
        let started = process_started(me).expect("own creation time");
        assert!(process_running(me, started));
        assert!(
            !process_running(me, started + 1),
            "a different creation time is another process"
        );
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping -n 2 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let t = process_started(pid).expect("child creation time");
        assert!(process_running(pid, t));
        child.wait().unwrap();
        assert!(
            !process_running(pid, t),
            "an exited process ends its session"
        );
        drop(child);
        // Once its last handle is closed the id belongs to nobody (or to a
        // new process with another creation time): ended either way.
        assert!(!process_running(pid, t));
        assert!(!process_running(0xFFFF_FFFC, 1), "no such process");
    }

    /// Reproduces the 0.2.1-alpha.1 Record freeze mechanism in-process and
    /// proves the sampler no longer takes part in it. A window is owned by a
    /// thread that blocks on a mutex this test holds (the UI thread waiting for
    /// the engine lock). `GetWindowTextW` on it from another thread blocks until
    /// the owner pumps messages again; `window_process` returns at once.
    #[test]
    fn sampling_own_window_never_waits_for_its_blocked_owner_thread() {
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::Duration;
        use windows::{
            core::w,
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DestroyWindow, GetWindowTextW, PeekMessageW, MSG, PM_REMOVE,
                    WINDOW_EX_STYLE, WS_OVERLAPPED,
                },
            },
        };
        let engine_lock = Arc::new(Mutex::new(()));
        let held = engine_lock.lock().unwrap();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (hwnd_tx, hwnd_rx) = mpsc::channel::<isize>();
        let owner = {
            let (lock, done) = (engine_lock.clone(), done.clone());
            std::thread::spawn(move || unsafe {
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("REPLAYX test window"),
                    WS_OVERLAPPED,
                    0,
                    0,
                    10,
                    10,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
                hwnd_tx.send(hwnd.0 as isize).unwrap();
                // Like the `status` command on the UI thread: wait for the lock.
                drop(lock.lock().unwrap());
                let mut msg = MSG::default();
                while !done.load(std::sync::atomic::Ordering::SeqCst) {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {}
                    std::thread::sleep(Duration::from_millis(5));
                }
                let _ = DestroyWindow(hwnd);
            })
        };
        let raw = hwnd_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let hwnd = HWND(raw as *mut _);

        // The fixed sampler returns immediately (own process is never a game).
        let started = std::time::Instant::now();
        assert!(window_process(hwnd).is_none());
        assert!(started.elapsed() < Duration::from_millis(500));

        // The old call blocks while the owner thread is stuck on the lock.
        let (text_tx, text_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u16; 64];
            let hwnd = HWND(raw as *mut _);
            let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
            let _ = text_tx.send(n);
        });
        assert!(
            text_rx.recv_timeout(Duration::from_millis(800)).is_err(),
            "GetWindowTextW on an own window should wait for its owner thread"
        );
        // Releasing the lock lets the owner pump WM_GETTEXT: the call completes.
        drop(held);
        assert!(text_rx.recv_timeout(Duration::from_secs(5)).unwrap() > 0);
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        owner.join().unwrap();
    }

    fn game(n: &str) -> Foreground {
        Foreground::Game(n.into())
    }

    #[test]
    fn catalog_entries_are_valid_and_safe() {
        let cat = catalog();
        assert!(cat.games.len() >= 120, "{} games", cat.games.len());
        let mut ids = std::collections::HashSet::new();
        // Short executable names identify a game only when reviewed as
        // distinctive; anything else must name its folder too.
        let reviewed = [
            "arma3.exe",
            "bf1.exe",
            "bf6.exe",
            "bfv.exe",
            "bg3.exe",
            "bh3.exe",
            "cod.exe",
            "cs2.exe",
            "csgo.exe",
            "dota2.exe",
            "f1_24.exe",
            "f1_25.exe",
            "fc24.exe",
            "fc25.exe",
            "fc26.exe",
            "gmod.exe",
            "gowr.exe",
            "gta5.exe",
            "gtaiv.exe",
            "mk12.exe",
            "osu!.exe",
            "rdr2.exe",
        ];
        for g in &cat.games {
            assert!(ids.insert(g.id.clone()), "duplicate id {}", g.id);
            assert!(!g.name.trim().is_empty(), "{}", g.id);
            assert!(
                !g.name
                    .contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']),
                "folder-safe name: {}",
                g.name
            );
            for exe in &g.exe {
                assert_eq!(exe, &exe.to_lowercase(), "{exe}");
                assert!(exe.ends_with(".exe"), "{exe}");
                assert!(
                    !cat.not_games.contains(exe),
                    "{exe} is also listed as not a game"
                );
                let stem = exe.trim_end_matches(".exe");
                if stem.len() <= 5 || ["discovery", "dungeons", "kingdomcome"].contains(&stem) {
                    assert!(
                        !g.path.is_empty() || reviewed.contains(&exe.as_str()),
                        "{exe} ({}) needs a path requirement",
                        g.name
                    );
                }
            }
        }
    }
    #[test]
    fn catalog_disambiguates_by_folder_and_maps_aliases() {
        let none: Vec<GameMapping> = Vec::new();
        let game = |n: &str| Foreground::Game(n.into());
        // Several executables, one game.
        assert_eq!(
            classify(
                r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe",
                "",
                &none
            ),
            game("Valorant")
        );
        assert_eq!(
            classify(
                r"D:\Steam\steamapps\common\Apex Legends\r5apex_dx12.exe",
                "",
                &none
            ),
            game("Apex Legends")
        );
        assert_eq!(
            classify(r"C:\Games\Genshin Impact Game\YuanShen.exe", "", &none),
            game("Genshin Impact")
        );
        // Same executable name, folder decides.
        assert_eq!(
            classify(
                r"C:\Steam\steamapps\common\Path of Exile 2\PathOfExileSteam.exe",
                "",
                &none
            ),
            game("Path of Exile 2")
        );
        assert_eq!(
            classify(
                r"C:\Steam\steamapps\common\Path of Exile\PathOfExileSteam.exe",
                "",
                &none
            ),
            game("Path of Exile")
        );
        // Generic names need their folder.
        assert_eq!(
            classify(
                r"C:\Steam\steamapps\common\THE FINALS\Discovery.exe",
                "",
                &none
            ),
            game("The Finals")
        );
        assert_eq!(
            classify(r"C:\Tools\discovery\Discovery.exe", "", &none),
            Foreground::App
        );
        assert_eq!(
            classify(r"C:\Program Files\Speaker Tools\peak.exe", "", &none),
            Foreground::App
        );
        // Launchers and helpers are never the game.
        for launcher in [
            r"C:\Program Files\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            r"C:\Riot Games\Riot Client\RiotClientServices.exe",
            r"C:\Windows\SystemApps\LockApp.exe",
            r"C:\Program Files\NVIDIA Corporation\NVIDIA App\NVIDIA App.exe",
        ] {
            assert_eq!(classify(launcher, "", &none), Foreground::App, "{launcher}");
        }
        // Unknown game in a store folder: named after its folder.
        assert_eq!(
            classify(
                r"E:\SteamLibrary\steamapps\common\Some Indie Game\bin\indie.exe",
                "",
                &none
            ),
            game("Some Indie Game")
        );
        // Unknown program elsewhere: not a game.
        assert_eq!(
            classify(r"C:\Program Files\Tool\tool.exe", "", &none),
            Foreground::App
        );
        assert_eq!(
            catalog().by_name("roblox").map(|g| g.id.as_str()),
            Some("roblox")
        );
    }
    #[test]
    fn known_games_are_recognised_by_process_identity() {
        let none = [];
        let c = |p: &str| classify(p, "", &none);
        assert_eq!(
            c(
                r"C:\Program Files\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe"
            ),
            game("Fortnite")
        );
        assert_eq!(
            c(r"C:\Users\x\AppData\Local\Roblox\Versions\version-1a2b\RobloxPlayerBeta.exe"),
            game("Roblox")
        );
        assert_eq!(
            c(
                r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"
            ),
            game("Valorant")
        );
        assert_eq!(
            c(
                r"D:\SteamLibrary\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe"
            ),
            game("Counter-Strike 2")
        );
        assert_eq!(
            c(
                r"C:\Program Files\WindowsApps\ROBLOXCORPORATION.ROBLOX_2.6_x64__55nm\Windows10Universal.exe"
            ),
            game("Roblox")
        );
    }

    #[test]
    fn launchers_and_helpers_are_not_games() {
        let none = [];
        for p in [
            r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            r"C:\Program Files\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteLauncher.exe",
            r"C:\Users\x\AppData\Local\Roblox\Versions\version-1\RobloxPlayerLauncher.exe",
            r"C:\Riot Games\Riot Client\RiotClientServices.exe",
            r"C:\Program Files (x86)\Steam\steam.exe",
            r"C:\Users\x\AppData\Local\Discord\app-1.0\Discord.exe",
            r"C:\Windows\explorer.exe",
            r"D:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper64.exe",
        ] {
            assert_eq!(classify(p, "", &none), Foreground::App, "{p}");
        }
        assert_eq!(
            classify(r"C:\Apps\REPLAYX\replayx.exe", "", &none),
            Foreground::Ignored
        );
    }

    #[test]
    fn store_folders_name_unknown_games_and_titles_only_disambiguate_java() {
        let none = [];
        assert_eq!(
            classify(
                r"D:\SteamLibrary\steamapps\common\Hollow Knight\hollow_knight.exe",
                "",
                &none
            ),
            game("Hollow Knight")
        );
        assert_eq!(
            classify(
                r"C:\XboxGames\Forza Horizon 5\Content\ForzaHorizon5.exe",
                "",
                &none
            ),
            game("Forza Horizon 5")
        );
        assert_eq!(
            classify(
                r"C:\Program Files\Java\bin\javaw.exe",
                "Minecraft 1.21.4",
                &none
            ),
            game("Minecraft Java Edition")
        );
        assert_eq!(
            classify(
                r"C:\Program Files\Java\bin\javaw.exe",
                "IntelliJ IDEA",
                &none
            ),
            Foreground::App
        );
        // A title alone never makes an unrelated app a game.
        assert_eq!(
            classify(
                r"C:\Program Files\Mozilla Firefox\firefox.exe",
                "Fortnite - YouTube",
                &none
            ),
            Foreground::App
        );
        assert_eq!(
            classify(r"C:\Tools\notepad.exe", "", &none),
            Foreground::App
        );
    }

    #[test]
    fn real_paths_from_the_development_pc() {
        // Collected from running processes on 2026-11-07 (Roblox in session).
        let none = [];
        let c = |p: &str| classify(p, "", &none);
        assert_eq!(
            c(
                r"C:\Users\Player\AppData\Local\Roblox\Versions\version-f0ea5da9412f4e35\RobloxPlayerBeta.exe"
            ),
            game("Roblox")
        );
        assert_eq!(
            c(
                r"C:\Program Files\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe"
            ),
            game("Fortnite")
        );
        for p in [
            r"C:\Users\Player\AppData\Local\Roblox\Versions\version-f0ea5da9412f4e35\RobloxCrashHandler.exe",
            r"C:\Program Files (x86)\Epic Games\Epic Online Services\EpicOnlineServicesUserHelper.exe",
            r"C:\Program Files (x86)\Epic Games\Launcher\Engine\Binaries\Win64\EpicWebHelper.exe",
            r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            r"C:\Program Files (x86)\Steam\bin\cef\cef.win64\steamwebhelper.exe",
            r"D:\OperaGX\opera.exe",
        ] {
            assert_eq!(c(p), Foreground::App, "{p}");
        }
    }

    #[test]
    fn user_mappings_take_precedence() {
        let maps = [
            GameMapping {
                exe: "MyIndieGame.exe".into(),
                name: "My Indie Game".into(),
            },
            GameMapping {
                exe: "cs2.exe".into(),
                name: "".into(),
            },
        ];
        assert_eq!(
            classify(r"C:\Games\myindiegame.exe", "", &maps),
            game("My Indie Game")
        );
        assert_eq!(
            classify(r"C:\Steam\steamapps\common\cs\cs2.exe", "", &maps),
            Foreground::App
        );
    }

    fn m(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn attribution_uses_the_whole_captured_interval() {
        // Five minutes of Fortnite, then Discord before pressing save.
        assert_eq!(attribute(&m(&[("Fortnite", 300.)]), 20.), "Fortnite");
        // A replay that is mostly desktop with a short game moment.
        assert_eq!(attribute(&m(&[("Fortnite", 10.)]), 50.), DESKTOP);
        assert_eq!(attribute(&m(&[]), 60.), DESKTOP);
        assert_eq!(attribute(&m(&[]), 0.), DESKTOP);
        // Two games sharing the clip: no false single-game claim.
        assert_eq!(
            attribute(&m(&[("Fortnite", 40.), ("Roblox", 35.)]), 5.),
            MIXED
        );
        // A clear winner despite a second game.
        assert_eq!(
            attribute(&m(&[("Roblox", 200.), ("Fortnite", 40.)]), 0.),
            "Roblox"
        );
    }

    #[test]
    fn activity_weights_time_between_changes() {
        let t0 = Instant::now();
        let s = |x: u64| t0 + Duration::from_secs(x);
        let mut a = Activity::default();
        a.record(s(0), Foreground::App);
        a.record(s(10), game("Fortnite"));
        a.record(s(11), game("Fortnite")); // unchanged samples are not stored
        a.record(s(310), Foreground::App); // alt-tab to Discord
        a.record(s(330), Foreground::Ignored); // pressing save in REPLAYX
        let (games, apps) = a.totals(s(0), s(335));
        assert_eq!(games["Fortnite"], 300.);
        assert_eq!(apps, 30.);
        assert_eq!(a.attribute(s(0), s(335)), "Fortnite");
        // Only the replay's own window counts: the last 30 s were Discord/REPLAYX.
        assert_eq!(a.attribute(s(305), s(335)), DESKTOP);
        assert_eq!(a.changes.len(), 4);
    }

    #[test]
    fn foreground_query_is_safe_to_call() {
        // Hardware/desktop dependent; must not crash or require privileges.
        if std::env::var_os("REPLAYX_NO_HARDWARE").is_some() {
            return;
        }
        let _ = sample(&[]);
    }
}
