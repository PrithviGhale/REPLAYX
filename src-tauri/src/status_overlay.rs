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
/// (compact, games_only) are ignored.
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
    /// Red dot while a recording is being written.
    pub show_recording: bool,
    /// Recording time next to the dot.
    pub show_timer: bool,
}
impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "bottom-right".into(),
            opacity: 90,
            show_replay: true,
            show_mic: true,
            show_recording: true,
            show_timer: false,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub icon: Icon,
    pub color: Rgb,
    /// Struck through (microphone off or muted).
    pub slash: bool,
    /// Text after the icon (the recording timer), digits and ':' only.
    pub text: Option<String>,
}
/// "1:05" / "1:02:05".
pub fn clock(s: u64) -> String {
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
/// Tiles for a view, from the corner outwards: Instant Replay, microphone,
/// then the recording dot (only while video is being written).
pub fn tiles(v: &View, o: &OverlaySettings) -> Vec<Tile> {
    let mut out = Vec::new();
    let tile = |icon, color, slash| Tile {
        icon,
        color,
        slash,
        text: None,
    };
    if o.show_replay {
        let color = match v.replay {
            ReplayState::Off => GREY,
            ReplayState::Starting | ReplayState::Saving => AMBER,
            ReplayState::On => VIOLET,
            ReplayState::Error => RED,
        };
        out.push(tile(Icon::Replay, color, v.replay == ReplayState::Off));
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
            out.push(tile(Icon::Mic, color, slash));
        }
    }
    if let (true, Some(s)) = (o.show_recording, v.recording) {
        out.push(Tile {
            text: o.show_timer.then(|| clock(s)),
            ..tile(Icon::Record, RED, false)
        });
    }
    out
}

// ---- Drawing ----
// Tiles are TILE x TILE display units at 96 dpi (26: about 19% smaller than
// the first 32-unit design). Icon shapes are designed on a 32-unit grid.

const TILE: f32 = 26.0;
const ICON_GRID: f32 = 32.0;
const GAP: f32 = 5.0;
const MARGIN: f32 = 14.0;
const RADIUS: f32 = 7.0;
/// Seven-segment glyph cell (units) and stroke half-width.
const GLYPH_H: f32 = 10.0;
const DIGIT_W: f32 = 6.0;
const COLON_W: f32 = 2.6;
const STROKE: f32 = 0.75;

