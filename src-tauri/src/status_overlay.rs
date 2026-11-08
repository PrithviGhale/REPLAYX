//! In-game status overlay: a small translucent pill in a corner of the display
//! the game is on, showing Instant Replay, recording (with its timer) and the
//! microphone. A normal top-most desktop window drawn with GDI on its own
//! thread: no injection into games, no hooks, click-through, never focused.
//!
//! It is excluded from capture (`WDA_EXCLUDEFROMCAPTURE`), so it never
//! appears in clips; `replayx-cli overlay-test` checks that on real Desktop
//! Duplication frames. Like any desktop window it shows over windowed and
//! borderless games, not over exclusive full-screen ones.
use crate::{engine::Status, model::Settings};
use serde::{Deserialize, Serialize};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Overlay preferences (part of `Settings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    pub enabled: bool,
    /// "bottom-right", "bottom-left", "top-right" or "top-left".
    pub position: String,
    /// Window opacity in percent (30-100).
    pub opacity: u8,
    /// Dots only (plus the recording timer).
    pub compact: bool,
    pub show_replay: bool,
    pub show_mic: bool,
    pub show_timer: bool,
    /// Only while a detected game is in front (otherwise always).
    pub games_only: bool,
}
impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "bottom-right".into(),
            opacity: 85,
            compact: false,
            show_replay: true,
            show_mic: true,
            show_timer: true,
            games_only: true,
        }
    }
}
pub const POSITIONS: [&str; 4] = ["bottom-right", "bottom-left", "top-right", "top-left"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReplayState {
    Off,
    Starting,
    On,
    Saving,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MicState {
    /// No microphone source configured.
    None,
    Off,
    Muted,
    /// Will be recorded when capture starts ("armed").
    Ready,
    /// Being recorded right now.
    Live,
    Error,
}
/// What the overlay shows, derived only from the engine's real state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct View {
    pub replay: ReplayState,
    /// Recording length in whole seconds, only once video is being written.
    pub recording: Option<u64>,
    pub mic: MicState,
}
pub fn view(status: &Status, settings: &Settings) -> View {
    let capturing = matches!(status.state.as_str(), "Buffering" | "Recording");
    let error = status.state == "Error";
    let replay = if error && status.replay {
        ReplayState::Error
    } else if status.replay && status.saving > 0 {
        ReplayState::Saving
    } else if status.replay && capturing {
        ReplayState::On
    } else if status.replay || status.state == "Starting" {
        ReplayState::Starting
    } else {
        ReplayState::Off
    };
    let recording = (status.recording
        && status.state == "Recording"
        && !status.pending_recording
        && status.recording_elapsed > 0.0)
        .then_some(status.recording_elapsed as u64);
    let mic = match settings.audio.iter().find(|a| a.kind == "microphone") {
        None => MicState::None,
        Some(m) if !m.enabled => MicState::Off,
        Some(_) if error => MicState::Error,
        Some(m) if m.muted => MicState::Muted,
        Some(_) if capturing => MicState::Live,
        Some(_) => MicState::Ready,
    };
    View {
        replay,
        recording,
        mic,
    }
}
/// Whether the overlay should be on screen.
pub fn visible(o: &OverlaySettings, in_game: bool) -> bool {
    o.enabled && (!o.games_only || in_game)
}

type Rgb = (u8, u8, u8);
const VIOLET: Rgb = (154, 140, 255);
const REC: Rgb = (255, 74, 90);
const AMBER: Rgb = (242, 181, 82);
const GREEN: Rgb = (58, 210, 159);
const GREY: Rgb = (122, 122, 138);
/// One dot-and-label item, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub color: Rgb,
    pub label: String,
}
fn clock(s: u64) -> String {
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
/// Items for a view. Compact mode keeps only the recording timer as text.
pub fn items(v: &View, o: &OverlaySettings) -> Vec<Item> {
    let mut out = Vec::new();
    let label = |full: &str| {
        if o.compact {
            String::new()
        } else {
            full.to_owned()
        }
    };
    if let Some(s) = v.recording {
        out.push(Item {
            color: REC,
            label: if o.show_timer {
                format!("REC {}", clock(s))
            } else {
                "REC".into()
            },
        });
    }
    if o.show_replay {
        let (color, text) = match v.replay {
            ReplayState::Off => (GREY, "Replay off"),
            ReplayState::Starting => (AMBER, "Starting…"),
            ReplayState::On => (VIOLET, "Replay on"),
            ReplayState::Saving => (AMBER, "Saving…"),
            ReplayState::Error => (REC, "Replay error"),
        };
        out.push(Item {
            color,
            label: label(text),
        });
    }
    if o.show_mic {
        let state = match v.mic {
            MicState::None => None,
            MicState::Off => Some((GREY, "Mic off")),
            MicState::Muted => Some((AMBER, "Mic muted")),
            MicState::Ready => Some((VIOLET, "Mic ready")),
            MicState::Live => Some((GREEN, "Mic live")),
            MicState::Error => Some((REC, "Mic error")),
        };
        if let Some((color, text)) = state {
            out.push(Item {
                color,
                label: label(text),
            });
        }
    }
    out
}

/// What the overlay thread draws; `None` hides it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub items: Vec<Item>,
    pub position: String,
    pub opacity: u8,
    /// Leave the window visible to screen capture (only for `overlay-test`).
    pub capturable: bool,
}
#[derive(Clone)]
pub struct StatusOverlay(mpsc::Sender<Option<Frame>>);
impl StatusOverlay {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Option<Frame>>();
        thread::Builder::new()
            .name("replayx-status-overlay".into())
            .spawn(move || run(rx))
            .expect("overlay thread");
        Self(tx)
    }
    pub fn update(&self, frame: Option<Frame>) {
        let _ = self.0.send(frame);
    }
}

thread_local! {
    static PAINT: std::cell::RefCell<(Vec<Item>, f32)> = const { std::cell::RefCell::new((Vec::new(), 1.0)) };
}

fn rgb(c: Rgb) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16)
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

// Layout in 96-dpi pixels.
const HEIGHT: f32 = 30.0;
const PAD: f32 = 12.0;
const DOT: f32 = 8.0;
const DOT_GAP: f32 = 6.0;
const ITEM_GAP: f32 = 14.0;
const FONT: f32 = 13.0;
const MARGIN: f32 = 18.0;

unsafe fn font(scale: f32) -> windows::Win32::Graphics::Gdi::HFONT {
    use windows::Win32::Graphics::Gdi::*;
    CreateFontW(
        -((FONT * scale).round() as i32),
        0,
        0,
        0,
        600,
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
}

/// Width of each label in device pixels.
unsafe fn measure(items: &[Item], scale: f32) -> Vec<i32> {
    use windows::Win32::{Foundation::SIZE, Graphics::Gdi::*};
    let dc = GetDC(None);
    let f = font(scale);
    let old = SelectObject(dc, f.into());
    let widths = items
        .iter()
        .map(|i| {
            if i.label.is_empty() {
                return 0;
            }
            let mut size = SIZE::default();
            let _ = GetTextExtentPoint32W(dc, &wide(&i.label), &mut size);
            size.cx
        })
        .collect();
    SelectObject(dc, old);
    let _ = DeleteObject(f.into());
    ReleaseDC(None, dc);
    widths
}

fn width_for(widths: &[i32], scale: f32) -> i32 {
    let px = |v: f32| (v * scale).round() as i32;
    let mut w = px(PAD) * 2;
    for (n, tw) in widths.iter().enumerate() {
        if n > 0 {
            w += px(ITEM_GAP);
        }
        w += px(DOT);
        if *tw > 0 {
            w += px(DOT_GAP) + tw;
        }
    }
    w
}

unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            PAINT.with(|p| {
                let p = p.borrow();
                paint(hwnd, dc, &p.0, p.1);
            });
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        // Click-through: the mouse goes to the game underneath.
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn paint(
    hwnd: windows::Win32::Foundation::HWND,
    dc: windows::Win32::Graphics::Gdi::HDC,
    items: &[Item],
    scale: f32,
) {
    use windows::Win32::{Foundation::RECT, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    let px = |v: f32| (v * scale).round() as i32;
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let bg = CreateSolidBrush(rgb((17, 17, 23)));
    FillRect(dc, &r, bg);
    let _ = DeleteObject(bg.into());
    let border = CreateSolidBrush(rgb((52, 48, 78)));
    FrameRect(dc, &r, border);
    let _ = DeleteObject(border.into());
    let widths = measure(items, scale);
    let f = font(scale);
    let old_font = SelectObject(dc, f.into());
    SetBkMode(dc, TRANSPARENT);
    let null_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let mut x = px(PAD);
    let mid = r.bottom / 2;
    for (n, (item, tw)) in items.iter().zip(&widths).enumerate() {
        if n > 0 {
            x += px(ITEM_GAP);
        }
        let b = CreateSolidBrush(rgb(item.color));
        let old = SelectObject(dc, b.into());
        let d = px(DOT);
        let _ = Ellipse(dc, x, mid - d / 2, x + d, mid - d / 2 + d);
        SelectObject(dc, old);
        let _ = DeleteObject(b.into());
        x += d;
        if *tw > 0 {
            x += px(DOT_GAP);
            SetTextColor(dc, rgb((236, 236, 242)));
            let mut tr = RECT {
                left: x,
                top: 0,
                right: x + tw + 2,
                bottom: r.bottom,
            };
            let mut text = wide(&item.label);
            DrawTextW(
                dc,
                &mut text,
                &mut tr,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
            x += tw;
        }
    }
    SelectObject(dc, null_pen);
    SelectObject(dc, old_font);
    let _ = DeleteObject(f.into());
}

/// The monitor rectangle to anchor to: the one with the foreground window.
/// A full-screen window covers the taskbar, so its whole monitor is used;
/// otherwise the work area (above the taskbar).
unsafe fn anchor() -> windows::Win32::Foundation::RECT {
    use windows::Win32::{Foundation::RECT, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    let fg = GetForegroundWindow();
    let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(mon, &mut info).as_bool() {
        return RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
    }
    let mut wr = RECT::default();
    let full = GetWindowRect(fg, &mut wr).is_ok()
        && wr.left <= info.rcMonitor.left
        && wr.top <= info.rcMonitor.top
        && wr.right >= info.rcMonitor.right
        && wr.bottom >= info.rcMonitor.bottom;
    if full {
        info.rcMonitor
    } else {
        info.rcWork
    }
}

fn place(area: (i32, i32, i32, i32), size: (i32, i32), position: &str, margin: i32) -> (i32, i32) {
    let (l, t, r, b) = area;
    let x = if position.ends_with("left") {
        l + margin
    } else {
        r - size.0 - margin
    };
    let y = if position.starts_with("top") {
        t + margin
    } else {
        b - size.1 - margin
    };
    (x, y)
}

fn run(rx: mpsc::Receiver<Option<Frame>>) {
    use windows::{
        core::w,
        Win32::{
            Foundation::{COLORREF, HWND},
            Graphics::Gdi::*,
            System::LibraryLoader::GetModuleHandleW,
            UI::{HiDpi::*, WindowsAndMessaging::*},
        },
    };
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        let class = w!("ReplayxStatusOverlay");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
        let mut hwnd: Option<HWND> = None;
        let mut shown: Option<(Frame, (i32, i32, i32, i32))> = None;
        let mut last_place = Instant::now() - Duration::from_secs(10);
        loop {
            // Coalesce: only the newest frame matters.
            let next = match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(f) => Some(rx.try_iter().last().unwrap_or(f)),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if let Some(frame) = next {
                match frame {
                    None => {
                        if let Some(h) = hwnd {
                            let _ = ShowWindow(h, SW_HIDE);
                        }
                        shown = None;
                    }
                    Some(frame) => {
                        let h = match hwnd {
                            Some(h) => h,
                            None => {
                                let Ok(h) = CreateWindowExW(
                                    WS_EX_TOPMOST
                                        | WS_EX_TOOLWINDOW
                                        | WS_EX_NOACTIVATE
                                        | WS_EX_LAYERED
                                        | WS_EX_TRANSPARENT,
                                    class,
                                    w!("REPLAYX status"),
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
                                    continue;
                                };
                                hwnd = Some(h);
                                h
                            }
                        };
                        // Never in clips: Desktop Duplication and Windows
                        // Graphics Capture skip this window (Windows 10 2004+).
                        let _ = SetWindowDisplayAffinity(
                            h,
                            if frame.capturable {
                                WDA_NONE
                            } else {
                                WDA_EXCLUDEFROMCAPTURE
                            },
                        );
                        let area = anchor();
                        let area = (area.left, area.top, area.right, area.bottom);
                        let unchanged = shown
                            .as_ref()
                            .is_some_and(|(f, a)| *f == frame && *a == area);
                        if !unchanged {
                            let scale = GetDpiForWindow(h).max(96) as f32 / 96.0;
                            let widths = measure(&frame.items, scale);
                            let size = (width_for(&widths, scale), (HEIGHT * scale).round() as i32);
                            let (x, y) =
                                place(area, size, &frame.position, (MARGIN * scale).round() as i32);
                            PAINT.with(|p| *p.borrow_mut() = (frame.items.clone(), scale));
                            let radius = (12.0 * scale) as i32;
                            let region =
                                CreateRoundRectRgn(0, 0, size.0 + 1, size.1 + 1, radius, radius);
                            let _ = SetWindowRgn(h, Some(region), false);
                            let alpha = (frame.opacity.clamp(30, 100) as f32 * 2.55).round() as u8;
                            let _ = SetLayeredWindowAttributes(h, COLORREF(0), alpha, LWA_ALPHA);
                            let _ = SetWindowPos(
                                h,
                                Some(HWND_TOPMOST),
                                x,
                                y,
                                size.0,
                                size.1,
                                SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            );
                            let _ = InvalidateRect(Some(h), None, true);
                            shown = Some((frame, area));
                            last_place = Instant::now();
                        } else if last_place.elapsed() > Duration::from_secs(3) {
                            // Stay above a game that raised itself.
                            let _ = SetWindowPos(
                                h,
                                Some(HWND_TOPMOST),
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                            );
                            last_place = Instant::now();
                        }
                    }
                }
            }
            if let Some(h) = hwnd {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, Some(h), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if let Some(h) = hwnd {
            let _ = DestroyWindow(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AudioSource;
    fn status(state: &str, replay: bool, recording: bool) -> Status {
        Status {
            state: state.into(),
            replay,
            recording,
            ..Default::default()
        }
    }
    fn with_mic(enabled: bool, muted: bool) -> Settings {
        Settings {
            audio: vec![AudioSource {
                kind: "microphone".into(),
                enabled,
                muted,
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    #[test]
    fn states_follow_the_engine_and_never_claim_capture_early() {
        let s = with_mic(true, false);
        let v = view(&status("Stopped", false, false), &s);
        assert_eq!(
            v,
            View {
                replay: ReplayState::Off,
                recording: None,
                mic: MicState::Ready
            }
        );
        assert_eq!(
            view(&status("Starting", true, false), &s).replay,
            ReplayState::Starting
        );
        assert_eq!(
            view(&status("Starting", true, false), &s).mic,
            MicState::Ready
        );
        let on = view(&status("Buffering", true, false), &s);
        assert_eq!((on.replay, on.mic), (ReplayState::On, MicState::Live));
        let mut saving = status("Buffering", true, false);
        saving.saving = 1;
        assert_eq!(view(&saving, &s).replay, ReplayState::Saving);
        let mut err = status("Error", true, false);
        err.error = Some("x".into());
        assert_eq!(
            (view(&err, &s).replay, view(&err, &s).mic),
            (ReplayState::Error, MicState::Error)
        );
        // The recording indicator waits for video to be written.
        let mut rec = status("Recording", true, true);
        rec.pending_recording = true;
        rec.recording_elapsed = 0.0;
        assert_eq!(view(&rec, &s).recording, None);
        rec.pending_recording = false;
        rec.recording_elapsed = 83.4;
        assert_eq!(view(&rec, &s).recording, Some(83));
        assert_eq!(view(&rec, &with_mic(true, true)).mic, MicState::Muted);
        assert_eq!(view(&rec, &with_mic(false, false)).mic, MicState::Off);
        assert_eq!(
            view(
                &rec,
                &Settings {
                    audio: vec![],
                    ..Default::default()
                }
            )
            .mic,
            MicState::None
        );
    }
    #[test]
    fn items_respect_settings() {
        let v = View {
            replay: ReplayState::On,
            recording: Some(3725),
            mic: MicState::Live,
        };
        let o = OverlaySettings::default();
        let labels: Vec<String> = items(&v, &o).into_iter().map(|i| i.label).collect();
        assert_eq!(labels, ["REC 1:02:05", "Replay on", "Mic live"]);
        let compact = OverlaySettings {
            compact: true,
            ..Default::default()
        };
        let labels: Vec<String> = items(&v, &compact).into_iter().map(|i| i.label).collect();
        assert_eq!(labels, ["REC 1:02:05", "", ""]);
        let minimal = OverlaySettings {
            show_mic: false,
            show_replay: false,
            show_timer: false,
            ..Default::default()
        };
        let labels: Vec<String> = items(&v, &minimal).into_iter().map(|i| i.label).collect();
        assert_eq!(labels, ["REC"]);
        assert!(visible(&o, true) && !visible(&o, false));
        assert!(visible(
            &OverlaySettings {
                games_only: false,
                ..Default::default()
            },
            false
        ));
        assert!(!visible(
            &OverlaySettings {
                enabled: false,
                ..Default::default()
            },
            true
        ));
    }
    #[test]
    fn placement_hugs_the_chosen_corner() {
        let area = (0, 0, 1920, 1080);
        assert_eq!(place(area, (200, 30), "bottom-right", 18), (1702, 1032));
        assert_eq!(place(area, (200, 30), "top-left", 18), (18, 18));
        assert_eq!(
            place((1920, 0, 3840, 1040), (200, 30), "bottom-left", 18),
            (1938, 992)
        );
    }
}
