/** First-run tour: steps and tooltip placement (pure, unit-tested). */
export type TourPage = 'home' | 'replay' | 'capture' | 'audio' | 'clips' | 'games' | 'performance' | 'settings';
export type TourStep = {
  id: string;
  /** Page to show first (the tour navigates; it never presses anything). */
  page: TourPage;
  /** data-tour anchor of the real component; none = centred welcome card. */
  target?: string;
  title: string;
  body: (hotkeys: { replay: string; record: string }) => string;
};

export const TOUR: TourStep[] = [
  { id: 'welcome', page: 'home', title: 'Welcome to REPLAYX',
    body: () => 'Capture everything. Miss nothing. This quick tour shows the essentials in about a minute. Skip it anytime with Esc.' },
  { id: 'record', page: 'home', target: 'record', title: 'Record',
    body: k => `Starts a normal recording; press it again to stop and save. Shortcut: ${k.record}.` },
  { id: 'replay', page: 'home', target: 'replay-switch', title: 'Instant Replay',
    body: () => 'Turn this on before you play. REPLAYX keeps the last moments of gameplay on disk, ready to save after something great happens.' },
  { id: 'save', page: 'home', target: 'save-replay', title: 'Save replay',
    body: k => `Saves the last moments as a clip while Instant Replay is on. Works in games too with the shortcut ${k.replay}.` },
  { id: 'video', page: 'capture', target: 'video-output', title: 'Resolution and frame rate',
    body: () => 'Choose the video size and frame rate here; quality and codec are just below. Balanced at 1080p60 suits most games.' },
  { id: 'audio', page: 'audio', target: 'mic-device', title: 'Your microphone',
    body: () => 'Pick your microphone and its volume. The switch on the card decides whether your voice is recorded, on its own track.' },
  { id: 'voice', page: 'audio', target: 'voice-enhancer', title: 'Voice Enhancer and Test mic',
    body: () => 'Choose how your voice is cleaned up and removes background noise. Use Test mic to hear yourself live, or Compare to hear the difference.' },
  { id: 'overlay', page: 'settings', target: 'overlay', title: 'Status overlay',
    body: () => 'Small icons in a corner show Instant Replay, your mic and recording. You see them while playing; they never appear in clips.' },
  { id: 'clips', page: 'clips', target: 'nav-clips', title: 'Your clips',
    body: () => 'Every replay and recording lands here, sorted by game. Play, rename, export to MP4 or delete them.' },
  { id: 'games', page: 'settings', target: 'game-detection', title: 'Game detection',
    body: () => 'REPLAYX notices which game you play and files clips in a folder per game. Correct it here if a game is missed.' },
  { id: 'tray', page: 'settings', target: 'tray', title: 'Runs in the background',
    body: () => 'REPLAYX can keep running in the system tray, so Instant Replay and shortcuts work while the window is closed. Quit from the tray icon.' },
  { id: 'settings', page: 'settings', target: 'settings-tabs', title: 'Settings and shortcuts',
    body: () => 'Change your keyboard shortcuts and other preferences here. You can replay this tour from Settings anytime. Have fun!' },
];

export type Rect = { left: number; top: number; width: number; height: number };
export type Placement = { x: number; y: number; side: 'below' | 'above' | 'right' | 'left' | 'center'; arrow: number };

/** Where the tooltip goes: below the target if it fits, else above, else
 * beside it, always inside the window; `arrow` is the arrow's offset along
 * the tooltip edge, pointing at the target's centre. */
export function placeTip(target: Rect | null, tip: { width: number; height: number }, view: { width: number; height: number }, gap = 14, margin = 12): Placement {
  const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
  if (!target) return { x: (view.width - tip.width) / 2, y: (view.height - tip.height) / 2, side: 'center', arrow: 0 };
  const cx = target.left + target.width / 2, cy = target.top + target.height / 2;
  const fitsBelow = target.top + target.height + gap + tip.height + margin <= view.height;
  const fitsAbove = target.top - gap - tip.height - margin >= 0;
  if (fitsBelow || fitsAbove) {
    const x = clamp(cx - tip.width / 2, margin, view.width - tip.width - margin);
    const y = fitsBelow ? target.top + target.height + gap : target.top - gap - tip.height;
    return { x, y, side: fitsBelow ? 'below' : 'above', arrow: clamp(cx - x, 18, tip.width - 18) };
  }
  const right = target.left + target.width + gap + tip.width + margin <= view.width;
  const x = right ? target.left + target.width + gap : clamp(target.left - gap - tip.width, margin, view.width - tip.width - margin);
  const y = clamp(cy - tip.height / 2, margin, view.height - tip.height - margin);
  return { x, y, side: right ? 'right' : 'left', arrow: clamp(cy - y, 18, tip.height - 18) };
}

/** Show automatically only on a real first run (the engine remembers). */
export const shouldAutoStart = (loaded: boolean, tourDone: boolean, native: boolean) => native && loaded && !tourDone;
