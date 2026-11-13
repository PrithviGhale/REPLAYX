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
    /// Creation time (FILETIME); 0 when Windows wouldn't tell (a protected
    /// process): the id alone identifies it then.
    pub started: u64,
}
/// Decides when a detection is news: once per game *session*, not per focus
/// change. A session is the life of the game's process (id plus creation
/// time).
///
/// - Alt+Tab, minimising, switching between full screen and borderless, or a
///   second window of the same process never repeat it, however long the
///   game runs.
/// - Closing the game and launching it again is a new session and is
///   announced again, however quickly it is relaunched.
/// - A second process of the same game that appears while an earlier one is
///   still running (launcher hand-off, a helper, a self-restart that starts
///   the new process before the old one has exited) continues the session.
/// - A reused process id is a different process (its creation time differs).
/// - Each game is tracked on its own.
///
/// `FLAP` suppresses a repeated notice for the same process identity when a
/// liveness check flickers. A distinct process starts a new session immediately.
#[derive(Default)]
pub struct Announcer {
    sessions: Vec<(String, ProcessId)>,
    announced: HashMap<String, (ProcessId, Instant)>,
}
impl Announcer {
    pub const FLAP: Duration = Duration::from_secs(3);
    /// `front`: the game in front and its process (None when no game is in
    /// front). `alive`: whether a tracked process is still running.
    pub fn observe(
        &mut self,
        front: Option<(&str, ProcessId)>,
        now: Instant,
        alive: impl Fn(ProcessId) -> bool,
    ) -> Option<String> {
        self.sessions.retain(|(_, process)| alive(*process));
        self.announced
            .retain(|_, (_, t)| now.duration_since(*t) < Self::FLAP);
        let (game, process) = front?;
        if self.sessions.iter().any(|(_, p)| *p == process) {
            return None;
        }
        let continuing = self.sessions.iter().any(|(g, _)| g == game);
        self.sessions.push((game.to_owned(), process));
        if continuing
            || self
                .announced
                .get(game)
                .is_some_and(|(previous, _)| *previous == process)
        {
            return None;
        }
        self.announced.insert(game.to_owned(), (process, now));
        Some(game.to_owned())
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
    /// Small caps line above the title ("GAME DETECTED", "REPLAYX").
    pub eyebrow: String,
    /// Symbol on the tile for capture events (Segoe MDL2 Assets code point);
    /// games show their icon, or their initial.
    pub glyph: Option<char>,
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
            eyebrow: "REPLAYX".into(),
            glyph: None,
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
    // Segoe MDL2 Assets: Record, CheckMark, Warning.
    let glyph = match event {
        CaptureEvent::RecordingStarted => '\u{E7C8}',
        CaptureEvent::ReplaySaved | CaptureEvent::RecordingSaved => '\u{E73E}',
        CaptureEvent::SaveFailed => '\u{E7BA}',
    };
    Notice {
        title,
        subtitle,
        glyph: Some(glyph),
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
    eyebrow: Vec<u16>,
    title: Vec<u16>,
    subtitle: Vec<u16>,
    /// Letter shown on the tile when the game has no icon.
    initial: Vec<u16>,
    glyph: Option<Vec<u16>>,
    active: bool,
    accent: (u8, u8, u8),
    icon: Option<windows::Win32::UI::WindowsAndMessaging::HICON>,
    scale: f32,
}

/// `a` blended towards `b` by `t` (0..1).
fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}
/// Card colours (REPLAYX's dark surfaces).
const CARD_TOP: (u8, u8, u8) = (27, 28, 37);
const CARD_BOTTOM: (u8, u8, u8) = (17, 18, 24);
const CARD_LINE: (u8, u8, u8) = (48, 50, 64);
const READY_GREEN: (u8, u8, u8) = (58, 210, 159);

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

/// A soft two-colour gradient across `r` (GDI GradientFill: cheap, no GPU).
unsafe fn gradient(
    dc: windows::Win32::Graphics::Gdi::HDC,
    r: windows::Win32::Foundation::RECT,
    from: (u8, u8, u8),
    to: (u8, u8, u8),
    vertical: bool,
) {
    use windows::Win32::Graphics::Gdi::*;
    let v = |x: i32, y: i32, c: (u8, u8, u8)| TRIVERTEX {
        x,
        y,
        Red: (c.0 as u16) << 8,
        Green: (c.1 as u16) << 8,
        Blue: (c.2 as u16) << 8,
        Alpha: 0xff00,
    };
    let vertices = [v(r.left, r.top, from), v(r.right, r.bottom, to)];
    let rect = GRADIENT_RECT {
        UpperLeft: 0,
        LowerRight: 1,
    };
    let _ = GradientFill(
        dc,
        &vertices,
        &rect as *const _ as *const core::ffi::c_void,
        1,
        if vertical {
            GRADIENT_FILL_RECT_V
        } else {
            GRADIENT_FILL_RECT_H
        },
    );
}

unsafe fn paint(
    hwnd: windows::Win32::Foundation::HWND,
    dc: windows::Win32::Graphics::Gdi::HDC,
    p: &Paint,
) {
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetClientRect};
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    draw_card(dc, r, p);
}

