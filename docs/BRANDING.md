# Branding and the new logo

**Status: the approved final logo has not been supplied yet.** REPLAYX keeps its existing mark until it is. Nothing in the app invents or substitutes a logo.

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
| README | `docs/images/banner.svg` | Replace the banner artwork. |

## Checks after applying

1. `npx tauri icon …`, then build (`npm run tauri build`).
2. Look at the tray icon at 100% and 150% display scaling, the taskbar, the installer, the Alt-Tab view and the startup screen.
3. Check the 16 × 16 and 32 × 32 icons in `src-tauri/icons/` by eye: the shape must still be recognisable.
4. Screenshot the sidebar, About and the first tour step for the README.
