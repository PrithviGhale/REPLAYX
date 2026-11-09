import { test } from 'node:test';
import assert from 'node:assert/strict';
import { byDay, dailySeries, dayKey, hours, range, totals } from '../src/playtime.ts';

const at = (y: number, m: number, d: number, h = 0, min = 0) => +new Date(y, m - 1, d, h, min);
const session = (game: string, start: number, seconds: number) => ({ id: `${game}${start}`, game, start, seconds, source: 'test' });

test('sessions are split at local midnight', () => {
  // 23:00 to 01:30 the next day: 1 h on the first day, 1.5 h on the second.
  const s = [session('Roblox', at(2026, 11, 7, 23), 2.5 * 3600)];
  const days = byDay(s, 0, at(2027, 1, 1));
  assert.equal(days.get('2026-11-07')?.get('Roblox'), 3600);
  assert.equal(days.get('2026-11-08')?.get('Roblox'), 5400);
});

test('a daylight-saving day counts its real length', () => {
  // A whole local day of play: whatever the time zone, the day holds exactly
  // the real time between its midnights (25 h where clocks fall back).
  const start = at(2026, 11, 1), end = at(2026, 11, 2);
  const s = [session('Fortnite', start, (end - start) / 1000)];
  assert.equal(byDay(s, 0, at(2027, 1, 1)).get('2026-11-01')?.get('Fortnite'), (end - start) / 1000);
});

test('ranges: today, yesterday, this week (Monday), this month, custom', () => {
  const now = new Date(2026, 10, 8, 15, 30); // Sunday 8 Nov 2026
  assert.deepEqual(range('today', now), { from: at(2026, 11, 8), to: at(2026, 11, 9) });
  assert.deepEqual(range('yesterday', now), { from: at(2026, 11, 7), to: at(2026, 11, 8) });
  assert.equal(range('week', now).from, at(2026, 11, 2), 'week starts Monday 2 Nov');
  assert.equal(range('month', now).from, at(2026, 11, 1));
  assert.equal(range('last7', now).from, at(2026, 11, 2));
  assert.deepEqual(range('custom', now, { from: '2026-11-05', to: '2026-11-03' }), { from: at(2026, 11, 3), to: at(2026, 11, 6) }, 'reversed dates are fine');
});

test('totals only count the part inside the period, most played first', () => {
  const s = [
    session('Roblox', at(2026, 11, 7, 23), 2 * 3600), // 1 h falls on the 8th
    session('Valorant', at(2026, 11, 8, 10), 1800),
    session('Roblox', at(2026, 11, 8, 12), 3600),
  ];
  const r = range('today', new Date(2026, 10, 8, 20));
  const t = totals(s, r.from, r.to);
  assert.equal(t.total, 3600 + 1800 + 3600);
  assert.deepEqual(t.games.map(g => [g.game, g.seconds, g.sessions]), [['Roblox', 7200, 2], ['Valorant', 1800, 1]]);
  assert.equal(totals([], r.from, r.to).total, 0, 'no data, no invented time');
});

test('daily series has every day, zeros included', () => {
  const r = range('last7', new Date(2026, 10, 8, 12));
  const series = dailySeries([session('Roblox', at(2026, 11, 5, 18), 600)], r.from, r.to);
  assert.equal(series.length, 7);
  assert.equal(series.find(d => d.day === '2026-11-05')?.seconds, 600);
  assert.equal(series.filter(d => d.seconds === 0).length, 6);
  assert.equal(dayKey(new Date(2026, 0, 5)), '2026-01-05');
});

test('durations read as hours and minutes', () => {
  assert.equal(hours(25 * 3600 + 4 * 60), '25 h 4 min');
  assert.equal(hours(12 * 60), '12 min');
  assert.equal(hours(3600), '1 h');
  assert.equal(hours(20), 'under a minute');
  assert.equal(hours(0), '0 min');
});
