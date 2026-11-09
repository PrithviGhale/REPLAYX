import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ChevronDown, CircleDot, ExternalLink, Headphones, Mic, MicOff, Play, RotateCcw, Save, Square, Trash2, Wand2 } from 'lucide-react';
import { api, errorText, native, type AudioSource, type Suppression, type VoiceConfig, type VoicePreset, type VoiceSample } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { Badge, Button, Card, IconButton, Meter, Segmented } from '../ui';
import { feedIndex } from './shared';
import { CLEAR_VOICE, identity, micState } from '../audio';

const PRESETS: { value: Exclude<VoicePreset, 'custom'>; label: string; hint: string }[] = [
  { value: 'raw', label: 'Off', hint: 'Your microphone exactly as Windows delivers it. No processing at all.' },
  { value: 'natural', label: 'Natural', hint: 'Minimal: removes rumble and a little background noise. Sounds like your mic, only cleaner.' },
  { value: 'clear', label: 'Clear', hint: 'Recommended. Background noise removed, voice easier to understand, levels evened out gently.' },
  { value: 'broadcast', label: 'Broadcast', hint: 'Fuller, steadier "streamer" voice: warmth, presence, stronger compression, softer S sounds.' },
  { value: 'noise_free', label: 'Noise-Free', hint: 'Strongest background-noise removal for loud rooms; quiet between words.' },
];
const STRENGTHS: { value: Suppression; label: string; hint: string }[] = [
  { value: 'off', label: 'Off', hint: 'No noise suppression.' },
  { value: 'light', label: 'Light', hint: 'About 10 dB less steady noise (fans, hiss). Most natural.' },
  { value: 'balanced', label: 'Balanced', hint: 'About 17 dB less steady noise. Recommended.' },
  { value: 'strong', label: 'Strong', hint: 'About 25 dB less steady noise. Best for loud rooms.' },
];
const presetLabel = (p: string) => PRESETS.find(x => x.value === p)?.label ?? 'Custom';

/** Microphone: device, recording on/off, live volume, Voice Enhancer, live
 * Test mic with original/enhanced A/B, a same-take comparison, fine-tuning. */
