import { useApp } from '../context';
import { activeOf, useHistory, useStatus } from '../store';
import { bytes, duration, encoderName } from '../format';
import { Badge, Card, Empty, Sparkline, Stat } from '../ui';
import { Section } from './shared';
import { Gauge } from 'lucide-react';

export function PerformancePage() {
  const { draft } = useApp();
  const active = useStatus(activeOf);
  const s = useStatus(x => ({
    fps: x.fps, dropped: x.dropped_frames, frames: x.frames, cpu: x.cpu_percent, mem: x.memory_mb, write: x.write_mbps,
    buffer: x.buffer_bytes, bufferS: x.buffer_seconds, elapsed: x.elapsed, encoder: x.encoder, scaling: x.scaling, detail: x.scaling_detail,
    ow: x.output_width, oh: x.output_height, frameAge: x.frame_age_ms, segAge: x.segment_age_ms, state: x.state,
    geo: x.geometry ? `${x.geometry.active_width}×${x.geometry.active_height} @ ${x.geometry.refresh_hz} Hz` : '',
  }));
  const audio = useStatus(x => ({ packets: x.audio_packets, disc: x.audio_discontinuities, drift: x.audio_drift_ppm, gaps: x.audio_gaps, written: x.audio_written_bytes, peaks: x.audio_peaks }));
  const history = useHistory();
  const names = draft.audio.filter(a => a.enabled).map(a => a.name);
  const dropPct = s.frames + s.dropped > 0 ? (100 * s.dropped) / (s.frames + s.dropped) : 0;

  return (
    <Section>
      {!active && <div className="banner info">Start Instant Replay or a recording to see live measurements. Nothing here is estimated.</div>}
      <div className="stat-grid">
        <Card className="metric"><Stat label="Capture rate" value={active ? `${s.fps.toFixed(1)} fps` : '—'} detail={`Target ${draft.fps} fps`} tone={active && s.fps < draft.fps * 0.9 ? 'warn' : undefined} /><Sparkline values={history.map(h => h.fps)} max={draft.fps} label="Capture rate history" /></Card>
        <Card className="metric"><Stat label="Dropped frames" value={active ? String(s.dropped) : '—'} detail={active ? `${dropPct.toFixed(2)}% of ${s.frames.toLocaleString()} frames` : 'Frames skipped to keep a constant frame rate'} tone={active && dropPct > 1 ? 'warn' : undefined} /><Sparkline values={history.map(h => h.dropped)} label="Dropped frames history" /></Card>
        <Card className="metric"><Stat label="Recorder CPU" value={active ? `${s.cpu.toFixed(1)}%` : '—'} detail="REPLAYX + encoder, share of all cores" /><Sparkline values={history.map(h => h.cpu)} label="CPU history" /></Card>
        <Card className="metric"><Stat label="Recorder memory" value={active ? `${s.mem.toFixed(0)} MB` : '—'} detail="Resident memory of REPLAYX processes" /><Sparkline values={history.map(h => h.mem)} label="Memory history" /></Card>
        <Card className="metric"><Stat label="Disk write rate" value={active ? `${s.write.toFixed(2)} MB/s` : '—'} detail={`${bytes(s.buffer)} buffered (${duration(s.bufferS)})`} /></Card>
        <Card className="metric"><Stat label="Session time" value={active ? duration(s.elapsed) : '—'} detail={s.state} /></Card>
      </div>

      <div className="grid-2">
        <Card title="Video pipeline">
          <dl className="facts">
            <div><dt>Encoder</dt><dd>{active ? encoderName(s.encoder) : '—'}</dd></div>
            <div><dt>Scaling path</dt><dd>{active ? s.scaling || '—' : '—'} {active && s.scaling.startsWith('GPU zero-copy') && <Badge tone="ok">no CPU copy</Badge>}{active && s.scaling === 'CPU fallback' && <Badge tone="warn">higher CPU</Badge>}</dd></div>
            <div><dt>Display mode</dt><dd>{s.geo || '—'}</dd></div>
            <div><dt>Encoded size</dt><dd>{active && s.ow ? `${s.ow}×${s.oh}` : '—'}</dd></div>
            <div><dt>Bitrate</dt><dd>{draft.bitrate_mbps} Mbps video · {draft.audio_bitrate_kbps} kbps per audio track</dd></div>
            <div><dt>Last frame / last segment</dt><dd>{active ? `${s.frameAge} ms / ${(s.segAge / 1000).toFixed(1)} s ago` : '—'}</dd></div>
            <div><dt>Encode latency</dt><dd className="muted">Not measured yet</dd></div>
            <div><dt>GPU encoder load</dt><dd className="muted">Not measured yet</dd></div>
          </dl>
          {active && s.detail && <details className="small"><summary>Faster paths that were not usable</summary><p className="mono">{s.detail}</p></details>}
        </Card>

        <Card title="Audio sources">
          {!active || names.length === 0 ? <Empty icon={Gauge} title={names.length ? 'Capture is off' : 'No audio sources'} /> : (
            <div className="table-wrap">
              <table className="table">
                <thead><tr><th>Source</th><th>Packets</th><th>Breaks</th><th>Clock ppm</th><th>Delivered</th></tr></thead>
                <tbody>
                  {names.map((n, i) => (
                    <tr key={i}>
                      <td>{n}</td>
                      <td>{(audio.packets[i] ?? 0).toLocaleString()}</td>
                      <td className={(audio.disc[i] ?? 0) > 0 ? 'warn-text' : ''}>{audio.disc[i] ?? 0}</td>
                      <td>{(audio.drift[i] ?? 0).toFixed(1)}</td>
                      <td>{bytes(audio.written[i] ?? 0)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <p className="muted small">Breaks count late audio after a stall, timing jumps and device-reported glitches. Clock ppm is the drift correction applied to keep audio in sync with video.</p>
        </Card>
      </div>
    </Section>
  );
}
