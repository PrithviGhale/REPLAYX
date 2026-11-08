import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Headphones, Mic, MicOff, Play, RotateCcw, Square } from 'lucide-react';
import { api, errorText, native, type AudioSource, type TestCapture } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { db } from '../format';
import { Badge, Button, Card, Empty, Field, IconButton, Meter, Modal, Toggle } from '../ui';
import { feedIndex } from './shared';
import { identity, micDefaults, micState } from '../audio';

/** Start/stop test of one audio source with in-app playback of the processed
 * result. Nothing is recorded until the user presses Start; the recording is
 * deleted when the panel closes. */
export function SourceTest({ source, onClose }: { source: AudioSource; onClose: () => void }) {
  const [phase, setPhase] = useState<'ready' | 'recording' | 'done'>('ready');
  const [level, setLevel] = useState({ peak: 0, elapsed: 0, max: 0 });
  const [result, setResult] = useState<TestCapture | null>(null);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const seconds = 10;
  const off = useRef<(() => void) | undefined>(undefined);
  useEffect(() => () => { off.current?.(); void api.audioTestStop().catch(() => undefined); void api.audioTestDiscard().catch(() => undefined); }, []);
  const start = async () => {
    setError(null); setResult(null); setAudioUrl(null); setLevel({ peak: 0, elapsed: 0, max: 0 });
    setPhase('recording');
    off.current?.();
    off.current = await listen<[number, number]>('audio-test-level', e => setLevel(l => ({ peak: e.payload[0], elapsed: e.payload[1], max: Math.max(l.max, e.payload[0]) })));
    try {
      const r = await api.audioTest({ ...source, enabled: true, muted: false }, seconds);
      setResult(r);
      setAudioUrl(await api.audioTestData());
      setPhase('done');
    } catch (e) { setError(errorText(e)); setPhase('ready'); }
    off.current?.(); off.current = undefined;
  };
  const mic = source.kind === 'microphone';
  const quiet = result && result.peak < 0.003;
  const limited = level.max >= 0.98;
  return (
    <Modal title={`Test — ${source.name}`} onClose={onClose} actions={<>
      <Button variant="ghost" onClick={onClose}>Close</Button>
      {phase === 'recording'
        ? <Button variant="danger" icon={Square} onClick={() => api.audioTestStop()}>Stop test</Button>
        : <Button variant="primary" icon={phase === 'done' ? RotateCcw : Mic} onClick={start}>{phase === 'done' ? 'Test again' : 'Start test'}</Button>}
    </>}>
      {phase === 'ready' && !error && (
        <p>{mic ? `Press Start test, then speak normally for up to ${seconds} seconds. You'll hear the result with your current volume, boost and processing.` : `Press Start test while this source plays sound (up to ${seconds} seconds).`} Nothing is recorded before you press Start, and the test is deleted when you close this window.</p>
      )}
      {phase === 'recording' && (
        <>
          <div className="test-live"><span className="rec-dot" /> Recording test · {level.elapsed.toFixed(1)} s of {seconds}</div>
          <Meter peak={level.peak} active />
          <div className="progress"><span style={{ width: `${Math.min(100, (level.elapsed / seconds) * 100)}%` }} /></div>
          {limited && mic && <div className="banner warn small">Very loud peaks: the limiter is catching them. Lower the boost if your voice sounds squashed.</div>}
        </>
      )}
      {error && <div className="banner bad">{error}</div>}
      {phase === 'done' && result && (
        <>
          {quiet ? <div className="banner warn">Almost silent (peak {db(result.peak)}). Check the device, Windows privacy settings (Settings → Privacy → Microphone) and any hardware mute.</div>
            : <div className="banner ok">Captured {result.seconds.toFixed(1)} s · peak {db(result.peak)} · {result.discontinuities === 0 ? 'no gaps' : `${result.discontinuities} discontinuities`}.</div>}
          {audioUrl && <audio className="test-player" controls autoPlay src={audioUrl}>Your test recording</audio>}
          <dl className="facts compact">
            <div><dt>Device format</dt><dd>{result.endpoint_mix_format ?? 'Converted by Windows (per-app capture)'}</dd></div>
            <div><dt>Device glitches</dt><dd>{result.device_glitches}</dd></div>
          </dl>
        </>
      )}
    </Modal>
  );
}

