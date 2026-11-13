import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_LENGTH, MIN_LENGTH, lengthAt, lengthMarks, lengthScale, positionOf, stepLength } from '../src/replay-length.ts';
import { durationLabel } from '../src/format.ts';

test('15 and 30 minutes saved by earlier versions are shown exactly, not clamped to 10', () => {
  // The 0.3.1-alpha.9 review: the slider stopped at 10 min for these.
  for (const saved of [900, 1800]) {
    const scale = lengthScale(saved);
    const position = positionOf(scale, saved);
    assert.equal(lengthAt(scale, position), saved, 'thumb position and setting agree');
    assert.notEqual(position, positionOf(scale, 600), 'not drawn at 10 min');
    assert.equal(durationLabel(saved), saved === 900 ? '15 min' : '30 min');
  }
  assert.equal(positionOf(lengthScale(1800), 1800), lengthScale(1800).max, '30 min is the end of the track');
});

test('every stop round-trips between position and length', () => {
  const scale = lengthScale(120);
  assert.equal(scale.stops[0], MIN_LENGTH);
  assert.equal(scale.stops.at(-1), MAX_LENGTH);
  for (const s of scale.stops) assert.equal(lengthAt(scale, positionOf(scale, s)), s);
  // 30 s to 10 min in 15 s steps, then 15 and 30 min.
  assert.deepEqual(scale.stops.slice(-4), [585, 600, 900, 1800]);
  assert.equal(scale.stops.filter(s => s <= 600).length, 39);
});

test('dragging between the long stops snaps to the nearest one', () => {
  const scale = lengthScale(120);
  const [p10, p15, p30] = [600, 900, 1800].map(s => positionOf(scale, s));
  assert.equal(lengthAt(scale, p10 + 1), 600);
  assert.equal(lengthAt(scale, p15 - 1), 900);
  assert.equal(lengthAt(scale, p15 + 2), 900);
  assert.equal(lengthAt(scale, p30 - 1), 1800);
  assert.equal(lengthAt(scale, scale.max + 10), 1800);
  assert.equal(lengthAt(scale, -5), 30);
  // The long stretch is wide enough to aim at with a mouse.
  assert.ok((p15 - p10) / scale.max > 0.1 && (p30 - p15) / scale.max > 0.1);
});

test('arrow and page keys move one or four stops and stay in range', () => {
  const scale = lengthScale(600);
  assert.equal(stepLength(scale, 600, 1), 900);
  assert.equal(stepLength(scale, 900, 1), 1800);
  assert.equal(stepLength(scale, 1800, 1), 1800);
  assert.equal(stepLength(scale, 1800, -1), 900);
  assert.equal(stepLength(scale, 30, -1), 30);
  assert.equal(stepLength(scale, 60, 4), 120);
});

test('a saved length that is not a stop is kept exactly until the slider moves', () => {
  for (const saved of [100, 1200]) {
    const scale = lengthScale(saved);
    assert.ok(scale.stops.includes(saved));
    assert.equal(lengthAt(scale, positionOf(scale, saved)), saved);
  }
  assert.equal(durationLabel(100), '1 min 40 sec');
  assert.equal(durationLabel(1200), '20 min');
});

test('scale labels sit at their real positions, in order, without crowding', () => {
  const marks = lengthMarks(lengthScale(120));
  assert.deepEqual(marks.map(m => m.seconds), [30, 300, 600, 900, 1800]);
  assert.equal(marks[0].percent, 0);
  assert.equal(marks.at(-1)!.percent, 100);
  for (let i = 1; i < marks.length; i++) assert.ok(marks[i].percent - marks[i - 1].percent >= 10, `${marks[i].seconds}`);
});
