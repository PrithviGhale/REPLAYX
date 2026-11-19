import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { ArrowLeft, ArrowRight, Compass, ExternalLink, X } from 'lucide-react';
import { api, native, VERSION } from './api';
import { BrandMark } from './brand';
import { useApp } from './context';
import { hotkeyLabel } from './format';
import { activeOf, firstPoll, usePollError, useStatus } from './store';
import { TOUR, placeTip, type Placement, type Rect, type TourPage, type TourStep } from './tour-steps';
import { advanceTourState, featuresFor, pendingTour, readTourState, RELEASES, releaseNotesUrl, RELEASES_PAGE, TOUR_STORAGE, type Outcome, type Release, type TourState } from './release-tour';
import { Button } from './ui';

const TOUR_EVENT = 'replayx-tour';
type Mode = 'welcome' | 'update';
export const startTour = () => window.dispatchEvent(new CustomEvent<Mode>(TOUR_EVENT, { detail: 'welcome' }));
export const startWhatsNew = () => window.dispatchEvent(new CustomEvent<Mode>(TOUR_EVENT, { detail: 'update' }));

type Session = { mode: Mode; step: number | null; manual: boolean };
const FEATURE_ANCHORS: Record<string, { page: TourPage; target: string }> = {
  workspaces: { page: 'home', target: 'nav-home' },
  crosshair: { page: 'crosshair', target: 'crosshair-studio' },
  'native-engine': { page: 'capture', target: 'engine-choice' },
};

function loadState() {
  try { return readTourState(localStorage.getItem(TOUR_STORAGE)); } catch { return null; }
}
function writeState(state: TourState) {
  try { localStorage.setItem(TOUR_STORAGE, JSON.stringify(state)); } catch { /* Available for this session. */ }
}

function TourDialog({ children, guided, close, arrows }: { children: ReactNode; guided: boolean; close: () => void; arrows: (delta: number) => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    const element = dialog.current!;
    const previous = document.activeElement;
    element.showModal();
    element.querySelector<HTMLElement>("[data-tour-primary]")?.focus({ preventScroll: true });
    return () => {
      element.close();
      if (previous instanceof HTMLElement && previous.isConnected && document.hasFocus()) previous.focus({ preventScroll: true });
    };
  }, []);
  return (
    <dialog ref={dialog} className={guided ? 'tour guided-tour' : 'tour tour-intro'} aria-labelledby="tour-title"
      onCancel={event => { event.preventDefault(); close(); }}
      onKeyDown={event => {
        if (guided && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
          event.preventDefault();
          arrows(event.key === 'ArrowLeft' ? -1 : 1);
        }
      }}>
      {children}
    </dialog>
  );
}