export function MicrophoneCard() {
  const { draft, saved, patch, caps, setMix } = useApp();
  const active = useStatus(activeOf);
  const peaks = useStatus(x => x.audio_peaks);
  const [fine, setFine] = useState(false);
  const [builtins, setBuiltins] = useState<Record<string, VoiceConfig>>({});
  useEffect(() => { if (native) api.voicePresets().then(setBuiltins).catch(() => undefined); }, []);
  const sources = draft.audio;
  const index = sources.findIndex(s => s.kind === 'microphone');
  const mic = index >= 0 ? sources[index] : null;
  const inputs = (caps?.devices ?? []).filter(d => d.kind === 'microphone');
  const outputs = (caps?.devices ?? []).filter(d => d.kind === 'system');
  const live = !!mic && saved.audio.length === sources.length && saved.audio.every((a, i) => identity(a) === identity(sources[i]));
  const fi = index >= 0 ? feedIndex(sources, index) : -1;
  const capturePeak = active && live && mic?.enabled && !mic.muted ? peaks[fi] ?? 0 : 0;
  const state = micState({ mic, capturing: active && live, deviceKnown: !mic?.device || inputs.some(d => d.id === mic.device) });
  const track = mic && mic.enabled && draft.separate_tracks ? sources.filter(s => s.enabled).indexOf(mic) + 2 : null;

  // Live Test mic ("hear yourself"), through the same processing as recordings.
  const [testing, setTesting] = useState(false);
  const [original, setOriginal] = useState(false);
  const [testPeak, setTestPeak] = useState(0);
  const [output, setOutput] = useState('');
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!native) return;
    const a = listen<number>('mic-monitor-level', e => setTestPeak(e.payload));
    const b = listen<{ error: string | null }>('mic-monitor-ended', e => { setTesting(false); setTestPeak(0); if (e.payload.error) setError(e.payload.error); });
    return () => { void a.then(u => u()); void b.then(u => u()); void api.micMonitorStop().catch(() => undefined); };
  }, []);
  // Settings changes reach the running test within 10 ms.
  const voiceKey = JSON.stringify(mic?.voice);
  useEffect(() => {
    if (testing && mic) void api.micMonitorSet(mic.volume, mic.voice, mic.boost_db, original).catch(() => undefined);
  }, [testing, original, mic?.volume, mic?.boost_db, voiceKey]); // eslint-disable-line react-hooks/exhaustive-deps
  const leaks = active && saved.audio.some(a => a.enabled && (a.kind === 'system' || a.kind === 'exclude'));

  if (!mic) {
    return (
      <Card title="Microphone">
        <div className="banner warn small">Five other audio sources are set up, so there is no room for the microphone. Remove one in Sound sources below.</div>
      </Card>
    );
  }
  const update = (p: Partial<AudioSource>) => patch({ audio: sources.map((a, n) => (n === index ? { ...a, ...p } : a)) });
  const setVoice = (p: Partial<VoiceConfig>) => update({ voice: { ...mic.voice, ...p, preset: 'custom' } });
  const usePreset = (name: VoicePreset) => {
    if (name === 'custom') return;
    update({ voice: { ...(builtins[name] ?? (name === 'clear' ? CLEAR_VOICE : mic.voice)), preset: name } });
  };
  const setVolume = (volume: number, muted: boolean) => { if (live) void setMix(index, volume, muted); else update({ volume, muted }); };
  const toggleTest = async () => {
    setError(null);
    if (testing) { void api.micMonitorStop(); return; }
    try { await api.micMonitorStart({ ...mic, enabled: true, muted: false }, output); setTesting(true); } catch (e) { setError(errorText(e)); }
  };
  const peak = testing ? testPeak : capturePeak;
  const locked = active; // Settings are saved with capture stopped.
  const v = mic.voice;
  const presetOptions = [
    ...PRESETS.map(p => ({ value: p.value as VoicePreset, label: p.label, hint: p.hint })),
    ...(v.preset === 'custom' ? [{ value: 'custom' as VoicePreset, label: 'Custom', hint: 'Your own settings' }] : []),
  ];
  return (
    <Card title="Microphone" className="mic-card"
      actions={<>
        <Badge tone={state.tone} dot>{state.label}</Badge>
        {track && <Badge tone="accent">Track {track}</Badge>}
        <button role="switch" aria-checked={mic.enabled} aria-label="Record microphone" title={mic.enabled ? 'Recorded in your clips' : 'Not recorded'}
          className={`switch ${mic.enabled ? 'on' : ''}`} disabled={active} onClick={() => update({ enabled: !mic.enabled })}><span /></button>
      </>}>
      <div className="mic-row" data-tour="mic-device">
        <select className="mic-device" aria-label="Microphone" value={mic.device} disabled={active || testing} onChange={e => update({ device: e.target.value })}>
          <option value="">Windows default microphone</option>
          {inputs.map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
          {mic.device && !inputs.some(d => d.id === mic.device) && <option value={mic.device}>Disconnected microphone</option>}
        </select>
        <IconButton label={mic.muted ? 'Unmute microphone' : 'Mute microphone'} icon={mic.muted ? MicOff : Mic} className={mic.muted ? 'muted' : ''}
          onClick={() => setVolume(mic.volume, !mic.muted)} />
        <input type="range" min={0} max={200} step={1} value={Math.round(mic.volume * 100)} aria-label="Microphone volume in the mix"
          title="Volume of your voice in the clip's mix (changes live)" onChange={e => setVolume(Number(e.target.value) / 100, mic.muted)} />
        <span className="range-value">{Math.round(mic.volume * 100)}%</span>
      </div>

      <div className="voice-block" data-tour="voice-enhancer">
        <div className="voice-head"><Wand2 size={15} /><strong>Voice Enhancer</strong>
          <span className="muted small">{v.preset === 'custom' ? 'Your own settings (Fine-tune below).' : PRESETS.find(p => p.value === v.preset)?.hint}</span></div>
        <div className="voice-presets">
          <Segmented label="Voice Enhancer preset" value={v.preset} disabled={locked} onChange={usePreset}
            options={presetOptions.map(o => (o.value === 'clear' ? { ...o, label: <>{o.label}<em className="rec-tag">Recommended</em></> } : o))} />
          {draft.voice_presets.length > 0 && (
            <select className="voice-saved" aria-label="Your presets" value="" disabled={locked}
              onChange={e => { const s = draft.voice_presets[Number(e.target.value)]; if (s) update({ voice: { ...s.voice, preset: 'custom' }, boost_db: s.boost_db }); }}>
              <option value="">Your presets…</option>
              {draft.voice_presets.map((p, i) => <option key={i} value={i}>{p.name}</option>)}
            </select>
          )}
        </div>
        <div className="voice-line">
          <span className="voice-label" title="Removes steady background sound: fans, hum, hiss, room noise">Noise suppression</span>
          <Segmented label="Noise suppression" value={v.suppression} disabled={locked} onChange={s => setVoice({ suppression: s })} options={STRENGTHS} />
        </div>
      </div>

      <div className="mic-row" data-tour="mic-test">
        <Button variant={testing ? 'danger' : 'primary'} icon={testing ? Square : Headphones} disabled={!native || (!testing && leaks)} onClick={toggleTest}>
          {testing ? 'Stop testing' : 'Test mic'}
        </Button>
        {testing
          ? <Segmented label="What you hear" value={original ? 'original' : 'enhanced'} onChange={x => setOriginal(x === 'original')}
              options={[{ value: 'enhanced', label: 'Enhanced', hint: 'With the Voice Enhancer, as it will be recorded' }, { value: 'original', label: 'Original', hint: 'Your raw microphone, at the same loudness' }]} />
          : <select className="mic-output" aria-label="Hear yourself on" value={output} onChange={e => setOutput(e.target.value)}>
              <option value="">Listen on: Windows default output</option>
              {outputs.map(d => <option key={d.id} value={d.id}>Listen on: {d.name}</option>)}
            </select>}
        <Meter peak={peak} active={peak > 0} />
      </div>
      {testing && <p className="mic-note">You're hearing yourself live, exactly as you'll be recorded. Switch Enhanced/Original to compare (same loudness). Nothing is recorded. Use headphones, speakers will echo.</p>}
      {!testing && leaks && <p className="mic-note">Test mic is off while Instant Replay records System audio, because you would hear yourself in the clip.</p>}
      {!mic.enabled && <p className="mic-note">Your microphone is not recorded. Turn on the switch above to record your voice on its own track.</p>}
      {peak >= 0.98 && <div className="banner warn small">Very loud: the limiter is catching peaks. Lower the recording gain, or the Windows input level.</div>}
      {error && <div className="banner bad small">{error}</div>}

      <CompareSample mic={mic} disabled={!native || testing} />

      <button className="disclosure mic-more" onClick={() => setFine(f => !f)} aria-expanded={fine}>
        <ChevronDown size={15} className={fine ? 'open' : ''} />Fine-tune
        <span className="muted small">Gain {mic.boost_db > 0 ? `+${mic.boost_db} dB` : '0 dB'} · {presetLabel(v.preset)}</span>
      </button>
      {fine && <FineTune mic={mic} locked={locked} setVoice={setVoice} update={update} builtins={builtins} />}
    </Card>
  );
}

