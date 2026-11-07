import { useState } from 'react';
import { ChevronDown, Monitor, RefreshCw } from 'lucide-react';
import type { CanvasMode, Rect } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { aspect, canvasWidth, encoderName, plan, sourceRect } from '../format';
import { Badge, Button, Card, Field, Segmented, Toggle } from '../ui';
import { Section } from './shared';

const MODES: { id: CanvasMode; title: string; text: string }[] = [
  { id: 'stretch', title: 'Stretch to fill', text: 'Fills 16:9 exactly. Use for stretched 4:3 gameplay — the video looks like your screen.' },
  { id: 'fit', title: 'Preserve aspect', text: 'Whole picture, no distortion. Black bars where the shape differs from 16:9.' },
  { id: 'crop', title: 'Crop to fill', text: 'Fills 16:9 without distortion by trimming the edges. Cropped at capture, so it costs nothing.' },
  { id: 'native', title: 'Native shape', text: 'Keeps the source shape at the chosen height. Output width follows your display.' },
];

function ModeDiagram({ mode, source, height }: { mode: CanvasMode; source: Rect; height: number }) {
  const p = plan(source, height, mode);
  // Draw the output frame scaled into a 120x68 box.
  const k = Math.min(120 / p.width, 68 / p.height);
  const fw = p.width * k, fh = p.height * k, fx = (130 - fw) / 2, fy = (78 - fh) / 2;
  const iw = p.iw * k, ih = p.ih * k, ix = fx + (fw - iw) / 2, iy = fy + (fh - ih) / 2;
  // For crop, show the discarded part of the source as a faint outline.
  const crop = mode === 'crop' && (p.crop.width < source.width || p.crop.height < source.height);
  const sx = crop ? fw / p.crop.width : 0;
  return (
    <svg viewBox="0 0 130 78" className="mode-diagram" aria-hidden="true">
      {crop && <rect x={fx - (p.crop.x - source.x) * sx} y={fy - (p.crop.y - source.y) * sx} width={source.width * sx} height={source.height * sx} className="discard" />}
      <rect x={fx} y={fy} width={fw} height={fh} className="frame" />
      <rect x={ix} y={iy} width={iw} height={ih} className="image" />
      <path d={`M${ix + iw * 0.2} ${iy + ih * 0.72} l${iw * 0.18} ${-ih * 0.3} l${iw * 0.14} ${ih * 0.18} l${iw * 0.14} ${-ih * 0.26} l${iw * 0.16} ${ih * 0.38}`} className="glyph" />
    </svg>
  );
}

