//! Crosshair Studio: the crosshair model, its built-in presets, geometry and
//! rasterizer. Pure (no Windows calls), so all of it is unit-tested. The
//! native window that shows the result is `crosshair_overlay`.
//!
//! Geometry is computed in device pixels for a DPI scale and is the single
//! source of truth: the overlay rasterizes it once per change, and the
//! Crosshair Studio preview draws the same shapes (mirrored in
//! `src/crosshair.ts`, checked against `tests/fixtures/crosshair-shapes.json`).

use serde::{Deserialize, Deserializer, Serialize};

/// Slider limits (logical pixels at 100% scaling). Every crosshair inside
/// them stays visible and never covers a large part of the screen.
pub const LENGTH: (f32, f32) = (2.0, 40.0);
pub const THICKNESS: (f32, f32) = (1.0, 10.0);
pub const GAP: (f32, f32) = (0.0, 30.0);
pub const DOT_SIZE: (f32, f32) = (1.0, 12.0);
pub const OUTLINE: (f32, f32) = (1.0, 4.0);
pub const OPACITY: (f32, f32) = (0.2, 1.0);
pub const OFFSET: (i32, i32) = (-1000, 1000);
/// Degrees; 45 turns a cross into an X.
pub const ROTATION: (f32, f32) = (0.0, 90.0);
pub const CIRCLE_RADIUS: (f32, f32) = (2.0, 40.0);

/// The crosshair's shape. New shapes are new variants with their parts in
/// `geometry`; the renderer and the window don't change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// Lines around a gap (which of the four is up to `arm_*`).
    #[default]
    Cross,
    /// Left, right and bottom lines (no top line).
    T,
    /// A ring.
    Circle,
    /// Only the center dot.
    Dot,
    /// A square outline around the center.
    Square,
    /// Two lines meeting above the center, like a "^".
    Chevron,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrosshairSettings {
    /// Off for new installations; the user turns it on.
    pub enabled: bool,
    /// The preset last chosen (for the gallery); the values below are what
    /// is drawn.
    pub preset: String,
    pub shape: Shape,
    /// "#RRGGBB".
    pub color: String,
    pub opacity: f32,
    pub length: f32,
    pub thickness: f32,
    pub gap: f32,
    /// Which lines a cross shows.
    pub arm_top: bool,
    pub arm_bottom: bool,
    pub arm_left: bool,
    pub arm_right: bool,
    /// Turns the whole crosshair, degrees (45 makes a cross an X).
    pub rotation: f32,
    pub dot: bool,
    pub dot_size: f32,
    /// A round dot instead of a square one.
    pub dot_round: bool,
    /// An extra ring around any shape.
    pub circle: bool,
    pub circle_radius: f32,
    pub outline: bool,
    pub outline_color: String,
    pub outline_thickness: f32,
    /// From the center of the display, logical pixels (right, down).
    pub offset_x: i32,
    pub offset_y: i32,
    /// "game": only while a recognised game is in front (default, least
    /// intrusive); "always": whenever it is on.
    pub visibility: String,
    /// Shows or hides the crosshair while it is on; "" for none.
    pub hotkey: String,
    /// Display index (0 = the primary display, then in Windows' order).
    pub display: u32,
}
impl Default for CrosshairSettings {
    fn default() -> Self {
        let mut s = Self {
            enabled: false,
            preset: String::new(),
            shape: Shape::Cross,
            color: "#00FF88".into(),
            opacity: 1.0,
            length: 7.0,
            thickness: 2.0,
            gap: 4.0,
            arm_top: true,
            arm_bottom: true,
            arm_left: true,
            arm_right: true,
            rotation: 0.0,
            dot: false,
            dot_size: 2.0,
            dot_round: false,
            circle: false,
            circle_radius: 12.0,
            outline: true,
            outline_color: "#000000".into(),
            outline_thickness: 1.0,
            offset_x: 0,
            offset_y: 0,
            visibility: "game".into(),
            hotkey: DEFAULT_HOTKEY.into(),
            display: 0,
        };
        apply_preset(&mut s, "classic");
        s
    }
}
/// Show / hide crosshair; not used by any other REPLAYX shortcut.
pub const DEFAULT_HOTKEY: &str = "Control+Shift+F6";

