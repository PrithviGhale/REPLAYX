/** Friendly errors: every message the engine or a command reports becomes a
 * short title, one plain sentence and actions that lead to the fix. The raw
 * text (with Windows codes) stays available as technical details and in
 * diagnostics; it is never the headline. */

export type Destination = { page: 'home' | 'replay' | 'capture' | 'audio' | 'clips' | 'games' | 'performance' | 'settings'; tab?: 'general' | 'hotkeys' | 'updates' | 'about'; anchor?: string };
export type ErrorAction =
  | { kind: 'navigate'; label: string; to: Destination }
  | { kind: 'windows'; label: string; page: 'sound' | 'microphone-privacy' | 'display' | 'storage' }
  /** Redoes exactly what failed, in order. Only offered when that is known. */
  | { kind: 'retry'; label: string; ops: RetryOp[] }
  | { kind: 'folder'; label: string };
/** Engine actions a Retry may repeat. */
export type RetryOp = 'start_replay' | 'start_recording' | 'save_replay';
export type ErrorCategory = 'settings' | 'audio' | 'microphone' | 'microphone-permission' | 'hotkey' | 'storage' | 'save' | 'encoder'
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

/** Placeholder in a rule: replaced by the operation that failed, or dropped
 * when that isn't known. */
const retry: ErrorAction = { kind: 'retry', label: 'Retry', ops: [] };
const sound: ErrorAction = { kind: 'windows', label: 'Windows sound settings', page: 'sound' };

type Rule = { category: ErrorCategory; test: (m: string) => boolean; severity?: 'warning'; title: string; message: string; actions: ErrorAction[] };
const has = (...words: string[]) => (m: string) => words.some(w => m.includes(w));

