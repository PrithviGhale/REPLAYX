import { useSyncExternalStore } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, native, VERSION, type UpdateStatus } from './api';

/** The engine's update status, kept current by its `update-status` event.
 * The interface only shows what the engine reports: no local guesses. */
export const emptyUpdate: UpdateStatus = {
  stage: 'idle', current: VERSION, latest: null, notes: null, date: null, downloaded: 0, total: null, error: null, detail: null,
  last_checked: null, simulated: false, configured: false, installable: false, waiting: false, prompt: false, release_url: null,
};
let status: UpdateStatus = emptyUpdate;
const listeners = new Set<() => void>();
let started = false;

function set(next: UpdateStatus) { status = next; listeners.forEach(l => l()); }
export function startUpdates() {
  if (!native || started) return;
  started = true;
  api.updateStatus().then(set).catch(() => undefined);
  void listen<UpdateStatus>('update-status', e => set(e.payload));
}
function subscribe(l: () => void) { listeners.add(l); return () => { listeners.delete(l); }; }
export function useUpdate(): UpdateStatus { return useSyncExternalStore(subscribe, () => status); }
/** A command's answer is the newest status too. */
export const applyUpdate = set;

/** Development builds (vite dev server) show the update preview with test data. */
export const DEV = !!(import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV;