/// A built-in preset: only the look (never on/off, color, position,
/// visibility, shortcut or display).
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub shape: Shape,
    pub length: f32,
    pub thickness: f32,
    pub gap: f32,
    pub dot: bool,
    pub dot_size: f32,
    pub dot_round: bool,
    pub rotation: f32,
    pub circle: bool,
    pub circle_radius: f32,
}
const fn preset(
    id: &'static str,
    name: &'static str,
    shape: Shape,
    (length, thickness, gap): (f32, f32, f32),
    dot: Option<f32>,
) -> Preset {
    Preset {
        id,
        name,
        shape,
        length,
        thickness,
        gap,
        dot: dot.is_some(),
        dot_size: match dot {
            Some(d) => d,
            None => 2.0,
        },
        dot_round: false,
        rotation: 0.0,
        circle: false,
        circle_radius: 12.0,
    }
}
/// Original styles covering the common kinds of crosshair: classic crosses
/// from tiny to bold, dots, circles, an X, a box and a chevron.
pub const PRESETS: [Preset; 16] = [
    preset("classic", "Classic", Shape::Cross, (7.0, 2.0, 4.0), None),
    preset("dot", "Dot", Shape::Dot, (7.0, 2.0, 4.0), Some(4.0)),
    preset(
        "cross-dot",
        "Cross + dot",
        Shape::Cross,
        (6.0, 2.0, 5.0),
        Some(2.0),
    ),
    preset(
        "precision",
        "Precision",
        Shape::Cross,
        (4.0, 1.0, 2.0),
        None,
    ),
    preset(
        "circle",
        "Circle + dot",
        Shape::Circle,
        (9.0, 1.0, 0.0),
        Some(2.0),
    ),
    preset("t", "T", Shape::T, (7.0, 2.0, 4.0), None),
    preset("micro", "Micro", Shape::Cross, (2.0, 1.0, 1.0), None),
    preset("bold", "Bold", Shape::Cross, (8.0, 4.0, 3.0), None),
    preset("wide", "Wide gap", Shape::Cross, (6.0, 2.0, 9.0), None),
    preset("sniper", "Long lines", Shape::Cross, (28.0, 1.0, 6.0), None),
    Preset {
        rotation: 45.0,
        ..preset("x", "X", Shape::Cross, (7.0, 2.0, 4.0), None)
    },
    Preset {
        rotation: 45.0,
        ..preset("x-dot", "X + dot", Shape::Cross, (6.0, 2.0, 4.0), Some(2.0))
    },
    Preset {
        circle: true,
        circle_radius: 14.0,
        ..preset("scope", "Scope", Shape::Cross, (6.0, 1.0, 4.0), None)
    },
    Preset {
        dot_round: true,
        ..preset(
            "round-dot",
            "Round dot",
            Shape::Dot,
            (7.0, 2.0, 4.0),
            Some(6.0),
        )
    },
    preset("box", "Box", Shape::Square, (4.0, 2.0, 4.0), Some(2.0)),
    preset("chevron", "Chevron", Shape::Chevron, (8.0, 2.0, 1.0), None),
];

/// Give `s` a preset's look (unknown ids change nothing). Color, outline,
/// position and the other settings stay.
pub fn apply_preset(s: &mut CrosshairSettings, id: &str) -> bool {
    let Some(p) = PRESETS.iter().find(|p| p.id == id) else {
        return false;
    };
    s.preset = p.id.into();
    s.shape = p.shape;
    s.length = p.length;
    s.thickness = p.thickness;
    s.gap = p.gap;
    s.dot = p.dot;
    s.dot_size = p.dot_size;
    s.dot_round = p.dot_round;
    s.rotation = p.rotation;
    s.circle = p.circle;
    s.circle_radius = p.circle_radius;
    s.arm_top = true;
    s.arm_bottom = true;
    s.arm_left = true;
    s.arm_right = true;
    true
}

/// "#RRGGBB" → (r, g, b).
pub fn parse_color(c: &str) -> Option<(u8, u8, u8)> {
    let hex = c.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn clamp(v: f32, (lo, hi): (f32, f32), fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

impl CrosshairSettings {
    /// The same crosshair with every value made safe: inside the slider
    /// limits, valid colors (else the defaults), a known visibility mode,
    /// and never invisible (something is always drawn: if nothing would be,
    /// the center dot is).
    pub fn sanitized(&self) -> Self {
        let d = Self::default();
        let mut s = self.clone();
        s.opacity = clamp(s.opacity, OPACITY, d.opacity);
        s.length = clamp(s.length, LENGTH, d.length);
        s.thickness = clamp(s.thickness, THICKNESS, d.thickness);
        s.gap = clamp(s.gap, GAP, d.gap);
        s.dot_size = clamp(s.dot_size, DOT_SIZE, d.dot_size);
        s.outline_thickness = clamp(s.outline_thickness, OUTLINE, d.outline_thickness);
        s.rotation = clamp(s.rotation, ROTATION, d.rotation).round();
        s.circle_radius = clamp(s.circle_radius, CIRCLE_RADIUS, d.circle_radius);
        s.offset_x = s.offset_x.clamp(OFFSET.0, OFFSET.1);
        s.offset_y = s.offset_y.clamp(OFFSET.0, OFFSET.1);
        if parse_color(&s.color).is_none() {
            s.color = d.color.clone();
        }
        if parse_color(&s.outline_color).is_none() {
            s.outline_color = d.outline_color.clone();
        }
        s.color = s.color.to_ascii_uppercase();
        s.outline_color = s.outline_color.to_ascii_uppercase();
        if !["game", "always"].contains(&s.visibility.as_str()) {
            s.visibility = d.visibility;
        }
        let lines = match s.shape {
            Shape::Cross => s.arm_top || s.arm_bottom || s.arm_left || s.arm_right,
            Shape::T => s.arm_bottom || s.arm_left || s.arm_right,
            Shape::Dot => false,
            Shape::Circle | Shape::Square | Shape::Chevron => true,
        };
        if !lines && !s.circle {
            s.dot = true;
        }
        s
    }
    /// Shown while a game is in front only (otherwise always).
    pub fn game_only(&self) -> bool {
        self.visibility != "always"
    }
}

/// Read a stored crosshair leniently: anything that isn't a valid crosshair
/// becomes the default (switched off) instead of making the whole settings
/// file unreadable; values out of range are clamped.
pub fn lenient<'de, D: Deserializer<'de>>(d: D) -> Result<CrosshairSettings, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value::<CrosshairSettings>(value)
        .map(|c| c.sanitized())
        .unwrap_or_default())
}

