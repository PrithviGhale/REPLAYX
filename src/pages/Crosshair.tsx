import { useEffect, useRef, useState } from 'react';
import type React from 'react';
import { AlertTriangle, Crosshair, EyeOff, Gamepad2, Monitor, RotateCcw } from 'lucide-react';
import { api, errorText, native, type CrosshairStatus } from '../api';
import { useApp } from '../context';
import { comboFrom } from '../hotkeys';
import { Badge, Button, Card, Kbd, Segmented, Toggle } from '../ui';
import { Section } from './shared';
import {
  LIMITS, PRESETS, defaultCrosshair, geometry, matchingPreset, sanitized, validColor, withPreset,
  type CrosshairSettings, type Part, type Shape,
} from '../crosshair';

/** Colors that read well over most games. */
const SWATCHES = ['#00FF88', '#00E5FF', '#FFFFFF', '#FFE600', '#FF3B5C', '#FF4FD8', '#8B7CFF', '#FF9F1C'];
type Backdrop = 'dark' | 'bright' | 'split';
const SHAPES: { value: Shape; label: string }[] = [
  { value: 'cross', label: 'Cross' }, { value: 't', label: 'T' }, { value: 'circle', label: 'Circle' },
  { value: 'dot', label: 'Dot' }, { value: 'square', label: 'Box' }, { value: 'chevron', label: 'Chevron' },
];
const ARMS = [['arm_top', 'Top'], ['arm_bottom', 'Bottom'], ['arm_left', 'Left'], ['arm_right', 'Right']] as const;
/** What the length and gap sliders mean for each shape. */
const SIZE_LABELS: Record<Shape, [string, string]> = {
  cross: ['Length', 'Center gap'], t: ['Length', 'Center gap'], circle: ['Radius', 'Extra radius'],
  dot: ['Length', 'Center gap'], square: ['Size', 'Extra size'], chevron: ['Length', 'Center gap'],
};

/** The crosshair drawn from the same geometry as the on-screen one. `scale`
 * is the display scaling it is drawn for; `zoom` enlarges it for editing,
 * at most to `fit` CSS pixels (large crosshairs zoom less). */
export function CrosshairArt({ c, scale, zoom = 1, fit, className }: { c: CrosshairSettings; scale: number; zoom?: number; fit?: number; className?: string }) {
  const g = geometry(c, scale);
  const half = g.size / 2;
  const actual = g.size / scale;
  const css = actual * (fit ? Math.max(1, Math.min(zoom, fit / actual)) : zoom);
  const draw = (p: Part, color: string, key: string) => p.kind === 'rect'
    ? <rect key={key} x={p.x0 + half} y={p.y0 + half} width={p.x1 - p.x0} height={p.y1 - p.y0} fill={color} />
    : p.kind === 'quad'
      // Turned lines are anti-aliased on screen too.
      ? <polygon key={key} points={p.points.map(([x, y]) => `${x + half},${y + half}`).join(' ')} fill={color} shapeRendering="geometricPrecision" />
    : p.kind === 'ring'
      ? <circle key={key} cx={half} cy={half} r={(p.r_inner + p.r_outer) / 2} fill="none" stroke={color} strokeWidth={p.r_outer - p.r_inner} />
      : <circle key={key} cx={half} cy={half} r={p.r} fill={color} />;
  return (
    <svg className={className} width={css} height={css} viewBox={`0 0 ${g.size} ${g.size}`} shapeRendering="crispEdges" aria-hidden>
      <g opacity={c.opacity}>
        {g.outline.map((p, i) => draw(p, c.outline_color, `o${i}`))}
        {g.fill.map((p, i) => draw(p, c.color, `f${i}`))}
      </g>
    </svg>
  );
}

