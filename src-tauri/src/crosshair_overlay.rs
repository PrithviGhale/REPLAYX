//! The on-screen crosshair: a small native layered window on its own thread.
//!
//! Cost model (Crosshair Studio must never slow a game down):
//! - No render loop. The crosshair is rasterized once per change (settings,
//!   display, DPI) and handed to Windows with `UpdateLayeredWindow`; Windows'
//!   compositor (DWM) keeps showing that bitmap. Nothing is drawn per frame.
//! - The thread sleeps in `GetMessageW` and only wakes for a change, a
//!   display change or a z-order nudge from the 1-second desktop loop.
//! - Turning the crosshair off destroys the window and ends the thread.
//! - No screen capture, no encoder, no game hooks or injection: it is an
//!   ordinary top-level window above the game.
//!
//! Window behaviour (each from its documented Win32 semantics):
//! - `WS_EX_LAYERED | WS_EX_TRANSPARENT` plus `HTTRANSPARENT`: mouse input
//!   passes to the window underneath (the game).
//! - `WS_EX_NOACTIVATE`, `MA_NOACTIVATE`, `SW_SHOWNOACTIVATE`,
//!   `SWP_NOACTIVATE`: it never takes keyboard focus or activation.
//! - `WS_EX_TOOLWINDOW`: no taskbar button and no Alt+Tab entry.
//! - `WS_EX_TOPMOST`: above windowed and borderless games. Exclusive
//!   full-screen games are not composited by DWM, so no desktop window can
//!   appear over them.
//! - `WDA_EXCLUDEFROMCAPTURE` (Windows 10 2004+): left out of screen capture,
//!   so REPLAYX's own clips (Desktop Duplication and Windows Graphics
//!   Capture) don't contain it.

use crate::crosshair::{self, CrosshairSettings, Display};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

const WM_UPDATE: u32 = 0x8000 + 0x41; // WM_APP + n
const WM_RAISE: u32 = WM_UPDATE + 1;
const WM_RELAYOUT: u32 = WM_UPDATE + 2;

/// What the crosshair window is doing, for the Crosshair Studio page.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct OverlayState {
    /// The window exists (the crosshair is on).
    pub running: bool,
    /// The crosshair is on screen right now.
    pub shown: bool,
    /// Left out of screen capture (and so out of clips).
    pub excluded_from_capture: bool,
    /// The display it is on, e.g. "Display 1 · 2560 × 1600 · 150%".
    pub display: Option<String>,
    pub error: Option<String>,
    /// The window handle while it exists (diagnostics and native tests).
    #[serde(skip)]
    pub hwnd: isize,
}

#[derive(Clone)]
struct Wanted {
    settings: CrosshairSettings,
    visible: bool,
}

struct Running {
    thread_id: u32,
    join: thread::JoinHandle<()>,
}

