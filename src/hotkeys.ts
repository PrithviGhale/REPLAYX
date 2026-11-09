/** Key events to global-shortcut strings (pure, unit-tested). */
const MODS = new Set(['Control', 'Shift', 'Alt', 'Meta']);

/** Build a global-shortcut string ("Control+Shift+F10") from a key event. */
export function comboFrom(e: { ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean; key: string; code: string }) {
  if (MODS.has(e.key)) return null;
  const mods = [e.ctrlKey && 'Control', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[];
  const code = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.code;
  const isF = /^F\d{1,2}$/.test(code);
  if (!mods.length && !isF) return { error: 'Add Ctrl, Alt or Shift so normal typing is not captured.' };
  return { combo: [...mods, code].join('+') };
}