function Range({ label, value, min, max, step = 1, unit = 'px', format, onChange, disabled, centered }: {
  label: string; value: number; min: number; max: number; step?: number; unit?: string; format?: (v: number) => string;
  onChange: (v: number) => void; disabled?: boolean; centered?: boolean;
}) {
  const at = ((value - min) / (max - min)) * 100;
  // Centered values (position) fill from the middle toward the thumb.
  const [from, to] = centered ? [Math.min(50, at), Math.max(50, at)] : [0, at];
  return (
    <label className={`xh-range ${disabled ? 'disabled' : ''}`}>
      <span className="xh-range-head"><span>{label}</span><b>{format ? format(value) : `${value}${unit}`}</b></span>
      <input type="range" min={min} max={max} step={step} value={value} disabled={disabled} aria-label={label}
        aria-valuetext={format ? format(value) : `${value} ${unit}`}
        style={{ background: `linear-gradient(to right, var(--line-2) ${from}%, var(--accent) ${from}%, var(--accent) ${to}%, var(--line-2) ${to}%)` }}
        onChange={e => onChange(Number(e.target.value))} />
    </label>
  );
}

function ColorPicker({ label, value, onChange, swatches = SWATCHES }: { label: string; value: string; onChange: (v: string) => void; swatches?: string[] }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <div className="xh-color" role="group" aria-label={label}>
      <span className="xh-label">{label}</span>
      <div className="xh-swatches">
        {swatches.map(s => (
          <button key={s} type="button" className={`xh-swatch ${s === value ? 'on' : ''}`} style={{ background: s }}
            aria-label={`${label} ${s}`} aria-pressed={s === value} onClick={() => onChange(s)} />
        ))}
        <label className="xh-custom" title="Any color">
          <input type="color" value={value.toLowerCase()} onChange={e => onChange(e.target.value.toUpperCase())} aria-label={`${label}: any color`} />
        </label>
        <input className={`xh-hex ${validColor(text) ? '' : 'bad'}`} value={text} maxLength={7} spellCheck={false} aria-label={`${label} hex value`}
          onChange={e => { const v = e.target.value.trim(); setText(v); if (validColor(v)) onChange(v.toUpperCase()); }}
          onBlur={() => setText(value)} />
      </div>
    </div>
  );
}

