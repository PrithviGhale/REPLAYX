import { useEffect, useState } from 'react';
import { Compass, Download, FolderOpen, Info, Keyboard, RefreshCw, Settings2, ShieldCheck } from 'lucide-react';
import { startTour } from '../Tour';
import { api, type Settings, type UpdateInfo } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { Badge, Button, Card, Field, Kbd, Segmented, Toggle } from '../ui';
import type { OverlayPosition, OverlaySettings } from '../api';
import { Section } from './shared';
import { VERSION } from '../api';

type Tab = 'general' | 'hotkeys' | 'updates' | 'about';
type HotkeyKey = 'replay_hotkey' | 'recording_hotkey' | 'pause_hotkey' | 'mute_hotkey';
const HOTKEYS: { key: HotkeyKey; label: string; text: string }[] = [
  { key: 'replay_hotkey', label: 'Save instant replay', text: 'Saves the last replay window to Clips.' },
  { key: 'recording_hotkey', label: 'Start / stop recording', text: 'Stopping saves and verifies the recording.' },
  { key: 'pause_hotkey', label: 'Turn Instant Replay on / off', text: 'Starts or stops the rolling buffer.' },
  { key: 'mute_hotkey', label: 'Mute / unmute microphones', text: 'Toggles every microphone source.' },
];
const MODS = new Set(['Control', 'Shift', 'Alt', 'Meta']);

