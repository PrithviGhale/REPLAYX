import { test } from 'node:test';
import assert from 'node:assert/strict';
import { visibleNotice } from '../src/notices.ts';

// Engine side (Status::set_notice): a new message or a returning one gets a
// new id; the same message reported again while showing keeps its id.
function engine() {
  let notice: string | null = null;
  let id = 0;
  return {
    set(text: string) { if (notice !== text) { id += 1; notice = text; } },
    clear() { notice = null; },
    get: () => ({ notice, id }),
  };
}

test('closing hides only that occurrence; the same message coming back shows again', () => {
  const e = engine();
  let closed: number | null = null;
  const shown = () => visibleNotice(e.get().notice, e.get().id, closed)?.text ?? null;

  e.set('Your clip is saved. Find it in Clips.');
  assert.equal(shown(), 'Your clip is saved. Find it in Clips.');
  closed = e.get().id; // ×
  assert.equal(shown(), null);

  // Before the engine has cleared it, repeated reports of the same message
  // never re-open it (no spam).
  e.set('Your clip is saved. Find it in Clips.');
  e.set('Your clip is saved. Find it in Clips.');
  assert.equal(shown(), null);

  // The engine clears it, then the same message happens again later: a new
  // occurrence, shown again (the 0.3.1-alpha.9 review: it stayed hidden).
  e.clear();
  assert.equal(shown(), null);
  e.set('Your clip is saved. Find it in Clips.');
  assert.equal(shown(), 'Your clip is saved. Find it in Clips.');
});

test('closing one message never hides a different one', () => {
  const e = engine();
  let closed: number | null = null;
  const shown = () => visibleNotice(e.get().notice, e.get().id, closed)?.text ?? null;
  e.set('Your sound device changed, so REPLAYX is reconnecting it.');
  closed = e.get().id;
  e.set('Your screen changed or was disconnected. Nothing you recorded is lost.');
  assert.equal(shown(), 'Your screen changed or was disconnected. Nothing you recorded is lost.');
  // And the first one returning is a new occurrence too.
  e.set('Your sound device changed, so REPLAYX is reconnecting it.');
  assert.equal(shown(), 'Your sound device changed, so REPLAYX is reconnecting it.');
});

test('no message, nothing shown', () => {
  assert.equal(visibleNotice(null, 3, null), null);
  assert.equal(visibleNotice('', 3, null), null);
  assert.deepEqual(visibleNotice('Hi', 3, 2), { text: 'Hi', id: 3 });
});