/// One part of a crosshair, centered on the origin, in device pixels.
/// Straight parts are `Rect`s (exact, crisp); turned or diagonal lines are
/// `Quad`s (four corners in order, anti-aliased).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    Rect { x0: f32, y0: f32, x1: f32, y1: f32 },
    Quad { points: [[f32; 2]; 4] },
    Ring { r_inner: f32, r_outer: f32 },
    Disc { r: f32 },
}
impl Part {
    fn reach(&self) -> f32 {
        match *self {
            Part::Rect { x0, y0, x1, y1 } => x0.abs().max(x1.abs()).max(y0.abs()).max(y1.abs()),
            Part::Quad { points } => points
                .iter()
                .map(|[x, y]| x.abs().max(y.abs()))
                .fold(0.0, f32::max),
            Part::Ring { r_outer, .. } => r_outer,
            Part::Disc { r } => r,
        }
    }
}

/// A line from (x0, y0) to (x1, y1), `h` either side (square ends), or a
/// round part, before turning. Every straight part of every shape is one.
#[derive(Clone, Copy)]
enum Prim {
    Seg {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        h: f32,
    },
    Ring {
        r_inner: f32,
        r_outer: f32,
    },
    Disc {
        r: f32,
    },
}
impl Prim {
    /// The outline: longer and wider by `by` on every side.
    fn grown(self, by: f32) -> Self {
        match self {
            Prim::Seg { x0, y0, x1, y1, h } => {
                let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                let (ux, uy) = if len > 0.0 {
                    ((x1 - x0) / len, (y1 - y0) / len)
                } else {
                    (1.0, 0.0)
                };
                Prim::Seg {
                    x0: x0 - ux * by,
                    y0: y0 - uy * by,
                    x1: x1 + ux * by,
                    y1: y1 + uy * by,
                    h: h + by,
                }
            }
            Prim::Ring { r_inner, r_outer } => Prim::Ring {
                r_inner: (r_inner - by).max(0.0),
                r_outer: r_outer + by,
            },
            Prim::Disc { r } => Prim::Disc { r: r + by },
        }
    }
    /// The finished part, turned by `angle` (degrees) around the center.
    fn part(self, angle: f32) -> Part {
        // No -0.0 and no float noise: fixed to 1/1000 of a pixel.
        let fix = |v: f32| (v * 1000.0).round() / 1000.0 + 0.0;
        match self {
            Prim::Seg { x0, y0, x1, y1, h } => {
                let (s, c) = angle.to_radians().sin_cos();
                let turn = |x: f32, y: f32| (fix(x * c - y * s), fix(x * s + y * c));
                let (ax, ay) = turn(x0, y0);
                let (bx, by) = turn(x1, y1);
                if ay == by || ax == bx {
                    // Still axis-aligned: an exact rectangle.
                    // Horizontal: wide by h; vertical: by h; a point (square
                    // dot): both.
                    let (hx, hy) = match (ax == bx, ay == by) {
                        (false, true) => (0.0, h),
                        (true, false) => (h, 0.0),
                        _ => (h, h),
                    };
                    Part::Rect {
                        x0: fix(ax.min(bx) - hx),
                        y0: fix(ay.min(by) - hy),
                        x1: fix(ax.max(bx) + hx),
                        y1: fix(ay.max(by) + hy),
                    }
                } else {
                    let len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
                    let (nx, ny) = (-(by - ay) / len * h, (bx - ax) / len * h);
                    Part::Quad {
                        points: [
                            [fix(ax + nx), fix(ay + ny)],
                            [fix(bx + nx), fix(by + ny)],
                            [fix(bx - nx), fix(by - ny)],
                            [fix(ax - nx), fix(ay - ny)],
                        ],
                    }
                }
            }
            Prim::Ring { r_inner, r_outer } => Part::Ring {
                r_inner: fix(r_inner),
                r_outer: fix(r_outer),
            },
            Prim::Disc { r } => Part::Disc { r: fix(r) },
        }
    }
}

/// What to draw for a crosshair at a DPI scale.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Geometry {
    pub fill: Vec<Part>,
    /// The outline: the fill parts grown by the outline width (drawn below).
    pub outline: Vec<Part>,
    /// Bitmap side; the crosshair's center is at (size / 2, size / 2), on a
    /// pixel center when the lines are an odd number of pixels wide, so every
    /// straight edge falls on a pixel boundary (crisp, no blur).
    pub size: u32,
}

/// Round a length to whole device pixels (at least `min`).
fn px(v: f32, scale: f32, min: f32) -> f32 {
    (v * scale).round().max(min)
}

