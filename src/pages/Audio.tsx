import { useEffect, useMemo, useState } from 'react';
import { AppWindow, Plus, RefreshCw, Speaker, Trash2, Volume2, VolumeX, Waves } from 'lucide-react';
import { api, errorText, native, type AudioKind, type AudioSource, type Session } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { Badge, Button, Card, Empty, Field, IconButton, Meter, Modal, Segmented, Toggle } from '../ui';
import { Section, SourceIcon, feedIndex, kindLabel } from './shared';
import { MicrophoneCard } from './Microphone';
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
  const room = sources.length < 5;
  return (
    <Section>
      <MicrophoneCard />

      {conflict && <div className="banner warn">System audio already contains every app's sound. Turn it off (or the app sources) so audio isn't recorded twice. REPLAYX won't save this combination.</div>}
      {!liveMix && sources.length > 0 && <div className="banner info">Save your changes to adjust volume live during capture.</div>}

      <Card title="Sound sources" subtitle="What else your clips hear. Each source gets its own track."
        actions={<Button variant="ghost" size="sm" icon={RefreshCw} disabled={!!busy} onClick={() => run('Refreshing devices', refreshCaps)}>Refresh</Button>}>
        {sources.every(s => s.kind === 'microphone') && <Empty icon={Waves} title="No other sources">Add your system sound, or individual apps such as the game and Discord.</Empty>}
        <ul className="strips">
          {sources.map((a, i) => {
            const fi = feedIndex(sources, i);
            const thisTrack = a.enabled && draft.separate_tracks ? ++track : null;
            // The microphone has its own card above (it is always present).
            if (a.kind === 'microphone') return null;
            const meterOn = active && a.enabled && !a.muted && liveMix;
            return (
              <li key={i} className={`strip ${a.enabled ? '' : 'off'}`}>
                <SourceIcon source={a} path={paths[a.pid]} />
                <div className="strip-main">
                  <div className="strip-title">
                    <input className="source-name" aria-label="Source name" value={a.name} disabled={active} onChange={e => update(i, { name: e.target.value })} />
                    {thisTrack && <Badge tone="accent">Track {thisTrack}</Badge>}
                  </div>
                  {a.kind === 'system' ? (
                    <select className="strip-select" aria-label="Device" value={a.device} disabled={active} onChange={e => update(i, { device: e.target.value })}>
                      <option value="">Windows default output</option>
                      {devices(a.kind).map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
                    </select>
                  ) : (
                    <div className="strip-app">
                      <span>{kindLabel[a.kind]} · {a.pid ? `PID ${a.pid}` : 'no app selected'}</span>
                      <button className="link" disabled={active} onClick={() => setPicker({ mode: a.kind === 'exclude' ? 'exclude' : 'application', index: i })}>Change</button>
                    </div>
                  )}
                </div>
                <div className="strip-mix">
                  <IconButton label={a.muted ? 'Unmute' : 'Mute'} icon={a.muted ? VolumeX : Volume2} className={a.muted ? 'muted' : ''}
                    onClick={() => setVolume(i, a.volume, !a.muted)} />
                  <input type="range" min={0} max={200} step={1} value={Math.round(a.volume * 100)} aria-label={`${a.name} volume`}
                    onChange={e => setVolume(i, Number(e.target.value) / 100, a.muted)} />
                  <span className="range-value">{Math.round(a.volume * 100)}%</span>
                  <Meter peak={meterOn ? peaks[fi] ?? 0 : 0} active={meterOn} />
                </div>
                <div className="strip-end">
                  <button role="switch" aria-checked={a.enabled} aria-label={`Record ${a.name}`} className={`switch ${a.enabled ? 'on' : ''}`} disabled={active}
                    onClick={() => update(i, { enabled: !a.enabled })}><span /></button>
                  <IconButton label="Remove source" icon={Trash2} disabled={active} onClick={() => patch({ audio: sources.filter((_, n) => n !== i) })} />
                </div>
                {active && a.enabled && liveMix && (stats.disc[fi] ?? 0) > 0 && <small className="warn-text strip-warn">{stats.disc[fi]} audio discontinuities this session</small>}
              </li>
            );
          })}
        </ul>
        <div className="add-row">
          <Button size="sm" icon={Speaker} disabled={active || !room} onClick={() => add('system')}>System sound</Button>
          <Button size="sm" icon={AppWindow} disabled={active || !room || !native} onClick={() => setPicker({ mode: 'application', index: null })}>An app…</Button>
          <Button size="sm" icon={Plus} variant="ghost" disabled={active || !room || !native} onClick={() => setPicker({ mode: 'exclude', index: null })}>Everything except an app…</Button>
          <span className="muted small">{room ? 'Tip: game + Discord, with System sound off.' : 'Five sources is the maximum (microphone included).'}</span>
        </div>
      </Card>

      <Card title="Tracks and quality">
        <div className="tracks-row">
          <Toggle label="Separate track per source" checked={draft.separate_tracks} disabled={active} onChange={v => patch({ separate_tracks: v })}
            description="Track 1 is always the full mix, so every player still plays everything." />
          <Field label="Quality (AAC per track)">
            <Segmented label="Audio bitrate" value={draft.audio_bitrate_kbps} disabled={active} onChange={v => patch({ audio_bitrate_kbps: v })}
              options={[128, 160, 192, 256, 320].map(k => ({ value: k, label: `${k}k` }))} />
          </Field>
        </div>
        <div className="track-map">
          <span className="track"><b>1</b>Full mix</span>
          {draft.separate_tracks && enabled.map((s, i) => <span className="track" key={i}><b>{i + 2}</b>{s.name}</span>)}
        </div>
      </Card>

      {picker && <AppPicker mode={picker.mode} onClose={() => setPicker(null)} onPick={s => {
        const name = s.name.replace(/\.exe$/i, '');
        const extra = { pid: s.pid, process_started: s.started, name: picker.mode === 'exclude' ? `All except ${name}` : name };
        if (picker.index !== null) update(picker.index, { ...extra, kind: picker.mode }); else add(picker.mode, extra);
        setPaths(p => ({ ...p, [s.pid]: s.path }));
        setPicker(null);
      }} />}
          </Section>
  );
}
