/** Friendly errors: every message the engine or a command reports becomes a
 * short title, one plain sentence and actions that lead to the fix. The raw
 * text (with Windows codes) stays available as technical details and in
 * diagnostics; it is never the headline. */

export type Destination = { page: 'home' | 'replay' | 'capture' | 'audio' | 'clips' | 'games' | 'performance' | 'settings'; tab?: 'general' | 'hotkeys' | 'updates' | 'about'; anchor?: string };
export type ErrorAction =
  | { kind: 'navigate'; label: string; to: Destination }
  | { kind: 'windows'; label: string; page: 'sound' | 'microphone-privacy' | 'display' | 'storage' }
  | { kind: 'retry'; label: string }
  | { kind: 'folder'; label: string };
export type ErrorCategory = 'audio' | 'microphone' | 'microphone-permission' | 'hotkey' | 'storage' | 'save' | 'encoder'
  | 'display' | 'runtime' | 'start' | 'capture' | 'unknown';
export type FriendlyError = {
  /** Stable for identical problems: used to collapse repeats. */
  key: string;
  category: ErrorCategory;
  severity: 'error' | 'warning';
  title: string;
  message: string;
  actions: ErrorAction[];
  /** The original text, for "Technical details" and diagnostics. */
  technical: string;
};

const retry: ErrorAction = { kind: 'retry', label: 'Retry' };
const sound: ErrorAction = { kind: 'windows', label: 'Windows sound settings', page: 'sound' };

type Rule = { category: ErrorCategory; test: (m: string) => boolean; severity?: 'warning'; title: string; message: string; actions: ErrorAction[] };
const has = (...words: string[]) => (m: string) => words.some(w => m.includes(w));

// Ordered: the first matching rule wins, most specific first.
const RULES: Rule[] = [
  { category: 'hotkey', severity: 'warning', test: has('already used by another application', 'hotkey', 'shortcut'),
    title: 'Shortcut already in use', message: 'Another app is using one of your REPLAYX shortcuts. Pick a different key combination.',
    actions: [{ kind: 'navigate', label: 'Change shortcuts', to: { page: 'settings', tab: 'hotkeys' } }] },
  // A start timeout names several devices; it is its own problem.
  { category: 'start', test: has('did not start within', "didn't start within"),
    title: "Capture didn't start", message: "Your display, graphics driver or sound device didn't respond in time.",
    actions: [retry, { kind: 'navigate', label: 'Video settings', to: { page: 'capture' } }] },
  { category: 'microphone-permission', test: m => m.includes('microphone') && has('permission', 'access is denied', '0x80070005', 'privacy')(m),
    title: 'Microphone access is blocked', message: "Windows isn't letting REPLAYX use your microphone. Allow access in Windows privacy settings.",
    actions: [{ kind: 'windows', label: 'Microphone privacy settings', page: 'microphone-privacy' },
      { kind: 'navigate', label: 'Microphone settings', to: { page: 'audio', anchor: 'microphone' } }] },
  { category: 'microphone', test: has('microphone'),
    title: 'Microphone unavailable', message: "We couldn't use your microphone. It may be unplugged, switched off or busy in another app.",
    actions: [{ kind: 'navigate', label: 'Fix microphone', to: { page: 'audio', anchor: 'microphone' } }, sound, retry] },
  { category: 'audio', test: has('sound device', 'system audio', 'audio capture', 'audio could not', "couldn't start. (technical", 'audio device', 'wasapi', '0x8889', 'audio output backpressure'),
    title: 'Computer audio unavailable', message: "We couldn't connect to your sound device. Check your audio settings and try again.",
    actions: [{ kind: 'navigate', label: 'Fix audio', to: { page: 'audio', anchor: 'sound-sources' } }, sound, retry] },
  { category: 'storage', test: has('not enough', 'free space', 'disk full', 'storage', 'space left', 'drive is full'),
    title: 'Not enough storage', message: 'The drive where REPLAYX saves clips is almost full. Free up space or choose another folder.',
    actions: [{ kind: 'navigate', label: 'Change save folder', to: { page: 'settings', tab: 'general', anchor: 'output-folder' } },
      { kind: 'windows', label: 'Windows storage settings', page: 'storage' }] },
  { category: 'save', test: has('save failed', "couldn't save", 'could not save', 'clip could not'),
    title: "Clip couldn't be saved", message: 'The footage was kept, so it can be recovered. Check the save folder and try again.',
    actions: [{ kind: 'navigate', label: 'Open Clips', to: { page: 'clips' } }, { kind: 'folder', label: 'Open save folder' }] },
  { category: 'encoder', test: has('encoder', 'nvenc', 'qsv', 'h264_amf', 'libopenh264', 'encode probe'),
    title: "Video encoder couldn't start", message: 'Your graphics driver refused the selected video encoder. Setting the encoder to Auto usually fixes this.',
    actions: [{ kind: 'navigate', label: 'Video settings', to: { page: 'capture', anchor: 'encoder' } }, retry] },
  { category: 'display', test: has('display', 'monitor', 'geometry', 'rotated'),
    title: 'Display unavailable', message: 'The screen REPLAYX records changed or was disconnected.',
    actions: [{ kind: 'navigate', label: 'Video settings', to: { page: 'capture', anchor: 'display' } },
      { kind: 'windows', label: 'Windows display settings', page: 'display' }, retry] },
  { category: 'runtime', test: has('ffmpeg', 'media runtime'),
    title: 'Recording components missing', message: "REPLAYX's recording components are missing or blocked. Reinstalling REPLAYX restores them.",
    actions: [{ kind: 'navigate', label: 'Check for updates', to: { page: 'settings', tab: 'updates' } }] },
  { category: 'capture', test: has('capture', 'recording'),
    title: 'Capture stopped', message: 'Something interrupted capture. Footage recorded before it was kept.',
    actions: [retry, { kind: 'navigate', label: 'Performance', to: { page: 'performance' } }] },
];

/** The first plain sentence of a raw message, without Windows codes. */
function plainSentence(raw: string): string {
  const text = raw.replace(/\(technical:[\s\S]*\)$/i, '').replace(/0x[0-9a-f]{8}/gi, '').replace(/\s+/g, ' ').trim();
  const first = text.split(/(?<=[.!?])\s/)[0] ?? '';
  return first.length > 160 ? `${first.slice(0, 157)}…` : first;
}

export function describeError(raw: string): FriendlyError {
  const technical = raw.trim();
  const m = technical.toLowerCase();
  const rule = RULES.find(r => r.test(m));
  if (rule) return { key: `${rule.category}:${technical.slice(0, 80)}`, category: rule.category, severity: rule.severity ?? 'error',
    title: rule.title, message: rule.message, actions: rule.actions, technical };
  return { key: `unknown:${technical.slice(0, 80)}`, category: 'unknown', severity: 'error', title: 'Something went wrong',
    message: plainSentence(technical) || 'REPLAYX hit an unexpected problem.', actions: [], technical };
}