pub fn geometry(c: &CrosshairSettings, scale: f32) -> Geometry {
    let c = c.sanitized();
    let scale = if scale.is_finite() {
        scale.clamp(0.5, 4.0)
    } else {
        1.0
    };
    let t = px(c.thickness, scale, 1.0);
    let length = px(c.length, scale, 1.0);
    let gap = px(c.gap, scale, 0.0);
    let h = t / 2.0;
    // Odd widths center on a pixel: the gap starts at that pixel's edge, so
    // the lines' ends fall on pixel boundaries too.
    let odd = t as u32 % 2 == 1;
    let start = gap + if odd { 0.5 } else { 0.0 };
    let mut prims = Vec::new();
    let seg = |x0: f32, y0: f32, x1: f32, y1: f32| Prim::Seg { x0, y0, x1, y1, h };
    let (a, b) = (start, start + length);
    let arms = match c.shape {
        Shape::Cross => [c.arm_top, c.arm_bottom, c.arm_left, c.arm_right],
        Shape::T => [false, c.arm_bottom, c.arm_left, c.arm_right],
        _ => [false; 4],
    };
    let [top, bottom, left, right] = arms;
    if top {
        prims.push(seg(0.0, -b, 0.0, -a));
    }
    if bottom {
        prims.push(seg(0.0, a, 0.0, b));
    }
    if left {
        prims.push(seg(-b, 0.0, -a, 0.0));
    }
    if right {
        prims.push(seg(a, 0.0, b, 0.0));
    }
    match c.shape {
        Shape::Circle => {
            // The ring's middle is `length + gap` from the center.
            let r = (length + gap).max(t + 1.0);
            prims.push(Prim::Ring {
                r_inner: r - h,
                r_outer: r + h,
            });
        }
        Shape::Square => {
            // Lines through `gap + length` from the center.
            let s = (gap + length).max(t + 1.0);
            prims.push(seg(-s - h, -s, s + h, -s));
            prims.push(seg(-s - h, s, s + h, s));
            prims.push(seg(-s, -s + h, -s, s - h));
            prims.push(seg(s, -s + h, s, s - h));
        }
        Shape::Chevron => {
            // Two lines from just below the center, down and outwards (^).
            let d = std::f32::consts::FRAC_1_SQRT_2;
            for side in [-1.0f32, 1.0] {
                prims.push(seg(
                    side * gap * d,
                    gap * d,
                    side * (gap + length) * d,
                    (gap + length) * d,
                ));
            }
        }
        _ => {}
    }
    if c.circle {
        let r = px(c.circle_radius, scale, 2.0).max(t + 1.0);
        prims.push(Prim::Ring {
            r_inner: r - h,
            r_outer: r + h,
        });
    }
    if c.dot {
        // Same parity as the lines, so the dot is centered on the same pixel.
        let mut d = px(c.dot_size, scale, 1.0);
        if (d as u32 % 2 == 1) != odd {
            d += 1.0;
        }
        let r = d / 2.0;
        prims.push(if c.dot_round {
            Prim::Disc { r }
        } else {
            Prim::Seg {
                x0: -r,
                y0: 0.0,
                x1: r,
                y1: 0.0,
                h: r,
            }
        });
    }
    let grow = px(c.outline_thickness, scale, 1.0);
    let fill: Vec<Part> = prims.iter().map(|p| p.part(c.rotation)).collect();
    let outline: Vec<Part> = if c.outline {
        prims
            .iter()
            .map(|p| p.grown(grow).part(c.rotation))
            .collect()
    } else {
        Vec::new()
    };
    let reach = fill
        .iter()
        .chain(&outline)
        .map(Part::reach)
        .fold(0.0f32, f32::max);
    // Odd: an odd side, so (size / 2) is a pixel center.
    let size = 2 * (reach.ceil() as u32 + 1) + u32::from(odd);
    Geometry {
        fill,
        outline,
        size,
    }
}

/// How much of the pixel at (px, py) a part covers (0 to 1). Rectangles are
/// exact; turned and round parts are sampled 4 x 4.
fn coverage(part: &Part, px: f32, py: f32) -> f32 {
    match *part {
        Part::Rect { x0, y0, x1, y1 } => {
            let ox = (px + 1.0).min(x1) - px.max(x0);
            let oy = (py + 1.0).min(y1) - py.max(y0);
            ox.max(0.0) * oy.max(0.0)
        }
        Part::Quad { points } => sampled(px, py, |x, y| {
            let side = |i: usize| {
                let [ax, ay] = points[i];
                let [bx, by] = points[(i + 1) % 4];
                (bx - ax) * (y - ay) - (by - ay) * (x - ax)
            };
            let s = [side(0), side(1), side(2), side(3)];
            s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0)
        }),
        Part::Ring { r_inner, r_outer } => sampled(px, py, |x, y| {
            let d = (x * x + y * y).sqrt();
            d >= r_inner && d <= r_outer
        }),
        Part::Disc { r } => sampled(px, py, |x, y| (x * x + y * y).sqrt() <= r),
    }
}
fn sampled(px: f32, py: f32, inside: impl Fn(f32, f32) -> bool) -> f32 {
    let mut hits = 0;
    for sy in 0..4 {
        for sx in 0..4 {
            if inside(px + (sx as f32 + 0.5) / 4.0, py + (sy as f32 + 0.5) / 4.0) {
                hits += 1;
            }
        }
    }
    hits as f32 / 16.0
}

