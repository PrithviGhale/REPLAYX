# Known limitations — 0.2.1-alpha.3

REPLAYX is **alpha** software. Evidence for what does work is in [VALIDATION.md](VALIDATION.md); measurement limits are in [PERFORMANCE.md](PERFORMANCE.md).

## Game performance and smoothness (0.2.1-alpha.3)

- **The game-FPS impact of this build has not been measured in a real game yet.** On a hybrid laptop (Intel display GPU + NVIDIA), 0.2.1-alpha.2 reduced Roblox from 150–200 to 80–100 FPS in the user's test. This build moves encoding to Intel's fixed-function encoder, but capture itself must still run on the display GPU.
- 120 fps doubles capture and encoding work. If the encoder cannot keep up, the Performance page says so, and capture stops after the video falls 8 s behind. Choose 60 fps or the Performance preset.
- A recording can only be as smooth as the game: if the game renders fewer frames than the recording rate (for example 90 FPS into a 120 fps recording), some frames are necessarily repeated.
- The audio fix (separate graphs plus encoder lookahead) is measured for NVENC. On the Intel Quick Sync path (normal laptop desktop) separate graphs apply, but recordings with sound are not yet measured there.
- *Preserve aspect* (black bars) needs an extra frame copy through the CPU; *Stretch* and *Crop* stay on the GPU.
- **NVIDIA/AMD-driven displays** (desktop PCs; laptops during full-screen games with Advanced Optimus): the bundled FFmpeg cannot resize Desktop Duplication frames on the GPU (`scale_d3d11` cannot create its output texture; mapping to Vulkan or D3D12 is not implemented). *Smooth* (default) records the display's pixels and marks the file to play at the chosen shape. Most players, browsers and editors honour this; an application that ignores the aspect flag shows the raw size (for example 1440×1080). *Exact size* gives a true 1920×1080 file through a CPU copy that can stutter in demanding games.
- The game-detected notice cannot appear over games in exclusive full-screen mode (Windows does not allow it); it shows over windowed and borderless games.

## Capture start (0.2.1-alpha.2)

- The **first** start for a display and set of video settings probes capture paths (1–2.5 s on the development laptop). The result is remembered; later starts reach the first frame in about 0.4–0.7 s. If the remembered path stops working (driver change, MUX switch), that start fails over to probing automatically, taking a few seconds longer once.
- *Recording* appears when FFmpeg reports its first encoded frame, which it does every 0.5 s; the label can lag the real start by up to 0.5 s.
- A start that has not finished after 60 s is abandoned with an error.
- Footage left by a crash or a forced close is kept and counted as an interrupted session until you recover or discard it (Instant Replay page). Discard moves it to the Recycle Bin.

## New in 0.2.1 and not yet verified on real hardware

- **Game detection** has only been checked against real process paths and desktop apps, not while a game was actually in front. If a game is not recognised, add a correction in Settings → General → Game detection, or use Change game in Clips.
- Steady-state 60 fps capture still shows occasional single-frame slips (0.36% in the measured replay); the first ~0.3 s after capture starts repeats frames while the pipeline warms up.
- Game attribution needs at least 30% of a clip's time in one game; shorter moments are filed as Desktop. Clips recorded before 0.2.1 stay in the clips folder as **Unsorted**.
- Long clips still take time to join: about 5 s for 10 minutes on the development laptop. The Clips page shows progress; the file appears only once it is validated.
- The first launch after upgrading indexes existing clips in the background (about 2 s per 100 clips); the list is shown immediately and details fill in.

## Not yet verified on real hardware

- **The redesigned app has only been rendered in a mocked preview**, not clicked through in the native window. Report anything that doesn't respond.
- **Stretched Valorant** (1440×1080 or 1280×960 shown full screen, then recorded to 1920×1080 with *Stretch to fill*) is implemented and validated by geometry tests and desktop captures, but no real Valorant session has been recorded. Both ways of changing resolution (Windows display settings, and GPU control panels) still need testing. On Optimus laptops, scaling is set in the Intel control panel unless the MUX switch is set to dGPU-only.
- **Microphone speech** hasn't been listened to. Microphone arrays are averaged to mono; use **Audio → Test** to check yours.
- **Game + Discord + microphone on separate tracks** works mechanically, using app sessions and per-process loopback, but hasn't been verified in a real voice call or in a video editor.
- **Long sessions:** no 30-minute soak has been run with this build. Audio drift correction is unit-tested over 120 s at ±300 ppm.
- **A/V sync** offset has not been measured end to end.
- **120/144 fps** are selectable but untested; the encoder or capture may not keep up.
- **The audio crackle fix** (0.1.3) passes objective measurements, but nobody has listened to a comparison.

## Capture

- Windows Graphics Capture isn't implemented; capture uses Desktop Duplication (DXGI), with GDI as a compatibility option. Only outputs on the default adapter are listed, and rotated displays aren't supported.
- The GPU path depends on hardware. On this Intel-display laptop, zero-copy Quick Sync is used, and NVENC needs a scaled-frame transfer. On desktops (display on the NVIDIA GPU), D3D11 + NVENC is tried first but hasn't been tested. AMD paths are untested. The CPU fallback always works but costs about 2 cores at 1080p60 from a 1600p source.
- Capture can't see protected content, secure desktops, or games that block it. Anti-cheat and DRM are respected, not bypassed.
- Display-mode changes trigger a bounded restart: replay starts fresh, recordings split into parts, and footage around the switch is quarantined. The detection polls every 500 ms.
- Replay saves use whole 2-second segments, so a clip may end about 2 s before the key press.
- Recording can't be paused, and there's no in-app trimming or editing.

## Audio

- Per-app capture needs Windows 10 build 20348 or later. If the app restarts, select it again; there's no automatic re-attachment.
- System output and per-app sources can't be combined, because that would record app audio twice.
- Device changes (unplugging, default switch) stop capture with an error; there's no automatic device following.
- The native FFmpeg AAC encoder produced artifacts on one synthetic test tone in a direct encode. REPLAYX clips didn't show this, but varied real content hasn't been characterized.
- Audio travels from the capture threads to FFmpeg over an unauthenticated 127.0.0.1 socket. Other software already running as the same user could interfere with it.

## Performance page

- Encoder latency and GPU encoder load aren't measured; the page says so. CPU and memory figures cover REPLAYX and its child processes. Dropped frames are frames FFmpeg skipped to keep a constant rate.

## Distribution

- The 0.2.1-alpha.2 installer is built but **not yet installed and tested** (install, launch, record, upgrade, uninstall). The tested build is the executable in `src-tauri/target/release/replayx.exe`. Don't use the 0.1.x installers; they predate the fixes.
- Binaries aren't Authenticode-signed, so Windows SmartScreen may warn.
- In-app updates are disabled until a publisher signing key and release endpoint exist. The signed update path hasn't been exercised.
- Before a public release, the FFmpeg corresponding source and the notices of its bundled libraries must be packaged with the release (see THIRD_PARTY_NOTICES.md). H.264/AAC patent questions are not resolved by the software license.
