import { test } from 'node:test';
import assert from 'node:assert/strict';
import { comboFrom } from '../src/hotkeys.ts';

const key = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}, k = code) =>
  ({ ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, key: k, code, ...mods });

test('Alt+F10 and other combinations become global-shortcut strings', () => {
  assert.deepEqual(comboFrom(key('F10', { altKey: true })), { combo: 'Alt+F10' });
  assert.deepEqual(comboFrom(key('F10', { ctrlKey: true, shiftKey: true })), { combo: 'Control+Shift+F10' });
  assert.deepEqual(comboFrom(key('KeyR', { ctrlKey: true, altKey: true }, 'r')), { combo: 'Control+Alt+R' });
  assert.deepEqual(comboFrom(key('Digit1', { altKey: true }, '1')), { combo: 'Alt+1' });
  assert.deepEqual(comboFrom(key('F9')), { combo: 'F9' }, 'a bare F-key is allowed');
});

test('modifier-only presses wait, and plain typing is refused', () => {
  assert.equal(comboFrom(key('AltLeft', { altKey: true }, 'Alt')), null);
  assert.ok('error' in (comboFrom(key('KeyA', {}, 'a')) ?? {}));
});
