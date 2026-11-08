# Scripts

Run from the repository root. Python scripts use only the standard library and the pinned FFmpeg runtime.

## Build and release

| Script | Purpose |
| --- | --- |
| `check.ps1` | Everything CI runs: `npm ci`, frontend build and tests, `cargo fmt --check`, Rust tests, Clippy with warnings denied |
| `setup-media.ps1` | Downloads the pinned LGPL FFmpeg runtime and verifies its SHA-256 (`tools/ffmpeg-lock.json`) |
| `verify-media.mjs` | Verifies an existing runtime against the lock file |
| `prepare-bundle.ps1` | Copies the runtime, licences and notices into the installer resources |
| `updater-manifest.ps1` | Writes `latest.json` for signed in-app updates (release workflow) |

## Measurement and testing

These capture the screen or start encoders. Run them only on a free desktop, never while someone is playing.

| Script | Measures |
| --- | --- |
| `overhead-bench.py` | Game-FPS impact (avg, 1% and 0.1% lows) of each capture/encoder path, using `replayx-gameproxy` as a game-like GPU workload, plus GPU engine use and recording smoothness |
| `gameproxy-check.py` | Smoothness of a recording of the game proxy: repeated frames, freezes, frame-advance percentiles |
| `motion-fixture.py` + `motion-check.py` | 60 Hz desktop motion pattern and per-frame analysis of a recording |
| `run-motion-tests.ps1`, `bench-pacing.py`, `frame-pacing.py` | Frame-pacing experiments on raw FFmpeg capture |
| `start-latency.py` | Launch-to-first-segment time of two CLI builds |
| `ui-record-check.mjs` + `window-check.py` | Drives the real window (throw-away data folder) and measures whether it keeps answering Windows messages |
| `ui-delete-check.mjs` | Clips page Delete workflow on generated clips |
| `measure-capture.ps1`, `bench-scaling.ps1`, `profile-run.py` | CPU/memory of a capture run; scaling-path comparison |
| `audio-tone.py`, `audio-clicks.py`, `av-sync.py` | Objective audio checks: tone purity, click detection, A/V offset |

The CLI (`replayx-cli`) drives the real engine without the UI, for example `capture-test`, `record-test`, `start-bench`, `save-bench`, `library-bench` and `detect`. Run it without arguments for the list.
