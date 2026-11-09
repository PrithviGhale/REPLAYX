import { test } from 'node:test';
import assert from 'node:assert/strict';
import { _resetForTests, applies, cachedThumb, forgetThumbs, loadThumb, thumbKey } from '../src/thumbs.ts';

const clip = (id: string, modified = 1, bytes = 100) => ({ id, modified, bytes });
const deferred = () => { let resolve!: (v: string) => void; const p = new Promise<string>(r => { resolve = r; }); return { p, resolve }; };

test('every clip has its own key; a replaced or renamed clip gets a new one', () => {
  assert.notEqual(thumbKey(clip('Roblox/a.mkv')), thumbKey(clip('Roblox/b.mkv')));
  assert.notEqual(thumbKey(clip('a.mkv', 1)), thumbKey(clip('a.mkv', 2)), 're-saved');
  assert.notEqual(thumbKey(clip('a.mkv', 1, 100)), thumbKey(clip('a.mkv', 1, 101)), 'replaced');
  assert.notEqual(thumbKey(clip('Roblox/a.mkv')), thumbKey(clip('Fortnite/a.mkv')), 'moved to another game');
});

test('rapid switching: a late answer for the previous clip is never shown', async () => {
  _resetForTests();
  const a = thumbKey(clip('A.mkv')), b = thumbKey(clip('B.mkv'));
  const slowA = deferred(), fastB = deferred();
  let shown: string | undefined;
  let current = a;
  const pa = loadThumb(a, () => slowA.p).then(u => { if (applies(a, current)) shown = u; });
  current = b; // user selects B before A's image arrives
  const pb = loadThumb(b, () => fastB.p).then(u => { if (applies(b, current)) shown = u; });
  fastB.resolve('image-B');
  await pb;
  slowA.resolve('image-A');
  await pa;
  assert.equal(shown, 'image-B');
  assert.equal(cachedThumb(a), 'image-A', 'A is still cached for when it is shown again');
});

test('requests for the same clip are shared, and failures can be retried', async () => {
  _resetForTests();
  const k = thumbKey(clip('C.mkv'));
  let calls = 0;
  const d = deferred();
  const p1 = loadThumb(k, () => { calls++; return d.p; });
  const p2 = loadThumb(k, () => { calls++; return d.p; });
  d.resolve('image-C');
  assert.deepEqual(await Promise.all([p1, p2]), ['image-C', 'image-C']);
  assert.equal(calls, 1);
  const bad = thumbKey(clip('D.mkv'));
  await assert.rejects(loadThumb(bad, () => Promise.reject(new Error('corrupt'))));
  assert.equal(await loadThumb(bad, () => Promise.resolve('image-D')), 'image-D', 'retry after a failure');
});

test('forgetting a clip drops only its images', async () => {
  _resetForTests();
  await loadThumb(thumbKey(clip('E.mkv')), () => Promise.resolve('e'));
  await loadThumb(thumbKey(clip('E.mkv.old')), () => Promise.resolve('x'));
  forgetThumbs('E.mkv');
  assert.equal(cachedThumb(thumbKey(clip('E.mkv'))), undefined);
  assert.equal(cachedThumb(thumbKey(clip('E.mkv.old'))), 'x');
});
