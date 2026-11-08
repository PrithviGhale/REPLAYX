import { useRef, useSyncExternalStore } from 'react';
import { api, emptyStatus, native, type Status } from './api';

/** Polled engine status with selector subscriptions, so a component re-renders
 * only when the slice it reads changes. Polls fast only while capturing. */
type Sample = { t: number; fps: number; cpu: number; mem: number; dropped: number };
let status: Status = emptyStatus;
let history: Sample[] = [];
let pollError: string | null = null;
const listeners = new Set<() => void>();
let timer: number | undefined;

const isActive = (s: Status) => s.replay || s.recording || s.saving > 0 || s.pending_recording || s.state === 'Starting' || s.state === 'Stopping' || s.state === 'Recovering';

/** Click → "Recording"/"Replay on" as the UI saw it (includes polling delay). */
let startRequest: { at: number; recording: boolean } | null = null;
let uiStart: { ms: number; recording: boolean } | null = null;
export function markStartRequest(recording: boolean) {
  startRequest = { at: performance.now(), recording };
}

async function poll() {
  try {
    const next = await api.status();
    status = next;
    pollError = null;
    if (startRequest) {
      const live = startRequest.recording ? next.recording && next.state === 'Recording' : next.state === 'Buffering';
      if (live) { uiStart = { ms: Math.round(performance.now() - startRequest.at), recording: startRequest.recording }; startRequest = null; }
      else if (next.state === 'Error' || next.state === 'Stopped' && performance.now() - startRequest.at > 2000) startRequest = null;
    }
    if (isActive(next)) {
      history = [...history.slice(-239), { t: Date.now(), fps: next.fps, cpu: next.cpu_percent, mem: next.memory_mb, dropped: next.dropped_frames }];
    }
  } catch (e) {
    pollError = String(e);
  }
  listeners.forEach(l => l());
  // While REPLAYX is hidden (a game in front, minimised, in the tray) nothing is
  // drawn, so poll every 2 s: the window should cost the game nothing.
  const hidden = document.visibilityState === 'hidden';
  timer = window.setTimeout(poll, hidden ? 2000 : isActive(status) ? 250 : 1000);
}

export function startPolling() {
  if (!native || timer !== undefined) return;
  // Back to fast polling as soon as the window is visible again.
  document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') refreshNow(); });
  void poll();
}
export const refreshNow = () => { if (native) { window.clearTimeout(timer); void poll(); } };

function subscribe(l: () => void) { listeners.add(l); return () => { listeners.delete(l); }; }

function shallowEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || !a || !b) return false;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every(k => shallowEqual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

/** Select part of the status; re-render only when it changes (shallow). */
export function useStatus<T>(select: (s: Status) => T): T {
  const last = useRef<{ v: T } | null>(null);
  const get = () => {
    const v = select(status);
    if (last.current && shallowEqual(last.current.v, v)) return last.current.v;
    last.current = { v };
    return v;
  };
  return useSyncExternalStore(subscribe, get, get);
}
export const useHistory = () => useSyncExternalStore(subscribe, () => history, () => history);
export const usePollError = () => useSyncExternalStore(subscribe, () => pollError, () => pollError);
export const useUiStart = () => useSyncExternalStore(subscribe, () => uiStart, () => uiStart);
export const activeOf = isActive;
