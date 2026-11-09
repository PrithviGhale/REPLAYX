# Branding and the new logo

**Status: applied in 0.3.1-alpha.3.** The approved logo (supplied by the owner on November 8, 2026) is the master file `assets/branding/replayx-logo-1024.png`: the owner's artwork with the black surround made transparent along the rounded square's own edge, nothing redrawn. The same image is used at every size; no simplified small variant was supplied, and at 32 × 32 the eye and play symbol stay recognisable. To change the logo, follow the steps below with a new master file.

Everything that shows the mark goes through a few places, so the new logo can be applied in one pass.

## What you need

- The approved logo as a square **SVG** (preferred), or a **PNG of at least 1024 × 1024** with a transparent background.
- A **simplified icon variant** if the full logo has text or fine detail: at 16 × 16 pixels (tray, taskbar), only a bold shape stays readable. Text must not appear in the small icon.

## Where it goes

| Place | File | What to change |
| --- | --- | --- |
| Sidebar, About, tour | `src/brand.tsx` | `BrandMark`: render the new mark (inline SVG or an imported image). |
| Startup screen | `index.html` | The block between `<!-- BRAND-MARK:start -->` and `<!-- BRAND-MARK:end -->`. Keep it inline (no extra request before the splash paints). |
| Windows executable, installer, tray, taskbar, notification icons | `src-tauri/icons/` | Run `npx tauri icon path\to\logo.png` (or the simplified variant): it regenerates every size, including `icon.ico` used by the `.exe`, the NSIS installer and the tray. |
| README | `assets/branding/banner.svg` | Replace the banner artwork. |

## Checks after applying

1. `npx tauri icon …`, then build (`npm run tauri build`).
2. Look at the tray icon at 100% and 150% display scaling, the taskbar, the installer, the Alt-Tab view and the startup screen.
3. Check the 16 × 16 and 32 × 32 icons in `src-tauri/icons/` by eye: the shape must still be recognisable.
4. Screenshot the sidebar, About and the first tour step for the README.