/// Handle to the crosshair window. Cheap to call often: changes are coalesced
/// and only a real change reaches the window.
pub struct CrosshairOverlay {
    running: Mutex<Option<Running>>,
    wanted: Arc<Mutex<Wanted>>,
    state: Arc<Mutex<OverlayState>>,
}
impl Default for CrosshairOverlay {
    fn default() -> Self {
        Self {
            running: Mutex::new(None),
            wanted: Arc::new(Mutex::new(Wanted {
                settings: CrosshairSettings::default(),
                visible: false,
            })),
            state: Arc::new(Mutex::new(OverlayState::default())),
        }
    }
}
impl Drop for CrosshairOverlay {
    fn drop(&mut self) {
        self.stop();
    }
}
impl CrosshairOverlay {
    /// Show the crosshair as `c` describes (`visible`: whether it should be
    /// on screen now). Off: the window and its thread are removed.
    pub fn apply(&self, c: &CrosshairSettings, visible: bool) {
        if !c.enabled {
            self.stop();
            return;
        }
        *self.wanted.lock().unwrap() = Wanted {
            settings: c.sanitized(),
            visible,
        };
        let mut running = self.running.lock().unwrap();
        if running.is_none() {
            *running = self.start();
        }
        if let Some(r) = running.as_ref() {
            post(r.thread_id, WM_UPDATE);
        }
    }
    /// Put the crosshair back on top (another topmost window may have come
    /// up). Does nothing when it isn't shown.
    pub fn raise(&self) {
        if let Some(r) = self.running.lock().unwrap().as_ref() {
            if self.state.lock().unwrap().shown {
                post(r.thread_id, WM_RAISE);
            }
        }
    }
    /// Raise it only when a visible window has come above it and overlaps
    /// it (a game that made itself topmost after it came to the front, a
    /// launcher, a notification). Cheap enough for every 1 s tick: it reads
    /// the z-order without messaging any window, and the crosshair's thread
    /// is woken only when there is something to fix.
    pub fn raise_if_covered(&self) {
        let hwnd = {
            let s = self.state.lock().unwrap();
            if !s.shown || s.hwnd == 0 {
                return;
            }
            s.hwnd
        };
        if covered(hwnd) {
            self.raise();
        }
    }
    /// Remove the window and end its thread (turned off, or REPLAYX exits).
    pub fn stop(&self) {
        let running = self.running.lock().unwrap().take();
        if let Some(r) = running {
            post(
                r.thread_id,
                windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
            );
            let _ = r.join.join();
        }
        *self.state.lock().unwrap() = OverlayState::default();
    }
    pub fn state(&self) -> OverlayState {
        self.state.lock().unwrap().clone()
    }
    fn start(&self) -> Option<Running> {
        let (tx, rx) = mpsc::sync_channel(1);
        let (wanted, state) = (self.wanted.clone(), self.state.clone());
        let join = thread::Builder::new()
            .name("replayx-crosshair".into())
            .spawn(move || run(tx, wanted, state))
            .ok()?;
        match rx.recv() {
            Ok(Some(thread_id)) => Some(Running { thread_id, join }),
            _ => {
                let _ = join.join();
                None
            }
        }
    }
}

/// The crosshair as the app drives it: the window, when it is shown (see
/// `crosshair::Visibility`) and the show/hide shortcut. `sync` is called by
/// the 1-second desktop loop and after every change; the window only hears
/// about real changes.
#[derive(Default)]
pub struct CrosshairRuntime {
    pub overlay: CrosshairOverlay,
    gate: Mutex<crosshair::Visibility>,
    hidden: std::sync::atomic::AtomicBool,
    applied: Mutex<Option<(CrosshairSettings, bool)>>,
    /// The last game seen in front in exclusive full screen (no window can
    /// show over it), until it is seen in front without it. Checked while
    /// the game is in front: such games minimize when left, so the page
    /// could never see it.
    exclusive: Mutex<Option<String>>,
}
impl CrosshairRuntime {
    /// Note whether `game`, in front now, runs in exclusive full screen.
    pub fn note_exclusive(&self, game: &str, exclusive: bool) {
        *self.exclusive.lock().unwrap() = exclusive.then(|| game.to_owned());
    }
    /// The game last seen in exclusive full screen, if any.
    pub fn exclusive_game(&self) -> Option<String> {
        self.exclusive.lock().unwrap().clone()
    }
    /// Bring the window in line with the settings and what is in front.
    /// Returns whether the crosshair should be on screen.
    pub fn sync(&self, c: &CrosshairSettings, game_in_front: bool) -> bool {
        let hidden = self.hidden.load(std::sync::atomic::Ordering::Relaxed);
        let visible =
            self.gate
                .lock()
                .unwrap()
                .update(c, game_in_front, hidden, std::time::Instant::now());
        let mut applied = self.applied.lock().unwrap();
        if !applied
            .as_ref()
            .is_some_and(|(settings, shown)| settings == c && *shown == visible)
        {
            self.overlay.apply(c, visible);
            *applied = Some((c.clone(), visible));
        }
        visible
    }
    /// The show/hide shortcut. Returns true when the crosshair is now hidden.
    pub fn toggle_hidden(&self) -> bool {
        !self
            .hidden
            .fetch_xor(true, std::sync::atomic::Ordering::Relaxed)
    }
    /// Turning the crosshair on shows it again (an earlier shortcut press
    /// doesn't keep it hidden).
    pub fn reset_hidden(&self) {
        self.hidden
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
    /// Remove the window (REPLAYX exits).
    pub fn stop(&self) {
        self.overlay.stop();
        *self.applied.lock().unwrap() = None;
    }
}

/// Whether a visible window above `hwnd` in the z-order overlaps it.
fn covered(hwnd: isize) -> bool {
    use windows::Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{GetWindow, GetWindowRect, IsWindowVisible, GW_HWNDPREV},
    };
    unsafe {
        let me = HWND(hwnd as *mut _);
        let mut mine = RECT::default();
        if GetWindowRect(me, &mut mine).is_err() {
            return false;
        }
        let mut w = me;
        // Topmost windows are few; the bound only guards a broken chain.
        for _ in 0..256 {
            w = match GetWindow(w, GW_HWNDPREV) {
                Ok(p) if !p.0.is_null() => p,
                _ => return false,
            };
            if !IsWindowVisible(w).as_bool() {
                continue;
            }
            let mut r = RECT::default();
            if GetWindowRect(w, &mut r).is_ok()
                && r.left < mine.right
                && mine.left < r.right
                && r.top < mine.bottom
                && mine.top < r.bottom
            {
                return true;
            }
        }
        false
    }
}

