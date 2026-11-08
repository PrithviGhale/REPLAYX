import type { CanvasMode, Display, Rect } from './api';

export const duration = (n: number) => {
  const s = Math.max(0, Math.floor(n));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}` : `${m}:${String(r).padStart(2, '0')}`;
};
export const durationLabel = (n: number) => (n < 60 ? `${n} sec` : n % 60 === 0 ? `${n / 60} min` : `${(n / 60).toFixed(1)} min`);
export const bytes = (n: number) => {
  if (n >= 1024 ** 4) return `${(n / 1024 ** 4).toFixed(1)} TB`;
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  return `${(n / 1024).toFixed(0)} KB`;
};
export const db = (peak: number) => (peak <= 0.00001 ? '−∞ dB' : `${(20 * Math.log10(peak)).toFixed(1)} dB`);
export const encoderName = (n: string) => ({
  h264_nvenc: 'NVIDIA NVENC', h264_amf: 'AMD AMF', h264_qsv: 'Intel Quick Sync', h264_qsv_lp: 'Intel Quick Sync (low-power)', libopenh264: 'OpenH264 (CPU)',
  mpeg4: 'MPEG-4 (CPU)', auto: 'Automatic',
} as Record<string, string>)[n] ?? (n || '—');
export const hotkeyLabel = (k: string) => k.split('+').map(p => (p === 'Control' ? 'Ctrl' : p)).join(' + ');
export const canvasWidth = (h: number) => ({ 480: 854, 720: 1280, 1080: 1920, 1440: 2560, 2160: 3840 } as Record<number, number>)[h] ?? 0;

/** Mirror of media::target_bitrate_mbps: preset bitrate at an output size and fps. */
export function targetBitrate(preset: string, custom: number, fps: number, width: number, height: number) {
  if (preset === 'custom') return custom;
  const base = preset === 'performance' ? 16 : preset === 'quality' ? 45 : 28;
  const pixels = (width * height) / (1920 * 1080);
  return Math.min(150, Math.max(4, Math.round(base * pixels ** 0.9 * (fps / 60) ** 0.75)));
}
/** Output width for settings (0 = 16:9 canvas for the height, as in older files). */
export const outputWidth = (width: number, height: number) => width || canvasWidth(height);

/** Mirror of geometry::plan for previews (the engine validates the real plan). */
export function plan(source: Rect, width: number, height: number, mode: CanvasMode) {
  const cw = width, ch = height;
  const even = (v: number) => Math.max(2, Math.floor(v) & ~1);
  const scaled = (v: number, num: number, den: number) => even(Math.round((v * num) / den));
  const { width: sw, height: sh } = source;
  if (mode === 'stretch') return { width: cw, height: ch, iw: cw, ih: ch, crop: source };
  if (mode === 'fit') {
    const [iw, ih] = sw * ch >= sh * cw ? [cw, Math.min(ch, scaled(sh, cw, sw))] : [Math.min(cw, scaled(sw, ch, sh)), ch];
    return { width: cw, height: ch, iw, ih, crop: source };
  }
  if (mode === 'crop') {
    const c = { ...source };
    if (sw * ch > sh * cw) { c.width = Math.min(sw, scaled(sh, cw, ch)); c.x += Math.floor((sw - c.width) / 2); }
    else { c.height = Math.min(sh, scaled(sw, ch, cw)); c.y += Math.floor((sh - c.height) / 2); }
    return { width: cw, height: ch, iw: cw, ih: ch, crop: c };
  }
  const w = scaled(sw, ch, sh);
  return { width: w, height: ch, iw: w, ih: ch, crop: source };
}
export const sourceRect = (d: Display | undefined, r: Rect | null): Rect =>
  r ?? { x: 0, y: 0, width: d ? Math.min(d.active_width, d.desktop_width) : 1920, height: d ? Math.min(d.active_height, d.desktop_height) : 1080 };
export const aspect = (w: number, h: number) => {
  const g = (a: number, b: number): number => (b ? g(b, a % b) : a);
  const d = g(w, h) || 1;
  // Reduced ratios: 8:5 is 16:10; 64:27, 43:18 and 12:5 are marketed as 21:9.
  const named: Record<string, string> = { '8:5': '16:10', '16:9': '16:9', '4:3': '4:3', '5:4': '5:4', '64:27': '21:9', '43:18': '21:9', '12:5': '21:9', '3:2': '3:2' };
  const k = `${w / d}:${h / d}`;
  return named[k] ?? (w / h).toFixed(2) + ':1';
};
