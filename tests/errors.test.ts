import { test } from 'node:test';
import assert from 'node:assert/strict';
import { describeError, describeSettingsProblem } from '../src/errors.ts';
import { createGuard, retryInOrder } from '../src/retry.ts';

const navigates = (e: ReturnType<typeof describeError>) =>
  e.actions.flatMap(a => (a.kind === 'navigate' ? [`${a.to.page}${a.to.tab ? `:${a.to.tab}` : ''}${a.to.anchor ? `#${a.to.anchor}` : ''}`] : []));

test('the reported system-audio error becomes a plain card with working actions', () => {
  const raw = "System audio couldn't connect to the sound device: it was changing or unavailable for 10 seconds. Reconnect the headset or check Windows Sound settings, then try again. (Technical: System audio: Windows returned an error: 0x88890004)";
  const e = describeError(raw, { retry: ['start_replay'] });
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

test('microphone problems lead to the microphone settings; only a real denial says access is blocked', () => {
  // The engine's packet-starvation message only *advises* checking the
  // permission: it is a microphone problem, with privacy as one option.
  const starved = describeError('Microphone packets stopped. Check Windows microphone permission and the selected device, then restart capture.', { retry: ['start_replay'] });
  assert.equal(starved.category, 'microphone');
  assert.equal(starved.title, 'Microphone stopped');
  assert.doesNotMatch(starved.title + starved.message, /blocked/i);
  assert.deepEqual(navigates(starved), ['audio#microphone']);
  assert.ok(starved.actions.some(a => a.kind === 'windows' && a.page === 'microphone-privacy'));
  // Explicit denial, in the engine's real wording and in short forms.
  for (const raw of [
    "Microphone couldn't start. (Technical: Microphone: Access is denied. (0x80070005))",
    'Microphone: access is denied (0x80070005)',
    'Microphone: E_ACCESSDENIED',
  ]) {
    const denied = describeError(raw);
    assert.equal(denied.category, 'microphone-permission', raw);
    assert.equal(denied.title, 'Microphone access is blocked');
    assert.ok(denied.actions.some(a => a.kind === 'windows' && a.page === 'microphone-privacy'));
  }
  // Unplugged (element not found after the settle window), busy, and a
  // start-up timeout: device problems, never "blocked".
  for (const raw of [
    "Microphone couldn't connect to the sound device: it was changing or unavailable for 10 seconds. Reconnect the headset or check Windows Sound settings, then try again. (Technical: Microphone: Element not found. (0x80070490))",
    "Microphone couldn't start. (Technical: Microphone: Windows returned an error: 0x88890026)",
    "Microphone couldn't connect to the sound device: it did not respond. Check Windows Sound settings, then try again.",
  ]) {
    const e = describeError(raw);
    assert.equal(e.category, 'microphone', raw);
    assert.equal(e.title, 'Microphone unavailable');
    assert.deepEqual(navigates(e), ['audio#microphone']);
  }
});

test('Retry repeats exactly what failed, and is never offered as a guess', () => {
  const raw = "System audio couldn't connect to the sound device: it did not respond. Check Windows Sound settings, then try again.";
  const retryOf = (e: ReturnType<typeof describeError>) => e.actions.flatMap(a => (a.kind === 'retry' ? [[a.label, a.ops.join('+')]] : []));
  assert.deepEqual(retryOf(describeError(raw)), [], 'unknown operation: no Retry');
  assert.deepEqual(retryOf(describeError(raw, { retry: ['start_recording'] })), [['Retry recording', 'start_recording']]);
  assert.deepEqual(retryOf(describeError(raw, { retry: ['start_replay'] })), [['Retry Instant Replay', 'start_replay']]);
  assert.deepEqual(retryOf(describeError(raw, { retry: ['start_replay', 'start_recording'] })), [['Retry', 'start_replay+start_recording']]);
  // Anything that isn't a start or a save is never repeated.
  assert.deepEqual(retryOf(describeError(raw, { retry: ['stop_all', 'discard_sessions'] })), []);
  // Categories without a Retry don't gain one.
  assert.deepEqual(retryOf(describeError('Not enough free space on the clips drive (2.1 GB left).', { retry: ['start_replay'] })), []);
  // The same problem keeps its key whatever the retry, so a failed retry
  // counts on the same notice instead of stacking a new one.
  assert.equal(describeError(raw).key, describeError(raw, { retry: ['start_recording'] }).key);
});

test('a Retry runs the failed operations in order, stops at a failure and ignores double clicks', async () => {
  const done: string[] = [];
  assert.equal(await retryInOrder(['start_replay', 'start_recording'], async op => { done.push(op); return true; }), true);
  assert.deepEqual(done, ['start_replay', 'start_recording']);
  done.length = 0;
  assert.equal(await retryInOrder(['start_replay', 'start_recording'], async op => { done.push(op); return false; }), false);
  assert.deepEqual(done, ['start_replay'], 'a failed replay start never goes on to start a recording');
  assert.equal(await retryInOrder([], async () => true), false, 'nothing to retry is not a success');

  const guard = createGuard();
  let release!: () => void;
  let runs = 0;
  const first = guard.run(() => { runs++; return new Promise<string>(r => { release = () => r('done'); }); });
  assert.equal(guard.running, true);
  assert.equal(await guard.run(async () => { runs++; return 'second'; }), undefined, 'the double click is ignored');
  release();
  assert.equal(await first, 'done');
  assert.equal(runs, 1);
  assert.equal(guard.running, false);
  assert.equal(await guard.run(async () => 'later'), 'later', 'a later click works again');
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

test('unknown errors never show paths, OS errors, parser output or error chains', () => {
  const generic = /^REPLAYX hit an unexpected problem\./;
  for (const raw of [
    String.raw`Failed at C:\Private\name\session.db (os error 5)`,
    'Access is denied. (os error 5)',
    'The process cannot access the file because it is being used by another process. (os error 32)',
    'expected value at line 1 column 1',
    'Settings are corrupt; restore or remove settings.json: EOF while parsing an object at line 1 column 18',
    "Couldn't open settings.json: Access is denied. (os error 5)",
    'open failed: worker: queue closed',
    String.raw`\\server\share\clips is unavailable`,
    '/home/user/clip.mp4 missing',
    'Error 0x80004005 in step 2',
    '{"code":5}',
  ]) {
    const e = describeError(raw);
    assert.equal(e.category, 'unknown', raw);
    assert.match(e.message, generic, raw);
    assert.equal(e.technical, raw, 'the details keep everything');
  }
});

test('the settings notice is plain words, with the raw error only in the details', () => {
  const technical = String.raw`Settings are corrupt; restore or remove settings.json: EOF while parsing an object at line 1 column 18 (C:\Users\Name\AppData\Local\com.replayx.app\settings.json)`;
  const damaged = describeSettingsProblem({ kind: 'damaged', backup: 'settings.unreadable-1731400000.json', technical });
  assert.equal(damaged.title, 'Your settings were reset');
  assert.match(damaged.message, /kept as settings\.unreadable-1731400000\.json\.$/);
  assert.equal(damaged.technical, technical);
  for (const p of [damaged,
    describeSettingsProblem({ kind: 'unreadable', technical: "Couldn't open settings.json: The process cannot access the file because it is being used by another process. (os error 32)" }),
    describeSettingsProblem({ kind: 'damaged_not_kept', technical })]) {
    assert.doesNotMatch(p.title + p.message, /os error|line \d|column|AppData|Technical|\\/, p.key);
    assert.equal(p.severity, 'warning');
  }
  assert.match(describeSettingsProblem({ kind: 'unreadable', technical: '' }).message, /won't change your saved settings/);
  // A settings change refused for that reason reads plainly too.
  const refused = describeError("REPLAYX couldn't open your settings file when it started, so it isn't changing your settings file. Close any program that may be using it, then restart REPLAYX.");
  assert.equal(refused.category, 'settings');
  assert.deepEqual(refused.actions, []);
});

test('unknown errors in plain words keep their first sentence; a code anywhere makes them generic', () => {
  const e = describeError('Unexpected thing happened at step 3. Wait a moment and try again.');
  assert.equal(e.category, 'unknown');
  assert.equal(e.title, 'Something went wrong');
  assert.equal(e.message, 'Unexpected thing happened at step 3.');
  assert.match(describeError('Unexpected thing happened at step 3. More internals here 0x80004005.').message, /^REPLAYX hit an unexpected problem\./);
  assert.equal(describeError('x').key, describeError('x').key, 'identical problems share a key (no repeated toasts)');
});
