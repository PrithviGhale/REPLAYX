import { useState } from 'react';
import { HardDrive, LifeBuoy, RotateCcw, Trash2 } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../context';
import { activeOf, useStatus } from '../store';
import { bytes, duration, durationLabel } from '../format';
import { Button, Card, Field, Kbd, Segmented } from '../ui';
import { Section } from './shared';

const LENGTHS = [30, 60, 180, 300, 600, 900, 1800];

export function ReplayPage() {
  const { draft, patch, run, refreshClips, busy } = useApp();
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const active = useStatus(activeOf);
  const s = useStatus(x => ({ buffer: x.buffer_seconds, bytes: x.buffer_bytes, replay: x.replay, recovery: x.recovery_sessions }));
  const tracks = draft.audio.filter(a => a.enabled).length;
  const audioTracks = tracks === 0 ? 0 : draft.separate_tracks ? tracks + 1 : 1;
  const bytesPerSecond = (draft.bitrate_mbps * 1e6 + audioTracks * draft.audio_bitrate_kbps * 1000) / 8 * 1.03;
  const needed = bytesPerSecond * draft.replay_seconds;
  const budget = draft.buffer_mb * 1024 * 1024;
  const covered = Math.floor(budget / bytesPerSecond);
  const short = covered < draft.replay_seconds;

  return (
    <Section>
      <div className="grid-2">
        <Card title="Replay length" subtitle="How much of the past a single save keeps.">
          <Segmented label="Replay length" value={draft.replay_seconds} disabled={active} onChange={v => patch({ replay_seconds: v })}
            options={LENGTHS.map(n => ({ value: n, label: durationLabel(n) }))} />
          <div className="buffer-bar" aria-label="Current buffer">
            <span style={{ width: `${Math.min(100, (s.buffer / draft.replay_seconds) * 100)}%` }} />
          </div>
          <div className="bar-legend"><span>{s.replay ? `${duration(s.buffer)} buffered` : 'Buffer empty — replay is off'}</span><span>{bytes(s.bytes)} on disk</span></div>
          <p className="muted small">Saves use complete 2-second segments, so a clip can end up to about two seconds before you press save. Saving never stops buffering.</p>
        </Card>

        <Card title="Disk budget" subtitle="The rolling buffer lives on disk, not in memory.">
          <Field label="Maximum buffer size" hint={`About ${bytes(needed)} needed for ${durationLabel(draft.replay_seconds)} at ${draft.bitrate_mbps} Mbps with ${audioTracks} audio track${audioTracks === 1 ? '' : 's'}.`}>
            <div className="range-row">
              <input type="range" min={256} max={16384} step={256} value={Math.min(draft.buffer_mb, 16384)} disabled={active}
                onChange={e => patch({ buffer_mb: Number(e.target.value) })} aria-label="Buffer size" />
              <span className="range-value">{bytes(budget)}</span>
            </div>
          </Field>
          <div className={`banner ${short ? 'warn' : 'ok'}`}>
            <HardDrive size={15} />
            {short ? `This budget holds only about ${duration(covered)}. Raise it or lower the bitrate to keep the full ${durationLabel(draft.replay_seconds)}.`
              : `Covers the full ${durationLabel(draft.replay_seconds)} (room for about ${duration(covered)}).`}
          </div>
        </Card>
      </div>

      <Card title="How Instant Replay works">
        <ol className="steps">
          <li><RotateCcw size={16} /><div><strong>Turn it on before you play.</strong> REPLAYX starts keeping a rolling window of your screen and sound. Nothing from before it was enabled can be saved.</div></li>
          <li><Kbd combo={draft.replay_hotkey} /><div><strong>Press the save shortcut</strong> (or <em>Save replay</em>) right after something happens. Saves run in the background and are fully decoded before they count as saved.</div></li>
          <li><span className="step-num">3</span><div><strong>Find it in Clips.</strong> Buffering continues, so you can save again moments later — overlapping saves are fine.</div></li>
        </ol>
      </Card>

      <Card title="Recover interrupted sessions" subtitle="After a crash, a forced close or power loss, finished segments stay on disk."
        actions={<>
          <Button icon={Trash2} variant="ghost" disabled={active || !!busy || s.recovery === 0} onClick={() => setConfirmDiscard(true)}>Discard…</Button>
          <Button icon={LifeBuoy} disabled={active || !!busy || s.recovery === 0}
            onClick={() => run('Recovering sessions', async () => {
              const done = await api.action<string[]>('recover');
              await refreshClips();
              return done;
            }, 'Recovery finished. Recovered clips are in your library.')}>Recover{s.recovery ? ` ${s.recovery}` : ''}</Button>
        </>}>
        <p className="muted small">{s.recovery ? `${s.recovery} session${s.recovery > 1 ? 's' : ''} found. Recover saves each as a clip after checking it; unreadable pieces are set aside, not deleted.` : 'Nothing to recover.'}</p>
        {confirmDiscard && s.recovery > 0 && (
          <div className="banner warn" role="alertdialog" aria-label="Discard interrupted sessions">
            <span>Move {s.recovery} interrupted session{s.recovery > 1 ? 's' : ''} to the Recycle Bin without recovering? You can restore them from the Recycle Bin.</span>
            <Button variant="danger" disabled={!!busy} onClick={() => { setConfirmDiscard(false); void run('Moving sessions to the Recycle Bin', () => api.action<{ moved: number }>('discard_sessions'), 'Sessions moved to the Recycle Bin.'); }}>Move to Recycle Bin</Button>
            <Button variant="ghost" onClick={() => setConfirmDiscard(false)}>Keep</Button>
          </div>
        )}
      </Card>
    </Section>
  );
}
