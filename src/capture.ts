import { useCallback } from 'react';
import { api, type ActionName, type Probe } from './api';
import { useApp } from './context';
import { duration } from './format';
import { markStartRequest, refreshNow, useStatus } from './store';
import { isStarting, recordMode } from './record';

/** Capture actions shared by the header dock, dashboard and tray-like controls. */
export function useCapture() {
  const { dirty, save, run, refreshClips, busy } = useApp();
  const go = useCallback(async (name: ActionName) => {
    // Starting capture uses the settings on screen: save pending edits first.
    if (dirty && name.startsWith('start') && !(await save())) return;
    const label = ({
      start_replay: 'Starting Instant Replay', stop_replay: 'Stopping Instant Replay', start_recording: 'Starting recording',
      stop_recording: 'Finalizing recording', save_replay: 'Saving replay', stop_all: 'Stopping capture', recover: 'Recovering',
      discard_sessions: 'Moving sessions to the Recycle Bin',
    } as Record<ActionName, string>)[name];
    if (name.startsWith('start')) markStartRequest(name === 'start_recording');
    // Show the engine's "Starting" state right away instead of at the next poll.
    window.setTimeout(refreshNow, 40);
    const result = await run(label, () => api.action<{ file?: string; probe?: Probe }>(name));
    if (result && name === 'recover') await refreshClips().catch(() => undefined);
    return result;
  }, [dirty, save, run, refreshClips]);
  const saved = (r: { file?: string; probe?: Probe } | undefined) =>
    r?.probe ? `${r.file} · ${duration(r.probe.duration)} · ready` : undefined;
  return { go, busy, saved };
}

/** One Record control for every place that shows it: Record, Cancel while
 * capture is still starting (nothing recorded yet), Stop while recording. */
export function useRecordButton(stopLabel = 'Stop') {
  const { go, busy } = useCapture();
  const { toast } = useApp();
  const s = useStatus(x => ({ recording: x.recording, pending: x.pending_recording, state: x.state }));
  const starting = isStarting({ recording: s.recording, pending_recording: s.pending, state: s.state });
  const mode = recordMode({ recording: s.recording, pending_recording: s.pending });
  const onClick = async () => {
    if (mode === 'cancel') {
      // Not through `run`: the start is still holding the busy indicator.
      // If the start finished first, this is an ordinary Stop: the recording is
      // saved and reported by the clip-save events instead.
      try {
        const r = await api.action<{ cancelled?: boolean }>('stop_recording');
        if (r?.cancelled) toast('info', 'Recording cancelled before it started. Nothing was saved.');
      } catch (e) { toast('error', String(e)); }
      refreshNow();
      return;
    }
    const r = await go(mode === 'stop' ? 'stop_recording' : 'start_recording') as { cancelled?: boolean } | undefined;
    if (r?.cancelled) toast('info', 'Recording cancelled before it started. Nothing was saved.');
  };
  return {
    mode,
    starting,
    label: mode === 'cancel' ? 'Cancel' : mode === 'stop' ? stopLabel : 'Record',
    // A pending start can always be cancelled, even while busy.
    disabled: mode === 'cancel' ? false : !!busy,
    onClick,
  };
}
