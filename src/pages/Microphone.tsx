import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ChevronDown, Headphones, Mic, MicOff, RotateCcw, Square } from 'lucide-react';
import { api, errorText, native, type AudioSource } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { Badge, Button, Card, IconButton, Meter } from '../ui';
import { feedIndex } from './shared';
import { identity, micDefaults, micState } from '../audio';

const PROCESSING: { key: 'high_pass' | 'noise_suppression' | 'noise_gate' | 'compressor'; label: string; text: string }[] = [
  { key: 'high_pass', label: 'Remove rumble', text: 'Cuts sound below about 80 Hz: desk bumps, fan rumble and breath pops. Voices are unaffected. Recommended.' },
  { key: 'noise_suppression', label: 'Noise suppression', text: 'Light reduction of steady background noise such as fans or hum. Kept gentle so speech stays natural.' },
  { key: 'noise_gate', label: 'Quiet between words', text: 'Lowers background noise between sentences by up to about 10 dB (a soft expander, not a hard cut-off).' },
  { key: 'compressor', label: 'Even out volume', text: 'Makes quiet and loud speech closer in level. Useful if you sometimes shout.' },
];

/** Microphone: always present (no "add" step). Device, record on/off, live
 * volume and mute, a Discord-style live test, and boost/processing tucked
 * away under Advanced. */
