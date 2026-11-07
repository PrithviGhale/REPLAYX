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

const isActive = (s: Status) => s.replay || s.recording || s.saving > 0 || s.state === 'Starting' || s.state === 'Recovering';

async function poll() {
  try {
    const next = await api.status();
    status = next;
    pollError = null;
    if (isActive(next)) {
      history = [...history.slice(-239), { t: Date.now(), fps: next.fps, cpu: next.cpu_percent, mem: next.memory_mb, dropped: next.dropped_frames }];
    }
  } catch (e) {
    pollError = String(e);
  }
  listeners.forEach(l => l());
  timer = window.setTimeout(poll, isActive(status) ? 250 : 1000);
}

export function startPolling() {
  if (!native || timer !== undefined) return;
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
export const activeOf = isActive;
