import React, { useEffect, useState } from 'react';
import { ErrorNotice } from './ErrorNotice';
import type { Destination } from './errors';
import { NAVIGATE_EVENT, revealAnchor } from './navigation';
import { AudioLines, Crosshair, Gamepad2, Circle, Film, Gauge, Home, LoaderCircle, MonitorPlay, RotateCcw, Save, Settings2, Square, X, CircleHelp } from 'lucide-react';
import { native, VERSION, VERSION_STAGE } from './api';
import { useApp } from './context';
import { useCapture, useRecordButton } from './capture';
import { ReplaySwitch } from './ReplaySwitch';
import { activeOf, startPolling, usePollError, useStatus } from './store';
import { duration, hotkeyLabel } from './format';
import { Button, PageBoundary } from './ui';
import { HomePage } from './pages/Home';
import { ReplayPage } from './pages/Replay';
import { CapturePage } from './pages/Capture';
import { AudioPage } from './pages/Audio';
import { ClipsPage } from './pages/Clips';
import { GamesPage } from './pages/Games';
import { CrosshairPage } from './pages/Crosshair';
import { PerformancePage } from './pages/Performance';
import { SettingsPage } from './pages/Settings';
import { UpdatePrompt } from './UpdatePrompt';
import { startUpdates, useUpdate } from './updates';
import { hasUpdate } from './update-logic';
import { QuitPrompt } from './QuitPrompt';
import { BrandMark } from './brand';
import { Tour, startTour } from './Tour';
import type { Release } from './release-tour';
import { DEFAULT_ROUTE, PAGES, WORKSPACES, parseRoute, routeHash, type Page } from './workspace';

export type { Page } from './workspace';
const icons = { home: Home, replay: RotateCcw, capture: MonitorPlay, audio: AudioLines, clips: Film, games: Gamepad2, crosshair: Crosshair, performance: Gauge, settings: Settings2 };
const subtitles: Record<Page, string> = {
  home: 'Monitor capture, audio and recent clips.',
  replay: 'Set how much footage Instant Replay keeps and recover interrupted sessions.',
  capture: 'Choose a display, recording engine and video quality.',
  audio: 'Choose your inputs and tune the tracks in your recordings.',
  clips: 'Play, organize and export your saved footage.',
  games: 'Find game clips and manage playtime tracking.',
  crosshair: 'Build and position a crosshair for supported windowed games.',
  performance: 'Inspect capture health, encoding and resource use.',
  settings: 'Manage your shortcuts, overlays, updates and app preferences.',
};

function StatePill() {
  const s = useStatus(x => ({ state: x.state, recording: x.recording, replay: x.replay, elapsed: x.recording_elapsed, saving: x.saving, pending: x.pending_recording }));
  // "Recording" only once the encoder is writing frames; before that, Starting.
  const starting = s.pending || s.state === 'Starting';
  const tone = s.state === 'Error' ? 'bad' : starting || s.state === 'Recovering' || s.state === 'Stopping' ? 'warn' : s.recording ? 'rec' : s.replay ? 'ok' : 'idle';
  const text = starting ? (s.pending || s.recording ? 'Starting recording…' : 'Starting…') : s.recording ? `Recording · ${duration(s.elapsed)}` : s.state === 'Buffering' ? 'Replay on' : s.saving > 0 && !s.replay ? 'Saving' : s.state;
  return <span className={`state-pill ${tone}`} role="status" aria-live="polite"><i />{text}{s.saving > 0 && (s.replay || s.recording) && <em>· saving</em>}</span>;
}

/** Game REPLAYX currently attributes footage to (only while capturing). */
function DetectedGame() {
  const s = useStatus(x => ({ game: x.detected_game, on: x.replay || x.recording }));
  if (!s.on || !s.game) return null;
  return <span className="detected" title="Detected from the foreground app; clips are filed under this game">{s.game}</span>;
}