/// The crosshair as a premultiplied BGRA bitmap (the format
/// `UpdateLayeredWindow` takes): (side, pixels). Drawn once per change.
pub fn render(c: &CrosshairSettings, scale: f32) -> (u32, Vec<u8>) {
    let c = c.sanitized();
    let g = geometry(&c, scale);
    let (fr, fg, fb) = parse_color(&c.color).unwrap_or((0, 255, 136));
    let (or, og, ob) = parse_color(&c.outline_color).unwrap_or((0, 0, 0));
    let n = g.size as usize;
    let center = g.size as f32 / 2.0;
    let mut out = vec![0u8; n * n * 4];
    let cover = |parts: &[Part], px: f32, py: f32| {
        parts
            .iter()
            .map(|p| coverage(p, px, py))
            .fold(0.0f32, f32::max)
            .min(1.0)
    };
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 - center, y as f32 - center);
            let f = cover(&g.fill, px, py);
            let o = cover(&g.outline, px, py) * (1.0 - f);
            let a = (f + o) * c.opacity;
            if a <= 0.0 {
                continue;
            }
            let ch = |fill: u8, line: u8| {
                ((fill as f32 * f + line as f32 * o) * c.opacity)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            let i = (y * n + x) * 4;
            out[i] = ch(fb, ob);
            out[i + 1] = ch(fg, og);
            out[i + 2] = ch(fr, or);
            out[i + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    (g.size, out)
}

/// A display in desktop coordinates (physical pixels) and its DPI scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub scale: f32,
}

/// Top-left corner for a crosshair bitmap of side `size` so its center is at
/// the display's center plus the offset (logical pixels, scaled). The offset
/// is limited so the whole crosshair stays on that display.
pub fn place(d: &Display, size: u32, offset: (i32, i32)) -> (i32, i32) {
    let half = size as f32 / 2.0;
    let center = |lo: i32, hi: i32, off: i32| {
        let mid = lo as f32 + (hi - lo) as f32 / 2.0;
        let reach = ((hi - lo) as f32 / 2.0 - half).max(0.0);
        mid + (off as f32 * d.scale).clamp(-reach, reach)
    };
    let cx = center(d.left, d.right, offset.0);
    let cy = center(d.top, d.bottom, offset.1);
    ((cx - half).round() as i32, (cy - half).round() as i32)
}

