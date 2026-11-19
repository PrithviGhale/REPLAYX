# Crosshair Studio V1 — design, validation and measurements

November 13, 2026. Branch `feature/crosshair-studio-v1`, from `main` after
0.3.1-alpha.9. Not merged, not released.

## What it is

A free crosshair overlay built into REPLAYX:
- **Styles:** 16 original presets: Classic, Dot, Cross + dot, Precision,
  Circle + dot, T, Micro, Bold, Wide gap, Long lines, X, X + dot, Scope,
  Round dot, Box and Chevron.
- **Shapes:** cross, T, circle, dot, box and chevron.
- **Look:**
  - color, length, thickness, gap and opacity;
  - each of a cross's four lines on or off;
  - rotation 0–90°;
  - a square or round center dot;
  - a ring that works with any shape;
  - an outline (color and width);
  - position offsets.
- **Visibility:** In-game only (default) or Always on.
- **Shortcut:** show or hide it with a shortcut (default Ctrl+Shift+F6).
- **Display:** choice of display.

New installations start with it off.

The implementation is independent. No code, assets, presets or interface
design were taken from other products.

## Design

| Part | File | Notes |
| --- | --- | --- |
| Model, presets, limits, geometry, rasterizer, placement, visibility | `src-tauri/src/crosshair.rs` | Pure Rust, no Windows calls |
| Native window | `src-tauri/src/crosshair_overlay.rs` | Win32 layered window on its own thread; `CrosshairRuntime` drives it |
| Settings | `src-tauri/src/model.rs` (`Settings::crosshair`) | Read leniently; kept within limits; shortcut kept distinct |
| Live command, sampling | `src-tauri/src/engine.rs` (`set_crosshair`, `activity_due`) | Works during capture; doesn't touch the recording |
| App wiring | `src-tauri/src/desktop.rs` | `set_crosshair`, `crosshair_status`, shortcut, 1 s loop, shutdown |
| Page | `src/pages/Crosshair.tsx`, `src/crosshair.ts` | Preview from the same geometry (shared fixture) |

### The native window

The crosshair is a small window: its bitmap is only as large as the
crosshair, about 20–60 pixels. It sits on its own thread and uses these
documented Win32 behaviours:

