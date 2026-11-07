# Contributing to REPLAYX

Thanks for helping. REPLAYX is a free, local-only Windows recorder, so contributions must keep it that way: no accounts, telemetry, ads or uploads.

## Ground rules

- **Honest UI.** No placeholder controls, fake meters or simulated data in the app. If something is unsupported, show that.
- **Evidence over claims.** Capture changes need a real recording that you inspected (ffprobe plus a full decode). Put measurements in `VALIDATION.md` and gaps in `KNOWN_LIMITATIONS.md`.
- **Don't break proven paths.** Audio timeline, segment/concat and geometry code is covered by regression tests. If a test fails, fix the cause; don't loosen the assertion.
- **Privacy in reports.** Never attach recordings, screenshots or logs that contain other people's voices, chats or account details.

## Setup

See [BUILDING.md](BUILDING.md). In short:

```powershell
npm ci
powershell -ExecutionPolicy Bypass -File scripts/setup-media.ps1   # pinned, checksum-verified FFmpeg
powershell -ExecutionPolicy Bypass -File scripts/check.ps1         # format, tests, Clippy, frontend build
npm run desktop                                                    # run the app
```

## Layout

| Path | What lives there |
| --- | --- |
| `src-tauri/src/audio.rs` | WASAPI capture, drift-corrected timeline, resampler |
| `src-tauri/src/media.rs` | FFmpeg probing, scaler ladder, encoding args, concat |
| `src-tauri/src/geometry.rs` | Display geometry (physical pixels) and aspect-mode plans |
| `src-tauri/src/engine.rs` | Capture pipeline, replay ring, saves, watchdog, recovery |
| `src-tauri/src/desktop.rs` | Tauri commands, tray, hotkeys, updater |
| `src-tauri/src/winutil.rs` | Audio sessions, icons, Recycle Bin, autostart |
| `src/` | React UI (`pages/`, `ui.tsx` components, `store.ts` status polling) |
| `scripts/` | Setup, checks, bundling and measurement scripts |
| `dev/` | Mocked UI preview and headless screenshot tools (not shipped) |

## Testing hardware changes

The CLI drives the real engine without the UI:

```powershell
src-tauri\target\release\replayx-cli.exe capture-test 30 system stretch1080 fps=60
powershell -File scripts/measure-capture.ps1 30 system stretch1080 fps=60   # adds CPU/memory sampling
powershell -File scripts/bench-scaling.ps1                                  # compares scaling paths
```

Audio analysis without listening: `scripts/audio-tone.py`, `scripts/audio-clicks.py`, and `scripts/av-sync.py` for A/V offset. Each script's docstring explains the stimulus it expects.

## UI work

Build the mocked preview and capture screenshots headlessly. No window opens, so this is safe while someone is gaming:

```powershell
npx vite build --config dev/vite.preview.ts
npx vite preview --config dev/vite.preview.ts        # serves http://127.0.0.1:1430
powershell -File dev/cdp-probe.ps1 -Url "http://127.0.0.1:1430/dev/preview.html?state=replay#audio" -Screenshot out.png
```

## Pull requests

Keep them focused, describe how you tested, and fill in the template. CI runs `scripts/check.ps1` on Windows. Hardware results come from contributors, so please include your GPU, display and audio devices.

By contributing you agree that your contribution is licensed under the MIT License.