fn post(thread_id: u32, msg: u32) {
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::PostThreadMessageW,
    };
    unsafe {
        let _ = PostThreadMessageW(thread_id, msg, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::*, System::Threading::GetCurrentThreadId, UI::WindowsAndMessaging::*,
    };
    match msg {
        // Click-through: the mouse goes to the game underneath.
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // A resolution, layout or scaling change: place it again.
        WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
            post(GetCurrentThreadId(), WM_RELAYOUT);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// A connected display, primary first, then in Windows' order.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DisplayInfo {
    pub index: u32,
    pub primary: bool,
    pub width: i32,
    pub height: i32,
    /// Windows scaling, 1.0 = 100%.
    pub scale: f32,
    #[serde(skip)]
    pub rect: Display,
}

/// The displays in desktop coordinates (physical pixels).
pub fn displays() -> Vec<DisplayInfo> {
    use windows::Win32::{
        Foundation::{LPARAM, RECT, TRUE},
        Graphics::Gdi::*,
        UI::HiDpi::*,
    };
    unsafe extern "system" fn each(
        mon: HMONITOR,
        _: HDC,
        _: *mut RECT,
        list: LPARAM,
    ) -> windows::core::BOOL {
        let list = &mut *(list.0 as *mut Vec<(HMONITOR, RECT, bool, u32)>);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            list.push((mon, info.rcMonitor, info.dwFlags & 1 == 1, dx.max(96)));
        }
        TRUE
    }
    unsafe {
        // Physical pixels whatever this thread's own DPI mode is.
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut list: Vec<(HMONITOR, RECT, bool, u32)> = Vec::new();
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
        if !previous.0.is_null() {
            SetThreadDpiAwarenessContext(previous);
        }
        // Primary first, keep Windows' order otherwise.
        list.sort_by_key(|(_, _, primary, _)| !primary);
        list.into_iter()
            .enumerate()
            .map(|(i, (_, r, primary, dpi))| {
                let scale = dpi as f32 / 96.0;
                DisplayInfo {
                    index: i as u32,
                    primary,
                    width: r.right - r.left,
                    height: r.bottom - r.top,
                    scale,
                    rect: Display {
                        left: r.left,
                        top: r.top,
                        right: r.right,
                        bottom: r.bottom,
                        scale,
                    },
                }
            })
            .collect()
    }
}

/// "Display 1 · 2560 × 1600 · 150%".
pub fn display_label(d: &DisplayInfo) -> String {
    format!(
        "Display {}{} · {} × {} · {}%",
        d.index + 1,
        if d.primary { " (main)" } else { "" },
        d.width,
        d.height,
        (d.scale * 100.0).round()
    )
}

