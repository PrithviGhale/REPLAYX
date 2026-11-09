import logo from './assets/replayx-logo.png';

/** REPLAYX branding in one place (see docs/BRANDING.md): this mark, the
 * BRAND-MARK block in index.html (startup screen), the app icons generated
 * from assets/branding/replayx-logo-1024.png (`npx tauri icon`) and the README banner. */
export const BRAND = {
  name: 'REPLAYX',
  tagline: 'Capture everything. Miss nothing.',
} as const;

/** The app logo at a given size (sidebar, About). */
export function BrandMark({ size = 28 }: { size?: number }) {
  return <img className="brand-mark" src={logo} width={size} height={size} alt="" aria-hidden="true" draggable={false} />;
}

/** "REPLAY" + violet "X". */
export const Wordmark = () => <span className="wordmark">REPLAY<b>X</b></span>;
