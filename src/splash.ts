/** Startup splash (markup and styles in index.html). Shows only steps that
 * are really happening and disappears the moment the app is ready: no
 * minimum duration, no artificial delay. Opening the window again from the
 * tray does not reload the page, so the splash is not replayed. */
let finished = false;
const node = () => document.getElementById('splash');

export const splashTimings: { step: string; ms: number }[] = [];

export function splashStep(text: string) {
  if (finished) return;
  splashTimings.push({ step: text, ms: Math.round(performance.now()) });
  const step = node()?.querySelector('.step');
  if (step) step.textContent = text;
}

/** Fade the splash out and remove it. Safe to call more than once. */
export function splashDone() {
  if (finished) return;
  finished = true;
  const ms = Math.round(performance.now());
  splashTimings.push({ step: 'Ready', ms });
  (window as unknown as { __replayxReady?: number }).__replayxReady = ms;
  const el = node();
  if (!el) return;
  el.classList.add('out');
  const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  window.setTimeout(() => el.remove(), reduce ? 0 : 320);
}

export const splashFinished = () => finished;