/// What is on screen now, so an unchanged request draws nothing.
#[derive(PartialEq)]
struct Drawn {
    look: CrosshairSettings,
    display: Display,
    pos: (i32, i32),
}

fn run(
    ready: mpsc::SyncSender<Option<u32>>,
    wanted: Arc<Mutex<Wanted>>,
    state: Arc<Mutex<OverlayState>>,
) {
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND, System::LibraryLoader::GetModuleHandleW,
            System::Threading::GetCurrentThreadId, UI::HiDpi::*, UI::WindowsAndMessaging::*,
        },
    };
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Create this thread's message queue before anyone posts to it.
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        let fail = |e: String| {
            *state.lock().unwrap() = OverlayState {
                error: Some(e),
                ..Default::default()
            };
        };
        let Ok(instance) = GetModuleHandleW(None) else {
            fail("Windows didn't allow the crosshair window.".into());
            let _ = ready.send(None);
            return;
        };
        let class = w!("ReplayxCrosshair");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            class,
            w!("REPLAYX crosshair"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        ) else {
            fail("Windows didn't allow the crosshair window.".into());
            let _ = ready.send(None);
            return;
        };
        let mut affinity = WINDOW_DISPLAY_AFFINITY(0);
        let excluded = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_ok()
            && GetWindowDisplayAffinity(hwnd, &mut affinity.0).is_ok()
            && affinity == WDA_EXCLUDEFROMCAPTURE;
        if !excluded {
            let _ = DestroyWindow(hwnd);
            fail("Windows couldn't exclude the crosshair from recording. The overlay was kept hidden.".into());
            let _ = ready.send(None);
            return;
        }
        {
            let mut s = state.lock().unwrap();
            *s = OverlayState {
                running: true,
                excluded_from_capture: excluded,
                hwnd: hwnd.0 as isize,
                ..Default::default()
            };
        }
        let _ = ready.send(Some(GetCurrentThreadId()));
        let mut drawn: Option<Drawn> = None;
        let mut shown = false;
        let update = |hwnd: HWND, force: bool, drawn: &mut Option<Drawn>, shown: &mut bool| {
            let w = wanted.lock().unwrap().clone();
            let list = displays();
            let Some(d) = list
                .iter()
                .find(|d| d.index == w.settings.display)
                .or_else(|| list.first())
            else {
                return;
            };
            let look = CrosshairSettings {
                // Only the look decides the bitmap.
                enabled: true,
                visibility: String::new(),
                hotkey: String::new(),
                display: 0,
                preset: String::new(),
                ..w.settings.clone()
            };
            if w.visible {
                let (side, pixels) = crosshair::render(&look, d.rect.scale);
                let pos = crosshair::place(&d.rect, side, (look.offset_x, look.offset_y));
                let next = Drawn {
                    look,
                    display: d.rect,
                    pos,
                };
                if force || drawn.as_ref() != Some(&next) {
                    crate::status_overlay::present(
                        hwnd,
                        pos,
                        &(side as i32, side as i32, pixels),
                        100,
                    );
                    *drawn = Some(next);
                }
                if !*shown {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    *shown = true;
                }
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            } else if *shown {
                let _ = ShowWindow(hwnd, SW_HIDE);
                *shown = false;
            }
            let mut s = state.lock().unwrap();
            s.shown = *shown;
            s.display = Some(display_label(d));
        };
        // Sleeps here between changes: no work at all while nothing changes.
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            match msg.message {
                WM_UPDATE => update(hwnd, false, &mut drawn, &mut shown),
                WM_RELAYOUT => update(hwnd, true, &mut drawn, &mut shown),
                WM_RAISE if shown => {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        let _ = DestroyWindow(hwnd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_labels_read_naturally() {
        let d = DisplayInfo {
            index: 0,
            primary: true,
            width: 2560,
            height: 1600,
            scale: 1.5,
            rect: Display {
                left: 0,
                top: 0,
                right: 2560,
                bottom: 1600,
                scale: 1.5,
            },
        };
        assert_eq!(display_label(&d), "Display 1 (main) · 2560 × 1600 · 150%");
    }

    #[test]
    fn displays_lists_the_primary_first() {
        let list = displays();
        assert!(!list.is_empty(), "at least one display");
        assert!(list[0].primary);
        assert!(list
            .iter()
            .all(|d| d.width > 0 && d.height > 0 && d.scale >= 1.0));
        for (i, d) in list.iter().enumerate() {
            assert_eq!(d.index, i as u32);
        }
    }

    #[test]
    fn turned_off_never_creates_a_window() {
        let o = CrosshairOverlay::default();
        o.apply(&CrosshairSettings::default(), true);
        assert_eq!(o.state(), OverlayState::default());
        o.raise();
        o.stop();
    }

    /// Native: shows a small crosshair at the center of the main display for
    /// about two seconds. Run on a free desktop:
    /// `cargo test --lib crosshair_overlay -- --ignored --test-threads=1`
    #[test]
    #[ignore = "shows a crosshair on screen"]
    fn native_window_is_click_through_never_focused_and_cleans_up() {
        use windows::Win32::{
            Foundation::{HWND, POINT},
            UI::WindowsAndMessaging::*,
        };
        let on = CrosshairSettings {
            enabled: true,
            visibility: "always".into(),
            ..Default::default()
        };
        let o = CrosshairOverlay::default();
        let wait = |f: &dyn Fn(&OverlayState) -> bool| {
            for _ in 0..100 {
                if f(&o.state()) {
                    return true;
                }
                thread::sleep(std::time::Duration::from_millis(20));
            }
            false
        };
        unsafe {
            let _pixels = crate::geometry::PhysicalPixels::enter().unwrap();
            let before = GetForegroundWindow();
            for round in 0..5 {
                o.apply(&on, true);
                assert!(wait(&|s| s.shown), "round {round}: shown");
                let s = o.state();
                let hwnd = HWND(s.hwnd as *mut _);
                assert!(IsWindowVisible(hwnd).as_bool());
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                for (flag, what) in [
                    (WS_EX_TRANSPARENT.0, "click-through"),
                    (WS_EX_LAYERED.0, "layered"),
                    (WS_EX_NOACTIVATE.0, "never activated"),
                    (WS_EX_TOOLWINDOW.0, "no taskbar / Alt+Tab"),
                    (WS_EX_TOPMOST.0, "above games"),
                ] {
                    assert!(ex & flag != 0, "{what}");
                }
                assert!(s.excluded_from_capture, "kept out of clips");
                // The mouse goes through even where the crosshair is drawn:
                // probe a fully opaque pixel of it (a layered window without
                // WS_EX_TRANSPARENT would take the click there).
                let mut r = windows::Win32::Foundation::RECT::default();
                GetWindowRect(hwnd, &mut r).unwrap();
                let scale = displays()[0].scale;
                let (side, pixels) = crosshair::render(&on, scale);
                let i = pixels
                    .chunks(4)
                    .position(|p| p[3] == 255)
                    .expect("opaque pixel");
                let (x, y) = ((i as u32 % side) as i32, (i as u32 / side) as i32);
                assert_eq!(r.right - r.left, side as i32, "window is the bitmap");
                let drawn = POINT {
                    x: r.left + x,
                    y: r.top + y,
                };
                assert_ne!(
                    WindowFromPoint(drawn),
                    hwnd,
                    "click-through on the crosshair itself"
                );
                assert_ne!(GetForegroundWindow(), hwnd, "never focused");
                assert_eq!(GetForegroundWindow(), before, "focus unchanged");
                // Hidden and shown again without a new window.
                o.apply(&on, false);
                assert!(wait(&|s| !s.shown));
                assert!(!IsWindowVisible(hwnd).as_bool());
                o.apply(&on, true);
                assert!(wait(&|s| s.shown));
                // Off: the window is gone and the thread has ended.
                o.apply(&CrosshairSettings::default(), false);
                assert_eq!(o.state(), OverlayState::default());
                assert!(!IsWindow(Some(hwnd)).as_bool(), "round {round}: destroyed");
            }
        }
    }
}