export function CapturePage() {
  const { draft, patch, caps, refreshCaps, run, busy } = useApp();
  const active = useStatus(activeOf);
  const live = useStatus(x => ({ scaling: x.scaling, detail: x.scaling_detail, encoder: x.encoder, ow: x.output_width, oh: x.output_height }));
  const [advanced, setAdvanced] = useState(!!draft.source_rect);
  const displays = caps?.displays ?? [];
  const display = displays.find(d => d.index === draft.display);
  const src = sourceRect(display, draft.source_rect);
  const p = plan(src, draft.height, draft.canvas_mode);
  const dda = draft.capture === 'desktop-duplication';
  const stretchX = p.iw / src.width, stretchY = p.ih / src.height;
  const distortion = draft.canvas_mode === 'stretch' && Math.abs(stretchX / stretchY - 1) > 0.01
    ? `stretched ${stretchX > stretchY ? 'horizontally' : 'vertically'} ×${(Math.max(stretchX, stretchY) / Math.min(stretchX, stretchY)).toFixed(2)}` : 'no distortion';

  return (
    <Section>
      <Card title="Source" actions={<Button variant="ghost" icon={RefreshCw} disabled={!!busy} onClick={() => run('Refreshing displays', refreshCaps)}>Refresh</Button>}>
        <Segmented label="Capture method" value={draft.capture} disabled={active}
          onChange={v => patch({ capture: v, source_rect: null })}
          options={[
            { value: 'desktop-duplication', label: 'Display', hint: 'Desktop Duplication — fastest, GPU-based' },
            { value: 'desktop', label: 'All displays (GDI)', hint: 'Compatibility capture of the whole virtual desktop' },
            { value: 'window', label: 'Window (GDI)', hint: 'Compatibility capture of one window by exact title' },
          ]} />
        {dda && (
          <div className="display-grid">
            {displays.length === 0 && <p className="muted small">No displays reported yet. Refresh, or check that the media runtime is available.</p>}
            {displays.map(d => (
              <button key={d.index} className={`display-card ${d.index === draft.display ? 'on' : ''}`} disabled={active}
                onClick={() => patch({ display: d.index, source_rect: null })} aria-pressed={d.index === draft.display}>
                <Monitor size={22} />
                <strong>Display {d.index + 1}</strong>
                <span>{d.active_width} × {d.active_height} · {aspect(d.active_width, d.active_height)}</span>
                <span>{d.refresh_hz} Hz{d.left === 0 && d.top === 0 ? ' · primary position' : ''}</span>
              </button>
            ))}
          </div>
        )}
        {draft.capture === 'window' && (
          <Field label="Exact window title" hint="GDI capture can't see most fullscreen games; prefer Display capture.">
            <input value={draft.window_title} disabled={active} onChange={e => patch({ window_title: e.target.value })} />
          </Field>
        )}
      </Card>

      <Card title="Output" subtitle={dda ? `${src.width}×${src.height} source → ${p.width}×${p.height} video, ${distortion}` : 'Scaled by the CPU for GDI sources'}>
        <div className="grid-2 tight">
          <Field label="Resolution">
            <Segmented label="Resolution" value={draft.height} disabled={active} onChange={v => patch({ height: v })}
              options={[480, 720, 1080, 1440, 2160].map(h => ({ value: h, label: h === 2160 ? '4K' : `${h}p`, hint: `${canvasWidth(h)}×${h} canvas` }))} />
          </Field>
          <Field label="Frame rate" hint={draft.fps > 60 ? 'Above 60 fps is experimental: encoder and display limits apply.' : undefined}>
            <Segmented label="Frame rate" value={draft.fps} disabled={active} onChange={v => patch({ fps: v })}
              options={[30, 60, 120, 144].map(f => ({ value: f, label: `${f}`, hint: f > 60 ? 'Experimental' : undefined }))} />
          </Field>
        </div>
        <div className="mode-grid" role="radiogroup" aria-label="Aspect ratio">
          {MODES.map(m => (
            <button key={m.id} role="radio" aria-checked={draft.canvas_mode === m.id} disabled={active}
              className={`mode-card ${draft.canvas_mode === m.id ? 'on' : ''}`} onClick={() => patch({ canvas_mode: m.id })}>
              <ModeDiagram mode={m.id} source={src} height={draft.height} />
              <strong>{m.title}</strong>
              <span>{m.text}</span>
            </button>
          ))}
        </div>
        <p className="muted small">Stretched 4:3 games (for example 1440×1080 shown full-screen) change the display mode itself. REPLAYX detects the change, restarts capture on the new mode and validates the encoded size, so the desktop can't leak into the frame.</p>
      </Card>

      <Card title="Encoding">
        <div className="grid-2 tight">
          <Field label="Encoder" hint="Automatic tries the most efficient working GPU path first.">
            <select value={draft.encoder} disabled={active} onChange={e => patch({ encoder: e.target.value })}>
              <option value="auto">Automatic</option>
              {(caps?.encoders ?? []).map(e => <option key={e.name} value={e.name} disabled={!e.available}>{encoderName(e.name)}{e.available ? '' : ' — not available'}</option>)}
            </select>
          </Field>
          <Field label="Speed / quality">
            <Segmented label="Preset" value={draft.preset} disabled={active} onChange={v => patch({ preset: v })}
              options={[{ value: 'performance', label: 'Fast' }, { value: 'balanced', label: 'Balanced' }, { value: 'quality', label: 'Quality' }]} />
          </Field>
        </div>
        <Field label="Video bitrate" hint={`≈ ${(draft.bitrate_mbps * 7.5).toFixed(0)} MB per minute of video.`}>
          <div className="range-row">
            <input type="range" min={2} max={60} value={draft.bitrate_mbps} disabled={active} onChange={e => patch({ bitrate_mbps: Number(e.target.value) })} aria-label="Bitrate" />
            <span className="range-value">{draft.bitrate_mbps} Mbps</span>
          </div>
        </Field>
        <Toggle label="GPU scaling" checked={draft.gpu_scaling} disabled={active} onChange={v => patch({ gpu_scaling: v })}
          description="Scale and convert frames on the GPU that owns the display. Turn off only to troubleshoot; the CPU path costs more." />
        <div className="path-box">
          <div><span className="muted small">Active path</span><strong>{active && live.scaling ? live.scaling : 'Selected when capture starts'}</strong></div>
          {active && live.encoder && <Badge tone="accent">{encoderName(live.encoder)} · {live.ow}×{live.oh}</Badge>}
          {active && live.detail && <details><summary>Why not a faster path?</summary><p className="mono small">{live.detail}</p></details>}
        </div>
      </Card>

      {dda && (
        <Card title={<button className="disclosure" onClick={() => setAdvanced(a => !a)} aria-expanded={advanced}><ChevronDown size={15} className={advanced ? 'open' : ''} />Manual source region</button>}
          subtitle="Only needed when a game shows in part of the display without changing the display mode.">
          {advanced && (
            <>
              <Toggle label="Capture only a region" checked={!!draft.source_rect} disabled={active}
                onChange={v => patch({ source_rect: v ? { x: 0, y: 0, width: src.width, height: src.height } : null })} />
              {draft.source_rect && (
                <>
                  <div className="rect-grid">
                    {(['x', 'y', 'width', 'height'] as const).map(k => (
                      <Field key={k} label={k === 'x' ? 'Left' : k === 'y' ? 'Top' : k === 'width' ? 'Width' : 'Height'}>
                        <input type="number" min={k === 'width' || k === 'height' ? 16 : 0} value={draft.source_rect![k]} disabled={active}
                          onChange={e => patch({ source_rect: { ...draft.source_rect!, [k]: Math.max(0, Math.round(Number(e.target.value))) } })} />
                      </Field>
                    ))}
                  </div>
                  {display && (
                    <Button variant="ghost" disabled={active} onClick={() => {
                      const h = display.active_height, w = Math.min(display.active_width, Math.round((h * 4) / 3 / 2) * 2);
                      patch({ source_rect: { x: Math.floor((display.active_width - w) / 2), y: 0, width: w, height: h } });
                    }}>Centre a 4:3 region</Button>
                  )}
                  <p className="muted small">Physical pixels on the selected display. The region is validated against the live display mode before capture starts.</p>
                </>
              )}
            </>
          )}
        </Card>
      )}
    </Section>
  );
}