function CaptureDock() {
  const { draft } = useApp();
  const { go, busy, saved } = useCapture();
  const { toast } = useApp();
  const rec = useRecordButton();
  const s = useStatus(x => ({ replay: x.replay, recording: x.recording, state: x.state, buffer: x.buffer_seconds }));
  const recovering = s.state === 'Recovering';
  if (recovering) return <div className="dock"><Button variant="danger" icon={X} disabled={!!busy} onClick={() => go('stop_all')}>Cancel recovery</Button></div>;
  return (
    <div className="dock">
      <span data-tour="replay-switch"><ReplaySwitch /></span>
      <Button variant="primary" icon={Save} data-tour="save-replay" disabled={!!busy || !native || !s.replay || s.buffer <= 0}
        title={`Save the last ${duration(draft.replay_seconds)} (${hotkeyLabel(draft.replay_hotkey)})`}
        onClick={async () => { const r = await go('save_replay'); const m = saved(r); if (m) toast('success', `Replay saved — ${m}`); }}>Save replay</Button>
      <Button variant="record" data-tour="record" icon={rec.mode === 'record' ? Circle : rec.mode === 'cancel' ? X : Square} disabled={rec.disabled || !native} aria-pressed={rec.mode !== 'record'}
        title={rec.mode === 'cancel' ? 'Cancel: capture is still starting, nothing has been recorded' : `${rec.mode === 'stop' ? 'Stop and save' : 'Start'} recording (${hotkeyLabel(draft.recording_hotkey)})`}
        className={rec.mode !== 'record' ? 'on' : ''}
        onClick={rec.onClick}>
        {rec.label}
      </Button>
    </div>
  );
}

/** Explains why settings are disabled: the engine only accepts changes while stopped. */
function LockNotice({ page }: { page: Page }) {
  const active = useStatus(activeOf);
  if (!active || !['replay', 'capture', 'audio', 'settings'].includes(page)) return null;
  return (
    <div className="banner info">
      {page === 'audio' ? 'Capture is running — volume and mute still apply instantly; other changes need capture stopped.' : 'Capture is running. Stop replay and recording to change these settings.'}
    </div>
  );
}

function SaveBar() {
  const { dirty, save, discard, busy } = useApp();
  const active = useStatus(activeOf);
  if (!dirty) return null;
  return (
    <div className="savebar" role="region" aria-label="Unsaved changes">
      <span>{active ? 'Stop capture to apply these changes.' : 'You have unsaved changes.'}</span>
      <Button variant="ghost" onClick={discard} disabled={!!busy}>Discard</Button>
      <Button variant="primary" icon={Save} onClick={() => save()} disabled={!!busy || active}>Save changes</Button>
    </div>
  );
}

function Toasts() {
  const { toasts, dismiss, busy } = useApp();
  return (
    <div className="toasts" aria-live="polite">
      {busy && <div className="toast info"><LoaderCircle className="spin" size={15} /><span>{busy}…</span></div>}
      {toasts.map(t => (
        t.error
          ? <ErrorNotice key={t.id} error={t.error} count={t.count} onDismiss={() => dismiss(t.id)} onAction={ok => { if (ok) dismiss(t.id); }} />
          : <div key={t.id} className={`toast ${t.kind}`} role="status">
              <span>{t.text}{t.count > 1 && ` (×${t.count})`}</span><button aria-label="Dismiss" onClick={() => dismiss(t.id)}><X size={14} /></button>
            </div>
      ))}
    </div>
  );
}

