import { SquareStack } from 'lucide-react';

/** REPLAYX branding in one place. The approved final logo has not been
 * supplied yet; this is the existing mark. To apply the new logo, follow
 * docs/BRANDING.md: replace `BrandMark` below, the BRAND-MARK block in
 * index.html (startup screen) and the app icons (`npx tauri icon`). */
export const BRAND = {
  name: 'REPLAYX',
  tagline: 'Capture everything. Miss nothing.',
  /** The final logo is pending from the owner (see docs/BRANDING.md). */
  logoPending: true,
} as const;

/** The app mark at a given size (sidebar, About, tour). */
export function BrandMark({ size = 28 }: { size?: number }) {
  return (
    <span className="brand-mark" style={{ width: size, height: size, borderRadius: Math.round(size * 0.29) }} aria-hidden="true">
      <SquareStack size={Math.round(size * 0.57)} />
    </span>
  );
}

/** "REPLAY" + violet "X". */
export const Wordmark = () => <span className="wordmark">REPLAY<b>X</b></span>;