function ShortcutPicker({ value, taken, onChange }: { value: string; taken: string[]; onChange: (v: string) => void }) {
  const [listening, setListening] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => {
    if (!listening) return;
    const key = (e: KeyboardEvent) => {
      e.preventDefault(); e.stopPropagation();
      if (e.key === 'Escape') { setListening(false); setMessage(null); return; }
      const r = comboFrom(e);
      if (!r) return;
      if ('error' in r) { setMessage(r.error ?? null); return; }
      setListening(false);
      if (taken.some(t => t.toLowerCase() === r.combo.toLowerCase())) { setMessage(`${r.combo.replace('Control', 'Ctrl')} is already a REPLAYX shortcut.`); return; }
      setMessage(null);
      if (native) api.testHotkey(r.combo).then(() => onChange(r.combo)).catch(err => setMessage(errorText(err)));
      else onChange(r.combo);
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [listening, taken, onChange]);
  return (
    <div className="xh-shortcut">
      <div className="xh-shortcut-row">
        <span className="xh-label">Show / hide shortcut</span>
        {listening ? <span className="listening">Press keys… <small>Esc to cancel</small></span> : value ? <Kbd combo={value} /> : <span className="muted small">None</span>}
        <Button size="sm" variant="ghost" onClick={() => setListening(l => !l)}>{listening ? 'Cancel' : 'Change'}</Button>
        {value && !listening && <Button size="sm" variant="ghost" onClick={() => { setMessage(null); onChange(''); }}>Remove</Button>}
      </div>
      <small className={message ? 'warn-text' : 'muted'}>{message ?? 'Works in games while the crosshair is on. Off: the key is released for other apps.'}</small>
    </div>
  );
}

export function CrosshairPage() {
  const { draft, syncFromEngine, toast } = useApp();
  const [c, setC] = useState<CrosshairSettings>(() => sanitized(draft.crosshair ?? defaultCrosshair()));
  const current = useRef(c);
  const lastSaved = useRef(c);
  const edit = useRef(0);
  const dirty = useRef(false);
  const pendingSave = useRef<(() => void) | undefined>(undefined);
  const timer = useRef<number | undefined>(undefined);
  const [status, setStatus] = useState<CrosshairStatus | null>(null);
  const [backdrop, setBackdrop] = useState<Backdrop>('dark');
  const [zoom, setZoom] = useState(4);
  const scale = typeof window !== 'undefined' ? Math.max(1, window.devicePixelRatio || 1) : 1;

  // Settings loaded after the page opened: take them over (no local edits yet).
  const engineCrosshair = JSON.stringify(draft.crosshair);
  useEffect(() => {
    if (!dirty.current && draft.crosshair) {
      const saved = sanitized(draft.crosshair);
      lastSaved.current = saved;
      current.current = saved;
      setC(saved);
    }
  }, [engineCrosshair]); // eslint-disable-line react-hooks/exhaustive-deps

  // What the on-screen crosshair is doing (once a second while this page is open).
  useEffect(() => {
    if (!native) { api.crosshairStatus().then(setStatus).catch(() => undefined); return; }
    let live = true;
    const poll = () => api.crosshairStatus().then(s => { if (live) setStatus(s); }).catch(() => undefined);
    void poll();
    const id = window.setInterval(poll, 1000);
    return () => { live = false; window.clearInterval(id); };
  }, []);

  const flush = () => {
    window.clearTimeout(timer.current);
    const save = pendingSave.current;
    pendingSave.current = undefined;
    save?.();
  };

  // Debounce slider changes and flush the latest edit when leaving this page.
  const update = (patch: Partial<CrosshairSettings>) => {
    const next = sanitized({ ...current.current, ...patch });
    const revision = ++edit.current;
    dirty.current = true;
    current.current = next;
    setC(next);
    window.clearTimeout(timer.current);
    pendingSave.current = () => {
      void api.setCrosshair(next).then(r => {
        const saved = sanitized(r.crosshair);
        lastSaved.current = saved;
        syncFromEngine({ crosshair: saved });
        if (r.warning) toast('error', r.warning);
        if (revision === edit.current) {
          dirty.current = false;
          current.current = saved;
          setC(saved);
          api.crosshairStatus().then(setStatus).catch(() => undefined);
        }
      }, e => {
        if (revision !== edit.current) return;
        dirty.current = false;
        toast('error', errorText(e));
        current.current = lastSaved.current;
        setC(lastSaved.current);
      });
    };
    timer.current = window.setTimeout(flush, 150);
  };
  useEffect(() => () => flush(), []);

  const overlay = status?.overlay;
  const game = status?.game;
  const running = game?.running ?? [];
  const waiting = c.enabled && !overlay?.error && !overlay?.shown && c.visibility === 'game' && !game?.in_front;
  const state = !c.enabled ? { tone: 'neutral' as const, text: 'Off' }
    : overlay?.error ? { tone: 'bad' as const, text: 'Not available' }
    : overlay?.shown ? { tone: 'ok' as const, text: 'On screen' }
    : waiting && running.length > 0 ? { tone: 'accent' as const, text: `${running[0]} found` }
    : waiting ? { tone: 'accent' as const, text: 'Waiting for a game' }
    : { tone: 'warn' as const, text: 'Hidden by shortcut' };
  const [sizeLabel, gapLabel] = SIZE_LABELS[c.shape];
  const lines = c.shape === 'cross' || c.shape === 't';
  const preset = matchingPreset(c);
  const reset = () => update({ ...defaultCrosshair(), enabled: c.enabled, visibility: c.visibility, hotkey: c.hotkey, display: c.display });
  const taken = [draft.replay_hotkey, draft.recording_hotkey, draft.mute_hotkey, draft.pause_hotkey, draft.window_hotkey];

  return (
    <Section>
      <Card className="xh-hero">
        <div className="xh-hero-row">
          <span className="xh-mark" aria-hidden><Crosshair size={22} /></span>
          <div className="xh-title">
            <h2>Crosshair Studio</h2>
            <p className="muted">Your aim. Your style.</p>
          </div>
          <Badge tone={state.tone} dot>{state.text}</Badge>
          <div className="xh-enable">
            <Toggle label={c.enabled ? 'Crosshair on' : 'Crosshair off'} checked={c.enabled} onChange={v => update({ enabled: v })} />
          </div>
        </div>
        {overlay?.error && <div className="banner bad">{overlay.error}</div>}
        {waiting && running.length > 0 && (
          <div className="banner info small"><Gamepad2 size={15} aria-hidden />
            <span>{running.join(', ')} {running.length > 1 ? 'are' : 'is'} running. Switch to {running.length > 1 ? 'a game' : 'it'} and the crosshair appears.</span>
          </div>
        )}
        {waiting && running.length === 0 && (
          <div className="banner info small"><Gamepad2 size={15} aria-hidden />
            <span>Start a game and the crosshair appears while it is in front. Game not recognized? Add it on the Games page, or choose Always on below.</span>
          </div>
        )}
        {c.enabled && game?.exclusive && (
          <div className="banner warn small"><AlertTriangle size={15} aria-hidden />
            <span>{game.exclusive} runs in exclusive full screen, where Windows shows no other window, the crosshair included. Choose Borderless or Windowed full screen in its video settings.</span>
          </div>
        )}
      </Card>

      <div className="xh-main">
        <Card title="Appearance" subtitle="Changes appear at once and are saved automatically."
          actions={<Button size="sm" variant="ghost" icon={RotateCcw} onClick={reset}>Reset</Button>}>
          <div className="xh-controls">
            <div>
              <span className="xh-label">Shape</span>
              <Segmented label="Shape" value={c.shape} onChange={v => update({ shape: v, rotation: 0 })} options={SHAPES} />
            </div>
            <ColorPicker label="Color" value={c.color} onChange={v => update({ color: v })} />
            {c.shape !== 'dot' && <>
              <Range label={sizeLabel} value={c.length} min={LIMITS.length[0]} max={LIMITS.length[1]} onChange={v => update({ length: v })} />
              <Range label="Thickness" value={c.thickness} min={LIMITS.thickness[0]} max={LIMITS.thickness[1]} onChange={v => update({ thickness: v })} />
              <Range label={gapLabel} value={c.gap} min={LIMITS.gap[0]} max={LIMITS.gap[1]} onChange={v => update({ gap: v })} />
            </>}
            {lines && (
              <div role="group" aria-label="Lines">
                <span className="xh-label">Lines</span>
                <div className="xh-arm-row">
                  {ARMS.map(([key, label]) => {
                    const off = c.shape === 't' && key === 'arm_top';
                    return (
                      <button key={key} type="button" className={`xh-arm ${c[key] && !off ? 'on' : ''}`} aria-pressed={c[key] && !off} disabled={off}
                        title={off ? 'A T has no top line.' : undefined} onClick={() => update({ [key]: !c[key] })}>{label}</button>
                    );
                  })}
                </div>
              </div>
            )}
            {!(c.shape === 'circle' && !(c.dot && !c.dot_round)) && !(c.shape === 'dot' && c.dot_round) && (
              <Range label="Rotation" value={c.rotation} min={LIMITS.rotation[0]} max={LIMITS.rotation[1]} unit="°"
                format={v => (v === 0 ? 'None' : `${v}°`)} onChange={v => update({ rotation: v })} />
            )}
            <Range label="Opacity" value={Math.round(c.opacity * 100)} min={LIMITS.opacity[0] * 100} max={100} unit="%" onChange={v => update({ opacity: v / 100 })} />
            <div className="xh-pair">
              <Toggle label="Center dot" checked={c.dot} disabled={c.shape === 'dot'} onChange={v => update({ dot: v })}
                description={c.shape === 'dot' ? 'Always on for the Dot style.' : undefined} />
              {c.dot && <>
                <Range label="Dot size" value={c.dot_size} min={LIMITS.dot_size[0]} max={LIMITS.dot_size[1]} onChange={v => update({ dot_size: v })} />
                <Segmented label="Dot shape" value={c.dot_round ? 'round' : 'square'} onChange={v => update({ dot_round: v === 'round' })}
                  options={[{ value: 'square', label: 'Square dot' }, { value: 'round', label: 'Round dot' }]} />
              </>}
            </div>
            <div className="xh-pair">
              <Toggle label="Ring" description="A circle around the center, with any shape." checked={c.circle} onChange={v => update({ circle: v })} />
              {c.circle && <Range label="Ring radius" value={c.circle_radius} min={LIMITS.circle_radius[0]} max={LIMITS.circle_radius[1]} onChange={v => update({ circle_radius: v })} />}
            </div>
            <div className="xh-pair">
              <Toggle label="Outline" description="A thin border keeps it readable on any background." checked={c.outline} onChange={v => update({ outline: v })} />
              {c.outline && <>
                <Range label="Outline thickness" value={c.outline_thickness} min={LIMITS.outline_thickness[0]} max={LIMITS.outline_thickness[1]} onChange={v => update({ outline_thickness: v })} />
                <ColorPicker label="Outline color" value={c.outline_color} onChange={v => update({ outline_color: v })} swatches={['#000000', '#FFFFFF', '#1A1A22', '#8B7CFF']} />
              </>}
            </div>
          </div>
        </Card>

        <Card className="xh-preview-card" title="Preview" subtitle="Exactly what appears on screen."
          actions={<Segmented label="Preview zoom" value={zoom} onChange={setZoom} options={[{ value: 1, label: 'Actual size' }, { value: 4, label: 'Zoom' }]} />}>
          <div className={`xh-stage ${backdrop}`} role="img" aria-label="Crosshair preview">
            <div className="xh-stage-grid" aria-hidden />
            <CrosshairArt c={c} scale={scale} zoom={zoom} fit={320} className="xh-art" />
          </div>
          <div className="xh-stage-foot">
            <Segmented label="Preview background" value={backdrop} onChange={setBackdrop}
              options={[{ value: 'dark', label: 'Dark' }, { value: 'bright', label: 'Bright' }, { value: 'split', label: 'Split' }]} />
            <span className="muted small">{preset ? PRESETS.find(p => p.id === preset)?.name : 'Custom'} · {Math.round(scale * 100)}% scaling</span>
          </div>
        </Card>
      </div>

      <Card title="Styles" subtitle="Start from a style, then make it yours.">
        <div className="xh-presets" role="radiogroup" aria-label="Crosshair style">
          {PRESETS.map(p => {
            const look = withPreset(c, p.id);
            return (
              <button key={p.id} type="button" role="radio" aria-checked={preset === p.id} className={`xh-preset ${preset === p.id ? 'on' : ''}`}
                onClick={() => update(withPreset(current.current, p.id))}>
                <span className="xh-preset-art"><CrosshairArt c={look} scale={scale} zoom={2} fit={64} /></span>
                <span>{p.name}</span>
              </button>
            );
          })}
        </div>
      </Card>

      <div className="grid-2">
        <Card title="Visibility" subtitle="When the crosshair is on screen.">
          <Segmented label="When to show the crosshair" value={c.visibility} onChange={v => update({ visibility: v })}
            options={[{ value: 'game', label: 'In-game only' }, { value: 'always', label: 'Always on' }]} />
          <p className="muted small xh-note">{c.visibility === 'game'
            ? 'Shown while a recognized game is in front. A quick Alt+Tab never makes it flicker; it hides a moment after you leave the game.'
            : 'Shown whenever the crosshair is on, also on the desktop.'}</p>
          <ShortcutPicker value={c.hotkey} taken={taken} onChange={v => update({ hotkey: v })} />
          <div className="xh-limits">
            <EyeOff size={15} aria-hidden />
            <p className="muted small">Shows over windowed and borderless full-screen games. Windows can't show any app over <b>exclusive full-screen</b> games: switch the game to borderless. The crosshair is a separate window above the game: it doesn't read or change the game, and REPLAYX's clips don't include it.</p>
          </div>
        </Card>
        <Card title="Position" subtitle="Centered on the display; nudge it if your game's center differs.">
          {status && status.displays.length > 1 && (
            <label className="field"><span className="field-label">Display</span>
              <select value={c.display} onChange={e => update({ display: Number(e.target.value) })}>
                {status.displays.map(d => <option key={d.index} value={d.index}>{d.label}</option>)}
              </select>
            </label>
          )}
          {status && status.displays.length <= 1 && overlay?.display && <p className="muted small xh-note"><Monitor size={14} aria-hidden /> {overlay.display}</p>}
          <Range label="Horizontal" value={c.offset_x} min={-200} max={200} centered format={v => (v === 0 ? 'Center' : `${v > 0 ? '+' : ''}${v} px`)} onChange={v => update({ offset_x: v })} />
          <Range label="Vertical" value={c.offset_y} min={-200} max={200} centered format={v => (v === 0 ? 'Center' : `${v > 0 ? '+' : ''}${v} px`)} onChange={v => update({ offset_y: v })} />
          <Button size="sm" variant="ghost" disabled={c.offset_x === 0 && c.offset_y === 0} onClick={() => update({ offset_x: 0, offset_y: 0 })}>Center</Button>
        </Card>
      </div>
    </Section>
  );
}
