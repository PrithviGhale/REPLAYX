/** Crosshair Studio model, presets, limits and geometry: a mirror of
 * `src-tauri/src/crosshair.rs`. The preview draws exactly what the
 * on-screen crosshair draws; both are checked against
 * tests/fixtures/crosshair-shapes.json. Pure (no React, no Tauri). */

export type Shape = 'cross' | 't' | 'circle' | 'dot' | 'square' | 'chevron';
export type CrosshairSettings = {
  enabled: boolean;
  preset: string;
  shape: Shape;
  color: string;
  opacity: number;
  length: number;
  thickness: number;
  gap: number;
  arm_top: boolean;
  arm_bottom: boolean;
  arm_left: boolean;
  arm_right: boolean;
  rotation: number;
  dot: boolean;
  dot_size: number;
  dot_round: boolean;
  circle: boolean;
  circle_radius: number;
  outline: boolean;
  outline_color: string;
  outline_thickness: number;
  offset_x: number;
  offset_y: number;
  visibility: 'game' | 'always';
  hotkey: string;
  display: number;
};

/** Slider limits (logical pixels at 100% scaling), as in the engine. */
export const LIMITS = {
  length: [2, 40], thickness: [1, 10], gap: [0, 30], dot_size: [1, 12],
  outline_thickness: [1, 4], opacity: [0.2, 1], offset: [-1000, 1000],
  rotation: [0, 90], circle_radius: [2, 40],
} as const;

export const DEFAULT_HOTKEY = 'Control+Shift+F6';

export type Preset = {
  id: string; name: string; shape: Shape; length: number; thickness: number; gap: number;
  dot: boolean; dot_size: number; dot_round: boolean; rotation: number; circle: boolean; circle_radius: number;
};
const preset = (id: string, name: string, shape: Shape, [length, thickness, gap]: [number, number, number], dot: number | null, extra: Partial<Preset> = {}): Preset => ({
  id, name, shape, length, thickness, gap, dot: dot !== null, dot_size: dot ?? 2, dot_round: false, rotation: 0, circle: false, circle_radius: 12, ...extra,
});
/** Original styles covering the common kinds of crosshair. */
export const PRESETS: Preset[] = [
  preset('classic', 'Classic', 'cross', [7, 2, 4], null),
  preset('dot', 'Dot', 'dot', [7, 2, 4], 4),
  preset('cross-dot', 'Cross + dot', 'cross', [6, 2, 5], 2),
  preset('precision', 'Precision', 'cross', [4, 1, 2], null),
  preset('circle', 'Circle + dot', 'circle', [9, 1, 0], 2),
  preset('t', 'T', 't', [7, 2, 4], null),
  preset('micro', 'Micro', 'cross', [2, 1, 1], null),
  preset('bold', 'Bold', 'cross', [8, 4, 3], null),
  preset('wide', 'Wide gap', 'cross', [6, 2, 9], null),
  preset('sniper', 'Long lines', 'cross', [28, 1, 6], null),
  preset('x', 'X', 'cross', [7, 2, 4], null, { rotation: 45 }),
  preset('x-dot', 'X + dot', 'cross', [6, 2, 4], 2, { rotation: 45 }),
  preset('scope', 'Scope', 'cross', [6, 1, 4], null, { circle: true, circle_radius: 14 }),
  preset('round-dot', 'Round dot', 'dot', [7, 2, 4], 6, { dot_round: true }),
  preset('box', 'Box', 'square', [4, 2, 4], 2),
  preset('chevron', 'Chevron', 'chevron', [8, 2, 1], null),
];

/** A preset's look on top of `c` (color, outline, position, visibility,
 * shortcut and display stay). */
export function withPreset(c: CrosshairSettings, id: string): CrosshairSettings {
  const p = PRESETS.find(x => x.id === id);
  if (!p) return c;
  return {
    ...c, preset: p.id, shape: p.shape, length: p.length, thickness: p.thickness, gap: p.gap, dot: p.dot, dot_size: p.dot_size,
    dot_round: p.dot_round, rotation: p.rotation, circle: p.circle, circle_radius: p.circle_radius,
    arm_top: true, arm_bottom: true, arm_left: true, arm_right: true,
  };
}

/** The preset whose look `c` has exactly, if any. */
export function matchingPreset(c: CrosshairSettings): string | null {
  return PRESETS.find(p => p.shape === c.shape && p.length === c.length && p.thickness === c.thickness && p.gap === c.gap
    && p.dot === c.dot && p.dot_size === c.dot_size && p.dot_round === c.dot_round && p.rotation === c.rotation
    && p.circle === c.circle && p.circle_radius === c.circle_radius
    && c.arm_top && c.arm_bottom && c.arm_left && c.arm_right)?.id ?? null;
}

