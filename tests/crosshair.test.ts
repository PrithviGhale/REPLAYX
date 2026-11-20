import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { DEFAULT_HOTKEY, LIMITS, PRESETS, defaultCrosshair, geometry, matchingPreset, orderedCrosshairSaves, sanitized, validColor, withPreset, type CrosshairSettings } from '../src/crosshair.ts';

const fixture: { settings: CrosshairSettings; scale: number; geometry: unknown }[] =
  JSON.parse(fs.readFileSync(new URL('./fixtures/crosshair-shapes.json', import.meta.url), 'utf8'));

/** Equal within 2/1000 px: the engine computes in f32, the preview in f64,
 * both fixed to 1/1000 px. Structure (part kinds, counts) must match exactly. */
function close(a: unknown, b: unknown, at: string): void {
  if (typeof a === 'number' && typeof b === 'number') {
    assert.ok(Math.abs(a - b) <= 2e-3, `${at}: ${a} vs ${b}`);
  } else if (Array.isArray(a) && Array.isArray(b)) {
    assert.equal(a.length, b.length, `${at}: length`);
    a.forEach((v, i) => close(v, b[i], `${at}[${i}]`));
  } else if (a && b && typeof a === 'object' && typeof b === 'object') {
    assert.deepEqual(Object.keys(a).sort(), Object.keys(b).sort(), `${at}: keys`);
    for (const k of Object.keys(a)) close((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], `${at}.${k}`);
  } else {
    assert.equal(a, b, at);
  }
}

test('the preview draws exactly the on-screen crosshair (shared fixture from the engine)', () => {
  assert.ok(fixture.length >= 90);
  for (const { settings, scale, geometry: expected } of fixture) {
    close(geometry(settings, scale), expected, `${settings.preset}/${settings.shape}/${settings.rotation} at ${scale}`);
  }
});

test('presets match the engine and keep the rest of the settings', () => {
  assert.deepEqual(PRESETS.map(p => p.id), ['classic', 'dot', 'cross-dot', 'precision', 'circle', 't', 'micro', 'bold', 'wide', 'sniper',
    'x', 'x-dot', 'scope', 'round-dot', 'box', 'chevron']);
  const custom = { ...defaultCrosshair(), enabled: true, color: '#FF0000', offset_x: 12, visibility: 'always' as const, hotkey: 'Control+Shift+F5', display: 1,
    arm_top: false, rotation: 30, circle: true };
  for (const p of PRESETS) {
    const c = withPreset(custom, p.id);
    assert.deepEqual(sanitized(c), c, `${p.id} within limits`);
    assert.equal(matchingPreset(c), p.id);
    assert.equal(c.enabled, true);
    assert.equal(c.color, '#FF0000');
    assert.equal(c.offset_x, 12);
    assert.equal(c.visibility, 'always');
    assert.equal(c.display, 1);
  }
  assert.equal(matchingPreset({ ...withPreset(custom, 'classic'), gap: 9 }), null, 'customized');
  assert.equal(matchingPreset({ ...withPreset(custom, 'classic'), arm_left: false }), null, 'a line turned off');
  assert.equal(matchingPreset({ ...withPreset(custom, 'x'), rotation: 30 }), null, 'turned differently');
  const x = geometry({ ...withPreset(custom, 'x'), outline: false }, 1);
  assert.equal(x.fill.length, 4);
  assert.ok(x.fill.every(p => p.kind === 'quad'), 'an X is four turned lines');
});

test('lines can be chosen, turned and given a ring', () => {
  const two = { ...defaultCrosshair(), arm_top: false, arm_left: false, outline: false };
  assert.equal(geometry(two, 1).fill.length, 2);
  const quarter = geometry({ ...two, rotation: 90 }, 1);
  assert.ok(quarter.fill.every(p => p.kind === 'rect'), 'a quarter turn stays crisp');
  const none = sanitized({ ...defaultCrosshair(), arm_top: false, arm_bottom: false, arm_left: false, arm_right: false });
  assert.equal(none.dot, true, 'no lines and no ring: the dot keeps it visible');
  const ringOnly = sanitized({ ...none, dot: false, circle: true });
  assert.equal(ringOnly.dot, false, 'a ring alone is visible');
  const wild = sanitized({ ...defaultCrosshair(), rotation: 400, circle_radius: -5 });
  assert.deepEqual([wild.rotation, wild.circle_radius], [LIMITS.rotation[1], LIMITS.circle_radius[0]]);
  assert.ok(geometry({ ...defaultCrosshair(), dot: true, dot_round: true, shape: 'dot' }, 1).fill.some(p => p.kind === 'disc'));
});

test('a new crosshair is off, in-game only, with its own shortcut', () => {
  const d = defaultCrosshair();
  assert.equal(d.enabled, false);
  assert.equal(d.visibility, 'game');
  assert.equal(d.hotkey, DEFAULT_HOTKEY);
  assert.deepEqual(sanitized(d), d);
});

test('values are kept inside the slider limits and never become invisible', () => {
  assert.ok(validColor('#00ff88') && !validColor('00FF88') && !validColor('#0F8') && !validColor('#GG0000'));
  const wild = sanitized({ ...defaultCrosshair(), color: 'green', outline_color: '#12345', opacity: 0, length: 500, thickness: -3,
    gap: Number.NaN, dot_size: 99, outline_thickness: 0, offset_x: 99999, offset_y: -99999, visibility: 'x' as never, shape: 'dot', dot: false });
  assert.equal(wild.color, '#00FF88');
  assert.equal(wild.outline_color, '#000000');
  assert.equal(wild.opacity, LIMITS.opacity[0]);
  assert.equal(wild.length, LIMITS.length[1]);
  assert.equal(wild.thickness, LIMITS.thickness[0]);
  assert.equal(wild.gap, 4);
  assert.equal(wild.dot_size, LIMITS.dot_size[1]);
  assert.equal(wild.outline_thickness, LIMITS.outline_thickness[0]);
  assert.deepEqual([wild.offset_x, wild.offset_y], [1000, -1000]);
  assert.equal(wild.visibility, 'game');
  assert.equal(wild.dot, true, 'a dot-only crosshair keeps its dot');
  assert.ok(geometry(wild, 1).fill.length > 0);
});

test('crosshair preference writes keep edit order even when one write fails', async () => {
  const started: number[] = [];
  let failFirst!: (reason: Error) => void;
  const firstWrite = new Promise<number>((_, reject) => { failFirst = reject; });
  const save = orderedCrosshairSaves(async settings => {
    started.push(settings.offset_x);
    return settings.offset_x === 1 ? firstWrite : settings.offset_x;
  });
  const defaults = defaultCrosshair();
  const first = save({ ...defaults, offset_x: 1 });
  const next = save({ ...defaults, offset_x: 2 });
  const rejected = assert.rejects(first, /disk error/);
  await Promise.resolve();
  assert.deepEqual(started, [1]);
  failFirst(new Error('disk error'));
  await rejected;
  assert.equal(await next, 2);
  assert.deepEqual(started, [1, 2]);
});