const PROCESSING: { key: 'high_pass' | 'noise_suppression' | 'noise_gate' | 'compressor'; label: string; text: string }[] = [
  { key: 'high_pass', label: 'Remove rumble (high-pass)', text: 'Cuts sound below about 80 Hz: desk bumps, fan rumble and breath pops. Voices are unaffected. Recommended.' },
  { key: 'noise_suppression', label: 'Noise suppression', text: 'Light reduction of steady background noise such as fans or hum. Kept gentle so speech stays natural.' },
  { key: 'noise_gate', label: 'Quiet between words', text: 'Lowers background noise between sentences by up to about 10 dB (a soft expander, not a hard cut-off).' },
  { key: 'compressor', label: 'Even out volume', text: 'Makes quiet and loud speech closer in level. Useful if you sometimes shout.' },
];

/** Microphone setup: device, availability, track, volume, boost, processing and test. */
export function MicrophoneCard() {
  const { draft, saved, patch, caps, setMix } = useApp();
  const active = useStatus(activeOf);
  const peaks = useStatus(x => x.audio_peaks);
  const [testing, setTesting] = useState(false);
  const sources = draft.audio;
  const index = sources.findIndex(s => s.kind === 'microphone');
  const mic = index >= 0 ? sources[index] : null;
  const inputs = (caps?.devices ?? []).filter(d => d.kind === 'microphone');
  // Volume/mute go to the running recorder only when the saved source list matches.
  const live = !!mic && saved.audio.length === sources.length && saved.audio.every((a, i) => identity(a) === identity(sources[i]));
  const fi = index >= 0 ? feedIndex(sources, index) : -1;
  const peak = active && live && mic?.enabled && !mic.muted ? peaks[fi] ?? 0 : 0;
  const state = micState({ mic, capturing: active && live, deviceKnown: !mic?.device || inputs.some(d => d.id === mic.device) });
  const update = (p: Partial<AudioSource>) => patch({ audio: sources.map((a, n) => (n === index ? { ...a, ...p } : a)) });
  const track = mic && mic.enabled && draft.separate_tracks ? sources.filter(s => s.enabled).indexOf(mic) + 2 : null;
  if (!mic) return <AddMicrophone />;
  return (
    <Card title="Microphone" subtitle="Your voice on its own track. Processing is applied to the recording only; Windows' microphone level is never changed."
      actions={<Badge tone={state.tone} dot>{state.label}</Badge>}>
      <div className="mic-grid">
        <Field label="Recording microphone">
          <select value={mic.device} disabled={active} onChange={e => update({ device: e.target.value })}>
            <option value="">Windows default microphone</option>
            {inputs.map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
            {mic.device && !inputs.some(d => d.id === mic.device) && <option value={mic.device}>Disconnected microphone</option>}
          </select>
        </Field>
        <Toggle label="Record microphone" checked={mic.enabled} disabled={active} onChange={v => update({ enabled: v })}
          description={track ? `Track ${track} in your clips, plus the mix on track 1.` : draft.separate_tracks ? 'Off: your voice is not recorded.' : 'Mixed into the single audio track.'} />
      </div>
      <div className="mix-row">
        <IconButton label={mic.muted ? 'Unmute microphone' : 'Mute microphone'} icon={mic.muted ? MicOff : Mic} className={mic.muted ? 'muted' : ''}
          onClick={() => (live ? setMix(index, mic.volume, !mic.muted) : update({ muted: !mic.muted }))} />
        <input type="range" min={0} max={200} step={1} value={Math.round(mic.volume * 100)} aria-label="Microphone volume"
          onChange={e => (live ? setMix(index, Number(e.target.value) / 100, mic.muted) : update({ volume: Number(e.target.value) / 100 }))} />
        <span className="range-value">{Math.round(mic.volume * 100)}%</span>
        <Meter peak={peak} active={peak > 0} />
      </div>
      {peak >= 0.98 && <div className="banner warn small">Peaks are reaching the limiter. Lower the boost or volume if your voice sounds squashed.</div>}
      <div className="mic-boost"><Field label={`Boost · +${mic.boost_db} dB`} hint="Makes a quiet microphone louder. A limiter always follows the boost, so loud moments are held below full scale instead of clipping.">
        <input type="range" min={0} max={24} step={1} value={mic.boost_db} disabled={active} aria-label="Microphone boost"
          onChange={e => update({ boost_db: Number(e.target.value) })} />
      </Field></div>
      <div className="processing">
        {PROCESSING.map(p => (
          <Toggle key={p.key} label={p.label} checked={mic[p.key]} disabled={active} onChange={v => update({ [p.key]: v } as Partial<AudioSource>)} description={p.text} />
        ))}
      </div>
      <div className="mic-actions">
        <Button icon={Play} disabled={active || !native} onClick={() => setTesting(true)}>Record a test</Button>
        <Button variant="ghost" icon={RotateCcw} disabled={active} onClick={() => update({ ...micDefaults(mic.device), name: mic.name })}>Reset to defaults</Button>
        {active && <span className="muted small">Stop capture to change the device, boost or processing (volume and mute work live).</span>}
      </div>
      <HearYourself source={mic} />
      {testing && <SourceTest source={mic} onClose={() => setTesting(false)} />}
    </Card>
  );
}

/** No microphone source yet: list what Windows reports (updated live when a
 * device is plugged in) and add the chosen one in one click. */
function AddMicrophone() {
  const { draft, patch, caps } = useApp();
  const active = useStatus(activeOf);
  const inputs = (caps?.devices ?? []).filter(d => d.kind === 'microphone');
  const [choice, setChoice] = useState('');
  const full = draft.audio.length >= 5;
  return (
    <Card title="Microphone" actions={<Badge tone={inputs.length ? 'ok' : 'warn'} dot>{inputs.length ? `${inputs.length} found` : 'None found'}</Badge>}>
      {inputs.length === 0 ? (
        <Empty icon={MicOff} title="No microphone found">Plug in your microphone or headset. REPLAYX notices it within a couple of seconds; no need to refresh. If it is plugged in, check Windows Settings → Privacy &amp; security → Microphone.</Empty>
      ) : (
        <>
          <p className="muted small">Your microphone is not recorded yet. Choose it to record your voice on its own track. Nothing is recorded until you start Instant Replay or a recording.</p>
          <div className="add-mic">
            <Field label="Microphone">
              <select value={choice} onChange={e => setChoice(e.target.value)}>
                <option value="">Windows default microphone</option>
                {inputs.map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
              </select>
            </Field>
            <Button variant="primary" icon={Mic} disabled={active || full} onClick={() => patch({ audio: [...draft.audio, micDefaults(choice)] })}>Use this microphone</Button>
          </div>
          {active && <div className="banner info small">Turn off Instant Replay and recording first; then add the microphone and save.</div>}
          {full && <div className="banner warn small">Five audio sources is the maximum. Remove one in Sources below.</div>}
        </>
      )}
    </Card>
  );
}

/** "Hear yourself": the microphone played live on an output, with its volume
 * and boost. Ends on Stop, when leaving the page, or after 10 minutes. */
function HearYourself({ source }: { source: AudioSource }) {
  const { caps } = useApp();
  const active = useStatus(activeOf);
  const outputs = (caps?.devices ?? []).filter(d => d.kind === 'system');
  const [output, setOutput] = useState('');
  const [on, setOn] = useState(false);
  const [peak, setPeak] = useState(0);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!native) return;
    const a = listen<number>('mic-monitor-level', e => setPeak(e.payload));
    const b = listen<{ error: string | null }>('mic-monitor-ended', e => { setOn(false); setPeak(0); if (e.payload.error) setError(e.payload.error); });
    return () => { void a.then(u => u()); void b.then(u => u()); void api.micMonitorStop().catch(() => undefined); };
  }, []);
  const start = async () => {
    setError(null);
    try { await api.micMonitorStart({ ...source, enabled: true, muted: false }, output); setOn(true); } catch (e) { setError(errorText(e)); }
  };
  const stop = () => { void api.micMonitorStop(); };
  return (
    <div className="hear">
      <div className="hear-head">
        <Headphones size={16} />
        <div><strong>Hear yourself</strong><small>Plays your microphone live so you can check how you sound. Use headphones: with speakers it echoes. Nothing is recorded.</small></div>
      </div>
      <div className="hear-row">
        <select aria-label="Listen on" value={output} disabled={on} onChange={e => setOutput(e.target.value)}>
          <option value="">Windows default output</option>
          {outputs.map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
        </select>
        {on
          ? <Button variant="danger" icon={Square} onClick={stop}>Stop listening</Button>
          : <Button variant="primary" icon={Headphones} disabled={active || !native} onClick={start}>Start listening</Button>}
        <Meter peak={on ? peak : 0} active={on} />
      </div>
      {on && <p className="muted small">Live with your volume and boost (about a tenth of a second delay). Rumble filter, noise suppression and the other processing are heard in <em>Record a test</em> and in your clips.</p>}
      {active && !on && <p className="muted small">Available while Instant Replay and recording are off.</p>}
      {error && <div className="banner bad small">{error}</div>}
    </div>
  );
}
