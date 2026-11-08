//! Status overlay: a small stack of icon tiles (Instant Replay, microphone,
//! recording) in a corner of the screen, in the style of GeForce's status
//! indicator. Shown over every app, game and the desktop while enabled.
//!
//! A normal top-most desktop window on its own thread: no injection into
//! games, no hooks, click-through, never focused. The icons are drawn here
//! (signed-distance shapes, anti-aliased at any DPI) into a per-pixel-alpha
//! layered window, repainted only when what it shows changes.
//!
//! It is excluded from capture (`WDA_EXCLUDEFROMCAPTURE`), so it is visible on
//! screen while recording but never appears in clips; `replayx-cli
//! overlay-test` checks that on real Desktop Duplication frames. Like any
//! desktop window it shows over windowed and borderless games, not over
//! exclusive full-screen ones.
use crate::{engine::Status, model::Settings};
use serde::{Deserialize, Serialize};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Overlay preferences (part of `Settings`). Older files' extra keys
/// (compact, show_timer, games_only) are ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    pub enabled: bool,
    /// "bottom-right", "bottom-left", "top-right" or "top-left".
    pub position: String,
    /// Opacity in percent (30-100).
    pub opacity: u8,
    pub show_replay: bool,
    pub show_mic: bool,
}
impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "bottom-right".into(),
            opacity: 90,
            show_replay: true,
            show_mic: true,
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
/// Whether the overlay should be on screen: whenever it is enabled.
pub fn visible(o: &OverlaySettings) -> bool {
    o.enabled
}

pub type Rgb = (u8, u8, u8);
const VIOLET: Rgb = (160, 146, 255);
const RED: Rgb = (255, 77, 94);
const AMBER: Rgb = (245, 186, 84);
const GREEN: Rgb = (62, 214, 160);
const GREY: Rgb = (128, 128, 146);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Replay,
    Mic,
    Record,
}
/// One icon tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub icon: Icon,
    pub color: Rgb,
    /// Struck through (microphone off or muted).
    pub slash: bool,
}
/// Tiles for a view, from the corner outwards: Instant Replay, microphone,
/// then the recording dot (only while video is being written).
pub fn tiles(v: &View, o: &OverlaySettings) -> Vec<Tile> {
    let mut out = Vec::new();
    if o.show_replay {
        let color = match v.replay {
            ReplayState::Off => GREY,
            ReplayState::Starting | ReplayState::Saving => AMBER,
            ReplayState::On => VIOLET,
            ReplayState::Error => RED,
        };
        out.push(Tile {
            icon: Icon::Replay,
            color,
            slash: v.replay == ReplayState::Off,
        });
    }
    if o.show_mic {
        let state = match v.mic {
            MicState::None => None,
            MicState::Off => Some((GREY, true)),
            MicState::Muted => Some((AMBER, true)),
            MicState::Ready => Some((VIOLET, false)),
            MicState::Live => Some((GREEN, false)),
            MicState::Error => Some((RED, true)),
        };
        if let Some((color, slash)) = state {
            out.push(Tile {
                icon: Icon::Mic,
                color,
                slash,
            });
        }
    }
    if v.recording.is_some() {
        out.push(Tile {
            icon: Icon::Record,
            color: RED,
            slash: false,
        });
    }
    out
}

// ---- Drawing (tile units: a tile is 32 x 32 at 96 dpi) ----

const TILE: f32 = 32.0;
const GAP: f32 = 6.0;
const MARGIN: f32 = 16.0;