/** Build a global-shortcut string ("Control+Shift+F10") from a key event. */
export function comboFrom(e: { ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean; key: string; code: string }) {
  if (MODS.has(e.key)) return null;
  const mods = [e.ctrlKey && 'Control', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[];
  const code = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.code;
  const isF = /^F\d{1,2}$/.test(code);
  if (!mods.length && !isF) return { error: 'Add Ctrl, Alt or Shift so normal typing is not captured.' };
  return { combo: [...mods, code].join('+') };
}

function HotkeyRow({ item, value, onChange, disabled, duplicate }: { item: typeof HOTKEYS[number]; value: string; onChange: (v: string) => void; disabled: boolean; duplicate: boolean }) {
  const [listening, setListening] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  useEffect(() => {
    if (!listening) return;
    const key = (e: KeyboardEvent) => {
      e.preventDefault(); e.stopPropagation();
      if (e.key === 'Escape') { setListening(false); setHint(null); return; }
      const r = comboFrom(e);
      if (!r) return;
      if ('error' in r) { setHint(r.error ?? null); return; }
      onChange(r.combo); setListening(false); setHint(null);
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [listening, onChange]);
  return (
    <div className={`hotkey-row ${duplicate ? 'bad' : ''}`}>
      <div><strong>{item.label}</strong><small>{hint ?? (duplicate ? 'Used by another action.' : item.text)}</small></div>
      {listening ? <span className="listening">Press keys… <small>Esc to cancel</small></span> : <Kbd combo={value} />}
      <Button size="sm" variant="ghost" disabled={disabled} onClick={() => setListening(l => !l)}>{listening ? 'Cancel' : 'Change'}</Button>
    </div>
  );
}

export function SettingsPage() {
  const { draft, patch, caps, run, busy } = useApp();
  const active = useStatus(activeOf);
  const hotkeyError = useStatus(x => x.hotkey_error);
  const [tab, setTab] = useState<Tab>('general');
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const counts = HOTKEYS.reduce<Record<string, number>>((m, h) => { const k = draft[h.key].toLowerCase(); m[k] = (m[k] ?? 0) + 1; return m; }, {});
  const tabs: { id: Tab; label: string; icon: typeof Settings2 }[] = [
    { id: 'general', label: 'General', icon: Settings2 }, { id: 'hotkeys', label: 'Hotkeys', icon: Keyboard },
    { id: 'updates', label: 'Updates', icon: Download }, { id: 'about', label: 'About', icon: Info },
  ];

  return (
    <Section>
      <div className="tabs" role="tablist" data-tour="settings-tabs">
        {tabs.map(t => <button key={t.id} role="tab" aria-selected={tab === t.id} className={tab === t.id ? 'on' : ''} onClick={() => setTab(t.id)}><t.icon size={15} />{t.label}</button>)}
      </div>

      {tab === 'general' && (
        <>
          <Card title="Saving">
            <Field label="Clips folder" hint="Must be a full local path. Clips never leave this PC.">
              <div className="input-action">
                <input value={draft.output_dir} disabled={active} onChange={e => patch({ output_dir: e.target.value })} spellCheck={false} />
                <Button icon={FolderOpen} onClick={() => run('Opening folder', api.openFolder)}>Open</Button>
              </div>
            </Field>
            <p className="muted small">Saved as Matroska (.mkv), which survives crashes and keeps every audio track. Export any clip to MP4 from the library.</p>
          </Card>
          <Card tour="game-detection" title="Game detection" subtitle="REPLAYX looks at which program is in front (process name only; nothing is read from games).">
            <Toggle label="Organize clips by game" checked={draft.organize_by_game} disabled={active} onChange={v => patch({ organize_by_game: v })}
              description="Saves into a folder per game, e.g. Fortnite\Fortnite_2026-11-07_15-30-22.mkv. Desktop activity goes to Desktop. Existing clips are not moved." />
            <Toggle label="Show a notice when a game is detected" checked={draft.game_notifications} disabled={active} onChange={v => patch({ game_notifications: v })}
              description="A small bar in the top-right corner for about 4 seconds, once per game: for example Roblox detected · Instant Replay ready. Shown over windowed and borderless games; Windows hides it behind exclusive full-screen games. While capture is off, REPLAYX checks the program in front once a second for this notice only." />
            <div className="mappings">
              <span className="field-label">Your corrections</span>
              {draft.game_mappings.length === 0 && <p className="muted small">None. Add one if a game isn't recognised, or to stop an app being treated as a game.</p>}
              {draft.game_mappings.map((m, i) => (
                <div className="mapping" key={i}>
                  <input aria-label="Executable" value={m.exe} disabled={active} placeholder="game.exe"
                    onChange={e => patch({ game_mappings: draft.game_mappings.map((x, n) => n === i ? { ...x, exe: e.target.value.trim() } : x) })} />
                  <span className="muted">→</span>
                  <input aria-label="Game name" value={m.name} disabled={active} placeholder="Game name (empty = not a game)"
                    onChange={e => patch({ game_mappings: draft.game_mappings.map((x, n) => n === i ? { ...x, name: e.target.value } : x) })} />
                  <Button size="sm" variant="ghost" disabled={active} onClick={() => patch({ game_mappings: draft.game_mappings.filter((_, n) => n !== i) })}>Remove</Button>
                </div>
              ))}
              <Button size="sm" disabled={active || draft.game_mappings.length >= 200} onClick={() => patch({ game_mappings: [...draft.game_mappings, { exe: '', name: '' }] })}>Add correction</Button>
            </div>
          </Card>
          <OverlayCard />
          <Card tour="tray" title="Startup and window">
            <Toggle label="Start with Windows" checked={draft.launch_at_startup} disabled={active} onChange={v => patch({ launch_at_startup: v })}
              description="Opens REPLAYX in the system tray when you sign in. Capture never starts on its own." />
            <Toggle label="Minimize to tray when closing" checked={draft.close_to_tray} disabled={active} onChange={v => patch({ close_to_tray: v })}
              description="Closing the window keeps REPLAYX running in the tray: Instant Replay, recording and hotkeys keep working. Quit from the tray menu; REPLAYX asks first if a recording or save is in progress." />
          </Card>
          <Card title="Getting started" subtitle="A one-minute guided tour of the main features. It only shows things; it never starts recording.">
            <Button icon={Compass} onClick={startTour}>Replay tutorial</Button>
          </Card>
        </>
      )}

      {tab === 'hotkeys' && (
        <Card title="Global hotkeys" subtitle="Work in games and on the desktop. Applied when you save.">
          {hotkeyError && <div className="banner warn">{hotkeyError}</div>}
          {HOTKEYS.map(h => <HotkeyRow key={h.key} item={h} value={draft[h.key]} disabled={active} duplicate={counts[draft[h.key].toLowerCase()] > 1}
            onChange={v => patch({ [h.key]: v } as Partial<Settings>)} />)}
          <p className="muted small">If another app (for example GeForce Experience) already owns a shortcut, REPLAYX reports it here after saving; the on-screen buttons keep working.</p>
        </Card>
      )}

      {tab === 'updates' && (
        <Card title="Updates" subtitle="Optional. Checks GitHub Releases only when you ask, or at startup if enabled.">
          {!caps?.updates_configured && <div className="banner info">This build has no signed update channel configured, so update checks are unavailable. Download new versions from the project's GitHub Releases page.</div>}
          <Toggle label="Check for updates at startup" checked={draft.check_updates} disabled={!caps?.updates_configured || active} onChange={v => patch({ check_updates: v })}
            description="A single request to GitHub. No usage data is sent." />
          <div className="row">
            <Button icon={RefreshCw} disabled={!caps?.updates_configured || !!busy} onClick={() => run('Checking for updates', async () => setUpdate(await api.checkUpdate()))}>Check now</Button>
            <span className="muted small">Current version {VERSION}</span>
          </div>
          {update && (update.available ? (
            <div className="update">
              <strong>Version {update.version} is available</strong>
              {update.notes && <pre className="notes">{update.notes}</pre>}
              <div className="row">
                <Button variant="primary" icon={Download} disabled={active || !!busy} onClick={() => run('Installing update', api.installUpdate)}>Install and restart</Button>
                <Button variant="ghost" onClick={() => setUpdate(null)}>Later</Button>
              </div>
              {active && <small className="muted">Stop capture first — updates never interrupt a recording.</small>}
            </div>
          ) : <div className="banner ok">You're on the latest version.</div>)}
          <p className="muted small">Updates are verified with the publisher's signature before installing. Settings and clips are kept.</p>
        </Card>
      )}

      {tab === 'about' && (
        <Card className="about">
          <div className="about-brand"><span className="brand-mark big" /><div><h2>REPLAY<b>X</b></h2><p>Capture everything. Miss nothing.</p></div><Badge tone="warn">Alpha · v{VERSION}</Badge></div>
          <ul className="principles">
            <li><ShieldCheck size={16} />Free and open source. No accounts, ads, subscriptions or telemetry.</li>
            <li><ShieldCheck size={16} />Everything stays on this PC. Capture only runs when you turn it on, and is always shown.</li>
            <li><ShieldCheck size={16} />Protected content, secure desktops and anti-cheat restrictions are respected, never bypassed.</li>
          </ul>
          <p className="muted small">REPLAYX is MIT licensed. Video and audio are processed with FFmpeg (LGPL build, shipped as separate programs). See THIRD_PARTY_NOTICES.md and PRIVACY.md in the install folder.</p>
        </Card>
      )}
    </Section>
  );
}

const CORNERS: { value: OverlayPosition; label: string }[] = [
  { value: 'top-left', label: 'Top left' }, { value: 'top-right', label: 'Top right' },
  { value: 'bottom-left', label: 'Bottom left' }, { value: 'bottom-right', label: 'Bottom right' },
];

/** The overlay's icons as SVG (same shapes the native overlay draws). */
export function OverlayIcon({ icon, color, slash }: { icon: 'replay' | 'mic' | 'rec'; color: string; slash?: boolean }) {
  return (
    <svg viewBox="0 0 32 32" className="ov-tile" aria-hidden="true">
      <rect x="0.5" y="0.5" width="31" height="31" rx="8" className="ov-tile-bg" />
      <g fill={color} stroke={color}>
        {icon === 'replay' && <>
          <path d="M 22.4 10.2 A 8.6 8.6 0 1 1 14.5 7.5" fill="none" strokeWidth="2.2" />
          <path d="M 25.3 8.2 L 20.0 7.7 L 24.6 12.8 Z" strokeWidth="0.4" transform="rotate(-8 22.4 10.2)" />
          <path d="M 14 12.6 L 14 19.4 L 19.6 16 Z" strokeWidth="0.4" strokeLinejoin="round" />
        </>}
        {icon === 'mic' && <>
          <rect x="12.7" y="6.9" width="6.6" height="11.6" rx="3.3" stroke="none" />
          <path d="M 9.9 15.2 A 6.1 6.1 0 0 0 22.1 15.2" fill="none" strokeWidth="1.9" />
          <path d="M 16 21.3 V 24.2 M 12.6 24.2 H 19.4" fill="none" strokeWidth="1.9" strokeLinecap="round" />
        </>}
        {icon === 'rec' && <>
          <circle cx="16" cy="16" r="6" stroke="none" />
          <circle cx="16" cy="16" r="9" fill="none" strokeWidth="1.2" opacity="0.45" />
        </>}
        {slash && <>
          <path d="M 9.5 9.5 L 22.5 22.5" stroke="rgb(14,14,21)" strokeWidth="4.6" />
          <path d="M 9.5 9.5 L 22.5 22.5" strokeWidth="2.1" strokeLinecap="round" />
        </>}
      </g>
    </svg>
  );
}

/** Status overlay preferences, with a live preview of the icon stack. */
function OverlayCard() {
  const { draft, patch } = useApp();
  const active = useStatus(activeOf);
  const o = draft.overlay;
  const set = (p: Partial<OverlaySettings>) => patch({ overlay: { ...o, ...p } });
  const stack = [
    o.show_replay && <OverlayIcon key="r" icon="replay" color="#a092ff" />,
    o.show_mic && <OverlayIcon key="m" icon="mic" color="#3ed6a0" />,
    <OverlayIcon key="c" icon="rec" color="#ff4d5e" />,
  ].filter(Boolean);
  const bottom = o.position.startsWith('bottom');
  return (
    <Card tour="overlay" title="Status overlay" subtitle="Small icons in a corner of your screen, over every app, game and the desktop: Instant Replay, microphone, and a red dot while recording. Click-through, and never in your clips.">
      <Toggle label="Show the status overlay" checked={o.enabled} disabled={active} onChange={v => set({ enabled: v })}
        description="Visible to you while you record, invisible in the saved video. Windows hides desktop windows behind exclusive full-screen games." />
      <div className={`ov-settings ${o.enabled ? '' : 'off'}`}>
        <div className="ov-preview" aria-hidden="true">
          <div className={`ov-screen ${o.position}`}>
            <div className={`ov-stack ${bottom ? 'up' : ''}`} style={{ opacity: o.opacity / 100 }}>{stack}</div>
          </div>
          <div className="ov-legend">
            <span><OverlayIcon icon="replay" color="#a092ff" />Replay on</span>
            <span><OverlayIcon icon="replay" color="#808092" slash />Replay off</span>
            <span><OverlayIcon icon="mic" color="#3ed6a0" />Mic live</span>
            <span><OverlayIcon icon="mic" color="#f5ba54" slash />Muted</span>
            <span><OverlayIcon icon="rec" color="#ff4d5e" />Recording</span>
          </div>
        </div>
        <div className="ov-options">
          <Field label="Corner">
            <Segmented label="Overlay position" value={o.position} disabled={active || !o.enabled} onChange={v => set({ position: v })} options={CORNERS} />
          </Field>
          <Field label={`Opacity · ${o.opacity}%`}>
            <input type="range" min={30} max={100} step={5} value={o.opacity} disabled={active || !o.enabled} aria-label="Overlay opacity" onChange={e => set({ opacity: Number(e.target.value) })} />
          </Field>
          <Toggle label="Instant Replay icon" checked={o.show_replay} disabled={active || !o.enabled} onChange={v => set({ show_replay: v })} />
          <Toggle label="Microphone icon" checked={o.show_mic} disabled={active || !o.enabled} onChange={v => set({ show_mic: v })}
            description="The red recording dot always appears while a recording is being written." />
        </div>
      </div>
    </Card>
  );
}
