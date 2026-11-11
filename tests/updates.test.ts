import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyMode, formatBytes, hasUpdate, highlights, lastChecked, microphoneOn, modeOf, normalize, percent, progressText, stageLabel } from '../src/update-logic.ts';

const prefs = { auto_check: true, interval_hours: 6, auto_download: false, auto_install: false, notify: true, idle_minutes: 5 };
const status = (over: object = {}) => ({
  stage: 'idle', current: '0.3.1-alpha.6', latest: null, notes: null, date: null, downloaded: 0, total: null, error: null, detail: null,
  last_checked: null, simulated: false, configured: true, installable: true, waiting: false, prompt: false, release_url: null, ...over,
}) as Parameters<typeof hasUpdate>[0];

test('the default is Notify me; modes map to the switches and back', () => {
  assert.equal(modeOf(prefs), 'notify');
  const manual = applyMode(prefs, 'manual');
  assert.deepEqual([manual.auto_check, manual.auto_download, manual.auto_install], [false, false, false]);
  assert.equal(modeOf(manual), 'manual');
  const auto = applyMode(prefs, 'auto');
  assert.deepEqual([auto.auto_check, auto.auto_download, auto.auto_install], [true, true, true]);
  assert.equal(modeOf(auto), 'auto');
  assert.equal(modeOf(applyMode(auto, 'notify')), 'notify');
  assert.equal(modeOf({ ...prefs, auto_download: true }), 'custom', 'download without install is its own combination');
  assert.equal(applyMode(prefs, 'auto').interval_hours, 6, 'a mode never changes the schedule');
});

test('automatic installing is never on without downloading and checking', () => {
  assert.deepEqual(normalize({ ...prefs, auto_check: false, auto_download: true, auto_install: true }), { ...prefs, auto_check: false, auto_download: false, auto_install: false });
  assert.equal(normalize({ ...prefs, auto_download: false, auto_install: true }).auto_install, false);
  assert.equal(normalize({ ...prefs, auto_download: true, auto_install: true }).auto_install, true);
});

test('the Settings dot shows only while a newer version is known', () => {
  assert.equal(hasUpdate(status()), false);
  assert.equal(hasUpdate(status({ stage: 'up_to_date' })), false);
  assert.equal(hasUpdate(status({ stage: 'failed', error: 'offline' })), false);
  for (const stage of ['available', 'downloading', 'verifying', 'ready', 'installing']) {
    assert.equal(hasUpdate(status({ stage, latest: '0.3.1-alpha.7' })), true, stage);
  }
  assert.equal(hasUpdate(status({ stage: 'available', latest: null })), false, 'no version, no dot');
});

test('progress is the real byte count, never an invented percentage', () => {
  assert.equal(percent({ downloaded: 0, total: null }), null, 'unknown size: no percentage');
  assert.equal(percent({ downloaded: 0, total: 61_138_175 }), 0);
  assert.equal(percent({ downloaded: 30_569_087, total: 61_138_175 }), 49);
  assert.equal(percent({ downloaded: 61_138_175, total: 61_138_175 }), 100);
  assert.equal(percent({ downloaded: 99, total: 10 }), 100, 'never above 100');
  assert.equal(progressText({ downloaded: 26_400_000, total: 61_138_175 }), '43% · 25.2 MB of 58.3 MB');
  assert.equal(progressText({ downloaded: 1_500_000, total: null }), '1.4 MB downloaded');
  assert.equal(formatBytes(512), '512 B');
  assert.equal(formatBytes(2048), '2 KB');
  assert.equal(formatBytes(3 * 1024 ** 3), '3.00 GB');
});

test('every stage has a clear label', () => {
  assert.equal(stageLabel(status({ stage: 'available', latest: '0.3.1-alpha.7' })), 'Update available — v0.3.1-alpha.7');
  assert.equal(stageLabel(status({ stage: 'ready' })), 'Update ready to install');
  assert.equal(stageLabel(status({ stage: 'ready', waiting: true })), 'Update ready — waiting for a safe time to install');
  for (const stage of ['idle', 'checking', 'up_to_date', 'downloading', 'verifying', 'installing', 'failed']) assert.ok(stageLabel(status({ stage })).length > 5, stage);
});

test("what's new comes from the release notes' bullet points", () => {
  const notes = "## What's New\n\n- **Audio** that survives `device` changes\n- Fix two\n* Fix three\nplain text\n- Four\n- Five";
  assert.deepEqual(highlights(notes, 3), ['Audio that survives device changes', 'Fix two', 'Fix three']);
  assert.deepEqual(highlights(null), []);
  assert.deepEqual(highlights('No bullets here.'), []);
});

test('last checked reads naturally', () => {
  const now = new Date(2026, 10, 11, 15, 0, 0).getTime();
  assert.equal(lastChecked(null, now), 'Never');
  assert.equal(lastChecked(now - 20_000, now), 'Just now');
  assert.equal(lastChecked(now - 60_000, now), '1 minute ago');
  assert.equal(lastChecked(now - 12 * 60_000, now), '12 minutes ago');
  assert.match(lastChecked(now - 3 * 3_600_000, now), /^Today, /);
  assert.match(lastChecked(now - 3 * 86_400_000, now), /2026/);
});

test('the microphone is On only when it is recorded and not muted', () => {
  const mic = (enabled: boolean, muted: boolean) => [{ kind: 'system' as const, enabled: true, muted: false }, { kind: 'microphone' as const, enabled, muted }];
  assert.equal(microphoneOn(mic(true, false)), true);
  assert.equal(microphoneOn(mic(true, true)), false, 'muted');
  assert.equal(microphoneOn(mic(false, false)), false, 'not recorded');
  assert.equal(microphoneOn([{ kind: 'system', enabled: true, muted: false }]), null, 'no microphone set up');
  assert.equal(microphoneOn([{ kind: 'system', enabled: true, muted: true }, { kind: 'microphone', enabled: true, muted: false }]), true, 'muting system audio does not turn the microphone off');
});
