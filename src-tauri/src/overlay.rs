//! "Game detected" notice: a small native window in the top-right corner of
//! the main display. Click-through, never focused, never over the centre of the
//! game, gone after a few seconds. Drawn with GDI on its own thread (no browser
//! view, no engine lock). Shows over windowed and borderless full-screen games;
//! Windows does not show any desktop window over exclusive full-screen games.
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// A game process: its id plus creation time, so a reused process id is
/// never mistaken for the same game session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessId {
    pub pid: u32,
    pub started: u64,
}
/// Decides when a detection is news: once per game *session*, not per focus
/// change. A session is the life of the game's process. Alt+Tab, minimising,
/// switching between full screen and borderless, or a second window never
/// repeat it, however long the game has been running. A second process of
/// the same game (launcher hand-off, helper) or a restart within `GRACE` of
/// the previous process ending continues the session; a later relaunch is a
/// new session and is announced again.
#[derive(Default)]
pub struct Announcer {
    sessions: Vec<(String, ProcessId)>,
    ended: HashMap<String, Instant>,
}
impl Announcer {
    pub const GRACE: Duration = Duration::from_secs(15);
    /// `front`: the game in front and its process (None when no game is in
    /// front). `alive`: whether a tracked process is still running.
    pub fn observe(
        &mut self,
        front: Option<(&str, ProcessId)>,
        now: Instant,
        alive: impl Fn(ProcessId) -> bool,
    ) -> Option<String> {
        let mut gone = Vec::new();
        self.sessions.retain(|(game, process)| {
            let running = alive(*process);
            if !running {
                gone.push(game.clone());
            }
            running
        });
        for game in gone {
            self.ended.insert(game, now);
        }
        self.ended
            .retain(|_, t| now.duration_since(*t) < Self::GRACE);
        let (game, process) = front?;
        if self.sessions.iter().any(|(_, p)| *p == process) {
            return None;
        }
        let continuing =
            self.sessions.iter().any(|(g, _)| g == game) || self.ended.contains_key(game);
        self.sessions.push((game.to_owned(), process));
        (!continuing).then(|| game.to_owned())
    }
}

/// What the notice says about capture, from the engine's real state.
pub fn capture_line(replay: bool, recording: bool, state: &str, save_hotkey: &str) -> String {
    let keys = save_hotkey.replace("Control", "Ctrl");
    if recording && state == "Recording" {
        "Recording".into()
    } else if replay && state == "Buffering" {
        format!("Instant Replay ready · {keys} saves")
    } else if replay || recording || state == "Starting" {
        "Capture starting…".into()
    } else {
        "Instant Replay off".into()
    }
}

pub struct Notice {
    pub title: String,
    pub subtitle: String,
    /// Positive state (replay ready / recording) gets the accent colour.
    pub active: bool,
    pub icon_exe: Option<PathBuf>,
    /// Bar and badge colour (default violet).
    pub accent: Option<(u8, u8, u8)>,
    /// How long it stays fully visible.
    pub hold_ms: u64,
    /// Narrower bar for short capture events.
    pub compact: bool,
}
impl Default for Notice {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            active: false,
            icon_exe: None,
            accent: None,
            hold_ms: 3600,
            compact: false,
        }
    }
}

pub const REC_RED: (u8, u8, u8) = (255, 77, 94);
pub const SAVED_GREEN: (u8, u8, u8) = (62, 214, 160);
pub const REPLAY_VIOLET: (u8, u8, u8) = (139, 92, 246);

