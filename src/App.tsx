import React, { useEffect, useState } from 'react';
import { ErrorNotice } from './ErrorNotice';
import type { Destination } from './errors';
import { NAVIGATE_EVENT, revealAnchor } from './navigation';
import { AudioLines, Gamepad2, Circle, Film, Gauge, Home, LoaderCircle, MonitorPlay, PanelLeftClose, PanelLeftOpen, RotateCcw, Save, Settings2, Square, X } from 'lucide-react';
import { native, VERSION } from './api';
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
import { PerformancePage } from './pages/Performance';
import { SettingsPage, type SettingsTab } from './pages/Settings';
import { UpdatePrompt } from './UpdatePrompt';
import { startUpdates, useUpdate } from './updates';
import { hasUpdate } from './update-logic';
import { QuitPrompt } from './QuitPrompt';
import { BrandMark } from './brand';
import { Tour } from './Tour';

export type Page = 'home' | 'replay' | 'capture' | 'audio' | 'clips' | 'games' | 'performance' | 'settings';
const NAV: { group: string; items: { id: Page; label: string; icon: React.ComponentType<{ size?: number }> }[] }[] = [
  { group: 'Capture', items: [
    { id: 'home', label: 'Home', icon: Home },
    { id: 'replay', label: 'Instant Replay', icon: RotateCcw },
    { id: 'capture', label: 'Video', icon: MonitorPlay },
    { id: 'audio', label: 'Audio', icon: AudioLines },
  ] },
  { group: 'Library', items: [{ id: 'clips', label: 'Clips', icon: Film }, { id: 'games', label: 'Games', icon: Gamepad2 }] },
  { group: 'System', items: [
    { id: 'performance', label: 'Performance', icon: Gauge },
    { id: 'settings', label: 'Settings', icon: Settings2 },
  ] },
];


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

export function App() {
  const [page, setPage] = useState<Page>(() => {
    const hash = window.location.hash.slice(1) as Page;
    return NAV.some(g => g.items.some(i => i.id === hash)) ? hash : 'home';
  });
  const { clips } = useApp();
  // The sidebar folds to an icon rail; the choice is remembered (best effort).
  const [collapsed, setCollapsed] = useState(() => { try { return localStorage.getItem('replayx.sidebar') === 'collapsed'; } catch { return false; } });
  const toggleSidebar = () => setCollapsed(c => {
    try { localStorage.setItem('replayx.sidebar', c ? 'open' : 'collapsed'); } catch { /* not stored */ }
    return !c;
  });
  const pollError = usePollError();
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('general');
  const updateWaiting = hasUpdate(useUpdate());
  useEffect(() => { startPolling(); void startUpdates(); }, []);
  const title = NAV.flatMap(g => g.items).find(i => i.id === page)!.label;
  // Tray "Settings…" and similar open a page by name.
  // "settings:updates" opens a Settings tab (tray "Update available").
  const go = React.useCallback((target: string) => {
    const [p, tab] = target.split(':');
    if (!NAV.some(g => g.items.some(i => i.id === p))) return;
    if (p === 'settings' && ['general', 'hotkeys', 'updates', 'about'].includes(tab)) setSettingsTab(tab as SettingsTab);
    setPage(p as Page);
  }, []);
  // Error actions open the page that fixes the problem, then its section.
  useEffect(() => {
    const open = (e: Event) => {
      const to = (e as CustomEvent<Destination>).detail;
      go(to.tab ? `${to.page}:${to.tab}` : to.page);
      if (to.anchor) revealAnchor(to.anchor);
    };
    window.addEventListener(NAVIGATE_EVENT, open);
    return () => window.removeEventListener(NAVIGATE_EVENT, open);
  }, [go]);
  return (
    <div className={`shell${collapsed ? ' collapsed' : ''}`}>
      <aside className="sidebar">
        <div className="brand"><BrandMark size={28} /><span>REPLAY<b>X</b></span></div>
        <button type="button" className="nav-item sidebar-toggle" aria-expanded={!collapsed} aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'} title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'} onClick={toggleSidebar}>
          {collapsed ? <PanelLeftOpen size={17} /> : <PanelLeftClose size={17} />}<span>Collapse</span>
        </button>
        <nav aria-label="Main">
          {NAV.map(g => (
            <div className="nav-group" key={g.group}>
              <div className="nav-title">{g.group}</div>
              {g.items.map(({ id, label, icon: Icon }) => (
                <button key={id} data-tour={`nav-${id}`} className={`nav-item ${page === id ? 'on' : ''}`} aria-current={page === id ? 'page' : undefined} aria-label={collapsed ? label : undefined} title={collapsed ? label : undefined} onClick={() => setPage(id)}>
                  <Icon size={17} /><span>{label}</span>{id === 'clips' && clips.length > 0 && <small>{clips.length}</small>}
                  {id === 'settings' && updateWaiting && <i className="nav-dot" role="img" aria-label="Update available" title="Update available" />}
                </button>
              ))}
            </div>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div className="local-note"><span className="dot" />Local only · no account</div>
          <div className="version">v{VERSION} <span>alpha</span></div>
        </div>
      </aside>
      <div className="main">
        <header className="topbar">
          <h1>{title}</h1>
          <StatePill />
          <DetectedGame />
          <div className="spacer" />
          <CaptureDock />
        </header>
        {!native && <div className="banner warn">Browser preview — capture controls need the REPLAYX desktop app.</div>}
        {pollError && <div className="banner bad">Lost connection to the recording engine: {pollError}</div>}
        <LockNotice page={page} />
        <main className="content" key={page}>
          <PageBoundary>
          {page === 'home' && <HomePage go={setPage} />}
          {page === 'replay' && <ReplayPage />}
          {page === 'capture' && <CapturePage />}
          {page === 'audio' && <AudioPage />}
          {page === 'clips' && <ClipsPage />}
          {page === 'games' && <GamesPage go={setPage} />}
          {page === 'performance' && <PerformancePage />}
          {page === 'settings' && <SettingsPage tab={settingsTab} setTab={setSettingsTab} />}
          </PageBoundary>
        </main>
        <SaveBar />
      </div>
      <UpdatePrompt suppressed={page === 'settings' && settingsTab === 'updates'} />
      <Toasts />
      <QuitPrompt onNavigate={go} />
      <Tour go={setPage} />
    </div>
  );
}
