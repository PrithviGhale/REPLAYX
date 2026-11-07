import React, { useEffect, useState } from 'react';
import { AudioLines, Circle, Film, Gauge, Home, LoaderCircle, MonitorPlay, RotateCcw, Save, Settings2, Square, SquareStack, X } from 'lucide-react';
import { native, VERSION } from './api';
import { useApp } from './context';
import { useCapture } from './capture';
import { activeOf, startPolling, usePollError, useStatus } from './store';
import { duration, hotkeyLabel } from './format';
import { Button, PageBoundary } from './ui';
import { HomePage } from './pages/Home';
import { ReplayPage } from './pages/Replay';
import { CapturePage } from './pages/Capture';
import { AudioPage } from './pages/Audio';
import { ClipsPage } from './pages/Clips';
import { PerformancePage } from './pages/Performance';
import { SettingsPage } from './pages/Settings';

export type Page = 'home' | 'replay' | 'capture' | 'audio' | 'clips' | 'performance' | 'settings';
const NAV: { group: string; items: { id: Page; label: string; icon: React.ComponentType<{ size?: number }> }[] }[] = [
  { group: 'Capture', items: [
    { id: 'home', label: 'Home', icon: Home },
    { id: 'replay', label: 'Instant Replay', icon: RotateCcw },
    { id: 'capture', label: 'Video', icon: MonitorPlay },
    { id: 'audio', label: 'Audio', icon: AudioLines },
  ] },
  { group: 'Library', items: [{ id: 'clips', label: 'Clips', icon: Film }] },
  { group: 'System', items: [
    { id: 'performance', label: 'Performance', icon: Gauge },
    { id: 'settings', label: 'Settings', icon: Settings2 },
  ] },
];


function StatePill() {
  const s = useStatus(x => ({ state: x.state, recording: x.recording, replay: x.replay, elapsed: x.elapsed, saving: x.saving }));
  const tone = s.state === 'Error' ? 'bad' : s.recording ? 'rec' : s.state === 'Recovering' || s.state === 'Starting' ? 'warn' : s.replay ? 'ok' : 'idle';
  const text = s.recording ? `Recording · ${duration(s.elapsed)}` : s.state === 'Buffering' ? 'Replay on' : s.saving > 0 && !s.replay ? 'Saving' : s.state;
  return <span className={`state-pill ${tone}`} role="status" aria-live="polite"><i />{text}{s.saving > 0 && (s.replay || s.recording) && <em>· saving</em>}</span>;
}

function CaptureDock() {
  const { draft } = useApp();
  const { go, busy, saved } = useCapture();
  const { toast } = useApp();
  const s = useStatus(x => ({ replay: x.replay, recording: x.recording, state: x.state, buffer: x.buffer_seconds }));
  const recovering = s.state === 'Recovering';
  if (recovering) return <div className="dock"><Button variant="danger" icon={X} disabled={!!busy} onClick={() => go('stop_all')}>Cancel recovery</Button></div>;
  return (
    <div className="dock">
      <Button icon={RotateCcw} className={s.replay ? 'on' : ''} disabled={!!busy || !native} aria-pressed={s.replay}
        title={`${s.replay ? 'Stop' : 'Start'} Instant Replay (${hotkeyLabel(draft.pause_hotkey)})`}
        onClick={() => go(s.replay ? 'stop_replay' : 'start_replay')}>{s.replay ? 'Replay on' : 'Enable replay'}</Button>
      <Button variant="primary" icon={Save} disabled={!!busy || !native || !s.replay || s.buffer <= 0}
        title={`Save the last ${duration(draft.replay_seconds)} (${hotkeyLabel(draft.replay_hotkey)})`}
        onClick={async () => { const r = await go('save_replay'); const m = saved(r); if (m) toast('success', `Replay saved — ${m}`); }}>Save replay</Button>
      <Button variant="record" icon={s.recording ? Square : Circle} disabled={!!busy || !native} aria-pressed={s.recording}
        title={`${s.recording ? 'Stop and save' : 'Start'} recording (${hotkeyLabel(draft.recording_hotkey)})`}
        className={s.recording ? 'on' : ''}
        onClick={async () => { const r = await go(s.recording ? 'stop_recording' : 'start_recording'); const m = saved(r); if (m) toast('success', `Recording saved — ${m}`); }}>
        {s.recording ? 'Stop' : 'Record'}
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
        <div key={t.id} className={`toast ${t.kind}`} role={t.kind === 'error' ? 'alert' : 'status'}>
          <span>{t.text}</span><button aria-label="Dismiss" onClick={() => dismiss(t.id)}><X size={14} /></button>
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
  const pollError = usePollError();
  useEffect(() => { startPolling(); }, []);
  const title = NAV.flatMap(g => g.items).find(i => i.id === page)!.label;
  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand"><span className="brand-mark" aria-hidden="true"><SquareStack size={16} /></span><span>REPLAY<b>X</b></span></div>
        <nav aria-label="Main">
          {NAV.map(g => (
            <div className="nav-group" key={g.group}>
              <div className="nav-title">{g.group}</div>
              {g.items.map(({ id, label, icon: Icon }) => (
                <button key={id} className={`nav-item ${page === id ? 'on' : ''}`} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>
                  <Icon size={17} /><span>{label}</span>{id === 'clips' && clips.length > 0 && <small>{clips.length}</small>}
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
          {page === 'performance' && <PerformancePage />}
          {page === 'settings' && <SettingsPage />}
          </PageBoundary>
        </main>
        <SaveBar />
      </div>
      <Toasts />
    </div>
  );
}
