import type { Clip } from './api';

/** Clip thumbnails for the session, keyed by the clip's identity and file
 * version: a renamed, moved, re-saved or replaced clip gets a new key and
 * therefore its own image. Requests in flight are shared. */
export const thumbKey = (c: Pick<Clip, 'id' | 'modified' | 'bytes'>) => `${c.id}\u0000${c.modified}\u0000${c.bytes}`;

const done = new Map<string, string>();
const pending = new Map<string, Promise<string>>();

export const cachedThumb = (key: string) => done.get(key);

/** Load once per key; a failed load can be retried later. */
export function loadThumb(key: string, fetch: () => Promise<string>): Promise<string> {
  const hit = done.get(key);
  if (hit) return Promise.resolve(hit);
  let p = pending.get(key);
  if (!p) {
    p = fetch().then(url => { done.set(key, url); return url; }).finally(() => pending.delete(key));
    pending.set(key, p);
  }
  return p;
}

/** Forget a clip's images (deleted, renamed or moved). */
export function forgetThumbs(id: string) {
  for (const k of [...done.keys()]) if (k.startsWith(`${id}\u0000`)) done.delete(k);
}

/** Only apply a result if it still belongs to what is displayed now. */
export const applies = (requested: string, current: string) => requested === current;

export const _resetForTests = () => { done.clear(); pending.clear(); };