fn seg(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
}
/// Signed distance to a rounded rectangle [0, w] x [0, h].
fn rounded_rect(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (px - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (py - h / 2.0).abs() - (h / 2.0 - r);
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
/// Signed distance to the icon's ink (32-unit icon grid).
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
            let e = f32::to_radians(gap_to);
            let (ex, ey) = (cx + 8.6 * e.cos(), cy + 8.6 * e.sin());
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
/// Distance to one seven-segment character in its own cell (origin top-left).
fn glyph_distance(ch: char, x: f32, y: f32) -> f32 {
    if ch == ':' {
        let c = COLON_W / 2.0;
        let d1 = ((x - c).powi(2) + (y - 3.2).powi(2)).sqrt();
        let d2 = ((x - c).powi(2) + (y - 6.8).powi(2)).sqrt();
        return d1.min(d2) - 0.85;
    }
    // Segments a b c d e f g as bit flags.
    let mask: u8 = match ch {
        '0' => 0b0111111,
        '1' => 0b0000110,
        '2' => 0b1011011,
        '3' => 0b1001111,
        '4' => 0b1100110,
        '5' => 0b1101101,
        '6' => 0b1111101,
        '7' => 0b0000111,
        '8' => 0b1111111,
        '9' => 0b1101111,
        _ => 0,
    };
    let (l, r, t, m, b) = (0.9, DIGIT_W - 0.9, 0.8, GLYPH_H / 2.0, GLYPH_H - 0.8);
    let segs = [
        (l, t, r, t), // a
        (r, t, r, m), // b
        (r, m, r, b), // c
        (l, b, r, b), // d
        (l, m, l, b), // e
        (l, t, l, m), // f
        (l, m, r, m), // g
    ];
    let mut d = f32::MAX;
    for (i, s) in segs.iter().enumerate() {
        if mask & (1 << i) != 0 {
            d = d.min(seg(x, y, s.0, s.1, s.2, s.3) - STROKE);
        }
    }
    d
}
fn text_width(text: &str) -> f32 {
    text.chars()
        .map(|c| if c == ':' { COLON_W } else { DIGIT_W + 1.2 })
        .sum::<f32>()
}
/// Width of a tile in display units.
fn tile_width(t: &Tile) -> f32 {
    match &t.text {
        Some(s) if !s.is_empty() => TILE - 3.0 + text_width(s) + 7.0,
        _ => TILE,
    }
}
fn coverage(distance: f32, scale: f32) -> f32 {
    (0.5 - distance * scale).clamp(0.0, 1.0)
}
/// Render the stack as top-down BGRA, premultiplied alpha (for
/// `UpdateLayeredWindow`). `upward`: the first tile is at the bottom;
/// `right`: tiles of different widths line up on the right edge.
pub fn render(tiles: &[Tile], scale: f32, upward: bool, right: bool) -> (i32, i32, Vec<u8>) {
    let tile_px = (TILE * scale).round() as i32;
    let gap_px = (GAP * scale).round() as i32;
    let n = tiles.len() as i32;
    let widths: Vec<i32> = tiles
        .iter()
        .map(|t| (tile_width(t) * scale).round() as i32)
        .collect();
    let w = widths.iter().copied().max().unwrap_or(tile_px).max(1);
    let h = (n * tile_px + (n - 1).max(0) * gap_px).max(1);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let k = ICON_GRID / TILE;
    for (i, tile) in tiles.iter().enumerate() {
        let slot = if upward { n - 1 - i as i32 } else { i as i32 };
        let top = slot * (tile_px + gap_px);
        let tw = widths[i];
        let left = if right { w - tw } else { 0 };
        let tw_units = tw as f32 / scale;
        let color = (
            tile.color.0 as f32,
            tile.color.1 as f32,
            tile.color.2 as f32,
        );
        let text_left = TILE - 3.0;
        let glyph_top = (TILE - GLYPH_H) / 2.0;
        for yy in 0..tile_px {
            for xx in 0..tw {
                // Pixel centre in display units within this tile.
                let (x, y) = ((xx as f32 + 0.5) / scale, (yy as f32 + 0.5) / scale);
                let body = rounded_rect(x, y, tw_units - 0.2, TILE - 0.2, RADIUS);
                let fill = coverage(body, scale);
                if fill <= 0.0 {
                    continue;
                }
                // Translucent background, a soft violet edge, then the ink.
                let mut c = (14.0f32, 14.0f32, 21.0f32);
                let mut a = 0.74 * fill;
                let border = coverage(body.abs() - 0.45, scale) * 0.55;
                c = mix(c, (88.0, 78.0, 150.0), border);
                a = a.max(border * fill);
                let (u, v) = (x * k, y * k);
                let mut ink = if x < TILE {
                    coverage(icon_distance(tile.icon, u, v) / k, scale)
                } else {
                    0.0
                };
                if tile.icon == Icon::Record && x < TILE {
                    let r = ((u - 16.0).powi(2) + (v - 16.0).powi(2)).sqrt();
                    ink = ink.max(coverage(((r - 9.0).abs() - 0.6) / k, scale) * 0.45);
                }
                if tile.slash {
                    let d = seg(u, v, 9.5, 9.5, 22.5, 22.5) / k;
                    ink *= 1.0 - coverage(d - 1.9, scale);
                    ink = ink.max(coverage(d - 0.85, scale));
                }
                c = mix(c, color, ink);
                a = a.max(ink * fill);
                // Timer digits (light text).
                if let Some(text) = &tile.text {
                    let mut gx = text_left;
                    let mut d = f32::MAX;
                    for ch in text.chars() {
                        let cw = if ch == ':' { COLON_W } else { DIGIT_W + 1.2 };
                        if x >= gx - 1.0 && x <= gx + cw + 1.0 {
                            d = d.min(glyph_distance(ch, x - gx, y - glyph_top));
                        }
                        gx += cw;
                    }
                    let t = coverage(d, scale);
                    c = mix(c, (238.0, 238.0, 244.0), t);
                    a = a.max(t * fill);
                }
                let o = (((top + yy) * w + left + xx) * 4) as usize;
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
                            let image = render(
                                &frame.tiles,
                                scale,
                                frame.position.starts_with("bottom"),
                                frame.position.ends_with("right"),
                            );
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
    fn tiles_stack_replay_mic_then_recording_with_optional_timer() {
        let o = OverlaySettings::default();
        let v = View {
            replay: ReplayState::On,
            recording: Some(65),
            mic: MicState::Live,
        };
        let t = tiles(&v, &o);
        assert_eq!(
            t.iter().map(|t| t.icon).collect::<Vec<_>>(),
            [Icon::Replay, Icon::Mic, Icon::Record]
        );
        assert_eq!((t[0].color, t[1].color, t[2].color), (VIOLET, GREEN, RED));
        assert_eq!(t[2].text, None, "timer off by default");
        let timed = tiles(
            &v,
            &OverlaySettings {
                show_timer: true,
                ..Default::default()
            },
        );
        assert_eq!(timed[2].text.as_deref(), Some("1:05"));
        let no_rec = tiles(
            &v,
            &OverlaySettings {
                show_recording: false,
                ..Default::default()
            },
        );
        assert_eq!(no_rec.len(), 2);
        let idle = View {
            replay: ReplayState::Off,
            recording: None,
            mic: MicState::Muted,
        };
        let t = tiles(&idle, &o);
        assert_eq!(t.len(), 2, "no recording tile while not recording");
        assert!(t[0].slash && t[1].slash);
        assert!(tiles(
            &idle,
            &OverlaySettings {
                show_replay: false,
                show_mic: false,
                ..Default::default()
            }
        )
        .is_empty());
        assert!(
            visible(&o)
                && !visible(&OverlaySettings {
                    enabled: false,
                    ..Default::default()
                })
        );
        assert_eq!(clock(3725), "1:02:05");
    }
    #[test]
    fn icons_render_smaller_crisp_tiles_with_a_gap() {
        let tile = |icon, color, slash| Tile {
            icon,
            color,
            slash,
            text: None,
        };
        let t = [
            tile(Icon::Replay, VIOLET, false),
            tile(Icon::Mic, GREEN, true),
        ];
        let (w, h, px) = render(&t, 1.0, true, true);
        assert_eq!((w, h), (26, 57), "about 19% smaller than the 32 px design");
        let alpha = |x: i32, y: i32| px[((y * w + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "rounded corner is transparent");
        assert_eq!(alpha(13, 28), 0, "gap between tiles is transparent");
        assert!(alpha(13, 3) > 150, "tile body is visible");
        let bright = |y0: i32| {
            (y0..y0 + 26)
                .flat_map(|y| (0..26).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let o = ((y * w + x) * 4) as usize;
                    px[o].max(px[o + 1]).max(px[o + 2]) as u32
                })
                .max()
                .unwrap()
        };
        assert!(bright(0) > 170 && bright(31) > 170);
        assert!(
            px.chunks(4)
                .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]),
            "premultiplied"
        );
        // The timer widens only its own tile, aligned to the corner.
        let timed = [
            tile(Icon::Mic, GREEN, false),
            Tile {
                text: Some("12:34".into()),
                ..tile(Icon::Record, RED, false)
            },
        ];
        let (w2, _, px2) = render(&timed, 1.0, true, true);
        assert!(w2 > 50, "{w2}");
        // Bottom tile (mic) is right-aligned: its left part is empty.
        let a2 = |x: i32, y: i32| px2[((y * w2 + x) * 4 + 3) as usize];
        assert_eq!(a2(2, 44), 0);
        assert!(a2(w2 - 13, 44) > 150);
        assert_eq!(render(&t, 1.5, true, true).0, 39, "scales with DPI");
    }
    #[test]
    fn placement_hugs_the_chosen_corner() {
        let area = (0, 0, 1920, 1080);
        assert_eq!(place(area, (26, 57), "bottom-right", 14), (1880, 1009));
        assert_eq!(place(area, (26, 57), "top-left", 14), (14, 14));
        assert_eq!(
            place((1920, 0, 3840, 1040), (32, 32), "bottom-left", 16),
            (1936, 992)
        );
    }
}
