import { useApp } from '../context';
import { ErrorNotice } from '../ErrorNotice';
import { describeError } from '../errors';
import { activeOf, useHistory, useStatus, useUiStart } from '../store';
import { bytes, duration, encoderName } from '../format';
import { Badge, Card, Empty, Sparkline, Stat } from '../ui';
import { Section } from './shared';
import { Gauge } from 'lucide-react';
import { VERSION } from '../api';
import { diagnosticReport } from '../diagnostics';

function DiagnosticReport() {
  const { saved, toast } = useApp();
  const report = useStatus(x => JSON.stringify(diagnosticReport(x, saved, VERSION), null, 2));
  return <Card title="Capture diagnostics" subtitle="A report without file paths, device names, game titles or raw error messages.">
    <button className="btn" onClick={() => navigator.clipboard.writeText(report).then(() => toast('success', 'Diagnostics copied'), () => toast('error', 'Copy failed. Select the report below.'))}>Copy diagnostics</button>
    <details><summary>Review report</summary><pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{report}</pre></details>
  </Card>;
}

export function PerformancePage() {
  const { draft } = useApp();
  const active = useStatus(activeOf);
  const s = useStatus(x => ({
    fps: x.fps, dropped: x.dropped_frames, duplicated: x.duplicated_frames, timings: x.last_save_timings, start: x.start_timings, frames: x.frames, cpu: x.cpu_percent, mem: x.memory_mb, write: x.write_mbps,
    buffer: x.buffer_bytes, bufferS: x.buffer_seconds, elapsed: x.elapsed, encoder: x.encoder, scaling: x.scaling, captureApi: x.capture_api, captureFallback: x.capture_fallback, detail: x.scaling_detail,
    ow: x.output_width, oh: x.output_height, frameAge: x.frame_age_ms, segAge: x.segment_age_ms, state: x.state,
    speed: x.encoder_speed, lag: x.video_lag_ms, mbps: x.target_bitrate_mbps, label: x.encoder_label, error: x.error, retry: (x.error_retry ?? []).join(','), capture: x.capture,
    geo: x.geometry ? `${x.geometry.active_width}×${x.geometry.active_height} @ ${x.geometry.refresh_hz} Hz` : '',
  }));
  const ui = useUiStart();
  const audio = useStatus(x => ({ packets: x.audio_packets, disc: x.audio_discontinuities, drift: x.audio_drift_ppm, gaps: x.audio_gaps, written: x.audio_written_bytes, peaks: x.audio_peaks, queue: x.audio_queue_ms }));
  const history = useHistory();
  const names = draft.audio.filter(a => a.enabled).map(a => a.name);
  const dropPct = s.frames + s.dropped > 0 ? (100 * s.dropped) / (s.frames + s.dropped) : 0;
  // Windows Graphics Capture delivers more frames than the output rate and the
  // nearest is kept, so skipped frames are expected; repeats are what matter.
  const wgc = active && s.captureApi === 'wgc';
  const repeatPct = s.frames > 0 ? (100 * (s.duplicated ?? 0)) / s.frames : 0;

  return (
    <Section>
      <DiagnosticReport />
      {s.error && <ErrorNotice error={describeError(s.error, { retry: s.retry ? s.retry.split(',') : [] })} />}
      {!active && <div className="banner info">Start Instant Replay or a recording to see live measurements. Nothing here is estimated.</div>}
      {active && s.speed > 0 && (s.speed < 0.97 || s.lag > 2000) && (
        <div className="banner warn" role="alert">The encoder is not keeping up ({s.speed.toFixed(2)}× real time, video {(s.lag / 1000).toFixed(1)} s behind). Expect stutter; capture stops if it falls 8 s behind. Lower the frame rate or resolution, or choose the Performance preset.</div>
      )}
      <div className="stat-grid">
        <Card className="metric"><Stat label="Encoded rate" value={active ? `${s.fps.toFixed(1)} fps` : '—'} detail={`Target ${draft.fps} fps`} tone={active && s.fps < draft.fps * 0.9 ? 'warn' : undefined} /><Sparkline values={history.map(h => h.fps)} max={draft.fps} label="Capture rate history" /></Card>
        {wgc
          ? <Card className="metric"><Stat label="Repeated frames" value={String(s.duplicated ?? 0)} detail={`${repeatPct.toFixed(2)}% of ${s.frames.toLocaleString()} frames: output slots with no new capture`} tone={repeatPct > 1 ? 'warn' : undefined} /></Card>
          : <Card className="metric"><Stat label="Dropped frames" value={active ? String(s.dropped) : '—'} detail={active ? `${dropPct.toFixed(2)}% of ${s.frames.toLocaleString()} frames` : 'Frames skipped to keep a constant frame rate'} tone={active && dropPct > 1 ? 'warn' : undefined} /><Sparkline values={history.map(h => h.dropped)} label="Dropped frames history" /></Card>}
        <Card className="metric"><Stat label="Recorder CPU" value={active ? `${s.cpu.toFixed(1)}%` : '—'} detail="REPLAYX + encoder, share of all cores" /><Sparkline values={history.map(h => h.cpu)} label="CPU history" /></Card>
        <Card className="metric"><Stat label="Recorder memory" value={active ? `${s.mem.toFixed(0)} MB` : '—'} detail="Resident memory of REPLAYX processes" /><Sparkline values={history.map(h => h.mem)} label="Memory history" /></Card>
        <Card className="metric"><Stat label="Buffer data rate" value={active ? `${s.write.toFixed(2)} MB/s` : '—'} detail={`${bytes(s.buffer)} buffered (${duration(s.bufferS)})`} /></Card>
        <Card className="metric"><Stat label="Session time" value={active ? duration(s.elapsed) : '—'} detail={s.state} /></Card>
      </div>

      <div className="grid-2">
        <Card title="Video pipeline">
          <dl className="facts">
            <div><dt>Encoder</dt><dd>{active ? encoderName(s.encoder) : '—'} {active && s.label && <span className="muted small mono">{s.label}</span>}</dd></div>
            <div><dt>Encoder speed</dt><dd>{active && s.speed > 0 ? <>{s.speed.toFixed(2)}× real time {s.speed >= 0.99 ? <Badge tone="ok">keeping up</Badge> : <Badge tone="warn">falling behind</Badge>}</> : '—'}</dd></div>
            <div><dt>Video behind real time</dt><dd>{active ? `${(s.lag / 1000).toFixed(1)} s` : '—'}</dd></div>
            <div><dt>Capture method</dt><dd>{active && s.captureApi ? (s.captureApi === 'wgc' ? 'Windows Graphics Capture' : 'Desktop Duplication') : '—'} {active && s.captureApi === 'wgc' && <Badge tone="warn">experimental</Badge>}{active && s.captureFallback && <Badge tone="warn">WGC could not start</Badge>}</dd></div>
            <div><dt>Scaling path</dt><dd>{active ? s.scaling || '—' : '—'} {active && s.scaling.startsWith('GPU zero-copy') && <Badge tone="ok">no CPU copy</Badge>}{active && s.scaling === 'CPU fallback' && <Badge tone="warn">higher CPU</Badge>}{active && s.scaling === 'GPU scaling (Intel VPP) + scaled-frame transfer' && <Badge tone="warn">frames copied to the NVIDIA encoder</Badge>}</dd></div>
            {active && s.scaling === 'GPU scaling (Intel VPP) + scaled-frame transfer' && <p className="muted small">This display runs on the integrated GPU, so every frame is copied through system memory to the NVIDIA encoder. In testing, that path fell behind (0.75× real time) while a game kept the NVIDIA GPU fully busy; Encoder: Auto records with Quick Sync on the integrated GPU and kept up.</p>}
            <div><dt>Display mode</dt><dd>{s.geo || '—'}</dd></div>
            <div><dt>Encoded size</dt><dd>{active && s.ow ? `${s.ow}×${s.oh}` : '—'}</dd></div>
            <div><dt>Bitrate</dt><dd>{active && s.mbps ? `${s.mbps} Mb/s target video (${draft.preset === 'custom' ? 'custom' : `${draft.preset} preset`})` : '—'} · {draft.audio_bitrate_kbps} kb/s per audio track</dd></div>
            <div><dt>Last frame / last segment</dt><dd>{active ? `${s.frameAge} ms / ${(s.segAge / 1000).toFixed(1)} s ago` : '—'}</dd></div>
            <div><dt>Encode latency</dt><dd className="muted">Not measured yet</dd></div>
            <div><dt>Repeated / dropped by the frame-rate grid</dt><dd>{active ? `${s.duplicated ?? 0} / ${s.dropped}` : '—'} <span className="muted small">{wgc ? `(Windows Graphics Capture delivers more than ${draft.fps} frames per second; the nearest is kept and the rest are skipped)` : `(frames FFmpeg repeated or skipped to hold ${draft.fps} fps)`}</span></dd></div>
            <div><dt>Source FPS / capture drops / queue depth</dt><dd className="muted">Not measured</dd></div>
            <div><dt>Per-frame latency / jitter / GPU copy load</dt><dd className="muted">Not measured live; use the offline pacing report</dd></div>
            <div><dt>GPU encoder load / adapter</dt><dd className="muted">Not measured in the app (Task Manager → Performance → GPU shows engine load)</dd></div>
          </dl>
          {active && s.detail && <details className="small"><summary>Faster paths that were not usable</summary><p className="mono">{s.detail}</p></details>}
        </Card>

        <Card title="Audio sources">
          {!active || names.length === 0 ? <Empty icon={Gauge} title={names.length ? 'Capture is off' : 'No audio sources'} /> : (
            <div className="table-wrap">
              <table className="table">
                <thead><tr><th>Source</th><th>Packets</th><th>Breaks</th><th>Clock ppm</th><th>Delivered</th><th>Queue</th></tr></thead>
                <tbody>
                  {names.map((n, i) => (
                    <tr key={i}>
                      <td>{n}</td>
                      <td>{(audio.packets[i] ?? 0).toLocaleString()}</td>
                      <td className={(audio.disc[i] ?? 0) > 0 ? 'warn-text' : ''}>{audio.disc[i] ?? 0}</td>
                      <td>{(audio.drift[i] ?? 0).toFixed(1)}</td>
                      <td>{bytes(audio.written[i] ?? 0)}</td>
                      <td>{audio.queue?.[i] === undefined ? 'Unmeasured' : `${audio.queue[i]} ms`}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <p className="muted small">Breaks count late audio after a stall, timing jumps and device-reported glitches. Clock ppm is the drift correction applied to keep audio in sync with video.</p>
        </Card>
      </div>
      {!!s.start?.length && <Card title="Last capture start · time from request">
        <dl className="facts">{s.start.map((t, i) => <div key={i}><dt>{t.stage}</dt><dd>{t.elapsed_ms} ms</dd></div>)}
          {ui && <div><dt>Click → {ui.recording ? 'Recording' : 'Replay on'} shown here</dt><dd>{ui.ms} ms</dd></div>}</dl>
        <p className="muted small">"First frame encoded" is reported by the encoder every 0.5 s, so it can be up to 500 ms late; the on-screen time also includes up to 250 ms of status polling. The video path is probed on the first start for a display and set of settings, then remembered.</p>
      </Card>}
      {!!s.timings?.length && <Card title="Last save · cumulative time from request">
        <dl className="facts">{s.timings.map((t, i) => <div key={i}><dt>{t.stage}</dt><dd>{t.elapsed_ms} ms</dd></div>)}</dl>
        <p className="muted small">Ready follows packet remux, stream inspection and start/end decode checks. Thumbnails run separately. Full decode is performed by the acceptance diagnostic.</p>
      </Card>}
    </Section>
  );
}