export function MicrophoneCard() {
  const { draft, saved, patch, caps, setMix } = useApp();
  const active = useStatus(activeOf);
  const peaks = useStatus(x => x.audio_peaks);
  const [advanced, setAdvanced] = useState(false);
  const sources = draft.audio;
  const index = sources.findIndex(s => s.kind === 'microphone');
  const mic = index >= 0 ? sources[index] : null;
  const inputs = (caps?.devices ?? []).filter(d => d.kind === 'microphone');
  const outputs = (caps?.devices ?? []).filter(d => d.kind === 'system');
  // Volume/mute go to the running recorder only when the saved source list matches.
  const live = !!mic && saved.audio.length === sources.length && saved.audio.every((a, i) => identity(a) === identity(sources[i]));
  const fi = index >= 0 ? feedIndex(sources, index) : -1;
  const capturePeak = active && live && mic?.enabled && !mic.muted ? peaks[fi] ?? 0 : 0;
  const state = micState({ mic, capturing: active && live, deviceKnown: !mic?.device || inputs.some(d => d.id === mic.device) });
  const track = mic && mic.enabled && draft.separate_tracks ? sources.filter(s => s.enabled).indexOf(mic) + 2 : null;

  // Live test ("hear yourself"): nothing is recorded.
  const [testing, setTesting] = useState(false);
  const [testPeak, setTestPeak] = useState(0);
  const [output, setOutput] = useState('');
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!native) return;
    const a = listen<number>('mic-monitor-level', e => setTestPeak(e.payload));
    const b = listen<{ error: string | null }>('mic-monitor-ended', e => { setTesting(false); setTestPeak(0); if (e.payload.error) setError(e.payload.error); });
    return () => { void a.then(u => u()); void b.then(u => u()); void api.micMonitorStop().catch(() => undefined); };
  }, []);
  // Hearing yourself while System audio is captured would put it in the clip.
  const leaks = active && saved.audio.some(a => a.enabled && (a.kind === 'system' || a.kind === 'exclude'));

  if (!mic) {
    return (
      <Card title="Microphone">
        <div className="banner warn small">Five other audio sources are set up, so there is no room for the microphone. Remove one in Sources below.</div>
      </Card>
    );
  }
  const update = (p: Partial<AudioSource>) => patch({ audio: sources.map((a, n) => (n === index ? { ...a, ...p } : a)) });
  const setVolume = (volume: number, muted: boolean) => {
    if (live) void setMix(index, volume, muted); else update({ volume, muted });
    if (testing) void api.micMonitorSet(volume, mic.boost_db).catch(() => undefined);
  };
  const setBoost = (boost_db: number) => {
    update({ boost_db });
    if (testing) void api.micMonitorSet(mic.volume, boost_db).catch(() => undefined);
  };
  const toggleTest = async () => {
    setError(null);
    if (testing) { void api.micMonitorStop(); return; }
    try { await api.micMonitorStart({ ...mic, enabled: true, muted: false }, output); setTesting(true); } catch (e) { setError(errorText(e)); }
  };
  const peak = testing ? testPeak : capturePeak;
  return (
    <Card title="Microphone" className="mic-card"
      actions={<>
        <Badge tone={state.tone} dot>{state.label}</Badge>
        {track && <Badge tone="accent">Track {track}</Badge>}
        <button role="switch" aria-checked={mic.enabled} aria-label="Record microphone" title={mic.enabled ? 'Recorded in your clips' : 'Not recorded'}
          className={`switch ${mic.enabled ? 'on' : ''}`} disabled={active} onClick={() => update({ enabled: !mic.enabled })}><span /></button>
      </>}>
      <div className="mic-row">
        <select className="mic-device" aria-label="Microphone" value={mic.device} disabled={active || testing} onChange={e => update({ device: e.target.value })}>
          <option value="">Windows default microphone</option>
          {inputs.map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
          {mic.device && !inputs.some(d => d.id === mic.device) && <option value={mic.device}>Disconnected microphone</option>}
        </select>
        <IconButton label={mic.muted ? 'Unmute microphone' : 'Mute microphone'} icon={mic.muted ? MicOff : Mic} className={mic.muted ? 'muted' : ''}
          onClick={() => setVolume(mic.volume, !mic.muted)} />
        <input type="range" min={0} max={200} step={1} value={Math.round(mic.volume * 100)} aria-label="Microphone volume"
          onChange={e => setVolume(Number(e.target.value) / 100, mic.muted)} />
        <span className="range-value">{Math.round(mic.volume * 100)}%</span>
      </div>
      <div className="mic-row">
        <Button variant={testing ? 'danger' : 'primary'} icon={testing ? Square : Headphones} disabled={!native || (!testing && leaks)} onClick={toggleTest}>
          {testing ? 'Stop testing' : 'Test mic'}
        </Button>
        <select className="mic-output" aria-label="Hear yourself on" value={output} disabled={testing} onChange={e => setOutput(e.target.value)}>
          <option value="">Listen on: Windows default output</option>
          {outputs.map(d => <option key={d.id} value={d.id}>Listen on: {d.name}</option>)}
        </select>
        <Meter peak={peak} active={peak > 0} />
      </div>
      {testing && <p className="mic-note">You're hearing yourself live (nothing is recorded). Use headphones, speakers will echo. Volume and boost change instantly.</p>}
      {!testing && leaks && <p className="mic-note">Test mic is off while Instant Replay records System audio, because you would hear yourself in the clip.</p>}
      {!mic.enabled && <p className="mic-note">Your microphone is not recorded. Turn on the switch above to record your voice on its own track.</p>}
      {peak >= 0.98 && <div className="banner warn small">Very loud: the limiter is catching peaks. Lower the boost or volume.</div>}
      {error && <div className="banner bad small">{error}</div>}

      <button className="disclosure mic-more" onClick={() => setAdvanced(a => !a)} aria-expanded={advanced}>
        <ChevronDown size={15} className={advanced ? 'open' : ''} />Boost and processing
        <span className="muted small">{[mic.boost_db > 0 && `+${mic.boost_db} dB`, ...PROCESSING.filter(p => mic[p.key]).map(p => p.label)].filter(Boolean).join(' · ') || 'Off'}</span>
      </button>
      {advanced && (
        <div className="mic-advanced">
          <label className="mic-boost-row" title="Makes a quiet microphone louder. A limiter always follows, so it never clips.">
            <span>Boost</span>
            <input type="range" min={0} max={24} step={1} value={mic.boost_db} disabled={active} aria-label="Microphone boost" onChange={e => setBoost(Number(e.target.value))} />
            <span className="range-value">+{mic.boost_db} dB</span>
          </label>
          <div className="mic-checks">
            {PROCESSING.map(p => (
              <label key={p.key} title={p.text} className={active ? 'disabled' : ''}>
                <input type="checkbox" checked={mic[p.key]} disabled={active} onChange={e => update({ [p.key]: e.target.checked } as Partial<AudioSource>)} />{p.label}
              </label>
            ))}
          </div>
          <div className="mic-adv-foot">
            <Button size="sm" variant="ghost" icon={RotateCcw} disabled={active} onClick={() => update({ ...micDefaults(mic.device), enabled: mic.enabled, name: mic.name })}>Reset to defaults</Button>
            <span className="muted small">Processing applies to recordings{active ? '; stop capture to change it' : ''}. Windows' microphone level is never changed.</span>
          </div>
        </div>
      )}
    </Card>
  );
}
