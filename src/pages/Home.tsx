import { useState } from 'react';
import { AlertTriangle, ChevronRight, Circle, Cpu, HardDrive, Mic, MicOff, MonitorPlay, RotateCcw, Save, Square, X } from 'lucide-react';
import { native } from '../api';
import { microphoneLabel, microphoneOn } from '../update-logic';
import { useApp } from '../context';
import { useCapture, useRecordButton } from '../capture';
import { ReplaySwitch } from '../ReplaySwitch';
import { activeOf, useStatus } from '../store';
import { bytes, duration, encoderName, outputWidth, plan, sourceRect } from '../format';
import { Badge, Button, Card, Empty, Kbd, Meter } from '../ui';
import { SourceIcon, Section, Thumb, feedIndex } from './shared';
import type { Page } from '../App';

function ReplayRing({ value, total, live }: { value: number; total: number; live: boolean }) {
  const r = 70, c = 2 * Math.PI * r, f = total > 0 ? Math.min(1, value / total) : 0;
  return (
    <div className={`ring ${live ? 'live' : ''}`}>
      <svg viewBox="0 0 160 160" aria-hidden="true">
        <circle cx="80" cy="80" r={r} className="ring-track" />
        <circle cx="80" cy="80" r={r} className="ring-fill" strokeDasharray={c} strokeDashoffset={c * (1 - f)} />
      </svg>
      <div className="ring-center">
        <strong>{duration(value)}</strong>
        <span>of {duration(total)}</span>
      </div>
    </div>
  );
}

/** The microphone row of the home Audio card: level and one On/Off switch.
 * The switch shows what the engine confirmed, also when it was changed from
 * the tray or with the shortcut. Off mutes the microphone in recordings (its
 * device, volume and Voice Enhancer settings stay as they are). */
function MicRow({ name, peak, capturing }: { name: string; peak: number; capturing: boolean }) {
  const { saved, setMicrophone } = useApp();
  const [pending, setPending] = useState(false);
  const mic = saved.audio.find(a => a.kind === 'microphone');
  const on = microphoneOn(saved.audio) ?? false;
  const state = useStatus(x => x.state);
  const label = microphoneLabel(mic, state);
  const toggle = async () => {
    if (pending) return;
    setPending(true);
    try { await setMicrophone(!on); } finally { setPending(false); }
  };
  return (
    <li className={`mic-row ${on ? 'on' : 'off'}`}>
      <span className="source-icon kind-microphone">{on ? <Mic size={18} /> : <MicOff size={18} />}</span>
      <span className="name" title={name}><span>{name}</span><em className="off-tag">{label}</em></span>
      <Meter peak={on ? peak : 0} active={capturing && on} />
      <button role="switch" aria-checked={on} aria-label={on ? 'Microphone on. Turn microphone off' : 'Microphone off. Turn microphone on'}
        title={on ? 'Microphone is recorded. Click to turn it off.' : 'Microphone is not recorded. Click to turn it on.'}
        className={`switch mic-switch ${on ? 'on' : ''} ${pending ? 'pending' : ''}`} aria-busy={pending} disabled={!native} onClick={toggle}><span /></button>
    </li>
  );
}

function Health() {
  const s = useStatus(x => ({
    active: x.replay || x.recording, state: x.state, frameAge: x.frame_age_ms, segAge: x.segment_age_ms, dropped: x.dropped_frames,
    frames: x.frames, disc: x.audio_discontinuities.reduce((a, b) => a + b, 0), audio: x.audio_written_bytes.length, free: x.free_bytes,
  }));
  if (!s.active) return <p className="muted small">Health checks run while capture is on.</p>;
  const dropPct = s.frames > 0 ? (100 * s.dropped) / (s.frames + s.dropped) : 0;
  const rows: [string, string, 'ok' | 'warn' | 'bad'][] = [
    ['Video frames', s.frameAge < 1500 ? 'Arriving' : `None for ${(s.frameAge / 1000).toFixed(1)} s`, s.frameAge < 1500 ? 'ok' : s.frameAge < 4000 ? 'warn' : 'bad'],
    ['Footage written', s.state === 'Starting' ? 'Waiting for first segment' : `${(s.segAge / 1000).toFixed(1)} s ago`, s.segAge < 3500 ? 'ok' : s.segAge < 6000 ? 'warn' : 'bad'],
    ['Dropped frames', `${s.dropped} (${dropPct.toFixed(2)}%)`, dropPct < 0.5 ? 'ok' : dropPct < 3 ? 'warn' : 'bad'],
    ['Audio continuity', s.audio ? (s.disc ? `${s.disc} discontinuities` : 'Continuous') : 'No audio sources', s.disc ? 'warn' : 'ok'],
    ['Disk space', bytes(s.free), s.free > 10 * 1024 ** 3 ? 'ok' : s.free > 1024 ** 3 ? 'warn' : 'bad'],
  ];
  return (
    <ul className="health">
      {rows.map(([k, v, t]) => <li key={k} className={t}><i />{k}<span>{v}</span></li>)}
    </ul>
  );
}

