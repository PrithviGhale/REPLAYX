import type { Destination } from './errors';

/** Open a page (and Settings tab) from anywhere, then bring the anchored
 * section (`data-anchor`) into view and highlight it briefly. */
export const NAVIGATE_EVENT = 'replayx-navigate';

export function navigateTo(to: Destination) {
  window.dispatchEvent(new CustomEvent<Destination>(NAVIGATE_EVENT, { detail: to }));
}

const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Waits briefly for the page to render the section (it may load lazily). */
export function revealAnchor(anchor: string, attempts = 20) {
  const el = document.querySelector<HTMLElement>(`[data-anchor="${CSS.escape(anchor)}"]`);
  if (!el) {
    if (attempts > 0) window.setTimeout(() => revealAnchor(anchor, attempts - 1), 50);
    return;
  }
  el.scrollIntoView({ block: 'center', behavior: reducedMotion() ? 'auto' : 'smooth' });
  el.classList.remove('anchor-flash');
  void el.offsetWidth; // restart the highlight animation
  el.classList.add('anchor-flash');
  window.setTimeout(() => el.classList.remove('anchor-flash'), 1800);
  // Keyboard users land on the first control of that section.
  el.querySelector<HTMLElement>('button:not([disabled]), select:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])')
    ?.focus({ preventScroll: true });
}
