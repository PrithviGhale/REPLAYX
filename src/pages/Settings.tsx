import { useEffect, useState } from 'react';
import { Download, FolderOpen, Info, Keyboard, RefreshCw, Settings2, ShieldCheck } from 'lucide-react';
import { api, type Settings, type UpdateInfo } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { Badge, Button, Card, Field, Kbd, Toggle } from '../ui';
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
      <div className="tabs" role="tablist">
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
          <Card title="Startup and window">
            <Toggle label="Start with Windows" checked={draft.launch_at_startup} disabled={active} onChange={v => patch({ launch_at_startup: v })}
              description="Opens REPLAYX in the system tray when you sign in. Capture never starts on its own." />
            <Toggle label="Keep running in the tray when closed" checked={draft.close_to_tray} disabled={active} onChange={v => patch({ close_to_tray: v })}
              description="Capture and hotkeys keep working. Use the tray menu to exit." />
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