/// Capture events announced with a short bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    RecordingStarted,
    ReplaySaved,
    RecordingSaved,
    SaveFailed,
}
/// Text and colour for a capture event. `game` is the clip's game folder
/// (Desktop, Mixed and Unsorted are not named), `seconds` its length.
pub fn capture_notice(
    event: CaptureEvent,
    game: Option<&str>,
    seconds: Option<f64>,
    stop_hotkey: &str,
) -> Notice {
    let length = seconds.map(|s| {
        let s = s.round() as u64;
        if s >= 3600 {
            format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
        } else {
            format!("{}:{:02}", s / 60, s % 60)
        }
    });
    let game = game.filter(|g| !["Desktop", "Mixed", "Unsorted"].contains(g));
    let detail = [game.map(str::to_owned), length, Some("in Clips".to_owned())]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let (title, subtitle, accent) = match event {
        CaptureEvent::RecordingStarted => (
            "Recording started".to_owned(),
            format!(
                "{} stops and saves it",
                stop_hotkey.replace("Control", "Ctrl")
            ),
            REC_RED,
        ),
        CaptureEvent::ReplaySaved => ("Instant Replay saved".to_owned(), detail, SAVED_GREEN),
        CaptureEvent::RecordingSaved => ("Recording saved".to_owned(), detail, SAVED_GREEN),
        CaptureEvent::SaveFailed => (
            "Save failed".to_owned(),
            "Open REPLAYX for details".to_owned(),
            REC_RED,
        ),
    };
    Notice {
        title,
        subtitle,
        active: true,
        accent: Some(accent),
        hold_ms: 2600,
        compact: true,
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct Overlay(mpsc::Sender<Notice>);
impl Overlay {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Notice>();
        thread::Builder::new()
            .name("replayx-notice".into())
            .spawn(move || {
                while let Ok(notice) = rx.recv() {
                    // Coalesce a burst: only the newest notice is shown.
                    let notice = rx.try_iter().last().unwrap_or(notice);
                    let _ = show(&notice);
                }
            })
            .expect("notice thread");
        Self(tx)
    }
    pub fn show(&self, notice: Notice) {
        let _ = self.0.send(notice);
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Paint>> = const { RefCell::new(None) };
}
struct Paint {
    title: Vec<u16>,
    subtitle: Vec<u16>,
    active: bool,
    accent: (u8, u8, u8),
    icon: Option<windows::Win32::UI::WindowsAndMessaging::HICON>,
    scale: f32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn rgb(r: u8, g: u8, b: u8) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    if msg == WM_PAINT {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        CURRENT.with(|c| {
            if let Some(p) = c.borrow().as_ref() {
                paint(hwnd, dc, p);
            }
        });
        let _ = EndPaint(hwnd, &ps);
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_NCHITTEST {
        // Click-through.
        return windows::Win32::Foundation::LRESULT(-1);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe fn paint(
    hwnd: windows::Win32::Foundation::HWND,
    dc: windows::Win32::Graphics::Gdi::HDC,
    p: &Paint,
) {
    use windows::Win32::{Foundation::RECT, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let px = |v: f32| (v * p.scale).round() as i32;
    let bg = CreateSolidBrush(rgb(24, 25, 31));
    FillRect(dc, &r, bg);
    let _ = DeleteObject(bg.into());
    let border = CreateSolidBrush(rgb(46, 47, 58));
    FrameRect(dc, &r, border);
    let _ = DeleteObject(border.into());
    let accent = CreateSolidBrush(rgb(p.accent.0, p.accent.1, p.accent.2));
    let bar = RECT {
        left: 0,
        top: 0,
        right: px(4.0),
        bottom: r.bottom,
    };
    FillRect(dc, &bar, accent);
    let _ = DeleteObject(accent.into());
    let icon_size = px(36.0);
    let ix = px(18.0);
    let iy = (r.bottom - icon_size) / 2;
    match p.icon {
        Some(icon) => {
            let _ = DrawIconEx(dc, ix, iy, icon, icon_size, icon_size, 0, None, DI_NORMAL);
        }
        None => {
            let b = CreateSolidBrush(rgb(p.accent.0, p.accent.1, p.accent.2));
            let old = SelectObject(dc, b.into());
            let pen = SelectObject(dc, GetStockObject(NULL_PEN));
            let _ = Ellipse(dc, ix, iy, ix + icon_size, iy + icon_size);
            SelectObject(dc, pen);
            SelectObject(dc, old);
            let _ = DeleteObject(b.into());
        }
    }
    let font = |height: f32, weight: i32| {
        CreateFontW(
            -px(height),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            windows::core::w!("Segoe UI"),
        )
    };
    SetBkMode(dc, TRANSPARENT);
    let left = ix + icon_size + px(14.0);
    let title_font = font(16.0, 600);
    let old = SelectObject(dc, title_font.into());
    SetTextColor(dc, rgb(244, 244, 247));
    let mut tr = RECT {
        left,
        top: px(15.0),
        right: r.right - px(14.0),
        bottom: px(38.0),
    };
    let mut title = p.title.clone();
    DrawTextW(
        dc,
        &mut title,
        &mut tr,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    let sub_font = font(13.0, 400);
    SelectObject(dc, sub_font.into());
    SetTextColor(
        dc,
        if p.active {
            rgb(196, 181, 253)
        } else {
            rgb(161, 161, 170)
        },
    );
    let mut sr = RECT {
        left,
        top: px(40.0),
        right: r.right - px(14.0),
        bottom: px(62.0),
    };
    let mut subtitle = p.subtitle.clone();
    DrawTextW(
        dc,
        &mut subtitle,
        &mut sr,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SelectObject(dc, old);
    let _ = DeleteObject(title_font.into());
    let _ = DeleteObject(sub_font.into());
}

fn show(n: &Notice) -> windows::core::Result<()> {
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{COLORREF, RECT},
            Graphics::Gdi::*,
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                HiDpi::{
                    GetDpiForSystem, SetThreadDpiAwarenessContext,
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                },
                Shell::ExtractIconExW,
                WindowsAndMessaging::*,
            },
        },
    };
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = w!("ReplayxGameNotice");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc); // Already registered on later notices: fine.
        let scale = GetDpiForSystem() as f32 / 96.0;
        let (w, h) = (
            ((if n.compact { 320.0 } else { 360.0 }) * scale) as i32,
            (76.0 * scale) as i32,
        );
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let margin = (20.0 * scale) as i32;
        let x = work.right - w - margin;
        let y = work.top + margin;
        let mut icon = None;
        if let Some(exe) = &n.icon_exe {
            let path: Vec<u16> = exe.as_os_str().encode_wide_nul();
            let mut large = HICON::default();
            if ExtractIconExW(PCWSTR(path.as_ptr()), 0, Some(&mut large), None, 1) > 0
                && !large.is_invalid()
            {
                icon = Some(large);
            }
        }
        CURRENT.with(|c| {
            *c.borrow_mut() = Some(Paint {
                title: wide(&n.title),
                subtitle: wide(&n.subtitle),
                active: n.active,
                accent: n.accent.unwrap_or((139, 92, 246)),
                icon,
                scale,
            })
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            class,
            w!("REPLAYX"),
            WS_POPUP,
            x + (24.0 * scale) as i32,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        // Never in clips (it used to be captured into the first seconds).
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        let radius = (14.0 * scale) as i32;
        let region = CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius);
        let _ = SetWindowRgn(hwnd, Some(region), false);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let pump = || {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };
        // Slide in and fade in (220 ms, ease-out), hold, fade out (300 ms).
        // About 30 short frames in total: no lasting work while a game runs.
        let ease = |t: f32| 1.0 - (1.0 - t).powi(3);
        let start = Instant::now();
        loop {
            let t = (start.elapsed().as_secs_f32() / 0.22).min(1.0);
            let e = ease(t);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x + ((1.0 - e) * 24.0 * scale) as i32,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), (e * 246.0) as u8, LWA_ALPHA);
            pump();
            if t >= 1.0 {
                break;
            }
            thread::sleep(Duration::from_millis(16));
        }
        let hold = Instant::now() + Duration::from_millis(n.hold_ms.clamp(800, 10_000));
        while Instant::now() < hold {
            pump();
            thread::sleep(Duration::from_millis(50));
        }
        let fade = Instant::now();
        loop {
            let t = (fade.elapsed().as_secs_f32() / 0.3).min(1.0);
            let _ =
                SetLayeredWindowAttributes(hwnd, COLORREF(0), ((1.0 - t) * 246.0) as u8, LWA_ALPHA);
            pump();
            if t >= 1.0 {
                break;
            }
            thread::sleep(Duration::from_millis(16));
        }
        let _ = DestroyWindow(hwnd);
        CURRENT.with(|c| {
            if let Some(p) = c.borrow_mut().take() {
                if let Some(icon) = p.icon {
                    let _ = DestroyIcon(icon);
                }
            }
        });
    }
    Ok(())
}

trait EncodeWideNul {
    fn encode_wide_nul(&self) -> Vec<u16>;
}
impl EncodeWideNul for std::ffi::OsStr {
    fn encode_wide_nul(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    const fn p(pid: u32, started: u64) -> ProcessId {
        ProcessId { pid, started }
    }
    /// Announcer driven by a fake process table.
    struct Sim {
        a: Announcer,
        t0: Instant,
        running: RefCell<Vec<ProcessId>>,
    }
    impl Sim {
        fn new() -> Self {
            Self {
                a: Announcer::default(),
                t0: Instant::now(),
                running: RefCell::new(Vec::new()),
            }
        }
        fn at(&mut self, s: u64, front: Option<(&str, ProcessId)>) -> Option<String> {
            let running = self.running.borrow().clone();
            self.a
                .observe(front, self.t0 + Duration::from_secs(s), |x| {
                    running.contains(&x)
                })
        }
    }
    #[test]
    fn a_game_is_announced_once_not_on_every_poll() {
        let mut s = Sim::new();
        let roblox = p(100, 1);
        s.running.borrow_mut().push(roblox);
        assert_eq!(s.at(0, Some(("Roblox", roblox))).as_deref(), Some("Roblox"));
        for t in 1..30 {
            assert_eq!(s.at(t, Some(("Roblox", roblox))), None);
        }
    }
    #[test]
    fn alt_tab_minimise_and_fullscreen_switches_never_repeat_it() {
        let mut s = Sim::new();
        let game = p(200, 7);
        s.running.borrow_mut().push(game);
        assert!(s.at(0, Some(("Fortnite", game))).is_some());
        // Alt+Tab out and back many times, also long after the old 10 minute
        // cooldown (the 0.3.1-alpha.8 repeat), and minimised for a while.
        for k in 1..20u64 {
            assert_eq!(s.at(k * 120, None), None);
            assert_eq!(s.at(k * 120 + 5, Some(("Fortnite", game))), None);
        }
        // Full screen <-> borderless keeps the process: still nothing new.
        assert_eq!(s.at(5000, Some(("Fortnite", game))), None);
    }
    #[test]
    fn closing_and_relaunching_is_a_new_session() {
        let mut s = Sim::new();
        let first = p(300, 1);
        s.running.borrow_mut().push(first);
        assert!(s.at(0, Some(("Valorant", first))).is_some());
        s.running.borrow_mut().clear(); // game closed
        assert_eq!(s.at(10, None), None);
        let second = p(301, 2);
        s.running.borrow_mut().push(second);
        assert_eq!(
            s.at(60, Some(("Valorant", second))).as_deref(),
            Some("Valorant")
        );
    }
    #[test]
    fn launcher_hand_off_and_quick_restart_continue_the_session() {
        let mut s = Sim::new();
        let launcher = p(400, 1);
        let client = p(401, 2);
        s.running.borrow_mut().extend([launcher, client]);
        assert!(s.at(0, Some(("Fortnite", launcher))).is_some());
        // The real game process takes over while the launcher still runs.
        assert_eq!(s.at(5, Some(("Fortnite", client))), None);
        // Crash and immediate restart (new process within the grace period).
        s.running.borrow_mut().clear();
        assert_eq!(s.at(30, None), None);
        let restarted = p(402, 3);
        s.running.borrow_mut().push(restarted);
        assert_eq!(s.at(35, Some(("Fortnite", restarted))), None);
    }
    #[test]
    fn a_reused_process_id_is_a_different_process() {
        let mut s = Sim::new();
        s.running.borrow_mut().push(p(500, 1));
        assert!(s.at(0, Some(("Minecraft", p(500, 1)))).is_some());
        s.running.borrow_mut().clear();
        assert_eq!(s.at(5, None), None);
        // Same pid, later creation time, after the grace period: new session.
        s.running.borrow_mut().push(p(500, 99));
        assert!(s.at(100, Some(("Minecraft", p(500, 99)))).is_some());
    }
    #[test]
    fn several_games_are_tracked_separately() {
        let mut s = Sim::new();
        let (a, b) = (p(600, 1), p(700, 1));
        s.running.borrow_mut().extend([a, b]);
        assert!(s.at(0, Some(("Roblox", a))).is_some());
        assert!(s.at(10, Some(("Minecraft", b))).is_some());
        assert_eq!(s.at(20, Some(("Roblox", a))), None);
        assert_eq!(s.at(30, Some(("Minecraft", b))), None);
    }
    #[test]
    fn capture_events_name_what_happened() {
        let started = capture_notice(
            CaptureEvent::RecordingStarted,
            None,
            None,
            "Control+Shift+F9",
        );
        assert_eq!(started.title, "Recording started");
        assert_eq!(started.subtitle, "Ctrl+Shift+F9 stops and saves it");
        assert_eq!(started.accent, Some(REC_RED));
        let replay = capture_notice(CaptureEvent::ReplaySaved, Some("Roblox"), Some(60.2), "x");
        assert_eq!(
            (replay.title.as_str(), replay.subtitle.as_str()),
            ("Instant Replay saved", "Roblox · 1:00 · in Clips")
        );
        let rec = capture_notice(
            CaptureEvent::RecordingSaved,
            Some("Desktop"),
            Some(3725.0),
            "x",
        );
        assert_eq!(
            (rec.title.as_str(), rec.subtitle.as_str()),
            ("Recording saved", "1:02:05 · in Clips")
        );
        assert_eq!(
            capture_notice(CaptureEvent::SaveFailed, None, None, "x").title,
            "Save failed"
        );
        assert!(replay.compact && replay.hold_ms < 3600, "short bar, brief");
    }
    #[test]
    fn the_notice_states_capture_truthfully() {
        assert_eq!(
            capture_line(false, false, "Stopped", "Control+Shift+F10"),
            "Instant Replay off"
        );
        assert_eq!(
            capture_line(true, false, "Buffering", "Control+Shift+F10"),
            "Instant Replay ready · Ctrl+Shift+F10 saves"
        );
        assert_eq!(
            capture_line(true, false, "Starting", "x"),
            "Capture starting…"
        );
        assert_eq!(
            capture_line(false, false, "Error", "x"),
            "Instant Replay off"
        );
        assert_eq!(capture_line(true, true, "Recording", "x"), "Recording");
    }
}
