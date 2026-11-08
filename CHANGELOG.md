# Changelog

## 0.2.1-alpha.3 — lighter capture on gaming laptops, real 1920×1080, sharper presets, recovery fixes

Game-FPS and smoothness improvements in this build are **not yet measured in a real game**. The measurements so far, and how to run the comparison, are in [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

- **Fixed: CPU fallback and a ~30 s start when the NVIDIA GPU drives the display** (desktop PCs, and laptops with Advanced Optimus/MUX switching during full-screen games). FFmpeg's `scale_d3d11` cannot create its output texture in the pinned build, so the only NVIDIA path never worked, and Quick Sync paths that cannot run on an NVIDIA display were probed first. REPLAYX now probes only paths for the GPU that drives the display: Direct3D 11 straight into NVENC (no copy) when no resizing is needed, otherwise one frame copy into CUDA, scaling there and NVENC. Probes time out after 5 s instead of 8, and a working path is remembered for each display mode, so switching between desktop and game does not probe again.
- **Less work on the display GPU.** On laptops whose screen runs on the Intel GPU, the RTX's game frames are composited by that same Intel GPU. REPLAYX was measured using a third of its 3D engine at 120 fps. Quick Sync now uses Intel's fixed-function low-power encoder (VDEnc) when available, with automatic fallback.
- **Real 1920×1080.** The output size is now independent of the display: 1920 × 1080 always saves a 1920 × 1080 file. Choose 720p, 1080p, 1440p, 4K, the display's own shape, or a custom size, and Stretch, Preserve (bars) or Crop, with the effect shown before you choose. Older settings that recorded 1728 × 1080 on a 16:10 display move to 1920 × 1080 with Stretch to fill.
- **Quality presets.** Performance, Balanced (28 Mb/s at 1080p60, up from a fixed 12) and High quality (45 Mb/s), scaled to resolution and frame rate, or a custom bitrate. VBR with a 1.5× peak cap, BT.709 colour conversion and tags.
- **Fixed: "Audio output backpressure exceeded 3s" stopped recordings.** The cause was the video side falling behind real time, so FFmpeg stopped reading audio. REPLAYX now measures encoder speed and video lag, warns on the Performance page, and stops with the real reason ("the encoder fell 8 s behind…") instead of an audio error.
- **Fixed: after a failed recording the app said "Capture is off" but Record answered "Recording is already active".** The recorded part is now saved, and a new recording can start immediately.
- **Game-detected notice.** A small bar in the top-right corner, for example *Roblox detected · Instant Replay ready*: once per game, never over the centre of the screen, and it can be turned off in Settings. It uses the same detection as clip folders and runs without the engine lock.
- **Instant Replay switch.** A real on/off switch (Off, Starting, On, Stopping, Error) in the header, on Home and on the Instant Replay page, driven by the engine's state.
- **The window costs less while you play.** When REPLAYX is hidden behind a game, it polls the engine every 2 s instead of four times a second.
- Performance page: encoder speed, video lag, target bitrate and the encoder's low-power state.
- Developer tools: `replayx-gameproxy` (game-like GPU workload with time-coded frames), `scripts/overhead-bench.py`, `scripts/gameproxy-check.py`.

## 0.2.1-alpha.2 — Record no longer freezes the app; faster starts; interrupted-session fixes

- **Fixed: pressing Record froze REPLAYX ("Not responding").** Root cause: once a second while capturing, the game detector asked Windows for the foreground window's title while holding the engine lock. When REPLAYX itself was in front (always true right after clicking Record), Windows delivers that request as a message to REPLAYX's own window thread, and that thread was waiting for the same lock to read the status: a deadlock. Capture kept writing replay segments, but the recording never received any and the window never recovered. Reproduced on the 0.2.1-alpha.1 build in 2 of 2 valid runs; 0 of 8 runs froze with this build.
- **The window can no longer wait for the engine.** Status and settings are read from published snapshots; every command that touches the engine runs on a worker thread; exit, settings, mixer and mute changes moved off the window thread; game sampling runs without the engine lock and never queries REPLAYX's own windows.
- **Starting capture no longer holds the engine lock.** The display query, capture-path probes, audio devices and encoder start run unlocked; Stop during that time cancels cleanly (nothing recorded, nothing left on disk). A start that does not finish within 60 s is abandoned with an error.
- **Faster starts.** The working GPU capture path is remembered per display and settings across launches (probing it took 1.1–2.4 s), and the media runtime is checked once per launch. After the first start, the first frame is encoded about 0.4–0.7 s after Record. Record while Instant Replay runs joins the running encoder instantly (no restart, no gap).
- **Honest state.** The app says *Starting recording…* within about 60 ms of the click and *Recording* only once the encoder writes frames. The recording timer counts the recording, not the capture session. The Record button becomes *Cancel* while a recording is starting.
- **Fixed: "interrupted sessions" piling up.** Stopping replay or recording never deleted the replay buffer, so every stop and every exit added an "interrupted session" on the next launch; frozen/killed processes added more, including empty folders. Stopped buffers are now deleted (retrying if a file is briefly in use), empty folders are not counted, unreadable leftovers are set aside after recovery so a session is never offered twice, and interrupted sessions can be discarded to the Recycle Bin. Footage left by crashes and forced closes is still kept for recovery.
- **Fixed: pressing Stop/Cancel during a slow start waited for the start to finish** (every action was serialized behind an update guard).
- New diagnostics: startup stage timings on the Performance page; `replayx-cli start-bench`; `scripts/ui-record-check.mjs` (drives the real window and measures whether it answers Windows messages); `scripts/ui-delete-check.mjs`; `scripts/start-latency.py`.

## 0.2.1-alpha.1 — smooth 60 fps, fast saves, game folders, working Delete

- **Stutter fixed (root cause found).** 0.2.0's timestamp re-grid pushed frames into future slots and caused repeated pictures and freezes on real motion (7.17%, up to 53 frames). Slot rounding (begun in the Codex session) brings a saved 1080p60 replay to 0.36% isolated single-frame slips. Windows Graphics Capture was measured and rejected (15.7%).
- **Saves are much faster and never block the app.** Segment start times are probed in the background during capture, the concat step no longer runs full stream detection per segment, validation decodes only the clip's ends, and Stop recording no longer restarts capture. A 60 s replay is ready in about 1 s (was about 6 s).
- **Delete works.** 0.2.0 passed a `\\?\` path to the Recycle Bin, which Windows rejects; every delete failed. Fixed, with a confirmation that names the file, Windows' real reason on failure, and protection against deleting anything outside the clips folder.
- **Automatic game detection.** REPLAYX recognises the game in front (Fortnite, Roblox, Valorant, Minecraft, Counter-Strike 2 and more, plus Steam/Epic/Xbox install folders) using only the process name and path, while it captures. Launchers do not count.
- **Clips organized by game.** New clips are saved as `Game\Game_YYYY-MM-DD_HH-MM-SS.mkv` based on what was actually played during the clip; `Desktop` and `Mixed` when appropriate. Library has game categories, search by game, Change game and corrections in Settings. Existing clips are not moved.
- **Library opens instantly.** A persistent metadata index replaces per-launch probing (100 clips: 6.5 s → 46 ms); thumbnails no longer queue behind each other.
- Codex session work included and checkpointed: save progress events, faster validation, motion test fixture, research notes.


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
