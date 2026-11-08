import { test } from 'node:test';
import assert from 'node:assert/strict';

// Minimal DOM stand-in for the splash element from index.html.
const classes = new Set<string>();
let removed = false;
const step = { textContent: 'Loading interface' };
const el = { classList: { add: (c: string) => classes.add(c) }, querySelector: () => step, remove: () => { removed = true; } };
let reduced = false;
const g = globalThis as unknown as Record<string, unknown>;
g.document = { getElementById: (id: string) => (id === 'splash' ? el : null) };
g.window = { matchMedia: () => ({ matches: reduced }), setTimeout: (f: () => void) => { f(); return 0; } };

const { splashStep, splashDone, splashFinished, splashTimings } = await import('../src/splash.ts');

test('splash shows only real steps and never waits once ready', () => {
  splashStep('Loading settings');
  assert.equal(step.textContent, 'Loading settings');
  splashStep('Checking recording engine');
  assert.deepEqual(splashTimings.map(t => t.step), ['Loading settings', 'Checking recording engine']);
  reduced = true;
  splashDone();
  assert.ok(classes.has('out'), 'fades out');
  assert.ok(removed, 'removed from the page (immediately with reduced motion)');
  assert.ok(splashFinished());
  assert.equal(splashTimings.at(-1)?.step, 'Ready');
});

test('splash steps after ready are ignored and done is idempotent', () => {
  splashStep('Late step');
  assert.equal(step.textContent, 'Checking recording engine');
  const n = splashTimings.length;
  splashDone();
  assert.equal(splashTimings.length, n);
});
