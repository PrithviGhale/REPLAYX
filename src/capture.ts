import { useCallback } from 'react';
import { api, type ActionName, type Probe } from './api';
import { useApp } from './context';
import { duration } from './format';

/** Capture actions shared by the header dock, dashboard and tray-like controls. */
export function useCapture() {
  const { dirty, save, run, refreshClips, busy } = useApp();
  const go = useCallback(async (name: ActionName) => {
    // Starting capture uses the settings on screen: save pending edits first.
    if (dirty && name.startsWith('start') && !(await save())) return;
    const label = ({
      start_replay: 'Starting Instant Replay', stop_replay: 'Stopping Instant Replay', start_recording: 'Starting recording',
      stop_recording: 'Finalizing recording', save_replay: 'Saving replay', stop_all: 'Stopping capture', recover: 'Recovering',
    } as Record<ActionName, string>)[name];
    const result = await run(label, () => api.action<{ file?: string; probe?: Probe }>(name));
    if (result && name === 'recover') await refreshClips().catch(() => undefined);
    return result;
  }, [dirty, save, run, refreshClips]);
  const saved = (r: { file?: string; probe?: Probe } | undefined) =>
    r?.probe ? `${r.file} · ${duration(r.probe.duration)} · ready` : undefined;
  return { go, busy, saved };
}
