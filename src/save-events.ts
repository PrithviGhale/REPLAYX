import type { Clip } from './api';
export type SaveEvent = { id: string; stage: string; elapsed_ms: number; clip: Clip | null; error: string | null };
export function applySaveEvent(pending: SaveEvent[], event: SaveEvent): SaveEvent[] {
  if (event.stage === 'Ready') return pending.filter(x => x.id !== event.id);
  const prior = pending.find(x => x.id === event.id);
  if (prior && (prior.stage === 'Failed' || prior.elapsed_ms > event.elapsed_ms)) return pending;
  return [...pending.filter(x => x.id !== event.id), event].slice(-20);
}
export { upsertClip } from './library.ts';
