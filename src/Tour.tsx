import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, X } from 'lucide-react';
import { api, native } from './api';
import { useApp } from './context';
import { hotkeyLabel } from './format';
import { TOUR, placeTip, shouldAutoStart, type Placement, type Rect, type TourPage } from './tour-steps';

/** Start the tour from anywhere (Settings → Replay tutorial). */
export const startTour = () => window.dispatchEvent(new Event('replayx-tour'));

const reduceMotion = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;

/** Spotlight tour over the real interface. Navigates between pages and
 * scrolls the highlighted component into view, but never clicks anything:
 * the dimmed layer takes all pointer input while the tour is open. */
export function Tour({ go }: { go: (page: TourPage) => void }) {
  const { saved, loaded } = useApp();
  const [step, setStep] = useState<number | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [missing, setMissing] = useState(false);
  const [place, setPlace] = useState<Placement | null>(null);
  const tip = useRef<HTMLDivElement>(null);
  const next = useRef<HTMLButtonElement>(null);
  const auto = useRef(false);

  // First run: once settings are loaded and the splash is gone.
  useEffect(() => {
    if (auto.current || !shouldAutoStart(loaded, saved.tour_done, native)) return;
    auto.current = true;
    const t = window.setTimeout(() => setStep(0), 400);
    return () => window.clearTimeout(t);
  }, [loaded, saved.tour_done]);
  useEffect(() => {
    const start = () => setStep(0);
    window.addEventListener('replayx-tour', start);
    // UI preview screenshots only (the app's own URL never has ?tour).
    if (new URLSearchParams(window.location.search).has('tour')) start();
    return () => window.removeEventListener('replayx-tour', start);
  }, []);

  const close = useCallback(() => {
    setStep(null);
    setRect(null);
    if (native) void api.markTourDone().catch(() => undefined);
  }, []);

  const current = step === null ? null : TOUR[step];
  // Go to the step's page, then find its component (it may render a moment later).
  useEffect(() => {
    if (!current) return;
    go(current.page);
    setMissing(false);
    setRect(null);
    if (!current.target) return;
    let tries = 0;
    const find = window.setInterval(() => {
      const el = document.querySelector<HTMLElement>(`[data-tour="${current.target}"]`);
      if (el) {
        window.clearInterval(find);
        el.scrollIntoView({ block: 'center', inline: 'nearest', behavior: reduceMotion() ? 'auto' : 'smooth' });
      } else if (++tries > 25) {
        window.clearInterval(find);
        setMissing(true);
      }
    }, 40);
    return () => window.clearInterval(find);
  }, [current, go]);
  // Follow the component while it scrolls, the window resizes or layout moves.
  useEffect(() => {
    if (!current?.target) return;
    let frame = 0;
    const track = () => {
      const el = document.querySelector<HTMLElement>(`[data-tour="${current.target}"]`);
      if (el) {
        const r = el.getBoundingClientRect();
        setRect(prev => (prev && Math.abs(prev.left - r.left) < 0.5 && Math.abs(prev.top - r.top) < 0.5 && Math.abs(prev.width - r.width) < 0.5 && Math.abs(prev.height - r.height) < 0.5
          ? prev : { left: r.left, top: r.top, width: r.width, height: r.height }));
      }
      frame = requestAnimationFrame(track);
    };
    frame = requestAnimationFrame(track);
    return () => cancelAnimationFrame(frame);
  }, [current]);
  useLayoutEffect(() => {
    if (!current || !tip.current) return;
    const box = tip.current.getBoundingClientRect();
    const pad = 6;
    const target = rect && !missing ? { left: rect.left - pad, top: rect.top - pad, width: rect.width + pad * 2, height: rect.height + pad * 2 } : null;
    setPlace(placeTip(target, { width: box.width, height: box.height }, { width: window.innerWidth, height: window.innerHeight }));
  }, [rect, missing, current]);
  useEffect(() => { next.current?.focus(); }, [step]);
  useEffect(() => {
    if (step === null) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); close(); }
      else if (e.key === 'ArrowRight') { e.preventDefault(); setStep(s => (s === null ? s : Math.min(TOUR.length - 1, s + 1))); }
      else if (e.key === 'ArrowLeft') { e.preventDefault(); setStep(s => (s === null ? s : Math.max(0, s - 1))); }
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [step, close]);

  if (step === null || !current) return null;
  const last = step === TOUR.length - 1;
  const pad = 6;
  const hole = rect && !missing ? { x: rect.left - pad, y: rect.top - pad, w: rect.width + pad * 2, h: rect.height + pad * 2 } : null;
  const keys = { replay: hotkeyLabel(saved.replay_hotkey), record: hotkeyLabel(saved.recording_hotkey) };
  return (
    <div className={`tour ${reduceMotion() ? 'still' : ''}`} role="dialog" aria-modal="true" aria-labelledby="tour-title" aria-describedby="tour-body">
      <svg className="tour-dim" width="100%" height="100%" aria-hidden="true">
        <defs>
          <mask id="tour-hole">
            <rect width="100%" height="100%" fill="#fff" />
            {hole && <rect x={hole.x} y={hole.y} width={hole.w} height={hole.h} rx="12" fill="#000" />}
          </mask>
        </defs>
        <rect width="100%" height="100%" fill="rgba(6,7,11,0.72)" mask="url(#tour-hole)" />
        {hole && <rect className="tour-ring" x={hole.x} y={hole.y} width={hole.w} height={hole.h} rx="12" />}
      </svg>
      <div ref={tip} className={`tour-tip side-${place?.side ?? 'center'}`}
        style={{ left: place?.x ?? -9999, top: place?.y ?? -9999, ['--arrow' as string]: `${place?.arrow ?? 0}px` }}>
        <div className="tour-top">
          <span className="tour-progress">Step {step + 1} of {TOUR.length}</span>
          <button className="tour-x" aria-label="Skip tour" title="Skip tour (Esc)" onClick={close}><X size={15} /></button>
        </div>
        <h3 id="tour-title">{current.title}</h3>
        <p id="tour-body">{current.body(keys)}{missing && ' (Not available right now.)'}</p>
        <div className="tour-dots" aria-hidden="true">{TOUR.map((_, i) => <i key={i} className={i === step ? 'on' : i < step ? 'done' : ''} />)}</div>
        <div className="tour-actions">
          <button className="tour-skip" onClick={close}>Skip tour</button>
          <span className="spacer" />
          {step > 0 && <button className="btn btn-ghost btn-sm" onClick={() => setStep(step - 1)}><ArrowLeft size={14} /><span>Back</span></button>}
          <button ref={next} className="btn btn-primary btn-sm" onClick={() => (last ? close() : setStep(step + 1))}>
            <span>{last ? 'Finish' : 'Next'}</span>{!last && <ArrowRight size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