// Ordered: the first matching rule wins, most specific first.
const RULES: Rule[] = [
  // Refused because start-up couldn't keep the settings file safe.
  { category: 'settings', severity: 'warning', test: has("isn't changing your settings file"),
    title: "Settings can't be changed right now", message: "REPLAYX couldn't open your settings file when it started, so it isn't changing it. Close any program that may be using it, then restart REPLAYX.",
    actions: [] },
  { category: 'hotkey', severity: 'warning', test: has('already used by another application', 'hotkey', 'shortcut'),
    title: 'Shortcut already in use', message: 'Another app is using one of your REPLAYX shortcuts. Pick a different key combination.',
    actions: [{ kind: 'navigate', label: 'Change shortcuts', to: { page: 'settings', tab: 'hotkeys' } }] },
  // A start timeout names several devices; it is its own problem.
  { category: 'start', test: has('did not start within', "didn't start within"),
    title: "Capture didn't start", message: "Your display, graphics driver or sound device didn't respond in time.",
    actions: [retry, { kind: 'navigate', label: 'Video settings', to: { page: 'capture' } }] },
  // Only real denial evidence counts: advice such as "check the microphone
  // permission" inside another message isn't one.
  { category: 'microphone-permission', test: m => m.includes('microphone') && has('access is denied', 'access denied', '0x80070005', 'e_accessdenied', 'permission denied', 'blocked by privacy')(m),
    title: 'Microphone access is blocked', message: "Windows isn't letting REPLAYX use your microphone. Allow access in Windows privacy settings.",
    actions: [{ kind: 'windows', label: 'Microphone privacy settings', page: 'microphone-privacy' },
      { kind: 'navigate', label: 'Microphone settings', to: { page: 'audio', anchor: 'microphone' } }] },
  { category: 'microphone', test: m => m.includes('microphone') && has('packets stopped', 'stopped sending', 'no packets')(m),
    title: 'Microphone stopped', message: 'Your microphone stopped sending sound. It may have been unplugged or switched off, or Windows may be blocking it.',
    actions: [{ kind: 'navigate', label: 'Fix microphone', to: { page: 'audio', anchor: 'microphone' } },
      { kind: 'windows', label: 'Microphone privacy settings', page: 'microphone-privacy' }, retry] },
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

const UNKNOWN = 'REPLAYX hit an unexpected problem. The technical details below can help if you report it.';

/** The first sentence of a raw message, only when the whole message is plain
 * words. Anything that looks technical (a file path, an OS error, a Windows
 * code, JSON or parser output, an error chain "a: b: c") gives the generic
 * summary instead: that text belongs in the technical details only. */
export function plainSentence(raw: string): string {
  const text = raw.replace(/\s+/g, ' ').trim();
  const first = text.split(/(?<=[.!?])\s/)[0] ?? '';
  const looksTechnical = /[\\/:;{}[\]<>"`|=_]|0x[0-9a-f]|os error|\berror \d|\bline \d|\bcolumn \d|\.(json|exe|dll|db|mkv|mp4|tmp|log)\b/i;
  if (!first || first.length > 160 || looksTechnical.test(text)) return UNKNOWN;
  return first;
}

const retryLabel = (ops: RetryOp[]) =>
  ops.length === 1 && ops[0] === 'start_recording' ? 'Retry recording'
    : ops.length === 1 && ops[0] === 'start_replay' ? 'Retry Instant Replay'
      : ops.length === 1 && ops[0] === 'save_replay' ? 'Save again' : 'Retry';

/** `context.retry`: the operations that failed, which Retry repeats. Without
 * it no Retry is offered: guessing could start the wrong capture. */
export function describeError(raw: string, context: { retry?: readonly string[] } = {}): FriendlyError {
  const technical = raw.trim();
  const m = technical.toLowerCase();
  const ops = (context.retry ?? []).filter((o): o is RetryOp => o === 'start_replay' || o === 'start_recording' || o === 'save_replay');
  const retryAction: ErrorAction[] = ops.length ? [{ kind: 'retry', label: retryLabel(ops), ops }] : [];
  const rule = RULES.find(r => r.test(m));
  if (!rule) return { key: `unknown:${technical.slice(0, 80)}`, category: 'unknown', severity: 'error', title: 'Something went wrong',
    message: plainSentence(technical), actions: retryAction, technical };
  const actions = rule.actions.flatMap(a => (a.kind === 'retry' ? retryAction : [a]));
  return { key: `${rule.category}:${technical.slice(0, 80)}`, category: rule.category, severity: rule.severity ?? 'error',
    title: rule.title, message: rule.message, actions, technical };
}

/** What start-up found wrong with settings.json (`Status.settings_problem`). */
export type SettingsProblem =
  | { kind: 'damaged'; backup: string; technical: string }
  | { kind: 'damaged_not_kept'; technical: string }
  | { kind: 'unreadable'; technical: string };

/** The settings notice in plain words; the raw error only in the details. */
export function describeSettingsProblem(p: SettingsProblem): FriendlyError {
  const base = { category: 'settings' as const, severity: 'warning' as const, technical: p.technical };
  switch (p.kind) {
    case 'damaged': return { ...base, key: `settings:damaged:${p.backup}`, title: 'Your settings were reset',
      message: `Your settings file was damaged, so REPLAYX started with its default settings. A copy of the old file was kept as ${p.backup}.`,
      actions: [{ kind: 'navigate', label: 'Review settings', to: { page: 'settings', tab: 'general' } }] };
    case 'damaged_not_kept': return { ...base, key: 'settings:damaged_not_kept', title: 'Using default settings for now',
      message: "Your settings file is damaged and REPLAYX couldn't keep a copy of it, so it started with its defaults and won't change the file. Restart REPLAYX to try again.",
      actions: [] };
    case 'unreadable': return { ...base, key: 'settings:unreadable', title: "Your settings couldn't be opened",
      message: "Another program may be using your settings file. REPLAYX started with its defaults and won't change your saved settings. Restart REPLAYX to load them.",
      actions: [] };
  }
}