function Slider({ label, value, min, max, step = 0.5, unit = 'dB', disabled, title, onChange }: {
  label: string; value: number; min: number; max: number; step?: number; unit?: string; disabled?: boolean; title?: string; onChange: (v: number) => void;
}) {
  return (
    <label className="ft-slider" title={title}>
      <span>{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} disabled={disabled} aria-label={label} onChange={e => onChange(Number(e.target.value))} />
      <span className="range-value">{value > 0 && unit === 'dB' ? '+' : ''}{value}{unit === ':1' ? ':1' : ` ${unit}`}</span>
    </label>
  );
}

function FineTune({ mic, locked, setVoice, update, builtins }: {
  mic: AudioSource; locked: boolean; setVoice: (p: Partial<VoiceConfig>) => void; update: (p: Partial<AudioSource>) => void; builtins: Record<string, VoiceConfig>;
}) {
  const { draft, patch } = useApp();
  const [name, setName] = useState('');
  const v = mic.voice;
  const check = (key: 'high_pass' | 'gate' | 'compressor' | 'deesser', label: string, title: string) => (
    <label className={`ft-check ${locked ? 'disabled' : ''}`} title={title}>
      <input type="checkbox" checked={v[key]} disabled={locked} onChange={e => setVoice({ [key]: e.target.checked } as Partial<VoiceConfig>)} />{label}
    </label>
  );
  const savePreset = () => {
    const n = name.trim();
    if (!n) return;
    const rest = draft.voice_presets.filter(p => p.name !== n);
    patch({ voice_presets: [...rest, { name: n, voice: { ...v, preset: 'custom' as const }, boost_db: mic.boost_db }].slice(-20) });
    setName('');
  };
  return (
    <div className="mic-advanced">
      <div className="ft-levels">
        <Slider label="Recording gain" value={mic.boost_db} min={0} max={24} step={1} disabled={locked} onChange={x => update({ boost_db: x })}
          title="Software gain REPLAYX applies to your recording, before its limiter (never clips). Does not change Windows." />
        <div className="ft-hw">
          <span className="muted small">Your microphone's hardware level is set in Windows; REPLAYX never changes it.</span>
          <Button size="sm" variant="ghost" icon={ExternalLink} disabled={!native} onClick={() => api.openSoundSettings().catch(() => undefined)}>Windows sound settings</Button>
        </div>
      </div>
      <div className="ft-grid">
        <div className="ft-col">
          <span className="ft-title">Clean-up</span>
          {check('high_pass', 'Remove rumble (80 Hz high-pass)', 'Cuts desk bumps, fan rumble and breath pops below 80 Hz. Voices are unaffected.')}
          {check('gate', 'Quiet between words', 'Lowers background sound when you are not talking (soft, never cuts quiet speech while you talk).')}
          {v.gate && <Slider label="Threshold" value={v.gate_threshold_db} min={-70} max={-20} step={1} disabled={locked} onChange={x => setVoice({ gate_threshold_db: x })}
            title="Sounds below this level count as silence. Lower it if quiet words get softer." />}
        </div>
        <div className="ft-col">
          <span className="ft-title">Dynamics</span>
          {check('compressor', 'Even out loud and quiet', 'Makes quiet and loud speech closer in level, like a streamer setup.')}
          {v.compressor && <>
            <Slider label="Threshold" value={v.comp_threshold_db} min={-50} max={-6} step={1} disabled={locked} onChange={x => setVoice({ comp_threshold_db: x })} title="Speech louder than this is evened out." />
            <Slider label="Ratio" value={v.comp_ratio} min={1} max={10} step={0.5} unit=":1" disabled={locked} onChange={x => setVoice({ comp_ratio: x })} title="How strongly loud speech is evened out." />
          </>}
          {check('deesser', 'Soften harsh S sounds', 'De-esser: reduces sharp "s" and "sh" sounds, useful with brighter settings.')}
        </div>
        <div className="ft-col">
          <span className="ft-title">Tone</span>
          <Slider label="Warmth" value={v.warmth_db} min={0} max={6} disabled={locked} onChange={x => setVoice({ warmth_db: x })} title="Fuller low end (140 Hz). A little goes a long way." />
          <Slider label="Less boxy" value={v.mud_db} min={-8} max={0} disabled={locked} onChange={x => setVoice({ mud_db: x })} title="Cuts the cardboard-box sound around 320 Hz." />
          <Slider label="Presence" value={v.presence_db} min={0} max={8} disabled={locked} onChange={x => setVoice({ presence_db: x })} title="Clearer words (3.2 kHz)." />
          <Slider label="Air" value={v.air_db} min={0} max={6} disabled={locked} onChange={x => setVoice({ air_db: x })} title="Brighter, more open top end (10 kHz)." />
        </div>
      </div>
      <div className="mic-adv-foot">
        <input className="ft-name" placeholder="Name your preset" value={name} maxLength={40} disabled={locked} onChange={e => setName(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') savePreset(); }} aria-label="Preset name" />
        <Button size="sm" icon={Save} disabled={locked || !name.trim()} onClick={savePreset}>Save preset</Button>
        {draft.voice_presets.length > 0 && <Button size="sm" variant="ghost" icon={Trash2} disabled={locked}
          onClick={() => patch({ voice_presets: [] })}>Delete my presets</Button>}
        <Button size="sm" variant="ghost" icon={RotateCcw} disabled={locked}
          onClick={() => update({ voice: { ...(builtins.clear ?? CLEAR_VOICE), preset: 'clear' }, boost_db: 0 })}>Reset to defaults</Button>
        {locked && <span className="muted small">Stop capture to change these (volume and mute work live).</span>}
      </div>
    </div>
  );
}

const b64ToUrl = (b64: string) => {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }));
};