export function HomePage({ go: navigate }: { go: (p: Page) => void }) {
  const { draft, caps, clips } = useApp();
  const { go, busy, saved } = useCapture();
  const { toast } = useApp();
  const s = useStatus(x => ({
    replay: x.replay, recording: x.recording, state: x.state, buffer: x.buffer_seconds, bufferBytes: x.buffer_bytes,
    elapsed: x.recording_elapsed, pending: x.pending_recording, fps: x.fps, pw: x.play_width, ph: x.play_height, encoder: x.encoder, scaling: x.scaling, ow: x.output_width, oh: x.output_height,
    free: x.free_bytes, error: x.error, notice: x.notice, hotkey: x.hotkey_error, recovery: x.recovery_sessions,
  }));
  const peaks = useStatus(x => x.audio_peaks);
  const active = useStatus(activeOf);
  const display = caps?.displays.find(d => d.index === draft.display);
  const p = plan(sourceRect(display, draft.source_rect), outputWidth(draft.width, draft.height), draft.height, draft.canvas_mode);
  const out = active && s.ow ? `${s.ow} × ${s.oh}${s.pw ? ` (plays as ${s.pw} × ${s.ph})` : ''}` : `${p.width} × ${p.height}`;
  // The microphone is always listed (with its switch), also when it is off.
  const enabled = draft.audio.map((a, i) => ({ a, i })).filter(x => x.a.enabled || x.a.kind === 'microphone');
  const rec = useRecordButton('Stop & save');
  const starting = s.pending || s.state === 'Starting';
  const headline = starting ? (s.pending || s.recording ? 'Starting recording…' : 'Starting capture…') : s.recording ? 'Recording' : s.replay ? 'Instant Replay is on' : s.state === 'Recovering' ? 'Recovering capture' : 'Capture is off';
  const sub = starting ? 'Opening the display, encoder and audio. Recording begins with the first encoded frame; Cancel stops it without saving.'
    : s.recording ? `${duration(s.elapsed)} recorded.${s.replay ? ' Replay keeps buffering alongside.' : ''}`
    : s.replay ? `The last ${duration(draft.replay_seconds)} is kept on disk. Save it the moment something happens.`
    : 'Turn on Instant Replay before you play. It works on the desktop too — no game required.';

  const alerts = [s.error && { tone: 'bad', text: s.error }, s.hotkey && { tone: 'warn', text: s.hotkey },
    caps?.media_error && { tone: 'bad', text: caps.media_error }, s.notice && { tone: 'info', text: s.notice }].filter(Boolean) as { tone: string; text: string }[];

  return (
    <Section>
      {alerts.map((a, i) => <div key={i} className={`banner ${a.tone}`} role={a.tone === 'bad' ? 'alert' : undefined}><AlertTriangle size={15} />{a.text}</div>)}
      {s.recovery > 0 && !active && (
        <button className="banner info link" onClick={() => navigate('replay')}>
          {s.recovery} interrupted capture session{s.recovery > 1 ? 's' : ''} can be recovered <ChevronRight size={14} />
        </button>
      )}
      <div className="home-grid">
        <Card className="hero">
          <div className="hero-body">
            <div className="hero-text">
              <Badge tone={starting ? 'warn' : s.recording ? 'rec' : s.replay ? 'ok' : s.state === 'Error' ? 'bad' : 'neutral'} dot>{starting ? 'STARTING' : s.recording ? 'REC' : s.replay ? 'BUFFERING' : s.state.toUpperCase()}</Badge>
              <h2>{headline}</h2>
              <p>{sub}</p>
              <div className="hero-actions">
                <ReplaySwitch size="lg" />
                <Button size="lg" variant="primary" icon={Save} disabled={!!busy || !s.replay || s.buffer <= 0} title={s.replay ? 'Save the last moments now' : 'Turn on Instant Replay first'}
                  onClick={async () => { const m = saved(await go('save_replay')); if (m) toast('success', `Replay saved — ${m}`); }}>Save replay</Button>
                <Button size="lg" variant="record" icon={rec.mode === 'record' ? Circle : rec.mode === 'cancel' ? X : Square} disabled={rec.disabled || !native}
                  onClick={rec.onClick}>
                  {rec.label}
                </Button>
              </div>
              <div className="hotkeys-inline">
                <span>Save replay <Kbd combo={draft.replay_hotkey} /></span>
                <span>Record <Kbd combo={draft.recording_hotkey} /></span>
              </div>
            </div>
            <ReplayRing value={s.buffer} total={draft.replay_seconds} live={s.replay} />
          </div>
        </Card>

        <Card title="Capture" actions={<button className="link-btn" onClick={() => navigate('capture')}>Video settings <ChevronRight size={13} /></button>}>
          <dl className="facts">
            <div><dt><MonitorPlay size={14} />Display</dt><dd>{display ? `${display.device.replace('\\\\.\\', '')} · ${display.active_width}×${display.active_height} @ ${display.refresh_hz} Hz` : draft.capture === 'desktop-duplication' ? `Display ${draft.display + 1}` : 'GDI capture'}</dd></div>
            <div><dt>Output</dt><dd>{out} · {active && s.fps ? `${s.fps.toFixed(0)} / ${draft.fps}` : draft.fps} fps</dd></div>
            <div><dt><Cpu size={14} />Encoder</dt><dd>{active && s.encoder ? encoderName(s.encoder) : `${encoderName(draft.encoder)} · chosen at start`}</dd></div>
            <div><dt>Scaling</dt><dd>{active && s.scaling ? s.scaling : draft.gpu_scaling ? 'Fastest working GPU path' : 'CPU (GPU scaling off)'}</dd></div>
            <div><dt><HardDrive size={14} />Free space</dt><dd>{native ? bytes(s.free) : '—'}</dd></div>
            <div><dt>Buffer on disk</dt><dd>{bytes(s.bufferBytes)}</dd></div>
          </dl>
        </Card>

        <Card title="Health"><Health /></Card>

        <Card title="Audio" actions={<button className="link-btn" onClick={() => navigate('audio')}>Mixer <ChevronRight size={13} /></button>}>
          {enabled.length === 0 ? <p className="muted small">No audio sources enabled — clips will be silent.</p> : (
            <ul className="mini-mixer">
              {enabled.map(({ a, i }) => a.kind === 'microphone' ? (
                <MicRow key={i} name={a.name} peak={a.enabled ? peaks[feedIndex(draft.audio, i)] ?? 0 : 0} capturing={active} />
              ) : (
                <li key={i}>
                  <SourceIcon source={a} />
                  <span className="name">{a.name}</span>
                  <Meter peak={a.muted ? 0 : peaks[feedIndex(draft.audio, i)] ?? 0} active={active && !a.muted} />
                </li>
              ))}
            </ul>
          )}
          <p className="muted small">{draft.separate_tracks ? 'Mix + one editable track per source' : 'Single mixed track'} · {draft.audio_bitrate_kbps} kbps</p>
        </Card>

        <Card title="Recent clips" className="span-2" actions={<button className="link-btn" onClick={() => navigate('clips')}>Library <ChevronRight size={13} /></button>}>
          {clips.length === 0 ? <Empty icon={Save} title="No clips yet">Saved replays and recordings appear here.</Empty> : (
            <div className="recent">
              {clips.slice(0, 4).map(c => (
                <button key={c.id} className="recent-item" onClick={() => navigate('clips')} title={c.name}>
                  <Thumb clip={c} />
                  <div><strong>{c.category === 'Unsorted' ? c.kind : `${c.category} · ${c.kind}`}</strong><span>{new Date(c.modified * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}{c.probe && ` · ${duration(c.probe.duration)}`}</span></div>
                </button>
              ))}
            </div>
          )}
        </Card>
      </div>
    </Section>
  );
}
