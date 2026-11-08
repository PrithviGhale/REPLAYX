import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TOUR, placeTip, shouldAutoStart } from '../src/tour-steps.ts';

const view = { width: 1280, height: 820 };
const tip = { width: 320, height: 170 };
const inside = (p: { x: number; y: number }) => p.x >= 0 && p.y >= 0 && p.x + tip.width <= view.width && p.y + tip.height <= view.height;

test('the tour has the twelve steps with real, distinct anchors', () => {
  assert.equal(TOUR.length, 12);
  assert.equal(TOUR[0].target, undefined, 'welcome is centred');
  const targets = TOUR.map(s => s.target).filter(Boolean);
  assert.equal(new Set(targets).size, targets.length);
  for (const id of ['record', 'replay-switch', 'save-replay', 'video-output', 'mic-device', 'voice-enhancer', 'overlay', 'nav-clips', 'game-detection', 'tray', 'settings-tabs'])
    assert.ok(targets.includes(id), id);
  assert.match(TOUR[3].body({ replay: 'Ctrl + Shift + F10', record: 'x' }), /Ctrl \+ Shift \+ F10/);
});

test('tooltip goes below, flips above or beside, and stays inside the window', () => {
  const below = placeTip({ left: 100, top: 100, width: 200, height: 40 }, tip, view);
  assert.equal(below.side, 'below');
  assert.ok(inside(below));
  assert.ok(below.y >= 140, 'does not cover the target');
  const above = placeTip({ left: 900, top: 700, width: 200, height: 40 }, tip, view);
  assert.equal(above.side, 'above');
  assert.ok(inside(above) && above.y + tip.height <= 700);
  // Tall target filling the height: beside it.
  const side = placeTip({ left: 40, top: 20, width: 300, height: 780 }, tip, view);
  assert.equal(side.side, 'right');
  assert.ok(inside(side) && side.x >= 340);
  // Target in the top-right corner: clamped inside, arrow still points at it.
  const corner = placeTip({ left: 1180, top: 10, width: 90, height: 34 }, tip, view);
  assert.ok(inside(corner));
  assert.ok(Math.abs(corner.x + corner.arrow - (1180 + 45)) < 1);
  assert.equal(placeTip(null, tip, view).side, 'center');
  // Small window still keeps the tip on screen.
  const small = { width: 400, height: 300 };
  const p = placeTip({ left: 10, top: 10, width: 50, height: 20 }, tip, small);
  assert.ok(p.x >= 0 && p.y >= 0 && p.x + tip.width <= small.width && p.y + tip.height <= small.height, JSON.stringify(p));
});

test('tour starts by itself only on a real first run', () => {
  assert.equal(shouldAutoStart(true, false, true), true);
  assert.equal(shouldAutoStart(true, true, true), false, 'finished or skipped is remembered');
  assert.equal(shouldAutoStart(false, false, true), false, 'waits for settings');
  assert.equal(shouldAutoStart(true, false, false), false, 'never in the browser preview');
});