function Spotlight({ step, index, total, go, move, close, done, keys }: {
  step: TourStep; index: number; total: number; go: (page: TourPage) => void; move: (delta: number) => void;
  close: () => void; done: () => void; keys: { replay: string; record: string };
}) {
  const tip = useRef<HTMLDivElement>(null);
  const next = useRef<HTMLButtonElement>(null);
  const [layout, setLayout] = useState<{ rect: Rect | null; place: Placement } | null>(null);
  const [missing, setMissing] = useState(false);

  useLayoutEffect(() => {
    go(step.page);
    setMissing(false);
    const root = document.getElementById('root')!;
    const view = () => ({ width: window.innerWidth, height: window.innerHeight });
    let target: HTMLElement | null = null;
    let frame = 0;
    const measure = () => {
      const box = tip.current?.getBoundingClientRect();
      if (!box) return;
      const r = target?.getBoundingClientRect();
      const rect = r && r.width && r.height && r.bottom > 0 && r.top < view().height
        ? { left: r.left - 5, top: r.top - 5, width: r.width + 10, height: r.height + 10 } : null;
      setLayout({ rect, place: placeTip(rect, box, view()) });
    };
    const queueMeasure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };
    const resize = new ResizeObserver(queueMeasure);
    const lookup = () => {
      if (document.querySelector<HTMLElement>('.workspace-content')?.dataset.page !== step.page) return;
      target = step.target ? root.querySelector<HTMLElement>('[data-tour="' + step.target + '"]') : null;
      if (!target) return;
      watch.disconnect();
      target.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
      resize.observe(target);
      const content = document.querySelector('.workspace-inner');
      if (content) resize.observe(content);
      queueMeasure();
    };
    const watch = new MutationObserver(lookup);
    if (tip.current) resize.observe(tip.current);
    if (step.target) {
      watch.observe(root, { childList: true, subtree: true });
      lookup();
    }
    const timeout = step.target && !target ? window.setTimeout(() => {
      watch.disconnect();
      if (!target) { setMissing(true); measure(); }
    }, 750) : 0;
    window.addEventListener('resize', queueMeasure);
    window.addEventListener('scroll', queueMeasure, true);
    measure();
    return () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(timeout);
      watch.disconnect();
      resize.disconnect();
      window.removeEventListener('resize', queueMeasure);
      window.removeEventListener('scroll', queueMeasure, true);
    };
  }, [step, go]);
  useEffect(() => { next.current?.focus({ preventScroll: true }); }, [step]);

  const hole = layout?.rect;
  const last = index + 1 === total;
  return <>
    <svg className="tour-dim" width="100%" height="100%" aria-hidden="true">
      <defs><mask id="tour-hole">
        <rect width="100%" height="100%" fill="white" />
        {hole && <rect x={hole.left} y={hole.top} width={hole.width} height={hole.height} rx="10" fill="black" />}
      </mask></defs>
      <rect width="100%" height="100%" fill="rgba(8,10,13,0.76)" mask="url(#tour-hole)" />
      {hole && <rect className="tour-ring" x={hole.left} y={hole.top} width={hole.width} height={hole.height} rx="10" />}
    </svg>
    <div ref={tip} className={'tour-tip side-' + (layout?.place.side ?? 'center')}
      style={{ left: layout?.place.x ?? 0, top: layout?.place.y ?? 0, visibility: layout ? 'visible' : 'hidden', ['--arrow' as string]: (layout?.place.arrow ?? 0) + 'px' }}>
      <div className="tour-top"><span className="tour-progress">Step {index + 1} of {total}</span>
        <button type="button" className="tour-x" aria-label="Skip tour" onClick={close}><X size={17} /></button></div>
      <h2 id="tour-title">{step.title}</h2>
      <p>{step.body(keys)}{missing && ' This control is currently unavailable.'}</p>
      <div className="tour-dots" aria-hidden="true">{Array.from({ length: total }, (_, i) => <i key={i} className={i === index ? 'on' : i < index ? 'done' : ''} />)}</div>
      <div className="tour-actions">
        <button className="tour-skip" onClick={close}>Skip</button><span className="spacer" />
        {index > 0 && <Button variant="ghost" icon={ArrowLeft} onClick={() => move(-1)}>Back</Button>}
        <button type="button" ref={next} className="btn btn-primary" onClick={() => last ? done() : move(1)}>
          {last ? 'Finish' : 'Next'}{!last && <ArrowRight size={15} />}
        </button>
      </div>
    </div>
  </>;
}