/** Same-take comparison: record a few seconds once (explicit click), then
 * listen to the original and the enhanced version at the same loudness.
 * Changing settings re-processes the same take. Kept in memory only. */
function CompareSample({ mic, disabled }: { mic: AudioSource; disabled: boolean }) {
  const active = useStatus(activeOf);
  const [phase, setPhase] = useState<'idle' | 'recording' | 'ready'>('idle');
  const [level, setLevel] = useState({ peak: 0, elapsed: 0 });
  const [result, setResult] = useState<VoiceSample | null>(null);
  const [which, setWhich] = useState<'enhanced' | 'original'>('enhanced');
  const [urls, setUrls] = useState<{ original: string; enhanced: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const player = useRef<HTMLAudioElement>(null);
  const seconds = 8;
  const voiceKey = useMemo(() => JSON.stringify([mic.voice, mic.boost_db]), [mic.voice, mic.boost_db]);
  useEffect(() => () => { void api.audioTestStop().catch(() => undefined); void api.voiceSampleDiscard().catch(() => undefined); }, []);
  useEffect(() => () => { if (urls) { URL.revokeObjectURL(urls.original); URL.revokeObjectURL(urls.enhanced); } }, [urls]);
  // Re-process the same take when settings change.
  useEffect(() => {
    if (phase !== 'ready') return;
    let cancelled = false;
    api.voiceSampleRender(mic.voice, mic.boost_db).then(r => {
      if (cancelled) return;
      setResult(r);
      setUrls({ original: b64ToUrl(r.original), enhanced: b64ToUrl(r.enhanced) });
    }).catch(e => setError(errorText(e)));
    return () => { cancelled = true; };
  }, [phase, voiceKey]); // eslint-disable-line react-hooks/exhaustive-deps
  // Switching keeps the playback position, so you hear the same words.
  const switchTo = (w: 'enhanced' | 'original') => {
    const el = player.current;
    const t = el?.currentTime ?? 0, playing = el ? !el.paused : false;
    setWhich(w);
    requestAnimationFrame(() => { if (player.current) { player.current.currentTime = t; if (playing) void player.current.play(); } });
  };
  const record = async () => {
    setError(null); setResult(null); setPhase('recording'); setLevel({ peak: 0, elapsed: 0 });
    const off = await listen<[number, number]>('voice-sample-level', e => setLevel({ peak: e.payload[0], elapsed: e.payload[1] }));
    try { await api.voiceSampleRecord({ ...mic, enabled: true, muted: false }, seconds); setPhase('ready'); }
    catch (e) { setError(errorText(e)); setPhase('idle'); }
    off();
  };
  const discard = () => { void api.voiceSampleDiscard(); setPhase('idle'); setResult(null); setUrls(null); };
  return (
    <div className="compare" data-tour="mic-compare">
      {phase === 'idle' && (
        <div className="compare-idle">
          <Button size="sm" icon={CircleDot} disabled={disabled || active} onClick={record}>Compare: record {seconds} s</Button>
          <span className="muted small">Say a few sentences once, then hear Original vs Enhanced of the same take. Kept in memory only, never saved.</span>
        </div>
      )}
      {phase === 'recording' && (
        <div className="compare-rec">
          <span className="rec-dot" /> Recording sample · {level.elapsed.toFixed(1)} of {seconds} s
          <Meter peak={level.peak} active />
          <Button size="sm" variant="danger" icon={Square} onClick={() => api.audioTestStop()}>Stop</Button>
        </div>
      )}
      {phase === 'ready' && (
        <div className="compare-ready">
          <div className="compare-row">
            <Segmented label="Listen to" value={which} onChange={switchTo}
              options={[{ value: 'original' as const, label: 'Original' }, { value: 'enhanced' as const, label: 'Enhanced' }]} />
            {urls && <audio ref={player} className="test-player" controls src={which === 'enhanced' ? urls.enhanced : urls.original} />}
            <IconButton label="Discard sample" icon={Trash2} onClick={discard} />
          </div>
          {result && (
            <div className="compare-stats">
              <span>Same loudness (original {result.match_db >= 0 ? '+' : ''}{result.match_db.toFixed(1)} dB)</span>
              <span>Background: {result.noise_floor_db.toFixed(0)} → {result.enhanced_noise_floor_db.toFixed(0)} dB</span>
              <span>Peaks: {result.input_peak_db.toFixed(1)} → {result.enhanced_peak_db.toFixed(1)} dBFS</span>
              {result.input_clipped && <span className="warn-text">Your mic clipped before REPLAYX: lower the Windows input level.</span>}
              <span className="muted"><Play size={11} /> Change any setting above to hear it on this same take.</span>
            </div>
          )}
        </div>
      )}
      {error && <div className="banner bad small">{error}</div>}
    </div>
  );
}