export function App({ tourFixture }: { tourFixture?: Release }) {
  const [route, setRoute] = useState(() => parseRoute(window.location.hash) ?? DEFAULT_ROUTE);
  const { page, tab: settingsTab } = route;
  const clipRequested = React.useRef<string | null>(null);
  const { clips } = useApp();
  const pollError = usePollError();
  const updateWaiting = hasUpdate(useUpdate());
  const group = WORKSPACES.find(workspace => workspace.pages.some(item => item.id === page)) ?? WORKSPACES[0];
  const title = PAGES.find(item => item.id === page)?.label;

  useEffect(() => { startPolling(); void startUpdates(); }, []);
  useEffect(() => {
    const read = () => setRoute(parseRoute(window.location.hash) ?? DEFAULT_ROUTE);
    window.addEventListener('popstate', read);
    window.addEventListener('hashchange', read);
    return () => {
      window.removeEventListener('popstate', read);
      window.removeEventListener('hashchange', read);
    };
  }, []);

  const go = React.useCallback((target: string, replace = false) => {
    const next = parseRoute(target);
    if (!next) return;
    const hash = routeHash(next);
    if (hash !== routeHash(parseRoute(window.location.hash) ?? DEFAULT_ROUTE)) {
      if (replace) window.history.replaceState(null, '', hash);
      else window.history.pushState(null, '', hash);
    }
    setRoute(current => routeHash(current) === hash ? current : next);
  }, []);
  const tourNavigate = React.useCallback((target: Page) => go(target, true), [go]);
  useEffect(() => {
    const open = (event: Event) => {
      const to = (event as CustomEvent<Destination>).detail;
      go(to.tab ? to.page + ':' + to.tab : to.page);
      if (to.anchor) revealAnchor(to.anchor);
    };
    window.addEventListener(NAVIGATE_EVENT, open);
    return () => window.removeEventListener(NAVIGATE_EVENT, open);
  }, [go]);

  return (
    <div className="shell workspace-shell">
      <a className="skip-content" href="#workspace-content" onClick={event => {
        event.preventDefault();
        document.getElementById('workspace-content')?.focus();
      }}>Skip to content</a>
      <header className="workspace-header">
        <button className="workspace-brand" aria-label="REPLAYX overview" onClick={() => go('home')}>
          <BrandMark size={31} /><span>REPLAY<b>X</b></span>
        </button>
        <nav className="workspace-nav" aria-label="Workspaces">
          {WORKSPACES.map(workspace => (
            <button type="button" key={workspace.title} onClick={() => go(workspace.pages[0].id)}
              aria-current={workspace === group ? 'true' : undefined}>
              {workspace.title}
              {workspace.title === 'Settings' && updateWaiting && <i className="nav-dot" role="img" aria-label="Update available" title="Update available" />}
            </button>
          ))}
        </nav>
        <button className="workspace-help" type="button" title="Replay tutorial" aria-label="Replay tutorial" onClick={startTour}><CircleHelp size={19} /></button>
      </header>
      <nav className="workspace-pages" aria-label={group.title + ' pages'}>
        {group.pages.map(({ id, label }) => {
          const Icon = icons[id];
          return <button key={id} type="button" className="workspace-page-link" data-tour={'nav-' + id}
            aria-current={page === id ? 'page' : undefined} onClick={() => go(id)}>
            <Icon size={16} /><span>{label}</span>{id === 'clips' && clips.length > 0 && <small>{clips.length}</small>}
          </button>;
        })}
      </nav>
      <div className="main workspace-main">
        {!native && <div className="banner warn">Browser preview · Capture controls require the desktop app.</div>}
        {pollError && <div className="banner bad" role="alert">Lost connection to the recording engine: {pollError}</div>}
        <LockNotice page={page} />
        <main className="content workspace-content" id="workspace-content" data-page={page} tabIndex={-1} key={page}>
          <div className="workspace-inner">
            <div className="workspace-heading">
              <h1>{title}</h1>
              <p>{subtitles[page]}</p>
            </div>
            <PageBoundary>
              {page === 'home' && <HomePage go={go} openClip={id => { clipRequested.current = id; go('clips'); }} />}
              {page === 'replay' && <ReplayPage />}
              {page === 'capture' && <CapturePage />}
              {page === 'audio' && <AudioPage />}
              {page === 'clips' && <ClipsPage requestedClip={clipRequested} />}
              {page === 'games' && <GamesPage go={go} />}
              {page === 'crosshair' && <CrosshairPage />}
              {page === 'performance' && <PerformancePage />}
              {page === 'settings' && <SettingsPage tab={settingsTab} setTab={tab => go('settings:' + tab)} />}
            </PageBoundary>
          </div>
        </main>
        <SaveBar />
      </div>
      <footer className="transport">
        <div className="transport-status"><StatePill /><DetectedGame /></div>
        <CaptureDock />
        <div className="transport-version" title={'REPLAYX ' + VERSION}>v{VERSION}{VERSION_STAGE && <span>{VERSION_STAGE}</span>}</div>
      </footer>
      <UpdatePrompt suppressed={page === 'settings' && settingsTab === 'updates'} />
      <Toasts />
      <QuitPrompt onNavigate={go} />
      <Tour go={tourNavigate} fixture={tourFixture} />
    </div>
  );
}