export const defaultCrosshair = (): CrosshairSettings => withPreset({
  enabled: false, preset: '', shape: 'cross', color: '#00FF88', opacity: 1, length: 7, thickness: 2, gap: 4,
  arm_top: true, arm_bottom: true, arm_left: true, arm_right: true, rotation: 0,
  dot: false, dot_size: 2, dot_round: false, circle: false, circle_radius: 12,
  outline: true, outline_color: '#000000', outline_thickness: 1,
  offset_x: 0, offset_y: 0, visibility: 'game', hotkey: DEFAULT_HOTKEY, display: 0,
}, 'classic');

export const validColor = (c: string) => /^#[0-9a-fA-F]{6}$/.test(c);
const clamp = (v: number, [lo, hi]: readonly [number, number], fallback: number) =>
  Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : fallback;

/** The same rules as the engine's `sanitized`: inside the limits, valid
 * colors, a known visibility, never invisible. */
export function sanitized(c: CrosshairSettings): CrosshairSettings {
  const d = defaultCrosshair();
  const s = { ...d, ...c };
  s.opacity = clamp(s.opacity, LIMITS.opacity, d.opacity);
  s.length = clamp(s.length, LIMITS.length, d.length);
  s.thickness = clamp(s.thickness, LIMITS.thickness, d.thickness);
  s.gap = clamp(s.gap, LIMITS.gap, d.gap);
  s.dot_size = clamp(s.dot_size, LIMITS.dot_size, d.dot_size);
  s.outline_thickness = clamp(s.outline_thickness, LIMITS.outline_thickness, d.outline_thickness);
  s.rotation = Math.round(clamp(s.rotation, LIMITS.rotation, d.rotation));
  s.circle_radius = clamp(s.circle_radius, LIMITS.circle_radius, d.circle_radius);
  s.offset_x = Math.round(clamp(s.offset_x, LIMITS.offset, 0));
  s.offset_y = Math.round(clamp(s.offset_y, LIMITS.offset, 0));
  s.color = (validColor(s.color) ? s.color : d.color).toUpperCase();
  s.outline_color = (validColor(s.outline_color) ? s.outline_color : d.outline_color).toUpperCase();
  if (s.visibility !== 'game' && s.visibility !== 'always') s.visibility = d.visibility;
  const lines = s.shape === 'cross' ? s.arm_top || s.arm_bottom || s.arm_left || s.arm_right
    : s.shape === 't' ? s.arm_bottom || s.arm_left || s.arm_right
    : s.shape !== 'dot';
  if (!lines && !s.circle) s.dot = true;
  return s;
}

export type Part =
  | { kind: 'rect'; x0: number; y0: number; x1: number; y1: number }
  | { kind: 'quad'; points: [number, number][] }
  | { kind: 'ring'; r_inner: number; r_outer: number }
  | { kind: 'disc'; r: number };
export type Geometry = { fill: Part[]; outline: Part[]; size: number };

type Prim =
  | { kind: 'seg'; x0: number; y0: number; x1: number; y1: number; h: number }
  | { kind: 'ring'; r_inner: number; r_outer: number }
  | { kind: 'disc'; r: number };

const px = (v: number, scale: number, min: number) => Math.max(min, Math.round(v * scale));
/** Fixed to 1/1000 of a pixel, never -0 (as the engine). */
const fix = (v: number) => Math.round(v * 1000) / 1000 + 0;

function grown(p: Prim, by: number): Prim {
  if (p.kind === 'seg') {
    const len = Math.hypot(p.x1 - p.x0, p.y1 - p.y0);
    const [ux, uy] = len > 0 ? [(p.x1 - p.x0) / len, (p.y1 - p.y0) / len] : [1, 0];
    return { kind: 'seg', x0: p.x0 - ux * by, y0: p.y0 - uy * by, x1: p.x1 + ux * by, y1: p.y1 + uy * by, h: p.h + by };
  }
  if (p.kind === 'ring') return { kind: 'ring', r_inner: Math.max(0, p.r_inner - by), r_outer: p.r_outer + by };
  return { kind: 'disc', r: p.r + by };
}

