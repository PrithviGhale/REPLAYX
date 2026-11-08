import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isStarting, recordMode } from '../src/record.ts';

const status = (recording: boolean, pending_recording: boolean, state: string) => ({ recording, pending_recording, state });

test('a recording that is still starting can be cancelled, not started twice', () => {
  assert.equal(recordMode(status(false, false, 'Stopped')), 'record');
  assert.equal(recordMode(status(false, true, 'Starting')), 'cancel');
  // Replay already on: Record joins the encoder, then Stop.
  assert.equal(recordMode(status(false, true, 'Buffering')), 'cancel');
  assert.equal(recordMode(status(true, false, 'Starting')), 'stop');
  assert.equal(recordMode(status(true, false, 'Recording')), 'stop');
});

test('Recording is not shown before the encoder writes frames', () => {
  assert.equal(isStarting(status(true, false, 'Starting')), true);
  assert.equal(isStarting(status(false, true, 'Stopped')), true);
  assert.equal(isStarting(status(true, false, 'Recording')), false);
  assert.equal(isStarting(status(false, false, 'Error')), false);
});
