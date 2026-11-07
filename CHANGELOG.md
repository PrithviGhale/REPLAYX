# Changelog


## 0.2.0-alpha.1 — GPU capture path, redesigned app

**Video**
- Root cause of "GPU scaling fails": on hybrid laptops the display (and so every Desktop Duplication frame) belongs to the integrated GPU while NVENC is on the discrete GPU, and `scale_d3d11` also cannot allocate its output pool on this driver. Capture now probes a ladder of real capture→scale→encode chains — D3D11 zero-copy, Intel Quick Sync zero-copy, Intel VPP + scaled-frame transfer, CPU — and uses the first that works. The path and the reason faster paths were rejected are shown in the app.
- A path that passes its probe but fails before the first valid segment is skipped automatically and capture restarts on the next one.
- Frames are re-timed onto exact output slots: on a 240 Hz display, 60 fps capture went from 37–57 dropped frames per 30 s to 0.
- Fixed Quick Sync producing a single keyframe, which stopped the replay buffer from ever closing a segment; hardware encoders now force IDR frames at segment starts.
- Fixed audio failing (“backpressure exceeded”) while FFmpeg initialised a GPU encoder.
- Aspect modes: Stretch to fill, Preserve aspect (letterbox), Crop to fill (cropped at capture), Native. Every mode's encoded size is validated against its plan.
- Optional GPU-scaling switch; 120/144 fps selectable (experimental); audio bitrate setting.

**Audio**
- Microphones open at their native channel count; arrays (e.g. 4 channels, no mask) are averaged to centred mono instead of relying on an unspecified Windows channel matrix.
- In-app device test with live level and playback.
- Mixer lists apps that actually have audio sessions (with icons), so game, Discord and browser can each get their own track.

**App**
- Complete UI redesign (Home, Instant Replay, Video, Audio, Clips, Performance, Settings), wired to the real engine; persistent capture controls on every page; unsaved-changes bar; error boundary per page.
- Clips library with thumbnails, track names, rename, MP4 export and delete-to-Recycle-Bin.
- Optional start with Windows (tray only; capture never auto-starts); a second launch focuses the running window.
- Lower monitoring overhead (process metrics no longer scan every process each second).

**Build and distribution**
- FFmpeg pinned to the immutable release `autobuild-2026-11-06-12-50` (the previous floating `latest` asset had already changed); binaries byte-identical to the validated runtime. License corrected to LGPL-3.0-or-later.
- GitHub CI and a draft-only release workflow with optional signed updater artifacts; contributing guide and issue/PR templates.
## 0.1.3-alpha.1 — audio crackle repair

- Fixed crackling/distorted audio. The capture thread re-placed every WASAPI packet by its jittery QPC timestamp, inserting or dropping samples at most 10ms packet boundaries. Samples are now contiguous; clock drift is corrected by a slow controller and a band-limited resampler; re-anchoring happens only after idle/starvation or gross (>50ms) timing jumps and is counted. Measured on real loopback: 570 to 0 phase discontinuities in 6s of a test tone; isolated capture matches the source file's THD+N.
- Fixed a 21ms audio/video timestamp hole at the first 2s seam of clips containing a session's first segment.
- Adaptive delivery holdback for high-latency devices; 100ms WASAPI buffer for scheduler stalls under load.
- New per-source audio counters (discontinuities, drift correction) in Performance and CLI reports.
- CLI: `device-formats`, `audio-test ... application PID`, `capture-test ... app:PID`; exact f32 producer output saved for analysis. Added `scripts/audio-tone.py` and `scripts/audio-clicks.py`.
- 12 new regression tests (45 total).


## 0.1.2-alpha.1 — runtime and shortcut repair

- Replaced the Defender-quarantined FFmpeg development snapshot with a checksum-pinned, Defender-scanned LGPL stable-branch runtime.
- Runtime discovery and bundle preparation use the reviewed lockfile, avoiding old cache/PATH copies and mixed DLL versions.
- Changed default shortcuts to Ctrl+Shift combinations and migrate the full legacy default set while preserving custom shortcuts. A conflict no longer unregisters every other shortcut or prevents saving unrelated settings.
- Added a runtime recheck button and tests for shortcut migration/parse uniqueness.


## 0.1.1-alpha.1 — beta hardening in progress

- Added Instant Replay instructions and persistent shortcut-conflict reporting.
- Added display enumeration, active source geometry checks, manual crop and fixed 16:9 stretch, preserving previous aspect-ratio settings.
- Added display-change detection, bounded same-display restart, transition quarantine and separate recording parts.
- Added finalized-media/audio-write health checks, microphone packet-stall detection and storage policy tests.
- Added Windows job cleanup, per-data-directory instance locking, recovery path checks and failed snapshot rollback.
- Corrected process-tree metrics; exposed actual scaling and media freshness; extended CLI hardware capture duration/sampling.
- Added documentation, physical test matrix and Windows CI source checks; hardened bundle preparation and updater configuration.
- This version is not hardware-qualified. Windows blocked the media runtime, preventing fresh hardware/media and installed-app verification. Earlier alpha results are documented separately.
