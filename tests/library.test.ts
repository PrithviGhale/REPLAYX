import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ALL, categories, filterClips, gameTiles, nextSelection, removeClip, upsertClip } from '../src/library.ts';

const clip = (id: string, kind = 'Replay', modified = 1, bytes = 1, duration = 10) => {
  const [category, name] = id.includes('/') ? id.split('/') : ['Unsorted', id];
  return { id, name, category, kind, modified, bytes, probe: { duration, bytes, video_codec: 'h264', audio_tracks: 2, width: 1920, height: 1080, start_time: 0, audio_titles: [], fps: 60, game: category } };
};
const library = [
  clip('Fortnite/Fortnite_2026-11-07_15-30-22.mkv', 'Replay', 5),
  clip('Fortnite/Fortnite_2026-11-07_15-40-00.mkv', 'Recording', 6, 900, 600),
  clip('Roblox/Roblox_2026-11-07_16-10-45.mkv', 'Replay', 4),
  clip('Desktop/Desktop_2026-11-07_18-20-30.mkv', 'Replay', 3),
  clip('Replay-20261106-221501-55667788.mkv', 'Replay', 2),
];

test('categories are derived from clips, games first, Desktop/Unsorted last', () => {
  assert.deepEqual(categories(library), [
    { name: 'Fortnite', count: 2 }, { name: 'Roblox', count: 1 }, { name: 'Desktop', count: 1 }, { name: 'Unsorted', count: 1 },
  ]);
});

test('filter by game, kind and search text (name or game)', () => {
  assert.equal(filterClips(library, 'Fortnite', 'all', '', 'new').length, 2);
  assert.equal(filterClips(library, 'Fortnite', 'Recording', '', 'new').length, 1);
  assert.equal(filterClips(library, ALL, 'all', 'roblox', 'new')[0].category, 'Roblox');
  assert.equal(filterClips(library, ALL, 'all', 'nothing-matches', 'new').length, 0);
  assert.equal(filterClips(library, ALL, 'all', '', 'long')[0].kind, 'Recording');
  assert.equal(filterClips(library, ALL, 'all', '', 'size')[0].bytes, 900);
});

test('deleting removes exactly that clip, also from a filtered game view', () => {
  const target = 'Fortnite/Fortnite_2026-11-07_15-30-22.mkv';
  const after = removeClip(library, target);
  assert.equal(after.length, library.length - 1);
  assert.equal(after.some(c => c.id === target), false);
  assert.equal(filterClips(after, 'Fortnite', 'all', '', 'new').length, 1);
  // Cancelling (no removal) or an unknown id leaves the library untouched.
  assert.deepEqual(removeClip(library, 'Fortnite/missing.mkv'), library);
});

test('selection moves to a neighbour after delete and clears when the list empties', () => {
  const visible = filterClips(library, 'Fortnite', 'all', '', 'new');
  assert.equal(nextSelection(visible, visible[0].id), visible[1].id);
  assert.equal(nextSelection(visible, visible[1].id), visible[0].id);
  assert.equal(nextSelection([visible[0]], visible[0].id), null);
  assert.equal(nextSelection(visible, 'not/there.mkv'), null);
});

test('a moved clip (game correction) replaces its old entry', () => {
  const moved = { ...clip('Roblox/Fortnite_2026-11-07_15-30-22.mkv', 'Replay', 5) };
  const after = upsertClip(removeClip(library, 'Fortnite/Fortnite_2026-11-07_15-30-22.mkv'), moved);
  assert.equal(after.filter(c => c.name === moved.name).length, 1);
  assert.equal(categories(after).find(c => c.name === 'Roblox')?.count, 2);
});

test('game tiles exist only for games that have clips, and follow deletions', () => {
  const tiles = gameTiles(library);
  assert.deepEqual(tiles.map(t => [t.name, t.count]), [['Fortnite', 2], ['Roblox', 1], ['Desktop', 1], ['Unsorted', 1]]);
  assert.equal(tiles[0].seconds, 610, 'total clip duration');
  assert.ok(!tiles.some(t => t.name === 'Minecraft'), 'known but unrecorded games get no tile');
  const after = removeClip(library, 'Roblox/Roblox_2026-11-07_16-10-45.mkv');
  assert.ok(!gameTiles(after).some(t => t.name === 'Roblox'), 'last clip deleted: tile gone');
  assert.equal(gameTiles(after.filter(c => c.category === 'Fortnite')).length, 1);
  assert.deepEqual(gameTiles([]), []);
});

test('more sorts and a date range', () => {
  assert.deepEqual(filterClips(library, ALL, 'all', '', 'game-az').map(c => c.category).slice(0, 3), ['Desktop', 'Fortnite', 'Fortnite']);
  assert.equal(filterClips(library, ALL, 'all', '', 'game-za')[0].category, 'Unsorted');
  assert.equal(filterClips(library, ALL, 'all', '', 'type')[0].kind, 'Recording');
  const day = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  const dated = [
    { ...library[0], id: 'a', modified: Math.floor(+new Date(2026, 10, 1, 12) / 1000) },
    { ...library[1], id: 'b', modified: Math.floor(+new Date(2026, 10, 5, 23, 59) / 1000) },
    { ...library[2], id: 'c', modified: Math.floor(+new Date(2026, 10, 9, 0, 1) / 1000) },
  ];
  const inRange = filterClips(dated, ALL, 'all', '', 'old', { from: day(new Date(2026, 10, 2)), to: day(new Date(2026, 10, 5)) });
  assert.deepEqual(inRange.map(c => c.id), ['b'], 'whole local days, inclusive');
  assert.equal(filterClips(dated, ALL, 'all', '', 'old', { from: '2026-11-09' }).length, 1);
});
