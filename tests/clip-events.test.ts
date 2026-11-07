import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applySaveEvent, upsertClip } from '../src/save-events.ts';
const event = (id: string, stage: string, elapsed_ms = 0) => ({ id, stage, elapsed_ms, clip: null, error: null });
test('concurrent saves remain independent and are not playable while pending', () => {
  const pending = applySaveEvent([event('one', 'Saving')], event('two', 'Remuxing', 20));
  assert.equal(pending.length, 2);
  assert.equal(pending.every(x => x.clip === null), true);
  assert.deepEqual(applySaveEvent(pending, event('one', 'Ready')), [event('two', 'Remuxing', 20)]);
});
test('late stages cannot overwrite failure or newer progress', () => {
  const failed = [event('one', 'Failed', 30)];
  assert.deepEqual(applySaveEvent(failed, event('one', 'Saving', 0)), failed);
  const newer = [event('two', 'Validating', 100)];
  assert.deepEqual(applySaveEvent(newer, event('two', 'Remuxing', 50)), newer);
});
test('ready events replace by name without a filesystem rescan', () => {
  const clip = {name:'Replay.mkv', modified:5, probe:null};
  assert.deepEqual(upsertClip([{...clip, modified:1}], clip), [clip]);
});