function part(p: Prim, angle: number): Part {
  if (p.kind === 'ring') return { kind: 'ring', r_inner: fix(p.r_inner), r_outer: fix(p.r_outer) };
  if (p.kind === 'disc') return { kind: 'disc', r: fix(p.r) };
  const rad = (angle * Math.PI) / 180;
  const [s, c] = [Math.sin(rad), Math.cos(rad)];
  const turn = (x: number, y: number) => [fix(x * c - y * s), fix(x * s + y * c)];
  const [ax, ay] = turn(p.x0, p.y0);
  const [bx, by] = turn(p.x1, p.y1);
  if (ay === by || ax === bx) {
    const [hx, hy] = ax !== bx && ay === by ? [0, p.h] : ax === bx && ay !== by ? [p.h, 0] : [p.h, p.h];
    return { kind: 'rect', x0: fix(Math.min(ax, bx) - hx), y0: fix(Math.min(ay, by) - hy), x1: fix(Math.max(ax, bx) + hx), y1: fix(Math.max(ay, by) + hy) };
  }
  const len = Math.hypot(bx - ax, by - ay);
  const [nx, ny] = [(-(by - ay) / len) * p.h, ((bx - ax) / len) * p.h];
  return { kind: 'quad', points: [[fix(ax + nx), fix(ay + ny)], [fix(bx + nx), fix(by + ny)], [fix(bx - nx), fix(by - ny)], [fix(ax - nx), fix(ay - ny)]] };
}

const reachOf = (p: Part) => p.kind === 'rect' ? Math.max(Math.abs(p.x0), Math.abs(p.x1), Math.abs(p.y0), Math.abs(p.y1))
  : p.kind === 'quad' ? Math.max(...p.points.map(([x, y]) => Math.max(Math.abs(x), Math.abs(y))))
  : p.kind === 'ring' ? p.r_outer : p.r;

/** Device-pixel geometry, centered on the origin (see crosshair.rs). */
export function geometry(input: CrosshairSettings, scaleIn: number): Geometry {
  const c = sanitized(input);
  const scale = Number.isFinite(scaleIn) ? Math.min(4, Math.max(0.5, scaleIn)) : 1;
  const t = px(c.thickness, scale, 1);
  const length = px(c.length, scale, 1);
  const gap = px(c.gap, scale, 0);
  const h = t / 2;
  const odd = t % 2 === 1;
  const start = gap + (odd ? 0.5 : 0);
  const prims: Prim[] = [];
  const seg = (x0: number, y0: number, x1: number, y1: number): Prim => ({ kind: 'seg', x0, y0, x1, y1, h });
  const [a, b] = [start, start + length];
  const [top, bottom, left, right] = c.shape === 'cross' ? [c.arm_top, c.arm_bottom, c.arm_left, c.arm_right]
    : c.shape === 't' ? [false, c.arm_bottom, c.arm_left, c.arm_right] : [false, false, false, false];
  if (top) prims.push(seg(0, -b, 0, -a));
  if (bottom) prims.push(seg(0, a, 0, b));
  if (left) prims.push(seg(-b, 0, -a, 0));
  if (right) prims.push(seg(a, 0, b, 0));
  if (c.shape === 'circle') {
    const r = Math.max(length + gap, t + 1);
    prims.push({ kind: 'ring', r_inner: r - h, r_outer: r + h });
  } else if (c.shape === 'square') {
    const s = Math.max(gap + length, t + 1);
    prims.push(seg(-s - h, -s, s + h, -s), seg(-s - h, s, s + h, s), seg(-s, -s + h, -s, s - h), seg(s, -s + h, s, s - h));
  } else if (c.shape === 'chevron') {
    const d = Math.SQRT1_2;
    for (const side of [-1, 1]) prims.push(seg(side * gap * d, gap * d, side * (gap + length) * d, (gap + length) * d));
  }
  if (c.circle) {
    const r = Math.max(px(c.circle_radius, scale, 2), t + 1);
    prims.push({ kind: 'ring', r_inner: r - h, r_outer: r + h });
  }
  if (c.dot) {
    let d = px(c.dot_size, scale, 1);
    if ((d % 2 === 1) !== odd) d += 1;
    const r = d / 2;
    prims.push(c.dot_round ? { kind: 'disc', r } : { kind: 'seg', x0: -r, y0: 0, x1: r, y1: 0, h: r });
  }
  const grow = px(c.outline_thickness, scale, 1);
  const fill = prims.map(p => part(p, c.rotation));
  const outline = c.outline ? prims.map(p => part(grown(p, grow), c.rotation)) : [];
  const reach = [...fill, ...outline].reduce((m, p) => Math.max(m, reachOf(p)), 0);
  const size = 2 * (Math.ceil(reach) + 1) + (odd ? 1 : 0);
  return { fill, outline, size };
}

// Serialize the desktop writes across page visits. A failed write must not
// prevent the next preference from reaching the engine.
export function orderedCrosshairSaves<T>(save: (settings: CrosshairSettings) => Promise<T>) {
  let previous = Promise.resolve();
  return (settings: CrosshairSettings): Promise<T> => {
    const request = previous.then(() => save(settings));
    previous = request.then(() => undefined, () => undefined);
    return request;
  };
}
