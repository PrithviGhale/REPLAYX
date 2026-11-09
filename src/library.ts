import type { Clip } from './api';

/** Pure library helpers (unit-tested in tests/library.test.ts). */

export const ALL = '__all__';

/** Category list with counts: games by size, then Mixed, Desktop, Unsorted. */
export function categories(clips: Clip[]): { name: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const c of clips) counts.set(c.category, (counts.get(c.category) ?? 0) + 1);
  const tail = ['Mixed', 'Desktop', 'Unsorted'];
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => {
      const ta = tail.indexOf(a.name), tb = tail.indexOf(b.name);
      if (ta !== tb) return ta - tb;
      return b.count - a.count || a.name.localeCompare(b.name);
    });
}

export type Sort = 'new' | 'old' | 'game-az' | 'game-za' | 'long' | 'size' | 'type';
export const SORTS: { value: Sort; label: string }[] = [
  { value: 'new', label: 'Newest first' }, { value: 'old', label: 'Oldest first' },
  { value: 'game-az', label: 'Game A to Z' }, { value: 'game-za', label: 'Game Z to A' },
  { value: 'long', label: 'Longest' }, { value: 'size', label: 'Largest' }, { value: 'type', label: 'Type' },
];
/** Local calendar day (YYYY-MM-DD) of a Unix-seconds time. */
const localDay = (secs: number) => { const d = new Date(secs * 1000); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };

/** Filter by category, kind and text (file name or game), then sort. */
export function filterClips(clips: Clip[], category: string, kind: string, query: string, sort: Sort, dates?: { from?: string; to?: string }): Clip[] {
  const q = query.trim().toLowerCase();
  const out = clips.filter(c =>
    (category === ALL || c.category === category) &&
    (kind === 'all' || c.kind === kind) &&
    (!dates?.from || localDay(c.modified) >= dates.from) &&
    (!dates?.to || localDay(c.modified) <= dates.to) &&
    (!q || c.name.toLowerCase().includes(q) || c.category.toLowerCase().includes(q) || (c.probe?.game ?? '').toLowerCase().includes(q)));
  const by: Record<Sort, (a: Clip, b: Clip) => number> = {
    new: (a, b) => b.modified - a.modified,
    old: (a, b) => a.modified - b.modified,
    long: (a, b) => (b.probe?.duration ?? 0) - (a.probe?.duration ?? 0),
    size: (a, b) => b.bytes - a.bytes,
    'game-az': (a, b) => a.category.localeCompare(b.category) || b.modified - a.modified,
    'game-za': (a, b) => b.category.localeCompare(a.category) || b.modified - a.modified,
    type: (a, b) => a.kind.localeCompare(b.kind) || b.modified - a.modified,
  };
  return out.sort((a, b) => by[sort](a, b) || a.id.localeCompare(b.id));
}

/** Insert or replace a clip by id, newest first. */
export function upsertClip(clips: Clip[], clip: Clip): Clip[] {
  return [clip, ...clips.filter(x => x.id !== clip.id)].sort((a, b) => b.modified - a.modified);
}

/** Remove one clip by id; other clips (even with the same file name) stay. */
export function removeClip(clips: Clip[], id: string): Clip[] {
  return clips.filter(c => c.id !== id);
}

/** Selection to keep after deleting: the next clip in the visible list, if any. */
export function nextSelection(visible: Clip[], deletedId: string): string | null {
  const i = visible.findIndex(c => c.id === deletedId);
  if (i < 0) return null;
  return visible[i + 1]?.id ?? visible[i - 1]?.id ?? null;
}

export type GameTile = { name: string; count: number; seconds: number; latest: number };
/** One tile per category that actually has clips (never from the game
 * catalog): games by clip count, then Mixed, Desktop and Unsorted. */
export function gameTiles(clips: Clip[]): GameTile[] {
  const map = new Map<string, GameTile>();
  for (const c of clips) {
    const t = map.get(c.category) ?? { name: c.category, count: 0, seconds: 0, latest: 0 };
    t.count += 1;
    t.seconds += c.probe?.duration ?? 0;
    t.latest = Math.max(t.latest, c.modified);
    map.set(c.category, t);
  }
  const tail = ['Mixed', 'Desktop', 'Unsorted'];
  return [...map.values()].sort((a, b) => {
    const ta = tail.indexOf(a.name), tb = tail.indexOf(b.name);
    if (ta !== tb) return ta - tb;
    return b.count - a.count || a.name.localeCompare(b.name);
  });
}