export function Tour({ go, fixture }: { go: (page: TourPage) => void; fixture?: Release }) {
  const version = fixture?.version ?? VERSION;
  const release = fixture ?? RELEASES.find(entry => entry.version === version);
  const { saved, loaded, toast } = useApp();
  const held = useStatus(status => ({ busy: activeOf(status) || !!status.foreground_game, damaged: !!status.settings_problem }));
  const engineError = usePollError();
  const [ready, setReady] = useState(!native);
  const [state, setState] = useState<TourState | null>(loadState);
  const [session, setSession] = useState<Session | null>(null);
  const [focused, setFocused] = useState(false);
  const [notesError, setNotesError] = useState(false);

  useEffect(() => {
    const read = () => setFocused(document.visibilityState === 'visible' && document.hasFocus());
    read();
    window.addEventListener('focus', read);
    window.addEventListener('blur', read);
    document.addEventListener('visibilitychange', read);
    return () => {
      window.removeEventListener('focus', read);
      window.removeEventListener('blur', read);
      document.removeEventListener('visibilitychange', read);
    };
  }, []);
  useEffect(() => {
    if (!loaded || !native) return;
    let cancelled = false;
    void Promise.all([firstPoll, state?.version === version ? Promise.resolve('existing' as const) : api.tourOrigin()]).then(([, origin]) => {
      if (cancelled) return;
      if (!origin) { setReady(true); return; }
      const next = advanceTourState(state, version, origin, saved.tour_done);
      if (state !== next) { writeState(next); setState(next); }
      setReady(true);
    }).catch(() => { if (!cancelled) setReady(true); });
    return () => { cancelled = true; };
  }, [loaded, saved.tour_done, state, version]);

  const safe = ready && focused && !held.busy && !engineError;
  useEffect(() => {
    if (!safe || !state || held.damaged || session) return;
    const mode = pendingTour(state, release ? [release] : []);
    if (mode) { setNotesError(false); setSession({ mode, step: null, manual: false }); }
  }, [state, session, safe, held.damaged, release]);
  useEffect(() => {
    const show = (event: Event) => {
      const mode = (event as CustomEvent<Mode>).detail;
      if (held.busy) { toast('info', 'Stop capture and return to REPLAYX to open the tour.'); return; }
      setNotesError(false);
      setSession({ mode: mode === 'update' ? 'update' : 'welcome', step: null, manual: true });
    };
    window.addEventListener(TOUR_EVENT, show);
    return () => window.removeEventListener(TOUR_EVENT, show);
  }, [held.busy, toast]);

  const updating = session?.mode === 'update';
  const previous = session?.manual ? null : state?.previous ?? null;
  const features = useMemo(() => release && updating ? featuresFor(release, previous) : [], [release, updating, previous]);
  const steps = useMemo(() => updating ? features.flatMap(({ id, title, summary }) => FEATURE_ANCHORS[id]
    ? [{ id, title, body: () => summary, ...FEATURE_ANCHORS[id] }] : []) : TOUR, [updating, features]);
  if (!session || !safe) return null;
  const step = session.step === null ? undefined : steps[session.step];
  const exit = (outcome: Outcome) => {
    if (state?.version === version) {
      const key = updating ? 'release:' + version : 'welcome';
      const value = state.outcomes[key] === 'completed' ? 'completed' : outcome;
      const next = { ...state, outcomes: { ...state.outcomes, [key]: value } };
      writeState(next);
      setState(next);
    }
    setSession(null);
  };
  const move = (delta: number) => setSession(current => current && current.step !== null
    ? { ...current, step: Math.max(0, Math.min(steps.length - 1, current.step + delta)) } : current);
  const openNotes = (all: boolean) => {
    if (!native) { window.open(all ? RELEASES_PAGE : releaseNotesUrl(version), '_blank', 'noopener'); return; }
    setNotesError(false);
    void api.installedReleaseNotes(all).catch(() => setNotesError(true));
  };
  return <TourDialog guided={!!step} close={() => exit('dismissed')} arrows={move}>
    {step ? <Spotlight step={step} index={session.step!} total={steps.length} go={go} move={move}
      close={() => exit('dismissed')} done={() => exit('completed')}
      keys={{ replay: hotkeyLabel(saved.replay_hotkey), record: hotkeyLabel(saved.recording_hotkey) }} />
      : <div className="tour-intro-body">
        <button type="button" className="tour-x" aria-label="Dismiss" onClick={() => exit('dismissed')}><X size={19} /></button>
        <BrandMark size={50} />
        <h2 id="tour-title">{!updating ? 'Welcome to REPLAYX' : features.length ? "What's new in " + version : 'REPLAYX Updated'}</h2>
        {!updating ? <p>Learn how to record, save the last few moments and find your clips. The tour takes about a minute.</p>
          : <>
            {features.length === 0 && <p>You’re using REPLAYX {version}.</p>}
            {features.length > 0 && <ul className="release-features">{features.map(feature => (
              <li key={feature.id}><strong>{feature.title}</strong><span>{feature.summary}</span></li>
            ))}</ul>}
            {release && (release.improvements.length > 0 || release.fixes.length > 0) && <>
              <h3>What’s improved</h3>
              <ul className="release-fixes">{[...release.improvements, ...release.fixes].map(note => <li key={note}>{note}</li>)}</ul>
            </>}
            {notesError && <p role="alert" className="release-note-error">Could not open release notes. Try All releases.</p>}
            <div className="release-links">
              <button type="button" className="link-btn" onClick={() => openNotes(false)}>Full release notes on GitHub <ExternalLink size={13} /></button>
              <button type="button" className="link-btn" onClick={() => openNotes(true)}>All releases</button>
            </div>
          </>}
        <div className="tour-intro-actions">
          {steps.length > 0 ? <>
            <Button variant="ghost" onClick={() => exit('dismissed')}>{updating ? 'Skip' : 'Maybe later'}</Button>
            <Button variant="primary" data-tour-primary icon={Compass} onClick={() => setSession({ ...session, step: 0 })}>{updating ? 'Show me what’s new' : 'Start tour'}</Button>
          </> : <Button variant="primary" data-tour-primary onClick={() => exit('completed')}>Done</Button>}
        </div>
      </div>}
  </TourDialog>;
}