/// The notice card. Drawn into any DC so tests can render it off screen.
unsafe fn draw_card(
    dc: windows::Win32::Graphics::Gdi::HDC,
    r: windows::Win32::Foundation::RECT,
    p: &Paint,
) {
    use windows::Win32::{Foundation::RECT, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    let px = |v: f32| (v * p.scale).round() as i32;
    let brush = |c: (u8, u8, u8)| CreateSolidBrush(rgb(c.0, c.1, c.2));
    let fill = |rect: &RECT, c: (u8, u8, u8)| {
        let b = brush(c);
        FillRect(dc, rect, b);
        let _ = DeleteObject(b.into());
    };
    let round = |rect: RECT, radius: i32, c: (u8, u8, u8)| {
        let b = brush(c);
        let old = SelectObject(dc, b.into());
        let pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let _ = RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right + 1,
            rect.bottom + 1,
            radius,
            radius,
        );
        SelectObject(dc, pen);
        SelectObject(dc, old);
        let _ = DeleteObject(b.into());
    };
    // Surface: a quiet top-to-bottom gradient, then the accent glowing in
    // from the left edge and fading out by a third of the width.
    gradient(dc, r, CARD_TOP, CARD_BOTTOM, true);
    let glow = RECT {
        right: r.left + (r.right - r.left) * 2 / 5,
        ..r
    };
    gradient(dc, glow, mix(CARD_TOP, p.accent, 0.16), CARD_TOP, false);
    // Hairline border and a one-pixel highlight along the top edge.
    let border = brush(CARD_LINE);
    FrameRect(dc, &r, border);
    let _ = DeleteObject(border.into());
    fill(
        &RECT {
            left: r.left + px(12.0),
            top: r.top + 1,
            right: r.right - px(12.0),
            bottom: r.top + 2,
        },
        (58, 60, 76),
    );
    // Accent pill, inset from the edges.
    round(
        RECT {
            left: px(7.0),
            top: px(16.0),
            right: px(10.0),
            bottom: r.bottom - px(16.0),
        },
        px(3.0),
        p.accent,
    );
    // Icon tile: the game's own icon, or its initial on an accent tile.
    let tile = px(46.0);
    let (tx, ty) = (px(20.0), (r.bottom - tile) / 2);
    let tile_rect = RECT {
        left: tx,
        top: ty,
        right: tx + tile,
        bottom: ty + tile,
    };
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
    match p.icon {
        Some(icon) => {
            round(tile_rect, px(12.0), (36, 38, 50));
            let size = px(32.0);
            let _ = DrawIconEx(
                dc,
                tx + (tile - size) / 2,
                ty + (tile - size) / 2,
                icon,
                size,
                size,
                0,
                None,
                DI_NORMAL,
            );
        }
        None if p.glyph.is_some() => {
            round(tile_rect, px(12.0), mix(CARD_TOP, p.accent, 0.22));
            let symbol = CreateFontW(
                -px(20.0),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                windows::core::w!("Segoe MDL2 Assets"),
            );
            let old = SelectObject(dc, symbol.into());
            SetTextColor(dc, rgb(p.accent.0, p.accent.1, p.accent.2));
            let mut lr = tile_rect;
            let mut glyph = p.glyph.clone().unwrap_or_default();
            DrawTextW(
                dc,
                &mut glyph,
                &mut lr,
                DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX,
            );
            SelectObject(dc, old);
            let _ = DeleteObject(symbol.into());
        }
        None => {
            round(tile_rect, px(12.0), mix(CARD_TOP, p.accent, 0.55));
            let letter = font(20.0, 700);
            let old = SelectObject(dc, letter.into());
            SetTextColor(dc, rgb(255, 255, 255));
            let mut lr = tile_rect;
            let mut initial = p.initial.clone();
            DrawTextW(
                dc,
                &mut initial,
                &mut lr,
                DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX,
            );
            SelectObject(dc, old);
            let _ = DeleteObject(letter.into());
        }
    }
    let left = tx + tile + px(14.0);
    let right = r.right - px(16.0);
    // Eyebrow: small, spaced, in the accent colour.
    let eyebrow_font = font(10.5, 700);
    let old = SelectObject(dc, eyebrow_font.into());
    SetTextCharacterExtra(dc, px(0.9));
    let eyebrow_colour = mix(p.accent, (255, 255, 255), 0.25);
    SetTextColor(
        dc,
        rgb(eyebrow_colour.0, eyebrow_colour.1, eyebrow_colour.2),
    );
    let mut er = RECT {
        left,
        top: px(14.0),
        right,
        bottom: px(28.0),
    };
    let mut eyebrow = p.eyebrow.clone();
    DrawTextW(
        dc,
        &mut eyebrow,
        &mut er,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SetTextCharacterExtra(dc, 0);
    // Title: the game (or the event).
    let title_font = font(16.5, 600);
    SelectObject(dc, title_font.into());
    SetTextColor(dc, rgb(246, 246, 250));
    let mut tr = RECT {
        left,
        top: px(28.0),
        right,
        bottom: px(51.0),
    };
    let mut title = p.title.clone();
    DrawTextW(
        dc,
        &mut title,
        &mut tr,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    // Status: a dot (green when capture is ready) and one line.
    let dot = px(7.0);
    let (dx, dy) = (left, px(56.0) + (px(18.0) - dot) / 2);
    // Capture readiness is green; capture events use their own colour.
    let dot_colour = match (p.active, p.glyph.is_some()) {
        (true, true) => p.accent,
        (true, false) => READY_GREEN,
        _ => (112, 116, 136),
    };
    let b = brush(dot_colour);
    let oldb = SelectObject(dc, b.into());
    let pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = Ellipse(dc, dx, dy, dx + dot + 1, dy + dot + 1);
    SelectObject(dc, pen);
    SelectObject(dc, oldb);
    let _ = DeleteObject(b.into());
    let sub_font = font(12.5, 400);
    SelectObject(dc, sub_font.into());
    SetTextColor(
        dc,
        if p.active {
            rgb(200, 204, 220)
        } else {
            rgb(150, 154, 172)
        },
    );
    let mut sr = RECT {
        left: left + dot + px(7.0),
        top: px(55.0),
        right,
        bottom: px(74.0),
    };
    let mut subtitle = p.subtitle.clone();
    DrawTextW(
        dc,
        &mut subtitle,
        &mut sr,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SelectObject(dc, old);
    let _ = DeleteObject(eyebrow_font.into());
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
            ((if n.compact { 330.0 } else { 372.0 }) * scale) as i32,
            (86.0 * scale) as i32,
        );
        // The work area of the monitor the game (foreground window) is on,
        // not always the primary one.
        let mut work = RECT::default();
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work = info.rcWork;
        } else {
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut _ as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }
        // Windows' "Animation effects" off (reduced motion): fade only.
        let mut animate = windows::core::BOOL(1);
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut animate as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let slide = if animate.as_bool() { 24.0 * scale } else { 0.0 };
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
                eyebrow: wide(&n.eyebrow.to_uppercase()),
                title: wide(&n.title),
                subtitle: wide(&n.subtitle),
                glyph: n.glyph.map(|g| wide(&g.to_string())),
                initial: wide(
                    &n.title
                        .chars()
                        .find(|c| c.is_alphanumeric())
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_else(|| "R".into()),
                ),
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
            x + slide as i32,
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
        // Slide in and fade in (240 ms, ease-out), hold, then fade out while
        // drifting 8 px right (260 ms). About 30 short frames in total: no
        // lasting work while a game runs. Reduced motion: fades only.
        let ease = |t: f32| 1.0 - (1.0 - t).powi(3);
        let start = Instant::now();
        loop {
            let t = (start.elapsed().as_secs_f32() / 0.24).min(1.0);
            let e = ease(t);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x + ((1.0 - e) * slide) as i32,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), (e * 248.0) as u8, LWA_ALPHA);
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
            let t = (fade.elapsed().as_secs_f32() / 0.26).min(1.0);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x + (t * t * slide / 3.0) as i32,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ =
                SetLayeredWindowAttributes(hwnd, COLORREF(0), ((1.0 - t) * 248.0) as u8, LWA_ALPHA);
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
    /// Renders a card off screen (no window) and returns its BGRA pixels.
    fn render(p: &Paint, w: i32, h: i32) -> Vec<u8> {
        use windows::Win32::{Foundation::RECT, Graphics::Gdi::*};
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let old = SelectObject(dc, bitmap.into());
            draw_card(
                dc,
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
                p,
            );
            let _ = GdiFlush();
            let pixels =
                std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
            if let Some(path) = std::env::var_os("REPLAYX_CARD_PREVIEW") {
                // Uncompressed BMP for a visual check (see docs: UI review).
                let mut bmp = Vec::new();
                let size = 54 + pixels.len() as u32;
                bmp.extend_from_slice(b"BM");
                bmp.extend_from_slice(&size.to_le_bytes());
                bmp.extend_from_slice(&[0; 4]);
                bmp.extend_from_slice(&54u32.to_le_bytes());
                bmp.extend_from_slice(&40u32.to_le_bytes());
                bmp.extend_from_slice(&w.to_le_bytes());
                bmp.extend_from_slice(&(-h).to_le_bytes());
                bmp.extend_from_slice(&1u16.to_le_bytes());
                bmp.extend_from_slice(&32u16.to_le_bytes());
                bmp.extend_from_slice(&[0; 24]);
                bmp.extend_from_slice(&pixels);
                let name = String::from_utf16_lossy(&p.title).replace(' ', "_");
                std::fs::write(std::path::Path::new(&path).join(format!("{name}.bmp")), bmp)
                    .unwrap();
            }
            SelectObject(dc, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
            pixels
        }
    }
    fn card(
        title: &str,
        eyebrow: &str,
        subtitle: &str,
        active: bool,
        accent: (u8, u8, u8),
    ) -> Paint {
        Paint {
            eyebrow: wide(eyebrow),
            title: wide(title),
            subtitle: wide(subtitle),
            initial: wide(&title[..1]),
            glyph: (eyebrow == "REPLAYX").then(|| {
                wide(if title.starts_with("Recording") {
                    "\u{E7C8}"
                } else {
                    "\u{E73E}"
                })
            }),
            active,
            accent,
            icon: None,
            scale: 1.0,
        }
    }
    #[test]
    fn the_notice_card_renders_its_surface_accent_and_text() {
        let (w, h) = (372, 86);
        let pick = |px: &[u8], x: i32, y: i32| {
            let i = ((y * w + x) * 4) as usize;
            (px[i + 2], px[i + 1], px[i])
        };
        let px = render(
            &card(
                "Fortnite",
                "GAME DETECTED",
                "Instant Replay ready · Ctrl+Shift+F10 saves",
                true,
                REPLAY_VIOLET,
            ),
            w,
            h,
        );
        // Dark surface away from the glow; accent pill near the left edge.
        let (r, g, b) = pick(&px, w - 30, h - 10);
        assert!(r < 40 && g < 40 && b < 50, "surface {r},{g},{b}");
        let (r, _, b) = pick(&px, 8, h / 2);
        assert!(r > 100 && b > 180, "accent pill {r},{b}");
        // Title text is drawn (light pixels in the title row).
        let light = (80..300).any(|x| {
            let (r, g, b) = pick(&px, x, 40);
            r > 200 && g > 200 && b > 200
        });
        assert!(light, "title text");
        for (title, eyebrow, sub, active, accent) in [
            (
                "Valorant",
                "GAME DETECTED",
                "Instant Replay off",
                false,
                REPLAY_VIOLET,
            ),
            (
                "Instant Replay saved",
                "REPLAYX",
                "Fortnite · 1:00 · in Clips",
                true,
                SAVED_GREEN,
            ),
            (
                "Recording started",
                "REPLAYX",
                "Ctrl+Shift+F9 stops and saves it",
                true,
                REC_RED,
            ),
        ] {
            render(&card(title, eyebrow, sub, active, accent), w, h);
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
    fn a_launcher_hand_off_continues_the_session() {
        let mut s = Sim::new();
        let launcher = p(400, 1);
        let client = p(401, 2);
        s.running.borrow_mut().extend([launcher, client]);
        assert!(s.at(0, Some(("Fortnite", launcher))).is_some());
        // The real game process takes over while the launcher still runs.
        assert_eq!(s.at(5, Some(("Fortnite", client))), None);
        // The launcher exits; the game keeps running: same session.
        s.running.borrow_mut().retain(|x| *x == client);
        assert_eq!(s.at(20, Some(("Fortnite", client))), None);
    }
    #[test]
    fn a_quick_relaunch_is_announced_again() {
        // The 0.3.1-alpha.9 review: a relaunch within 15 s was silent.
        for gap in [4u64, 5, 10, 14] {
            let mut s = Sim::new();
            let first = p(400, 1);
            s.running.borrow_mut().push(first);
            assert!(s.at(0, Some(("Fortnite", first))).is_some());
            assert_eq!(s.at(20, Some(("Fortnite", first))), None);
            // Closed (or crashed) and launched again `gap` seconds later.
            s.running.borrow_mut().clear();
            assert_eq!(s.at(30, None), None);
            let again = p(402, 3);
            s.running.borrow_mut().push(again);
            assert_eq!(
                s.at(30 + gap, Some(("Fortnite", again))).as_deref(),
                Some("Fortnite"),
                "relaunched after {gap} s"
            );
            // ...and once only, through Alt+Tab and full-screen switches.
            for t in 1..30 {
                let front = (t % 3 != 0).then_some(("Fortnite", again));
                assert_eq!(s.at(30 + gap + t, front), None);
            }
        }
    }
    #[test]
    fn a_distinct_process_is_announced_within_the_flap_window() {
        for (delay, again) in [(1, p(401, 2)), (2, p(400, 2))] {
            let mut s = Sim::new();
            s.running.borrow_mut().push(p(400, 1));
            assert!(s.at(0, Some(("Fortnite", p(400, 1)))).is_some());
            s.running.borrow_mut().clear();
            assert_eq!(s.at(1, None), None);
            s.running.borrow_mut().push(again);
            assert_eq!(
                s.at(delay, Some(("Fortnite", again))).as_deref(),
                Some("Fortnite")
            );
            assert_eq!(s.at(10, Some(("Fortnite", again))), None);
            assert_eq!(s.at(11, None), None);
            assert_eq!(s.at(12, Some(("Fortnite", again))), None);
        }
    }
    #[test]
    fn a_relaunch_seen_without_a_gap_in_front_is_announced_too() {
        // The old process is gone by the next check and the new one is
        // already in front: no "nothing in front" sample in between.
        let mut s = Sim::new();
        s.running.borrow_mut().push(p(800, 1));
        assert!(s.at(0, Some(("Apex Legends", p(800, 1)))).is_some());
        *s.running.borrow_mut() = vec![p(801, 2)];
        assert!(s.at(6, Some(("Apex Legends", p(801, 2)))).is_some());
    }
    #[test]
    fn a_reused_process_id_is_a_different_process() {
        let mut s = Sim::new();
        s.running.borrow_mut().push(p(500, 1));
        assert!(s.at(0, Some(("Minecraft", p(500, 1)))).is_some());
        s.running.borrow_mut().clear();
        assert_eq!(s.at(5, None), None);
        // Same pid, later creation time, soon after: a new session.
        s.running.borrow_mut().push(p(500, 99));
        assert!(s.at(9, Some(("Minecraft", p(500, 99)))).is_some());
        // The same id and creation time is the same process.
        assert_eq!(s.at(12, Some(("Minecraft", p(500, 99)))), None);
    }
    #[test]
    fn a_liveness_glitch_right_after_the_notice_does_not_repeat_it() {
        let mut s = Sim::new();
        let game = p(900, 1);
        s.running.borrow_mut().push(game);
        assert!(s.at(0, Some(("Roblox", game))).is_some());
        // The check wrongly reports it ended for one sample, then it is back.
        s.running.borrow_mut().clear();
        assert_eq!(s.at(1, None), None);
        s.running.borrow_mut().push(game);
        assert_eq!(s.at(2, Some(("Roblox", game))), None);
    }
    #[test]
    fn games_relaunch_independently() {
        let mut s = Sim::new();
        let (a, b) = (p(600, 1), p(700, 1));
        s.running.borrow_mut().extend([a, b]);
        assert!(s.at(0, Some(("Roblox", a))).is_some());
        assert!(s.at(10, Some(("Minecraft", b))).is_some());
        // Roblox is relaunched while Minecraft keeps running.
        s.running.borrow_mut().retain(|x| *x == b);
        let a2 = p(601, 2);
        s.running.borrow_mut().push(a2);
        assert!(s.at(20, Some(("Roblox", a2))).is_some());
        assert_eq!(s.at(25, Some(("Minecraft", b))), None);
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