fn seg(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
}
fn rounded_box(px: f32, py: f32, half: f32, r: f32) -> f32 {
    let qx = (px - TILE / 2.0).abs() - (half - r);
    let qy = (py - TILE / 2.0).abs() - (half - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}
/// Signed distance to a triangle (negative inside).
fn triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let d = seg(p.0, p.1, a.0, a.1, b.0, b.1)
        .min(seg(p.0, p.1, b.0, b.1, c.0, c.1))
        .min(seg(p.0, p.1, c.0, c.1, a.0, a.1));
    let side = |u: (f32, f32), v: (f32, f32)| (v.0 - u.0) * (p.1 - u.1) - (v.1 - u.1) * (p.0 - u.0);
    let (s1, s2, s3) = (side(a, b), side(b, c), side(c, a));
    let inside = (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0);
    if inside {
        -d
    } else {
        d
    }
}
/// Signed distance to the icon's ink at a point (tile units).
fn icon_distance(icon: Icon, x: f32, y: f32) -> f32 {
    let (cx, cy) = (16.0, 16.0);
    match icon {
        Icon::Record => ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - 6.0,
        Icon::Mic => {
            let capsule = seg(x, y, 16.0, 10.2, 16.0, 15.2) - 3.3;
            let r = ((x - 16.0).powi(2) + (y - 15.2).powi(2)).sqrt();
            let holder = if y >= 15.2 {
                (r - 6.1).abs() - 0.95
            } else {
                f32::MAX
            };
            let stem = seg(x, y, 16.0, 21.3, 16.0, 24.2) - 0.95;
            let base = seg(x, y, 12.6, 24.2, 19.4, 24.2) - 0.95;
            capsule.min(holder).min(stem).min(base)
        }
        Icon::Replay => {
            // Circular arrow (counter-clockwise) with a play triangle inside.
            let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let angle = (y - cy).atan2(x - cx).to_degrees();
            let (gap_from, gap_to) = (-100.0, -42.0);
            let ring = if angle > gap_from && angle < gap_to {
                f32::MAX
            } else {
                (r - 8.6).abs() - 1.1
            };
            let e = gap_to.to_radians();
            let (ex, ey) = (cx + 8.6 * e.cos(), cy + 8.6 * e.sin());
            // Tangent pointing into the gap (counter-clockwise on screen).
            let (tx, ty) = (e.sin(), -e.cos());
            let (nx, ny) = (e.cos(), e.sin());
            let tip = (ex + tx * 3.4, ey + ty * 3.4);
            let b1 = (ex - tx * 0.6 + nx * 3.0, ey - ty * 0.6 + ny * 3.0);
            let b2 = (ex - tx * 0.6 - nx * 3.0, ey - ty * 0.6 - ny * 3.0);
            let head = triangle((x, y), tip, b1, b2);
            let play = triangle((x, y), (14.0, 12.6), (14.0, 19.4), (19.6, 16.0)) - 0.2;
            ring.min(head).min(play)
        }
    }
}
fn coverage(distance: f32, scale: f32) -> f32 {
    (0.5 - distance * scale).clamp(0.0, 1.0)
}
/// Render the stack as top-down BGRA, premultiplied alpha (for
/// `UpdateLayeredWindow`). `upward`: the first tile is at the bottom.
pub fn render(tiles: &[Tile], scale: f32, upward: bool) -> (i32, i32, Vec<u8>) {
    let tile_px = (TILE * scale).round() as i32;
    let gap_px = (GAP * scale).round() as i32;
    let n = tiles.len() as i32;
    let (w, h) = (tile_px, (n * tile_px + (n - 1).max(0) * gap_px).max(1));
    let mut px = vec![0u8; (w * h * 4) as usize];
    for (i, tile) in tiles.iter().enumerate() {
        let slot = if upward { n - 1 - i as i32 } else { i as i32 };
        let top = slot * (tile_px + gap_px);
        for yy in 0..tile_px {
            for xx in 0..tile_px {
                // Sample at the pixel centre, in tile units.
                let (x, y) = ((xx as f32 + 0.5) / scale, (yy as f32 + 0.5) / scale);
                let body = rounded_box(x, y, 15.5, 8.0);
                let fill = coverage(body, scale);
                if fill <= 0.0 {
                    continue;
                }
                // Background, a soft border, then the icon.
                let mut c = (14.0f32, 14.0f32, 21.0f32);
                let mut a = 0.82 * fill;
                let border = coverage(body.abs() - 0.5, scale) * 0.6;
                c = mix(c, (88.0, 78.0, 150.0), border);
                a = a.max(border * fill);
                let mut ink = coverage(icon_distance(tile.icon, x, y), scale);
                if tile.icon == Icon::Record {
                    // Soft glow ring around the dot.
                    let r = ((x - 16.0).powi(2) + (y - 16.0).powi(2)).sqrt();
                    ink = ink.max(coverage((r - 9.0).abs() - 0.6, scale) * 0.45);
                }
                if tile.slash {
                    let d = seg(x, y, 9.5, 9.5, 22.5, 22.5);
                    ink *= 1.0 - coverage(d - 2.3, scale);
                    ink = ink.max(coverage(d - 1.05, scale));
                }
                let color = (
                    tile.color.0 as f32,
                    tile.color.1 as f32,
                    tile.color.2 as f32,
                );
                c = mix(c, color, ink);
                a = a.max(ink * fill);
                let o = (((top + yy) * w + xx) * 4) as usize;
                px[o] = (c.2 * a).round() as u8;
                px[o + 1] = (c.1 * a).round() as u8;
                px[o + 2] = (c.0 * a).round() as u8;
                px[o + 3] = (255.0 * a).round() as u8;
            }
        }
    }
    (w, h, px)
}
fn mix(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> (f32, f32, f32) {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

/// What the overlay thread draws; `None` hides it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub tiles: Vec<Tile>,
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

unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    match msg {
        // Click-through: the mouse goes to whatever is underneath.
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
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

/// Copy premultiplied pixels into the layered window at `pos`.
unsafe fn present(
    hwnd: windows::Win32::Foundation::HWND,
    pos: (i32, i32),
    (w, h, pixels): &(i32, i32, Vec<u8>),
    opacity: u8,
) {
    use windows::Win32::{
        Foundation::{COLORREF, POINT, SIZE},
        Graphics::Gdi::*,
        UI::WindowsAndMessaging::*,
    };
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: *w,
            biHeight: -*h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(bitmap) = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
        if !bits.is_null() {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
        }
        let old = SelectObject(mem, bitmap.into());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (opacity.clamp(30, 100) as f32 * 2.55).round() as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            hwnd,
            Some(screen),
            Some(&POINT { x: pos.0, y: pos.1 }),
            Some(&SIZE { cx: *w, cy: *h }),
            Some(mem),
            Some(&POINT { x: 0, y: 0 }),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(bitmap.into());
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
}

/// What is on screen: the frame, the monitor area and the DPI it was drawn for.
type Shown = (Frame, (i32, i32, i32, i32), u32);
fn run(rx: mpsc::Receiver<Option<Frame>>) {
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
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
        let mut shown: Option<Shown> = None;
        let mut last_raise = Instant::now();
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
                        // On screen but never in clips: Desktop Duplication and
                        // Windows Graphics Capture skip this window.
                        let excluded = SetWindowDisplayAffinity(
                            h,
                            if frame.capturable {
                                WDA_NONE
                            } else {
                                WDA_EXCLUDEFROMCAPTURE
                            },
                        );
                        // Windows before 10 2004 cannot exclude it: then it is
                        // never shown, rather than ending up in clips.
                        if (excluded.is_err() && !frame.capturable) || frame.tiles.is_empty() {
                            let _ = ShowWindow(h, SW_HIDE);
                            shown = None;
                            continue;
                        }
                        let area = anchor();
                        let area = (area.left, area.top, area.right, area.bottom);
                        let dpi = GetDpiForWindow(h).max(96);
                        let unchanged = shown
                            .as_ref()
                            .is_some_and(|(f, a, d)| *f == frame && *a == area && *d == dpi);
                        if !unchanged {
                            let scale = dpi as f32 / 96.0;
                            let image =
                                render(&frame.tiles, scale, frame.position.starts_with("bottom"));
                            let pos = place(
                                area,
                                (image.0, image.1),
                                &frame.position,
                                (MARGIN * scale).round() as i32,
                            );
                            present(h, pos, &image, frame.opacity);
                            let _ = SetWindowPos(
                                h,
                                Some(HWND_TOPMOST),
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            );
                            shown = Some((frame, area, dpi));
                            last_raise = Instant::now();
                        } else if last_raise.elapsed() > Duration::from_secs(3) {
                            // Stay above a window that raised itself.
                            let _ = SetWindowPos(
                                h,
                                Some(HWND_TOPMOST),
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                            );
                            last_raise = Instant::now();
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
    fn tiles_stack_replay_mic_then_recording() {
        let o = OverlaySettings::default();
        let v = View {
            replay: ReplayState::On,
            recording: Some(12),
            mic: MicState::Live,
        };
        let t = tiles(&v, &o);
        assert_eq!(
            t.iter().map(|t| t.icon).collect::<Vec<_>>(),
            [Icon::Replay, Icon::Mic, Icon::Record]
        );
        assert_eq!((t[0].color, t[1].color, t[2].color), (VIOLET, GREEN, RED));
        let idle = View {
            replay: ReplayState::Off,
            recording: None,
            mic: MicState::Muted,
        };
        let t = tiles(&idle, &o);
        assert_eq!(t.len(), 2, "no recording tile while not recording");
        assert!(t[0].slash && t[1].slash);
        let none = tiles(
            &idle,
            &OverlaySettings {
                show_replay: false,
                show_mic: false,
                ..Default::default()
            },
        );
        assert!(none.is_empty());
        assert!(
            visible(&o)
                && !visible(&OverlaySettings {
                    enabled: false,
                    ..Default::default()
                })
        );
    }
    #[test]
    fn icons_render_crisp_tiles_with_a_gap() {
        let t = [
            Tile {
                icon: Icon::Replay,
                color: VIOLET,
                slash: false,
            },
            Tile {
                icon: Icon::Mic,
                color: GREEN,
                slash: true,
            },
        ];
        let (w, h, px) = render(&t, 1.0, true);
        assert_eq!((w, h), (32, 70));
        let alpha = |x: i32, y: i32| px[((y * w + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "rounded corner is transparent");
        assert_eq!(alpha(16, 34), 0, "gap between tiles is transparent");
        assert!(alpha(16, 4) > 180, "tile body is mostly opaque");
        // Icon ink is brighter than the background in each tile.
        let bright = |y0: i32| {
            (y0..y0 + 32)
                .flat_map(|y| (0..32).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let o = ((y * w + x) * 4) as usize;
                    px[o].max(px[o + 1]).max(px[o + 2]) as u32
                })
                .max()
                .unwrap()
        };
        assert!(bright(0) > 180 && bright(38) > 180);
        // Premultiplied: colour never exceeds alpha.
        assert!(px
            .chunks(4)
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
        // High DPI scales the tile.
        let (w2, _, _) = render(&t, 1.5, true);
        assert_eq!(w2, 48);
    }
    #[test]
    fn placement_hugs_the_chosen_corner() {
        let area = (0, 0, 1920, 1080);
        assert_eq!(place(area, (32, 70), "bottom-right", 16), (1872, 994));
        assert_eq!(place(area, (32, 70), "top-left", 16), (16, 16));
        assert_eq!(
            place((1920, 0, 3840, 1040), (32, 32), "bottom-left", 16),
            (1936, 992)
        );
    }
}