/// When the crosshair is shown. "In-game only" shows it as soon as a game is
/// in front and hides it only after the game has been away for `HIDE_AFTER`,
/// so a quick Alt+Tab, a notification or a launcher popping up never makes
/// it flicker. The shortcut hides or shows it on top of that.
#[derive(Debug, Default)]
pub struct Visibility {
    shown: bool,
    away_since: Option<std::time::Instant>,
}
impl Visibility {
    pub const HIDE_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);
    pub fn update(
        &mut self,
        c: &CrosshairSettings,
        game_in_front: bool,
        hidden_by_shortcut: bool,
        now: std::time::Instant,
    ) -> bool {
        let wanted = if !c.enabled || hidden_by_shortcut {
            self.away_since = None;
            false
        } else if !c.game_only() || game_in_front {
            self.away_since = None;
            true
        } else if !self.shown {
            false
        } else {
            let since = *self.away_since.get_or_insert(now);
            now.duration_since(since) < Self::HIDE_AFTER
        };
        self.shown = wanted;
        wanted
    }
    pub fn shown(&self) -> bool {
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn opaque(c: &CrosshairSettings, scale: f32) -> (u32, Vec<u8>) {
        render(c, scale)
    }
    fn alpha(img: &(u32, Vec<u8>), x: u32, y: u32) -> u8 {
        img.1[((y * img.0 + x) * 4 + 3) as usize]
    }

    /// Crosshairs whose geometry the Crosshair Studio preview must draw
    /// exactly like the on-screen crosshair (`src/crosshair.ts`).
    fn fixture_cases() -> Vec<(CrosshairSettings, f32)> {
        let mut cases = Vec::new();
        for p in &PRESETS {
            let mut c = CrosshairSettings::default();
            apply_preset(&mut c, p.id);
            for scale in [1.0, 1.25, 1.5, 2.0] {
                cases.push((c.clone(), scale));
            }
            c.outline = false;
            cases.push((c.clone(), 1.0));
        }
        let custom = [
            CrosshairSettings {
                length: 40.0,
                thickness: 10.0,
                gap: 30.0,
                dot: true,
                dot_size: 12.0,
                outline_thickness: 4.0,
                ..Default::default()
            },
            CrosshairSettings {
                thickness: 3.0,
                dot: true,
                dot_size: 2.0,
                outline: false,
                ..Default::default()
            },
            CrosshairSettings {
                shape: Shape::Circle,
                length: 20.0,
                gap: 5.0,
                thickness: 2.0,
                dot: false,
                ..Default::default()
            },
        ];
        let more = [
            CrosshairSettings {
                arm_top: false,
                arm_right: false,
                rotation: 30.0,
                circle: true,
                circle_radius: 9.0,
                dot: true,
                dot_round: true,
                dot_size: 3.0,
                ..Default::default()
            },
            CrosshairSettings {
                shape: Shape::T,
                circle: true,
                rotation: 90.0,
                ..Default::default()
            },
            CrosshairSettings {
                shape: Shape::Square,
                thickness: 3.0,
                rotation: 45.0,
                ..Default::default()
            },
        ];
        for c in custom.into_iter().chain(more) {
            cases.push((c.clone(), 1.0));
            cases.push((c, 1.75));
        }
        cases
    }

    #[test]
    fn geometry_matches_the_shared_fixture_used_by_the_preview() {
        let actual: serde_json::Value = fixture_cases()
            .into_iter()
            .map(|(c, scale)| {
                serde_json::json!({ "settings": c, "scale": scale, "geometry": geometry(&c, scale) })
            })
            .collect();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/crosshair-shapes.json");
        if std::env::var_os("REPLAYX_WRITE_FIXTURES").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
        }
        let expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path).expect("tests/fixtures/crosshair-shapes.json"),
        )
        .unwrap();
        // Numbers within a millionth: JSON text doesn't round-trip every float
        // bit for bit, and the geometry is fixed to 1/1000 of a pixel.
        fn close(a: &serde_json::Value, b: &serde_json::Value) -> bool {
            use serde_json::Value::*;
            match (a, b) {
                (Number(x), Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-6,
                (Array(x), Array(y)) => {
                    x.len() == y.len() && x.iter().zip(y).all(|(a, b)| close(a, b))
                }
                (Object(x), Object(y)) => {
                    x.len() == y.len()
                        && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| close(v, w)))
                }
                _ => a == b,
            }
        }
        assert!(
            close(&actual, &expected),
            "regenerate with REPLAYX_WRITE_FIXTURES=1"
        );
    }

    #[test]
    fn new_installations_start_switched_off_with_a_safe_default() {
        let d = CrosshairSettings::default();
        assert!(!d.enabled);
        assert!(d.game_only(), "in-game only by default");
        assert_eq!(d.sanitized(), d);
        assert_eq!(d.preset, "classic");
        assert_eq!(d.hotkey, DEFAULT_HOTKEY);
    }

    #[test]
    fn every_preset_is_valid_and_visible() {
        for p in &PRESETS {
            let mut c = CrosshairSettings::default();
            assert!(apply_preset(&mut c, p.id), "{}", p.id);
            assert_eq!(c.sanitized(), c, "{} within limits", p.id);
            let img = render(&c, 1.0);
            let visible = img.1.chunks(4).filter(|px| px[3] > 0).count();
            assert!(visible > 0, "{} draws something", p.id);
            assert!(img.0 < 100, "{} stays small: {}", p.id, img.0);
        }
        let ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 16);
        for (i, id) in ids.iter().enumerate() {
            assert!(!ids[..i].contains(id), "{id} is unique");
        }
        assert!(!apply_preset(&mut CrosshairSettings::default(), "unknown"));
    }

    #[test]
    fn colors_and_out_of_range_values_are_made_safe() {
        assert_eq!(parse_color("#00ff88"), Some((0, 255, 136)));
        for bad in ["00FF88", "#0F8", "#GG0000", "", "#00FF889"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
        let wild = CrosshairSettings {
            color: "green".into(),
            outline_color: "#12345".into(),
            opacity: 0.0,
            length: 500.0,
            thickness: -3.0,
            gap: f32::NAN,
            dot_size: 99.0,
            outline_thickness: 0.0,
            offset_x: 99_999,
            offset_y: -99_999,
            visibility: "sometimes".into(),
            shape: Shape::Dot,
            dot: false,
            ..Default::default()
        }
        .sanitized();
        let d = CrosshairSettings::default();
        assert_eq!(wild.color, d.color);
        assert_eq!(wild.outline_color, d.outline_color);
        assert_eq!(wild.opacity, OPACITY.0, "never invisible");
        assert_eq!(wild.length, LENGTH.1, "never huge");
        assert_eq!(wild.thickness, THICKNESS.0);
        assert_eq!(wild.gap, d.gap);
        assert_eq!(wild.dot_size, DOT_SIZE.1);
        assert_eq!(wild.outline_thickness, OUTLINE.0);
        assert_eq!((wild.offset_x, wild.offset_y), (OFFSET.1, OFFSET.0));
        assert_eq!(wild.visibility, "game");
        assert!(wild.dot, "a dot-only crosshair keeps its dot");
        assert!(render(&wild, 1.0).1.chunks(4).any(|p| p[3] > 0));
    }

    #[test]
    fn stored_crosshairs_are_read_leniently() {
        #[derive(Deserialize)]
        struct Holder {
            #[serde(default, deserialize_with = "lenient")]
            crosshair: CrosshairSettings,
        }
        let read = |json: &str| serde_json::from_str::<Holder>(json).unwrap().crosshair;
        // Missing: the default.
        assert_eq!(read("{}"), CrosshairSettings::default());
        // Partial: the rest from the defaults.
        let partial = read(r##"{"crosshair":{"enabled":true,"color":"#ff0000"}}"##);
        assert!(partial.enabled);
        assert_eq!(partial.color, "#FF0000");
        assert_eq!(partial.length, CrosshairSettings::default().length);
        // Wrong types or an unknown shape: the default, never an error.
        assert_eq!(
            read(r#"{"crosshair":{"length":"long"}}"#),
            CrosshairSettings::default()
        );
        assert_eq!(
            read(r#"{"crosshair":{"shape":"star"}}"#),
            CrosshairSettings::default()
        );
        assert_eq!(read(r#"{"crosshair":42}"#), CrosshairSettings::default());
        // Out of range: clamped.
        assert_eq!(read(r#"{"crosshair":{"gap":900}}"#).gap, GAP.1);
        // Round trip.
        let mut c = CrosshairSettings::default();
        apply_preset(&mut c, "circle");
        c.enabled = true;
        let json = format!(r#"{{"crosshair":{}}}"#, serde_json::to_string(&c).unwrap());
        assert_eq!(read(&json), c);
    }

    #[test]
    fn classic_geometry_is_symmetric_and_crisp() {
        let c = CrosshairSettings::default(); // classic 7 long, 2 thick, gap 4
        let g = geometry(&c, 1.0);
        assert_eq!(g.fill.len(), 4);
        assert!(g.fill.contains(&Part::Rect {
            x0: 4.0,
            y0: -1.0,
            x1: 11.0,
            y1: 1.0
        }));
        assert!(g.fill.contains(&Part::Rect {
            x0: -1.0,
            y0: -11.0,
            x1: 1.0,
            y1: -4.0
        }));
        assert_eq!(g.outline.len(), 4, "outline on by default");
        assert_eq!(g.size % 2, 0, "even lines: even bitmap");
        let img = opaque(&c, 1.0);
        let n = img.0;
        // Mirror symmetric both ways, and the gap is empty.
        for y in 0..n {
            for x in 0..n {
                let a = alpha(&img, x, y);
                assert_eq!(a, alpha(&img, n - 1 - x, y), "({x},{y}) left/right");
                assert_eq!(a, alpha(&img, x, n - 1 - y), "({x},{y}) up/down");
            }
        }
        assert_eq!(alpha(&img, n / 2, n / 2), 0, "gap at the center");
        // Crisp: every pixel is fully covered or empty.
        assert!(img.1.chunks(4).all(|p| p[3] == 0 || p[3] == 255));
    }

    #[test]
    fn odd_lines_center_on_a_pixel_and_stay_crisp() {
        let mut c = CrosshairSettings::default();
        apply_preset(&mut c, "precision"); // 1 px lines
        c.outline = false;
        let g = geometry(&c, 1.0);
        assert_eq!(g.size % 2, 1);
        let img = render(&c, 1.0);
        assert!(img.1.chunks(4).all(|p| p[3] == 0 || p[3] == 255));
        // The vertical arm is exactly one pixel wide, at the center column.
        let mid = img.0 / 2;
        let top = mid - 3; // inside the top arm (gap 2, length 4)
        assert_eq!(alpha(&img, mid, top), 255);
        assert_eq!(alpha(&img, mid - 1, top), 0);
        assert_eq!(alpha(&img, mid + 1, top), 0);
    }

    #[test]
    fn shapes_have_the_right_parts() {
        let parts = |id: &str| {
            let mut c = CrosshairSettings::default();
            apply_preset(&mut c, id);
            geometry(&c, 1.0).fill
        };
        assert_eq!(parts("classic").len(), 4);
        assert_eq!(parts("cross-dot").len(), 5);
        assert_eq!(parts("dot").len(), 1);
        let t = parts("t");
        assert_eq!(t.len(), 3);
        assert!(
            !t.iter()
                .any(|p| matches!(p, Part::Rect { y1, .. } if *y1 < 0.0)),
            "no top line"
        );
        let circle = parts("circle");
        assert!(circle.iter().any(|p| matches!(p, Part::Ring { .. })));
        assert!(
            circle.iter().any(|p| matches!(p, Part::Rect { .. })),
            "center dot"
        );
        // An X: four turned lines (anti-aliased quads).
        let x = parts("x");
        assert_eq!(x.len(), 4);
        assert!(x.iter().all(|p| matches!(p, Part::Quad { .. })));
        // A box: four straight edges plus its dot; a chevron: two diagonals.
        assert_eq!(parts("box").len(), 5);
        let chevron = parts("chevron");
        assert_eq!(chevron.len(), 2);
        assert!(chevron.iter().all(|p| matches!(p, Part::Quad { .. })));
        // A scope: a cross with a ring; a round dot is a disc.
        assert!(parts("scope")
            .iter()
            .any(|p| matches!(p, Part::Ring { .. })));
        assert!(matches!(parts("round-dot")[..], [Part::Disc { .. }]));
    }

    #[test]
    fn lines_can_be_chosen_and_turned() {
        let c = CrosshairSettings {
            arm_top: false,
            arm_left: false,
            ..Default::default()
        };
        assert_eq!(geometry(&c, 1.0).fill.len(), 2, "two lines left");
        // Turning a quarter or not at all keeps exact, crisp rectangles.
        let straight = geometry(&c, 1.0);
        let quarter = geometry(
            &CrosshairSettings {
                rotation: 90.0,
                ..c.clone()
            },
            1.0,
        );
        assert!(quarter.fill.iter().all(|p| matches!(p, Part::Rect { .. })));
        assert_eq!(quarter.size, straight.size);
        // 45 degrees: quads, drawn mirror symmetric.
        let x = CrosshairSettings {
            rotation: 45.0,
            outline: false,
            ..Default::default()
        };
        let img = render(&x, 1.0);
        let n = img.0;
        for y in 0..n {
            for xx in 0..n {
                assert_eq!(alpha(&img, xx, y), alpha(&img, n - 1 - xx, y), "({xx},{y})");
            }
        }
        // All lines off and nothing else: the dot keeps it visible.
        let none = CrosshairSettings {
            arm_top: false,
            arm_bottom: false,
            arm_left: false,
            arm_right: false,
            ..Default::default()
        };
        assert!(none.sanitized().dot);
        assert!(render(&none, 1.0).1.chunks(4).any(|p| p[3] > 0));
        // Out-of-range turn and ring radius are clamped.
        let wild = CrosshairSettings {
            rotation: 400.0,
            circle_radius: -5.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (wild.rotation, wild.circle_radius),
            (ROTATION.1, CIRCLE_RADIUS.0)
        );
    }

    #[test]
    fn outline_and_opacity_change_the_pixels_as_expected() {
        let mut c = CrosshairSettings {
            outline: false,
            ..Default::default()
        };
        let plain = render(&c, 1.0);
        c.outline = true;
        let outlined = render(&c, 1.0);
        let count = |img: &(u32, Vec<u8>)| img.1.chunks(4).filter(|p| p[3] > 0).count();
        assert!(count(&outlined) > count(&plain));
        // An outline pixel is the outline color; a line pixel the fill color.
        let mid = outlined.0 / 2;
        let line = ((mid - 8) * outlined.0 + mid) as usize * 4; // top arm
        assert_eq!(&outlined.1[line..line + 4], &[0x88, 0xFF, 0x00, 255]);
        c.opacity = 0.5;
        let half = render(&c, 1.0);
        assert_eq!(half.1[line + 3], 128);
        assert_eq!(half.1[line + 1], 128, "premultiplied");
    }

    #[test]
    fn high_dpi_scales_every_part_in_whole_pixels() {
        let c = CrosshairSettings::default();
        let one = geometry(&c, 1.0);
        let two = geometry(&c, 2.0);
        let width = |g: &Geometry| match g.fill[3] {
            Part::Rect { x0, x1, .. } => x1 - x0,
            _ => 0.0,
        };
        assert_eq!(width(&two), 2.0 * width(&one));
        for g in [geometry(&c, 1.25), geometry(&c, 1.5), geometry(&c, 1.75)] {
            for p in &g.fill {
                if let Part::Rect { x0, y0, x1, y1 } = *p {
                    let center = g.size as f32 / 2.0;
                    for v in [x0, y0, x1, y1] {
                        assert_eq!((v + center).fract(), 0.0, "edge on a pixel boundary");
                    }
                }
            }
        }
    }

    #[test]
    fn placement_centers_on_the_display_and_keeps_it_on_screen() {
        let d = Display {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1440,
            scale: 1.0,
        };
        assert_eq!(place(&d, 20, (0, 0)), (1270, 710));
        assert_eq!(place(&d, 21, (0, 0)), (1270, 710), "odd: center pixel 1280");
        assert_eq!(place(&d, 20, (100, -50)), (1370, 660));
        // An offset never pushes it off the display.
        assert_eq!(place(&d, 20, (5000, 5000)), (2540, 1420));
        assert_eq!(place(&d, 20, (-5000, -5000)), (0, 0));
        // A second display to the left, at 150% scaling.
        let left = Display {
            left: -1920,
            top: -200,
            right: 0,
            bottom: 880,
            scale: 1.5,
        };
        assert_eq!(place(&left, 20, (0, 0)), (-970, 330));
        assert_eq!(place(&left, 20, (10, 10)), (-955, 345), "offset scaled");
    }

    #[test]
    fn in_game_visibility_never_flickers_on_a_quick_alt_tab() {
        let on = CrosshairSettings {
            enabled: true,
            ..Default::default()
        };
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut v = Visibility::default();
        assert!(!v.update(&on, false, false, at(0)), "desktop: hidden");
        assert!(v.update(&on, true, false, at(1000)), "game in front: shown");
        // Alt+Tab away for under 1.5 s and back: stays shown throughout.
        assert!(v.update(&on, false, false, at(2000)));
        assert!(v.update(&on, false, false, at(3000)));
        assert!(v.update(&on, true, false, at(3200)));
        // Away longer: hidden, then shown again with the game.
        assert!(v.update(&on, false, false, at(4000)));
        assert!(!v.update(&on, false, false, at(5600)));
        assert!(!v.update(&on, false, false, at(9000)));
        assert!(v.update(&on, true, false, at(9500)));
        // The shortcut hides it at once and shows it again.
        assert!(!v.update(&on, true, true, at(9600)));
        assert!(v.update(&on, true, false, at(9700)));
        // Always on: also on the desktop.
        let always = CrosshairSettings {
            visibility: "always".into(),
            ..on.clone()
        };
        assert!(v.update(&always, false, false, at(9800)));
        // Turned off: hidden at once, whatever is in front.
        assert!(!v.update(&CrosshairSettings::default(), true, false, at(9900)));
        assert!(!v.shown());
    }
}
