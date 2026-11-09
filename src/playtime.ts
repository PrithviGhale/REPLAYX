import type { PlaySession } from './api';

/** Playtime analytics, pure and in the user's local time zone. Sessions are
 * split at local midnights (daylight-saving days are 23 or 25 hours long:
 * durations come from real instants, never from clock arithmetic). */

export type RangeKind = 'today' | 'yesterday' | 'last7' | 'week' | 'last30' | 'month' | 'all' | 'custom';
export const RANGES: { value: RangeKind; label: string }[] = [
  { value: 'today', label: 'Today' }, { value: 'yesterday', label: 'Yesterday' }, { value: 'last7', label: 'Last 7 days' },
  { value: 'week', label: 'This week' }, { value: 'last30', label: 'Last 30 days' }, { value: 'month', label: 'This month' },
  { value: 'all', label: 'All time' }, { value: 'custom', label: 'Custom' },
];

const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
export const dayKey = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
/** Local date from 'YYYY-MM-DD'. */
export const fromKey = (k: string) => { const [y, m, d] = k.split('-').map(Number); return new Date(y, m - 1, d); };

/** [from, to) in Unix ms for a range; weeks start on Monday. */
export function range(kind: RangeKind, now: Date, custom?: { from: string; to: string }): { from: number; to: number } {
  const today = midnight(now);
  switch (kind) {
    case 'today': return { from: +today, to: +addDays(today, 1) };
    case 'yesterday': return { from: +addDays(today, -1), to: +today };
    case 'last7': return { from: +addDays(today, -6), to: +addDays(today, 1) };
    case 'week': { const back = (today.getDay() + 6) % 7; return { from: +addDays(today, -back), to: +addDays(today, 1) }; }
    case 'last30': return { from: +addDays(today, -29), to: +addDays(today, 1) };
    case 'month': return { from: +new Date(now.getFullYear(), now.getMonth(), 1), to: +addDays(today, 1) };
    case 'custom': {
      if (!custom?.from || !custom.to) return { from: +today, to: +addDays(today, 1) };
      const a = fromKey(custom.from), b = fromKey(custom.to);
      const [lo, hi] = a <= b ? [a, b] : [b, a];
      return { from: +lo, to: +addDays(hi, 1) };
    }
    default: return { from: 0, to: +addDays(today, 1) };
  }
}

/** Seconds per local day and game for the part of each session in [from, to). */
export function byDay(sessions: PlaySession[], from: number, to: number): Map<string, Map<string, number>> {
  const out = new Map<string, Map<string, number>>();
  for (const s of sessions) {
    let a = Math.max(s.start, from);
    const end = Math.min(s.start + s.seconds * 1000, to);
    while (a < end) {
      const day = midnight(new Date(a));
      const next = Math.min(+addDays(day, 1), end);
      const key = dayKey(day);
      const games = out.get(key) ?? new Map<string, number>();
      games.set(s.game, (games.get(s.game) ?? 0) + (next - a) / 1000);
      out.set(key, games);
      a = next;
    }
  }
  return out;
}

export type GameTotal = { game: string; seconds: number; sessions: number; last: number };
/** Per-game totals in [from, to), most played first. */
export function totals(sessions: PlaySession[], from: number, to: number): { total: number; games: GameTotal[] } {
  const map = new Map<string, GameTotal>();
  for (const s of sessions) {
    const a = Math.max(s.start, from), b = Math.min(s.start + s.seconds * 1000, to);
    if (b <= a) continue;
    const g = map.get(s.game) ?? { game: s.game, seconds: 0, sessions: 0, last: 0 };
    g.seconds += (b - a) / 1000;
    g.sessions += 1;
    g.last = Math.max(g.last, s.start + s.seconds * 1000);
    map.set(s.game, g);
  }
  const games = [...map.values()].sort((x, y) => y.seconds - x.seconds || x.game.localeCompare(y.game));
  return { total: games.reduce((n, g) => n + g.seconds, 0), games };
}

/** Daily series for a chart: every local day in [from, to), zeros included. */
export function dailySeries(sessions: PlaySession[], from: number, to: number, game?: string): { day: string; seconds: number }[] {
  const days = byDay(game ? sessions.filter(s => s.game === game) : sessions, from, to);
  const out: { day: string; seconds: number }[] = [];
  for (let d = midnight(new Date(from)); +d < to; d = addDays(d, 1)) {
    const games = days.get(dayKey(d));
    out.push({ day: dayKey(d), seconds: games ? [...games.values()].reduce((a, b) => a + b, 0) : 0 });
  }
  return out;
}

/** "25 h 4 min", "12 min", "under a minute". */
export function hours(seconds: number): string {
  const m = Math.floor(seconds / 60);
  if (m < 1) return seconds > 0 ? 'under a minute' : '0 min';
  const h = Math.floor(m / 60);
  return h ? `${h} h${m % 60 ? ` ${m % 60} min` : ''}` : `${m} min`;
}