| Requirement | How |
| --- | --- |
| Transparent | `WS_EX_LAYERED` + `UpdateLayeredWindow` with a premultiplied-alpha bitmap |
| Click-through | `WS_EX_TRANSPARENT`; `WM_NCHITTEST` → `HTTRANSPARENT` |
| Never takes focus | `WS_EX_NOACTIVATE`, `WM_MOUSEACTIVATE` → `MA_NOACTIVATE`, `SW_SHOWNOACTIVATE`, `SWP_NOACTIVATE` |
| No taskbar or Alt+Tab entry | `WS_EX_TOOLWINDOW` |
| Above windowed and borderless games | `WS_EX_TOPMOST`; raised again when the game in front changes |
| Not in clips | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` (Windows 10 2004 and later) |
| DPI | Per-monitor DPI aware v2 thread; `GetDpiForMonitor`; every part rounded to whole device pixels and aligned to pixel edges (crisp at 100–200%) |
| Display and resolution changes | `WM_DISPLAYCHANGE` / `WM_DPICHANGED` / `WM_SETTINGCHANGE` → placed again |
| Centering | Display center + offset; the offset is limited so the crosshair stays on the display |

**No render loop.**
- The bitmap is drawn when something changes (settings, display or DPI) and
  handed to Windows once. The compositor (DWM) keeps showing it.
- The thread waits in `GetMessageW` and does no work until a change arrives.
- Turning the crosshair off destroys the window and ends the thread.
- There is no screen capture, no encoder, no GPU work per frame, and no game
  hooks or injection.

**Preview parity.** The Crosshair Studio preview draws the same parts as
SVG. `src/crosshair.ts` mirrors the geometry. Both the Rust and the
TypeScript tests compare against `tests/fixtures/crosshair-shapes.json`: 92
cases covering every preset, scales from 100% to 200%, rotations, partial
lines, rings and extreme values. Coordinates are fixed to 1/1000 px; the
TypeScript test allows 2/1000 px because the engine computes in 32-bit
floats.

**Geometry.**
- Every line is a segment with square ends.
- Straight segments (rotation 0° or 90°) become exact rectangles: crisp,
  aligned to pixel edges.
- Turned segments become quads, which are anti-aliased with 4 × 4 samples per
  pixel.
- Rings and round dots are sampled the same way.
- The outline is each part grown by the outline width.
- Everything is drawn once per change.

**In-game only.**
- It uses REPLAYX's existing game detection, sampled once a second; the
  sampling also runs while idle when this mode is on.
- **Games running behind other windows:**
  - Detection follows the window in front. While the Crosshair page is open,
    REPLAYX itself is in front, so a game started before REPLAYX could
    previously only show as "Waiting for a game".
  - `crosshair_status` now also lists recognized games with a visible
    window anywhere. The page names them ("Counter-Strike 2 is running.
    Switch to it…").
  - The scan reads window and process information only, without messaging
    other windows. It runs only while the page is open: about 1.1 ms per
    call, on a background thread.
- **Exclusive full screen:**
  - While a game is in front with the crosshair shown,
    `SHQueryUserNotificationState` is checked once a second (about 0.14 ms).
  - If the game runs in exclusive full screen, where no window can show, the
    page warns and suggests borderless.
  - The check runs while the game is in front because such games minimize
    when you leave them.
- **Staying on top:** while the same game stays in front, the 1 s loop walks
  the z-order above the crosshair (`GetWindow(GW_HWNDPREV)`, no messages).
  The crosshair is raised only if a visible window overlaps it.
- It shows the crosshair as soon as a recognized game is in front. It hides
  it only after the game has been away for 1.5 s, so a quick Alt+Tab never
  makes it flicker.
- The window only hears about real changes, at most one message per change.

**Shortcut.**
- It is a sixth global shortcut, registered only while the crosshair is on.
  Off, the key is released for other apps.
- It is registered together with REPLAYX's other shortcuts, so a duplicate
  registration isn't possible.
- If Windows refuses it, REPLAYX keeps the previous one and says so.

**Settings.**
- `Settings::crosshair` is read leniently: a damaged entry (wrong types,
  unknown shape) falls back to the default and never makes the rest of the
  settings unreadable. Values outside the limits are clamped.
- A shortcut that collides with another REPLAYX shortcut is replaced with a
  free one, or none.
- The crosshair has its own live command, so the general Settings save never
  overwrites it.

## Recording behaviour (verified)

`replayx-cli crosshair-capture-test` (`test-output/crosshair-capture/`):
- **Method.** It grabs real frames with both capture methods REPLAYX records
  with, over a solid backdrop. It records three states: the crosshair hidden,
  shown (excluded from capture), and shown with capture allowed. The last is a
  control that proves the test can see the crosshair.
- **Display.** The test ran on the main display, 1920 × 1080.

| Capture method | Excluded vs hidden (mean pixel difference) | Capture allowed vs hidden |
| --- | --- | --- |
| Desktop Duplication (default) | 0.0 | 28.8 (clearly visible) |
| Windows Graphics Capture | 0.0 | 28.8 (clearly visible) |

**REPLAYX's clips don't contain the crosshair**, with either capture method.
On Windows earlier than 10 2004, exclusion isn't available: the crosshair is
still shown and would appear in clips (`excluded_from_capture` is false in
`crosshair_status`).

## Measurements

The test PC is a laptop with an Intel Core i9-13900H and an RTX 4070 Laptop
GPU, where the Intel GPU drives the panel, running Windows 11.

`replayx-cli crosshair-bench 12` (`test-output/crosshair-bench-*/`), with the
crosshair at the center of the main display:

| REPLAYX process, 5 s windows | Off | On | Off again |
| --- | --- | --- | --- |
| CPU time | 0.00 ms | **0.00 ms** | 0.00 ms |
| Working set | 11.9 MB | 12.9 MB | 13.0 MB |
| GDI / USER objects | 0 / 1 | 0 / 4 | 0 / 1 |

| `replayx-gameproxy`, borderless full screen, uncapped, 12 s each | FPS | 1% low | 0.1% low | Presentation |
| --- | --- | --- | --- | --- |
| Crosshair off | 946.6 | 580.8 | 364.6 | hardware overlay plane (352 of 352 samples) |
| Crosshair on | 940.6 | 571.6 | 352.2 | hardware overlay plane (352 of 352) |
| Crosshair off | 952.5 | 648.8 | 558.3 | hardware overlay plane (355 of 355) |

**Reading.**
- **CPU and memory:** while shown, the crosshair costs no measurable CPU in
  REPLAYX, about 1 MB of memory and three window objects.
- **Average FPS:** with the crosshair on, average FPS was 0.6–1.3% below the
  two "off" runs, which differ from each other by 0.6%.
- **Lows:** the 1% and 0.1% lows vary much more between the two "off" runs
  (11% and 53%) than any crosshair effect. These 12 s runs can't resolve a
  difference in the lows.
- **Presentation:** Windows kept presenting the workload's frames on a
  hardware overlay plane with the crosshair on top. The crosshair didn't
  force the workload into composed presentation.

**Limits of this evidence.**
- One A/B/A on one laptop, with a synthetic GPU workload rather than a real
  game.
- GPU time spent by DWM wasn't measured separately.
- No claim of zero FPS cost is made.
- A longer, interleaved A/B/A in real games (with PresentMon) is the next
  step, with the owner's approval.

## Automated tests

| Area | Tests |
| --- | --- |
| Geometry, presets, shapes, lines and rotation, colors, limits, lenient reading, crispness, DPI, outline and opacity, placement (multi-display, negative coordinates, 150%), visibility transitions | `crosshair::tests` (13) |
| Preview parity, presets, defaults, limits, lines, rotation and ring | `tests/crosshair.test.ts` (5; 92 shared fixture cases) |
| Running-games scan finds a mapped game behind other windows; full-screen query is safe | `games::tests::process_identity_follows_the_real_process_lifetime` |
| Settings: older files, damaged crosshair entry, shortcut collisions, limits on save | `model::tests` (3 new) |
| Live save during capture, refusal of a taken shortcut, idle game sampling | `engine::tests::crosshair_changes_save_at_once_and_keep_other_settings` |
| Shortcut registered only while on, refused shortcut keeps the previous one | `desktop::tests` (1 new; 1 updated) |
| Display list, labels, no window while off | `crosshair_overlay::tests` (3) |
| **Native (opt-in, shows a crosshair for about 2 s)** | `crosshair_overlay::tests::native_window_is_click_through_never_focused_and_cleans_up`: 5 rounds of on, hide, show and off |

The native test checks:
- the window styles;
- capture exclusion;
- click-through on an opaque crosshair pixel (`WindowFromPoint`);
- that focus is unchanged;
- that the window is destroyed when turned off.

**End-to-end in the real app** (development build, isolated data folder,
driven through WebView2 DevTools):
1. No window before the crosshair is turned on.
2. The page opens.
3. Turning it on shows the native window: visible, never focused, with the
   expected styles (`0x080800A8`).
4. Settings save automatically.
5. Ctrl+Shift+F6 hides it and shows it again.
6. Turning it on and off repeatedly works; off removes the window.
7. After a restart it is back as saved.
8. Quitting removes the window.

All passed.

## Compatibility

| Scenario | Status |
| --- | --- |
| Windows desktop | Verified (end-to-end, native test) |
| Borderless full screen (D3D11 workload) | Verified with `replayx-gameproxy`: the crosshair shows over it, presentation unchanged |
| Windowed games | Expected (same mechanism); not tested with a real game |
| Exclusive full-screen games | Not supported: DWM doesn't compose desktop windows over them. The page says so and suggests borderless. |
| Real games (Fortnite, Valorant, CS2, Roblox and others) | **Not tested.** Anti-cheat: the crosshair never reads or changes a game, injects nothing and hooks nothing. It is an ordinary top-level window, like other desktop overlays. No statement about any game's policy or ban risk is made. |
| Multiple displays | Placement math tested; the display choice works; tested on one connected display |
| 125–200% scaling | Geometry tested; on screen only at 100% (the connected display during the test) |

## Not yet verified on real hardware

- Real games, windowed and borderless, including frame-time impact with
  PresentMon.
- In-game only with a real game in front (the visibility logic and game
  detection are tested separately).
- A second display and non-100% scaling on screen.
- Windows 10.
- Games or launchers that put their own topmost windows above the crosshair.

## Future (not in V1)

Ideas for later versions, none built yet:
- import or export, and sharing codes;
- game profiles and automatic switching;
- animation;
- an optional crosshair in clips;
- an Xbox Game Bar widget for exclusive full screen.

The data model (`Shape` variants plus parts in `geometry`) is designed to grow
without changing the renderer or the window.
