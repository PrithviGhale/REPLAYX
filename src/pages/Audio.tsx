import { useEffect, useMemo, useState } from 'react';
import { AppWindow, Mic, Plus, RefreshCw, Speaker, Trash2, Volume2, VolumeX, Waves } from 'lucide-react';
import { api, errorText, native, type AudioKind, type AudioSource, type Session } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { Badge, Button, Card, Empty, Field, IconButton, Meter, Modal, Segmented, Toggle } from '../ui';
import { Section, SourceIcon, feedIndex, kindLabel } from './shared';
import { MicrophoneCard, SourceTest } from './Microphone';
import { identity, micDefaults } from '../audio';

function AppPicker({ mode, onPick, onClose }: { mode: 'application' | 'exclude'; onPick: (s: Session) => void; onClose: () => void }) {
  const [sessions, setSessions] = useState<Session[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = () => { setError(null); api.sessions().then(setSessions).catch(e => setError(errorText(e))); };
  useEffect(load, []);
  return (
    <Modal title={mode === 'application' ? 'Record an application' : 'Record everything except an application'} onClose={onClose}
      actions={<><Button variant="ghost" icon={RefreshCw} onClick={load}>Refresh</Button><Button onClick={onClose}>Cancel</Button></>}>
      <p className="muted small">Apps that have opened audio on this PC. The app and the processes it starts are included. Requires Windows 10 build 20348 or later.</p>
      {error && <div className="banner bad">{error}</div>}
      {!sessions && !error && <p className="muted">Looking for apps with audio…</p>}
      {sessions?.length === 0 && <Empty icon={AppWindow} title="No apps with audio">Start the app (game, Discord, browser) and refresh.</Empty>}
      <ul className="app-list">
        {sessions?.map(s => (
          <li key={s.pid}><button onClick={() => onPick(s)}>
            <SourceIcon source={{ kind: 'application', muted: false }} path={s.path} />
            <span className="name">{s.name.replace(/\.exe$/i, '')}<small>PID {s.pid}</small></span>
            {s.active ? <Badge tone="ok" dot>Playing</Badge> : <Badge>Idle</Badge>}
          </button></li>
        ))}
      </ul>
    </Modal>
  );
}

export function AudioPage() {
  const { draft, saved, patch, caps, refreshCaps, run, busy, setMix } = useApp();
  const active = useStatus(activeOf);
  const peaks = useStatus(x => x.audio_peaks);
  const stats = useStatus(x => ({ disc: x.audio_discontinuities, drift: x.audio_drift_ppm }));
  const [picker, setPicker] = useState<{ mode: 'application' | 'exclude'; index: number | null } | null>(null);
  const [testing, setTesting] = useState<AudioSource | null>(null);
  const [paths, setPaths] = useState<Record<number, string>>({});
  const sources = draft.audio;
  const liveMix = useMemo(() => saved.audio.length === sources.length && saved.audio.every((a, i) => identity(a) === identity(sources[i])), [saved.audio, sources]);
  const enabled = sources.filter(s => s.enabled);
  const conflict = enabled.some(s => s.kind === 'system' || s.kind === 'exclude') && enabled.some(s => s.kind === 'application');
  const devices = (kind: string) => (caps?.devices ?? []).filter(d => d.kind === (kind === 'microphone' ? 'microphone' : 'system'));

  // Resolve executable paths for application sources so icons can be shown.
  useEffect(() => {
    if (!native || !sources.some(s => s.kind === 'application' || s.kind === 'exclude')) return;
    api.sessions().then(list => {
      const map: Record<number, string> = {};
      list.forEach(s => { map[s.pid] = s.path; });
      setPaths(map);
    }).catch(() => undefined);
  }, [sources]);

  const update = (i: number, p: Partial<AudioSource>) => patch({ audio: sources.map((a, n) => (n === i ? { ...a, ...p } : a)) });
  const add = (kind: AudioKind, extra: Partial<AudioSource> = {}) => patch({
    audio: [...sources, { ...micDefaults(''), kind, name: kindLabel[kind], high_pass: kind === 'microphone', ...extra }],
  });
  const setVolume = (i: number, volume: number, muted: boolean) => (liveMix ? setMix(i, volume, muted) : update(i, { volume, muted }));

  let track = 1;
  return (
    <Section>
      <Card title="Tracks" subtitle="Separate tracks let you balance game, voice chat and microphone later in an editor.">
        <Toggle label="Separate track for each source" checked={draft.separate_tracks} disabled={active} onChange={v => patch({ separate_tracks: v })}
          description="Track 1 is always the combined mix, so normal players still play everything." />
        <Field label="Audio quality (AAC, per track)">
          <Segmented label="Audio bitrate" value={draft.audio_bitrate_kbps} disabled={active} onChange={v => patch({ audio_bitrate_kbps: v })}
            options={[128, 160, 192, 256, 320].map(k => ({ value: k, label: `${k}k` }))} />
        </Field>
        <div className="track-map">
          <span className="track"><b>1</b>Mix of all sources</span>
          {draft.separate_tracks && enabled.map((s, i) => <span className="track" key={i}><b>{i + 2}</b>{s.name}</span>)}
        </div>
      </Card>

      <MicrophoneCard />

      {conflict && <div className="banner warn">System audio already contains every app's sound. Disable it (or the app sources) so audio isn't recorded twice — REPLAYX won't save this combination.</div>}
      {!liveMix && sources.length > 0 && <div className="banner info">Save your source changes to adjust volume live during capture.</div>}

      <Card title="Sources" actions={<Button variant="ghost" icon={RefreshCw} disabled={!!busy} onClick={() => run('Refreshing devices', refreshCaps)}>Devices</Button>}>
        {sources.length === 0 && <Empty icon={Waves} title="No audio sources">Add your system sound, a microphone, or individual apps.</Empty>}
        <ul className="sources">
          {sources.map((a, i) => {
            const fi = feedIndex(sources, i);
            const thisTrack = a.enabled && draft.separate_tracks ? ++track : null;
            const meterOn = active && a.enabled && !a.muted && liveMix;
            return (
              <li key={i} className={`source ${a.enabled ? '' : 'off'}`}>
                <div className="source-head">
                  <SourceIcon source={a} path={paths[a.pid]} />
                  <input className="source-name" aria-label="Source name" value={a.name} disabled={active} onChange={e => update(i, { name: e.target.value })} />
                  <Badge>{kindLabel[a.kind]}</Badge>
                  {thisTrack && <Badge tone="accent">Track {thisTrack}</Badge>}
                  <div className="spacer" />
                  <button role="switch" aria-checked={a.enabled} aria-label={`Record ${a.name}`} className={`switch ${a.enabled ? 'on' : ''}`} disabled={active}
                    onClick={() => update(i, { enabled: !a.enabled })}><span /></button>
                  <IconButton label="Remove source" icon={Trash2} disabled={active} onClick={() => patch({ audio: sources.filter((_, n) => n !== i) })} />
                </div>
                <div className="source-body">
                  {a.kind === 'system' || a.kind === 'microphone' ? (
                    <select aria-label="Device" value={a.device} disabled={active} onChange={e => update(i, { device: e.target.value })}>
                      <option value="">Windows default {a.kind === 'microphone' ? 'microphone' : 'output'}</option>
                      {devices(a.kind).map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
                    </select>
                  ) : (
                    <div className="app-binding">
                      <span>{a.pid ? `${a.name} · PID ${a.pid}` : 'No app selected'}</span>
                      <Button size="sm" variant="ghost" disabled={active} onClick={() => setPicker({ mode: a.kind === 'exclude' ? 'exclude' : 'application', index: i })}>Change</Button>
                    </div>
                  )}
                  <div className="mix-row">
                    <IconButton label={a.muted ? 'Unmute' : 'Mute'} icon={a.muted ? VolumeX : Volume2} className={a.muted ? 'muted' : ''}
                      onClick={() => setVolume(i, a.volume, !a.muted)} />
                    <input type="range" min={0} max={200} step={1} value={Math.round(a.volume * 100)} aria-label={`${a.name} volume`}
                      onChange={e => setVolume(i, Number(e.target.value) / 100, a.muted)} />
                    <span className="range-value">{Math.round(a.volume * 100)}%</span>
                    <Meter peak={meterOn ? peaks[fi] ?? 0 : 0} active={meterOn} />
                    <Button size="sm" variant="ghost" disabled={active || (a.kind !== 'system' && a.kind !== 'microphone' && !a.pid)} onClick={() => setTesting(a)}>Test</Button>
                  </div>
                  {active && a.enabled && liveMix && (stats.disc[fi] ?? 0) > 0 && <small className="warn-text">{stats.disc[fi]} audio discontinuities this session</small>}
                </div>
              </li>
            );
          })}
        </ul>
        <div className="add-row">
          <Button icon={Speaker} disabled={active || sources.length >= 5} onClick={() => add('system')}>System output</Button>
          <Button icon={Mic} disabled={active || sources.length >= 5} onClick={() => add('microphone')}>Microphone</Button>
          <Button icon={AppWindow} disabled={active || sources.length >= 5 || !native} onClick={() => setPicker({ mode: 'application', index: null })}>Application…</Button>
          <Button icon={Plus} variant="ghost" disabled={active || sources.length >= 5 || !native} onClick={() => setPicker({ mode: 'exclude', index: null })}>Everything except an app…</Button>
        </div>
        <p className="muted small">Up to five sources. Typical setup for games with voice chat: the game app, Discord, and your microphone — with System output off.</p>
      </Card>

      {picker && <AppPicker mode={picker.mode} onClose={() => setPicker(null)} onPick={s => {
        const name = s.name.replace(/\.exe$/i, '');
        const extra = { pid: s.pid, process_started: s.started, name: picker.mode === 'exclude' ? `All except ${name}` : name };
        if (picker.index !== null) update(picker.index, { ...extra, kind: picker.mode }); else add(picker.mode, extra);
        setPaths(p => ({ ...p, [s.pid]: s.path }));
        setPicker(null);
      }} />}
      {testing && <SourceTest source={testing} onClose={() => setTesting(null)} />}
    </Section>
  );
}
