import { test } from 'node:test';
import assert from 'node:assert/strict';
import { describeError } from '../src/errors.ts';

const navigates = (e: ReturnType<typeof describeError>) =>
  e.actions.flatMap(a => (a.kind === 'navigate' ? [`${a.to.page}${a.to.tab ? `:${a.to.tab}` : ''}${a.to.anchor ? `#${a.to.anchor}` : ''}`] : []));

test('the reported system-audio error becomes a plain card with working actions', () => {
  const raw = "System audio couldn't connect to the sound device: it was changing or unavailable for 10 seconds. Reconnect the headset or check Windows Sound settings, then try again. (Technical: System audio: Windows returned an error: 0x88890004)";
  const e = describeError(raw);
  assert.equal(e.category, 'audio');
  assert.equal(e.title, 'Computer audio unavailable');
  assert.doesNotMatch(e.title + e.message, /0x|Windows returned/);
  assert.deepEqual(navigates(e), ['audio#sound-sources']);
  assert.ok(e.actions.some(a => a.kind === 'windows' && a.page === 'sound'));
  assert.ok(e.actions.some(a => a.kind === 'retry'));
  assert.equal(e.technical, raw, 'technical details keep the code');
  // The previous wording of the same failure maps the same way.
  assert.equal(describeError('System audio: Windows returned an error: 0x88890004. The audio device was changing or not ready, and still wasn\'t after several retries.').category, 'audio');
});

test('microphone problems lead to the microphone settings, permissions to Windows privacy', () => {
  const missing = describeError('Microphone packets stopped. Check Windows microphone permission and the selected device, then restart capture.');
  assert.equal(missing.category, 'microphone-permission');
  const busy = describeError("Microphone couldn't start. (Technical: Microphone: Windows returned an error: 0x88890026)");
  assert.equal(busy.category, 'microphone');
  assert.deepEqual(navigates(busy), ['audio#microphone']);
  const denied = describeError('Microphone: access is denied (0x80070005)');
  assert.ok(denied.actions.some(a => a.kind === 'windows' && a.page === 'microphone-privacy'));
});

test('hotkeys, storage, saves, encoders and displays each lead to their own setting', () => {
  const hotkey = describeError('Control+Shift+F10 is already used by another application. Change that application\'s hotkey or choose another shortcut.');
  assert.equal(hotkey.severity, 'warning');
  assert.deepEqual(navigates(hotkey), ['settings:hotkeys']);
  assert.deepEqual(navigates(describeError('Not enough free space on the clips drive (2.1 GB left).')), ['settings:general#output-folder']);
  const save = describeError('Save failed: remux error. The footage was kept for recovery.');
  assert.equal(save.category, 'save');
  assert.ok(save.actions.some(a => a.kind === 'folder'));
  assert.deepEqual(navigates(describeError('Selected encoder failed its encode probe')), ['capture#encoder']);
  assert.deepEqual(navigates(describeError('Selected display disconnected or unavailable on the default DXGI adapter')), ['capture#display']);
  assert.equal(describeError('Capture did not start within 60 s (display, GPU encoder or audio device did not respond).').category, 'start');
});

test('unknown errors still read as a sentence, never as a stack of codes', () => {
  const e = describeError('Unexpected thing happened at step 3. More internals here 0x80004005.');
  assert.equal(e.category, 'unknown');
  assert.equal(e.title, 'Something went wrong');
  assert.equal(e.message, 'Unexpected thing happened at step 3.');
  assert.equal(describeError('x').key, describeError('x').key, 'identical problems share a key (no repeated toasts)');
});
