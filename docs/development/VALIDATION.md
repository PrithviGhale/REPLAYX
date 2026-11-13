# Validation — 0.3.1-alpha.9 (November 13, 2026)

Independent review of `a63ef4474e403f9865fd44aa4cf16a2191debffe` passed the three requested fixes. Local safe checks passed 190 Rust tests (registry and Recycle Bin tests excluded, one optional benchmark ignored), 71 frontend tests, nine Python tests, TypeScript and the production frontend build, Clippy with warnings denied, rustfmt and whitespace checks. All 98 checked local documentation/image references resolved. Component checks verified eight slider keyboard transitions and exact saved-length round trips for all 1,771 supported integer durations.

The owner subsequently authorized merging to main and publishing an alpha prerelease. Release notes list the unfinished isolated installation, upgrade, uninstall, hardware and real installer-update tests explicitly. This publication authorization does not establish that those tests passed. No owner-profile installation or capture is part of release preparation.

The `cf92d5a` installer predates later fixes and is superseded. Its original files are preserved in `src-tauri/target-alpha9/superseded-cf92d5a/`.

Alpha.9 was published as a prerelease on November 13, 2026 from main commit `fd526f31a704b92508e07efef921298ea5f3a6c5`. Main's GitHub source checks passed. The fresh optimized release and NSIS build completed. The app and installer's FileVersion/ProductVersion are `0.3.1-alpha.9`, all seven checked version declarations agree, and the generated NSIS script uses current-user installation. All ten bundled runtime binary files match the unchanged pinned runtime byte for byte. The updater signature verifies for exactly this version and `latest.json` carries the same signature and release-specific URL. A Windows Defender custom scan found no threats. Authenticode remains unsigned.

| Artifact | Bytes | SHA-256 |
| --- | --- | --- |
| `src-tauri/target-alpha9/release/replayx.exe` | 17,731,584 | `185832062a303862f759b34b681780118c72b3e609bbb618b9a1ce57a071ac07` |
| `src-tauri/target-alpha9/release/bundle/nsis/REPLAYX_0.3.1-alpha.9_x64-setup.exe` | 61,411,826 | `ab3ced4497129a193ede572d195074f2d8c889535e40eff2ace6496806507ba2` |
| `ffmpeg-corresponding-source-n9.0.2-47-gb204e94e90.tar` | 1,256,304,640 | `41ec89ffcef1f690e58c5de25008021e167c50f4768db94fe5c741ecb9daa090` |

The corresponding-source archive is reused from Alpha.6 because its runtime pin is unchanged. Its checksum matches the previous public release and its source README identifies the matching FFmpeg and build-recipe revisions. All five uploaded assets were checked against GitHub's server-side size/digest before publication. After publication, the public installer, signature, manifest and checksums downloaded successfully without authentication. The downloaded installer retains the expected SHA-256 and its exact-version update signature verifies. The published tag points to the recorded merge commit.

The installed owner executable still matches the original Alpha.4 hash. The settings and update-state files differ from the older 06:19 UTC baseline, with modification times of 16:21 and 15:17 UTC respectively, both before this release preparation began at 18:40 UTC. The library index and absent playtime file match the older baseline. No owner-profile application or installer was launched and user recordings were not accessed during release preparation. This does not provide new installed-app or hardware qualification.

The owner approved a simpler download layout after publication. Public assets now use `REPLAYX.exe`, `latest.json` and `FFmpeg-source.tar`. The installer bytes and SHA-256 are unchanged. The updated public manifest points to `REPLAYX.exe` and its embedded exact-version signature verifies the downloaded installer. Standalone signature/checksum assets were removed after verification; the checksum remains in the release notes and build records.

# Validation — 0.3.1-alpha.4 (November 9, 2026)

Game icons for installed games, Open/hide shortcut, text-fit fixes. Builds ran at below-normal priority; the running app was not touched.

| Check | Result |
| --- | --- |
| Installed-game icons | `replayx-cli game-icons` on this PC: 9 catalog games found with a usable 256 px icon (Fortnite, Counter-Strike 2, Valorant, Apex Legends, PUBG, Genshin Impact, F1 25, Roblox, Roblox Studio); 429 installed entries read in 16–34 ms. Unit tests: name matching (launcher suffixes, "for <user>", no fuzzy match for short names), `DisplayIcon` parsing (skips uninstallers), executable search. |
| Open/hide shortcut | Registered with the other four (same refusal fallback); settings v6 picks a free key if Ctrl+Shift+F11 is taken (unit test). Pressing it on the real desktop is a user test. |
| Text fit | `dev/text-fit-check.mjs`: every page and Settings tab at 1280, 1180, 1100, 1000, 900 and 820 px wide, normal and 130% text: no text clipped or spilling out of its card (before the fix: the Replay length cards at 1100–1280 px, and Performance values at 130%). Scroll check: all pages scroll. |

Automated: Rust 140 tests, frontend 34 tests, Clippy `-D warnings`, rustfmt, TypeScript, production build, NSIS installer.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/replayx.exe` (ProductVersion 0.3.1-alpha.4) | 16,984,064 | `24bc2a5372e1c9b82f3d823493060113948d3c8a306eabc391e31a7abddedb5a` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.4_x64-setup.exe` | 61,134,250 | `f1e218d14eb3eb0e4b9f1d4445dda3462a1d74506a7324f443a61c5e0b7da206` |

# Validation — 0.3.1-alpha.3 (November 9, 2026)

New logo, playtime on by default, lower background impact on games. No game was running; builds ran at below-normal priority; REPLAYX was not launched (the user's settings and data were not touched).

| Check | Result |
| --- | --- |
| Logo | Master `branding/replayx-logo-1024.png` cut from the owner's image (background flood-filled from the corners; inside area matches the rounded square, no artwork altered). Icons regenerated (`npx tauri icon`). Seen in headless Chrome: startup screen, sidebar, About. Icon read back from the built `replayx.exe` and from the installer (which previously had NSIS's default icon; `installerIcon` set). |
| Playtime default | `PlaytimeSettings::default().enabled = true`; settings v5 switches it on once, and a later choice to turn it off is kept (unit test). |
| Background tick cost | `replayx-cli tick-cost`: 17.2–17.8 µs per once-a-second check (foreground process, game classification, idle time) ≈ 0.002% of one core. |
| Helpers below normal priority | Probe, segment checks, clip join, playback checks, thumbnails, export, full decode: `BELOW_NORMAL_PRIORITY_CLASS`. Capture/encoder processes unchanged. |
| Hidden page | Hide to tray, `--background` start and minimise now call WebView2 `SetIsVisible(false)` (wry hides only its child container); every show path makes it visible again. Not yet observed in the running app. |

Automated: Rust 136 tests, frontend 34 tests, Clippy `-D warnings`, rustfmt, TypeScript, production build, NSIS installer.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/replayx.exe` (ProductVersion 0.3.1-alpha.3) | 17,035,264 | `6a77ec5bcef6f2cc96804b428ce6f7576c3f08885cc9b87bc6049d67fbdb4a7b` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.3_x64-setup.exe` | 61,105,995 | `f634801715d1a80f1776d897a00188ee25f301e6c4e93188a37541afa1e3e604` |

**Needs you:** install, then check the icon in the taskbar, tray and Alt-Tab; the pop-ups; that the Games page counts a game session; and that with REPLAYX in the tray your game's FPS is the same as without it.

# Validation — 0.3.1-alpha.2 (November 8, 2026)

Adds the capture pop-ups (Recording started, Instant Replay saved, Recording saved, Save failed). No game was running; builds ran at below-normal priority. No microphone was opened and no recordings were made or touched.

| Check | Result |
| --- | --- |
| Pop-up text and colours | Unit test: titles, hotkey shown as Ctrl+…, game omitted for Desktop/Mixed/Unsorted, m:ss and h:mm:ss lengths, short hold. |
| When they appear | Saves: from the shared clip-save path (hotkey, tray and app), only at *Ready* or *Failed*. Recording started: only once frames are being written (state *Recording*, not pending, elapsed > 0); shown once per recording, also across a capture restart. |
| Setting | `capture_notices` defaults to on; older settings files load with it on. |
| Not yet seen on a real screen | The pop-ups use the same notice window (and capture exclusion) as the game-detected notice, but were not shown on screen or checked against a recording in this session. |

Automated: Rust 135 tests, frontend 34 tests, Clippy `-D warnings`, rustfmt, TypeScript, production build, NSIS installer.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/replayx.exe` (ProductVersion 0.3.1-alpha.2) | 16,843,264 | `18fcbc85785a5f2449847596d568e0a413ed7cb2c251032df8aa5a823751672e` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.2_x64-setup.exe` | 60,729,743 | `e2d0dfcaf509baa90e5061daea5eae84072ea4f9b59d9cf7f380ba1aed07b5b4` |

# Validation — 0.3.1-alpha.1 (November 8, 2026)

Development laptop. Roblox was open during the final checks, so nothing that shows windows or takes the foreground was run then (no overlay test, no full-screen recording benchmark, no app launch); builds and tests ran at idle CPU priority. No microphone was opened. No recordings or recovery sessions were touched; test data lived in temporary folders.

## Verified

| Check | Result |
| --- | --- |
| Thumbnail mix-up (root cause) | Reproduced in code: `Thumb` kept its first image and skipped loading for a new clip. Fixed; tests cover rapid switching with a late answer for the previous clip, shared requests, retries and invalidation on rename/move/replace. |
| New thumbnail generator | Real clip: 0.2 s per thumbnail (representative of 30 frames, square pixels, 640 px). |
| Tray root cause | The user's settings had `close_to_tray: false` (the old default), so closing quit the app. Default and one-time migration now enable it; click-to-open and menu wired. |
| Alt+F10 | Parses and records as `Alt+F10`; a refused shortcut keeps the previous one with the requested message (unit-tested planner). Registration against the real NVIDIA App is a user test. |
| Game catalog | 140 entries validated (unique ids, folder-safe names, lower-case executables, generic names need folder evidence); disambiguation and launcher exclusion tests. |
| Playtime tracker | Unit tests: counting, game switches, background/alt-tab, short switches, sleep gaps, idle removal and opt-out, disabled/paused/excluded, crash recovery exactly once, CSV export. Frontend: local-midnight split, daylight-saving day length, periods, totals, daily series. |
| Overlay renderer | 26 px tiles (from 32), timer digits, corner alignment, DPI scaling (unit tests and a rendered PNG). |
| Scrolling | 24/24 page/size combinations (now including Games). |
| Executable version | `replayx.exe` ProductVersion and FileVersion 0.3.1-alpha.1; installer file name matches. |

## Automated

Rust 134 tests, frontend 34 tests, Clippy `-D warnings`, rustfmt, TypeScript, production build, NSIS installer.

## Build artifacts (not installed, not published)

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/replayx.exe` | 16,840,704 | `a15498c8a5765a13c3c029d525f73eaed860b17ebf1105f5fe33408156cdef99` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.1_x64-setup.exe` | 60,716,159 | `90d78c9fe79279155fe7a092491c08d6ab2ec5fa1ee47ece6045cffc0303bfdc` |

## Pending (needs the user, or a moment with no game running)

- Tray icon visible after closing (it may sit in the hidden-icons area), reopening by click, Quit from the tray.
- Alt+F10 with the NVIDIA App running (expected: the conflict message) and after freeing it in NVIDIA (expected: registered); shortcuts while REPLAYX is in the tray; persistence across restarts.
- Correct thumbnail when switching between real recordings; By game tiles and counts with real clips.
- Playtime with real games (opt in on the Games page, play, alt-tab, check totals).
- Game artwork appearing for games seen with this version.
- Overlay exclusion re-check with the smaller tiles (`replayx-cli overlay-test`) and a 60 fps recording regression run (`scripts/bench/overhead-bench.py`); both use the same mechanisms as 0.3.0-alpha.1, which passed.

---

# Validation — 0.3.0-alpha.1 (November 8, 2026)

Same laptop (RTX 4070 Laptop GPU driving the 250 Hz panel during these tests). The user was away for the unattended parts. **No microphone was opened** during development or testing: voice processing was evaluated with Windows text-to-speech and synthetic noise. No game was launched or touched; capture tests used REPLAYX's own game proxy and throw-away folders. The user's settings file was backed up before each start-up test and is byte-identical afterwards.

## Measured

| Test | Result |
| --- | --- |
| Previous noise suppression (FFmpeg `afftdn`, as shipped) | Removed 2.2 dB of steady pink (fan-like) noise at −48 dBFS. |
| Voice Enhancer noise suppression (`scripts/bench/voice-eval.ps1`, TTS speech + noise) | Fan: Light −9.5, Balanced −17.0, Strong −25.0 dB, Noise-Free preset −28.6 dB. Hiss: −10.0 / −18.4 / −28.0 / −32.3 dB. Clean speech level change ≤0.3 dB. Keyboard clicks only partly reduced. |
| Voice Enhancer cost | 0.5–1.0% of one CPU core (release build); 11 ms added latency with suppression (RNNoise 480 samples, measured by cross-correlation and asserted by a test; limiter 1 ms). |
| Startup (`dev/startup-bench.mjs`, warm medians, back to back) | Interface mounted 455 ms (0.2.2-alpha.2) vs 427 ms (0.3.0-alpha.1); app ready 483 ms; splash adds no delay. Cold first launch 1.5–2.4 s in both. |
| Recording regression, 60 fps, system audio, game proxy | 4.0% game-FPS cost, 0.56% repeated frames (the stutter fix's established range is 0–0.6%). |
| Recording, 120 fps, game proxy | **Not smooth:** 43% repeated frames with audio, 45% silent, 45% with 32-frame look-ahead, 38% with oversampled capture. Independent of audio and of this release's changes; listed in KNOWN_LIMITATIONS. |
| Mouse-wheel scrolling (`dev/scroll-check.mjs`) | 21/21 page/size combinations scroll. |
| Git history (38 commits) | No keys, tokens, passwords, recordings, audio, settings files, personal paths or e-mail addresses. |
| Final build smoke test | Starts tray-only, reports ready, no orphan processes after closing; user settings unchanged. |

## Automated

Rust 121 tests (new: Voice Enhancer presets, raw pass-through, RNNoise delay, suppression strengths and Off, quiet speech through the expander, no clipping at +24 dB for every preset, compressor level spread, level matching, garbage input, 10 ms frame budget, tour/tray-hint persistence, FFmpeg only limiting loud sources), frontend 20 tests (splash lifecycle and reduced motion, tour steps/placement/auto-start, rate mirrors, microphone defaults), Clippy `-D warnings`, rustfmt, TypeScript, production build, `npm audit` 0 vulnerabilities, NSIS installer built.

## Build artifacts (not installed, not published)

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/replayx.exe` | 16,443,904 | `833a0a43f5956d1048b5d018cff602be899a750526240a14452ecb5b1be83b02` |
| `src-tauri/target/release/replayx-cli.exe` | 3,302,400 | `49b8b88fd20541b14bed9a35a68207cd17e8e1c5518854229bb09968ce0775fc` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.0-alpha.1_x64-setup.exe` | 60,630,748 | `9489c307ea2afeb287e666d5a0063a579e9b3e0529f61082ea917c1996409e49` |

## Not verified (needs the user)

- Listening: every Voice Enhancer preset and suppression strength on the user's headset; Test mic Enhanced/Original; Compare; a recording with the microphone; a fair comparison with Voicemod on the same recording.
- The splash and tour in the real window (first launch shows the tour; Skip, Esc, Replay tutorial).
- 120 fps with a real game; overlay over a real game; tray Quit while recording; installing the installer over an existing version.
- Release gate: corresponding source for each LGPL library bundled with FFmpeg (FFmpeg itself and the FFmpeg-Builds recipes are prepared in `release/ffmpeg-source/`).

---

# Validation — 0.2.2-alpha.1 (November 8, 2026)

Same laptop (RTX 4070 Laptop GPU drove the panel during these tests: Advanced Optimus, DISPLAY5). The user was away. No microphone was opened (no microphone test was run), nothing was played, no game was launched or touched; capture tests used REPLAYX's own game proxy and throw-away folders under `test-output/`. The user's settings file was backed up before the start-up smoke test and is byte-identical afterwards.

## Measured

| Test | Result |
| --- | --- |
| Quality per GB (VMAF/SSIM/PSNR, `scripts/bench/quality-bench.py`, real Roblox clip + synthetic scene) | See docs/development/PERFORMANCE.md. New Balanced H.264 (CQ 28): 14.7 Mb/s, VMAF 97.6 vs 16.7 Mb/s, 97.9 before. AV1 CQ 34: 12.5 Mb/s, VMAF 97.7. |
| Game FPS and smoothness per codec (`scripts/bench/overhead-bench.py`, game proxy full screen, system audio, Balanced 1080p60) | H.264 2.9% FPS cost, 0.0% repeated frames; HEVC 4.4%, 0.0%; AV1 3.0%, 0.14%; AV1 High 3.9%, 0.0%. Saved clips verified and decodable. |
| NVENC colour conversion (solid colours from system memory, CUDA and D3D11 frames; H.264, HEVC, AV1) | Always BT.601 matrix, limited range. Saved H.264/HEVC/AV1 clips from the real capture path now tagged `tv, smpte170m, bt709`. Quick Sync VPP: BT.709 (unchanged). |
| Overlay excluded from capture (`replayx-cli overlay-test`, real Desktop Duplication frames saved losslessly) | Excluded vs hidden: mean pixel difference 0.0. Control with capture allowed: 77.0 (overlay clearly visible). Files and PNGs in `test-output/overlay/`. |
| Mouse-wheel scrolling (`dev/scroll-check.mjs`, 7 pages × 3 window sizes) | 21/21 before the fix failed, 21/21 pass. |
| Microphone processing chain (real FFmpeg, automated) | +24 dB boost of a 0.9 sine with compressor stays between −12 and −0.3 dBFS (limiter). |
| Built app start-up with the user's real settings (`replayx.exe --background`) | Starts in the tray, responsive, 42 MB, 62 ms CPU over 5 s idle; encoder probes exit by themselves; no orphan processes after the test instance was stopped. |
| HEVC/AV1 MP4 remux | `hvc1` / `av01` tags. |

## Automated

Rust 109 tests (new: codec mapping and fallback order, capped constant quality per codec, NVENC colour tags against a real encode, overlay states never claiming capture early, overlay items and placement, microphone chain order/limiter and a real-FFmpeg clipping test, separate audio/video graphs and lookahead kept), frontend 15 tests (rate mirrors match the engine, codec ids, storage estimate, microphone state), Clippy `-D warnings`, rustfmt, TypeScript, production build, `npm audit` 0 vulnerabilities, NSIS installer built.

## Build artifacts (not installed)

| File | Bytes | Version | SHA-256 |
| --- | --- | --- | --- |
| `src-tauri/target/release/replayx.exe` | 15,186,944 | 0.2.2-alpha.1 | `98c137b770686703c27184bf9b69e17bd71bf239fdee95d4efb08d066b9cbf00` |
| `src-tauri/target/release/replayx-cli.exe` | 1,765,376 | 0.2.2-alpha.1 | `9010f2516f1cc0a51ed6220e664a1100b1f930d53a4c603d1fb5a29abc36b347` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.2.2-alpha.1_x64-setup.exe` | 59,830,982 | 0.2.2-alpha.1 | `22215b2d95bfb4da7eb751dbfca133b6e35835c99f8d7b881149a351961ebf02` |

## Not verified (needs the user)

- Microphone: device test, listening to the processed result, boost and processing by ear, a real recording with the voice on its own track.
- Real games: FPS and smoothness with the new presets and with AV1; picture quality by eye against GeForce.
- The overlay over a real borderless/windowed game and that it stays out of a real game clip; how it looks at other DPI settings.
- Tray: the menu, Quit while recording (confirmation), close to tray and the one-time notice. A graceful close request sent by `taskkill` did not reach the hidden window, so the app's own quit path was not exercised automatically.
- Installing the NSIS installer over the existing version.

---

# Validation — 0.2.1-alpha.2 (November 7, 2026)

Same laptop (Iris Xe drives the 2560×1600 @ 240 Hz panel; RTX 4070 Laptop). Roblox, Steam and the Epic launcher were open in the background; capture tests ran after the user approved about 12 minutes of silent capture (no microphone, nothing played; one 15 s run with system-audio loopback was approved separately). All runs used throw-away data folders under `test-output/`; the user's settings, clips and sessions were not touched.

## The Record freeze

| Evidence | Result |
| --- | --- |
| Windows error reports for the user's freezes | `AppHangB1`, `replayx.exe` 0.2.1.0 at `src-tauri\target\release\replayx.exe` (the 0.2.1-alpha.1 build, not an installed copy), 18:20:53 and 18:21:40. A 0.2.0.0 hang is also recorded at 16:13:37. |
| User's data folder after the freezes | Each frozen session: a buffer folder full of 2 s segments (87 and 17) and an **empty** recording folder. Capture ran, but the engine's tick never reached the step that adds segments to the recording. |
| In-process reproduction (`games::tests::sampling_own_window_never_waits_for_its_blocked_owner_thread`) | `GetWindowTextW` on a window owned by the calling process blocks while the window's thread waits on a mutex, and completes once it is released. The new `window_process` returns immediately for REPLAYX's own windows. |
| Real window, 0.2.1-alpha.1 (`scripts/bench/ui-record-check.mjs`, Record pressed in the page) | Froze in 2 of 2 valid runs: window stopped answering 1.4 s after Record (`IsHungAppWindow` true; force-closed after 6 s in one run, after more than 3 min in the other), status stuck on *Stopped*, recording folder empty. A third run lost the foreground at the click (another window was in front), so the deadlock could not occur; it stayed responsive. |
| Real window, this build | 8 of 8 runs responsive: 5 × Record/Stop, 2 × Cancel while starting, 1 × Replay → Record → Stop → Save replay → Replay off. REPLAYX was in the foreground for over 99% of samples; worst time to answer a window message 6.3 ms (the frozen build: no answer within 1 s). |

## Hardware tests run with this build

| Test | Result |
| --- | --- |
| Record acknowledgment (real window) | *Starting recording…* shown 57–69 ms after the click (5 runs). |
| Cold first start (new data folder; capture paths probed) | *Recording* shown 1.4–2.6 s after the click (5 runs). Bench: probing 1.1–2.4 s, first frame 1.75–3.0 s. |
| Start after relaunch (remembered path) / same session | First frame encoded 403–681 ms / 654–668 ms after the request; the start command returns in 8–20 ms. |
| Record while Instant Replay runs | Joins the running encoder (same process id), no capture gap; state *Recording* immediately. Stopping it kept replay buffering; *Save replay* afterwards worked. |
| Cancel while starting (real window) | Back to *Stopped* about 0.6 s after Cancel; no clip, no session folder left (2 runs). |
| Rapid start/stop, failures (automated, synthetic source) | No encoder or folder left behind; missing runtime, unusable buffer folder and audio-source failure return an error and allow a new attempt. |
| Interrupted sessions | After every bench and UI run: 0 sessions left and no folders on disk. |
| 60 fps smoothness (silent motion pattern, decoded per frame) | Replay save: 0.42% irregular after warm-up. Recording, final build: 0.09% and 0.17%; 0.2.1-alpha.1 on the same test: 0.06%, 0.17% and 1.17%. Warm-up (first 0.2 s) repeats as before. Builds with a 0.25 s progress interval measured 0.42%, 1.87% and 1.39%, so the interval was kept at 0.5 s. |
| System audio (15 s, loopback only) | Two AAC tracks (*Playback mix*, *System audio*), 1,552 packets, 0 gaps, 0 discontinuities, 5.4 ppm drift correction. |
| Stop → clip ready (6 s recordings) | 1.35–1.94 s (9 recordings; 2.2 s when replay keeps running) (waits for the segment containing the Stop moment; replay keeps running). A replay save made during another save got a collision-free name. |
| Delete in the real Clips page (`scripts/bench/ui-delete-check.mjs`, generated clips) | 10/10: the clicked clip is the one deleted, the dialog names it, it lands in the Recycle Bin from the test folder, the list updates, the other clips are untouched; a clip held open by another program is refused with Windows' reason and kept. |

## Automated

Rust 91 tests (13 new: status never waits for a busy engine, the deadlock mechanism, failed starts, the start watchdog, cancel during start, rapid start/stop, output-folder and audio failures, Record joining replay, session counting, busy-segment cleanup), frontend 10 tests (Record/Cancel/Stop mode, *Starting* until frames are written), Clippy `-D warnings`, rustfmt, TypeScript, production build, `npm ci` with 0 vulnerabilities.

## Build artifacts (not installed)

| File | Bytes | Version | SHA-256 |
| --- | --- | --- | --- |
| `src-tauri/target/release/replayx.exe` | 14,855,168 | 0.2.1-alpha.2 | `2AE632A3FC4B6D1343901307379ADB79AD2FCA0C1484A7B502097ACEB30DAA45` |
| `src-tauri/target/release/replayx-cli.exe` | 1,986,048 | 0.2.1-alpha.2 | `5F995B7E783198D8D37E537E7C42300651D2430289175A9449F3F79FA943997B` |
| `src-tauri/target/release/bundle/nsis/REPLAYX_0.2.1-alpha.2_x64-setup.exe` | 59,691,411 | 0.2.1-alpha.2 | `B6C2FAA191B55C20DD6FAABF7EEE83499EFC87E3C5EF3809B15FC71E4C7181CE` |

Smoke test of this exact `replayx.exe` in the real window: responsive (worst 0.8 ms), *Starting recording…* at 58 ms, *Recording* at 1.7 s (first start in a new data folder), clip ready 2.0 s after Stop, no session left.

## Not verified

Game detection while actually playing; in-game FPS impact; a 15-minute soak; microphone; the NSIS installer installed on this PC; stretched Valorant.

---

# Validation — 0.2.1-alpha.1 (November 7, 2026)

Same laptop as below (Iris Xe drives the 2560×1600 @ 240 Hz panel; RTX 4070 Laptop). Roblox was open in the background for the whole session; capture tests ran only after the user approved them, while nobody was playing.

## Hardware tests actually run

| Test | Result |
| --- | --- |
| A. Smooth 60 fps (silent motion pattern, decoded tick per frame) | Current pipeline 0.83% repeats in 20 s, all in the first 0.3 s; steady state 0 repeats/0 skips over 1,181 frames. Full app path (saved replay, 28 s): 0.36% isolated single-frame slips after startup. 0.2.0's re-grid: 7.17%, freezes up to 53 frames. `gfxcapture`: 15.7%. |
| B. Replay save (real capture) | Saved into `Desktop\Desktop_2026-11-07_18-05-31.mkv`, 1920×1080 60 fps, ready 608 ms after the save request (snapshot 24 ms, no segment probing needed, remux 186 ms, validation 397 ms). Concurrent second save also succeeded. |
| C. Stop recording (synthetic source: generated pattern/tones, nothing captured) | Clip handed over 706 ms after Stop, ready ≈1.0 s after Stop; replay kept encoding (685 → 777 frames in the next 2 s). 22.0 s file with titled tracks. A 60 s real-desktop recording was not run (outside the approved capture time). |
| E. Recycle Bin | Root cause reproduced with a temporary file (`\\?\` path → SHFileOperation error 124, file kept). After the fix a real round trip moved a temporary 1-byte file to the Recycle Bin. |
| F. Game detection (read-only) | Live foreground sampling classified Opera and VS Code as desktop apps. Classifier verified against the real installed paths of running processes: RobloxPlayerBeta → Roblox; EpicGamesLauncher, EpicWebHelper, EpicOnlineServicesUserHelper, steam, steamwebhelper, RobloxCrashHandler → not games. **Detection while actually playing Fortnite, Roblox or Valorant in the foreground was not observed.** |

## Measurements without capture (fixtures)

- Save path, synthetic 1080p60 12 Mbps segments: 60 s replay ≈6 s → 0.96 s; 10 min ≈56–62 s → 5.0 s. Concat per-file probing removed: 10 min remux 10.0 s → 3.9 s with packet-identical output (framemd5).
- Real capture segments: the first segment of a session starts at 0.021 s (AAC priming) and the final segment of one session at 0.032 s, so start times are probed per segment, not assumed.
- Library (D): 10 clips 623 → 15 ms, 100 clips 6,477 → 46 ms, 1,000 clips 478 ms warm (first-ever index 20.4 s in the background); filtering 1,000 clips takes microseconds.

## Automated

Rust: 79 tests (library ids/traversal/delete/in-use/index, game classification and attribution, concat list, audio timeline, geometry, encoders). Frontend: 8 tests (save events, library filters, delete/selection, category moves). Clippy `-D warnings`, rustfmt and TypeScript clean.

## Build artifacts

Built November 7, 18:13 local from commit `4a9d54e`:
- `src-tauri/target/release/replayx.exe`, 14,612,480 bytes, SHA-256 `4D36039722C4D35072556D644E20BC64CCDB70940401A9D1879CEDCDFEDAD228`
- `src-tauri/target/release/bundle/nsis/REPLAYX_0.2.1-alpha.1_x64-setup.exe`, 59,627,334 bytes, SHA-256 `5A16E45E41EC806C62B65298DA36AC5A90E2C391D75B784C7AB6CA1D93179363`

Both report FileVersion/ProductVersion 0.2.1-alpha.1. The installer was produced in the same build from this executable with media staged by `prepare-bundle.ps1`; its compressed payload was not extracted for independent inspection (no extractor installed). It has **not** been installed.

## Not verified

Real gameplay detection (Fortnite/Roblox/Valorant in the foreground), 15-minute soak (not authorized this session), in-game FPS impact, audio with real devices after these changes (only synthetic tones; the audio timeline code was not modified and its tests pass), a 60 s real-desktop recording, installer install/upgrade, stretched Valorant.

---

# Validation — 0.2.0-alpha.1 (November 7, 2026)

All measurements are on the development laptop: **ASUS ROG Zephyrus M16 (GU604VI), Intel Iris Xe + RTX 4070 Laptop GPU (Optimus), 2560×1600 @ 240 Hz panel at 125% scaling, Windows 11**. The panel is driven by the Intel GPU. A game (Fortnite) was running for the whole session, so every capture also had a real game in the background. Release CLI builds were used unless noted.

## GPU scaling (root cause and fix)

- `scale_d3d11` fails with `Could not create the texture (80070057)` even on the Intel GPU alone, and Desktop Duplication textures cannot reach NVENC on the other adapter. The old probe discarded FFmpeg's error, so this was never visible.
- Isolated encoder-process CPU, 15 s at 60 fps, 2560×1600 → 1920×1080 (`scripts/bench/bench-scaling.ps1`): CPU scale + NVENC **1.94 cores** (9.7% of the machine); Intel VPP + NVENC **1.07** (−45%); Intel VPP + Quick Sync zero-copy **0.56** (−71%); D3D11 + NVENC fails.
- Full pipeline, 30 s, 60 fps, system audio, two overlapping saves (`scripts/bench/measure-capture.ps1`, whole-run CPU including probes and the save-verification decodes): CPU path 2.05 cores, 57 dropped / 2,665; hybrid 1.76, 36 / 2,788; zero-copy 1.17–1.25, 37–46 dropped per ~2,500 frames.
- Bugs found on the way: Quick Sync emitted one IDR, so no 2 s segment closed (fixed with forced IDR); audio failed while FFmpeg initialised Quick Sync (3 s hand-off buffer, demuxer queue); a probe-passing path failing in the pipeline now falls back automatically. The fallback was observed working: an NVENC request whose hybrid path had been denied ran on CPU and saved valid clips.

## Frame pacing

- Raw Desktop Duplication at 60 fps on the 240 Hz panel: 599 frames in 10 s (right count), but present-time stamps put **186 frame pairs in the same 1/60 s slot and left 187 slots empty**. At 30 fps pacing was clean.
- Slot re-gridding (`media::regrid_filter`): 20 s, Quick Sync: dropped 5 → **0**, duplicated 33 → 20 (remaining duplicates are slots with no new frame).
- Full pipeline after the fix, 30 s at 60 fps: **0 dropped of 2,658 frames, 59.7 fps**, zero-copy Quick Sync, 0 audio discontinuities, both clips decode-verified.
- Real game capture: a 65 s 1080p60 run with Fortnite in front recorded the game correctly with **0 dropped frames**. The planned flash/beep A/V-sync measurement in that run was invalid: the game covered the stimulus window. Its beeps were also audible to the player; audible tests were stopped once the game was noticed.

## Audio

- The 0.1.3 crackle fix is unchanged and its regression tests pass. A UI/engine change exposed a separate startup bug (above), now fixed.
- Microphones: `replayx-cli device-formats` negotiates, without starting, the native format. The Realtek array opens as **4 ch float** and is averaged to mono; Voicemod as 2 ch. Speech has not been recorded or listened to, because nobody was asked to speak.
- App sessions: enumeration on the live system listed Discord, Fortnite, FxSound and Voicemod (playing) and Spotify, Opera GX and Steam (idle). The extracted Discord icon decodes as valid 32×32 RGBA PNG (FFmpeg).

## FFmpeg runtime

Re-pinned to `autobuild-2026-11-06-12-50` (`n9.0.2-47-gb204e94e90`, SHA-256 `8e83cbb0…78ee`). All 224 files are byte-identical to the previously validated runtime. Defender custom scan: no threats (1.463.176.0). `ffmpeg -L` reports LGPL v3+; no GPL or nonfree flags. The old floating `latest` URL had already been republished with different bytes.

## UI

Rendered with the mocked-IPC preview (`dev/`) in headless Edge and inspected page by page: Home, Video, Audio, Clips (with detail), Performance, Settings. Fixes found that way: hero layout, 16:10 label, missing explanation of locked settings, and an error boundary. **The redesigned UI has not been run inside the native app window yet**: that needs the desktop, which the game was using.

## Automated checks

`scripts/check.ps1`: npm ci (0 known vulnerabilities), TypeScript/Vite build, rustfmt, **59 Rust tests passed, 0 failed**, Clippy `-D warnings` clean.

## Build artifacts

Built November 7, 15:30 local, from commit `19a21e5` plus this record:
- `src-tauri/target/release/replayx.exe`, 14,322,176 bytes, SHA-256 `F15B64A2CA41BD7BE67BBA718774F0AE4D76A39B154795261FA61EE44F7C8C54`
- `src-tauri/target/release/bundle/nsis/REPLAYX_0.2.0-alpha.1_x64-setup.exe`, 59,466,337 bytes, SHA-256 `82520298A6488FCCEB940A9F73E2617FC58B637F6D64FC345A7893DEA76F3377`

Both report FileVersion 0.2.0-alpha.1. Resources were staged by `prepare-bundle.ps1` from the re-pinned runtime (`n9.0.2-47-gb204e94e90-20261106`). The installer has **not** been installed or run.

## Pending (needs the user or a free desktop)

| Item | Why pending |
| --- | --- |
| Launch and click through the native app | Would cover the running game |
| Installer install / launch / record / uninstall / upgrade | Changes the system; needs approval and GUI |
| Valorant 1440×1080 stretched, via Windows and NVIDIA/Intel scaling | Needs the user to play |
| 30-minute replay soak, memory and A/V drift over time | Would load the PC during gameplay |
| 120 / 144 fps capture | Same |
| Microphone speech, Discord + game isolation in a real call | Needs the user's voice and consent |
| Human listening test of the 0.1.3 audio fix | Needs the user |

---

# Audio repair validation — 0.1.3-alpha.1 (November 7, 2026)

**Reported defect:** recorded audio sounded crackly/distorted. Clips open in the default Windows player (there is no in-app player), so "preview" and "independent playback" use the same file; the defect was measured in the file and in the raw producer PCM before FFmpeg.

**Root cause (measured, not assumed):** `audio.rs` re-placed every 10ms WASAPI packet by its QPC timestamp (truncated to whole frames) on a wall-clock timeline. Timestamp jitter of a few frames made it insert zero frames or drop samples at almost every packet boundary. Format handling was correct (48kHz stereo f32 via AUTOCONVERTPCM; bytes decoded as f32, not integers); no clipping, channel, AAC, concat or player cause was found for the crackle.

Evidence (analysis scripts: `scripts/bench/audio-tone.py`, `scripts/bench/audio-clicks.py`; a single inserted/dropped frame shifts tone phase by 7.5°/11.25°):

| Measurement | Before fix | After fix |
| --- | --- | --- |
| Earlier raw producer WAV (`audio-test-system-20261106-235750-24267021`, real content) | 165 isolated zero-frame insertions in 10s; 163 exactly 1 frame long, all at the same phase of the 480-frame packet grid; ~12 click events/s | — |
| User's own GUI buffer segment (real content, AAC) | ~15.6 click events per active second | — |
| Real FxSound/system loopback, known L=1kHz/R=1.5kHz tone at −20dBFS, clean 0–6s window (`audio-test-system-20261107-122248-df9cebc9` vs `…-123903-7a97c35e`) | **570/572** phase discontinuities, THD+N −7.5dB, crosstalk −25dB, amplitude 0.0929 | **0/0** discontinuities, 1 click event (stream onset), crosstalk −57/−52dB, amplitude 0.09998. Residual (other PC audio playing) is broadband; tone harmonics −75 to −95dB |
| Isolated per-process loopback, same tone (`audio-test-application-20261107-124053-5fecbe13`, exact f32 producer output) | — | 0 discontinuities, 0 clicks, THD+N −75.9/−77.3dB (source WAV: −75.8/−77.3dB, its 16-bit floor), crosstalk −127dB |
| End-to-end NVENC + AAC + segments + two concurrent saves, app tone (`capture-test-20261107-124121` debug; `capture-test-20261107-130017` release, 1920x1080 stretch) | — | Both tracks of both clips: 0 discontinuities, 0 clicks, THD+N about −62dB (AAC 192k), amplitude 0.09995 |
| Same, system loopback, release (`capture-test-20261107-130309`) | — | 0 reported discontinuities, 0 zero insertions. Two click-detector events coincide with other PC audio; residual there matches arbitrary instants (no local anomaly) |

**Second defect found and fixed:** clips containing a session's first segment had a 21ms audio *and* video timestamp hole at the 2s seam (segment 0 starts at 0.021s after AAC priming; the concat demuxer re-bases by it, but the declared duration did not). After the fix, release clips containing segment 0 show no packet gap >1.5ms (previously one 21ms gap). Decoded PCM was contiguous either way; players that honour timestamps could show a dropout/held frame.

**Regression tests:** 12 new (11 audio timeline/resampler, 1 concat). A mutation check restoring per-packet re-placement makes `jittered_timestamps_do_not_insert_or_drop_samples` fail. Simulations cover ±20-frame timestamp jitter, ±300ppm clock drift over 120s, channel isolation, idle-loopback gaps, a 150ms capture stall, an 80ms backward timestamp jump, 120ms device delivery latency and missing/implausible timestamps.

**Full check:** `scripts/check.ps1` exit 0 — 45 passed, 0 failed/ignored; rustfmt; Clippy `-D warnings`; TypeScript/Vite; npm audit 0 vulnerabilities.

**Endpoint formats on this PC** (`replayx-cli device-formats`, no stream opened): FxSound Speakers (user's selected source), Voicemod Dummy Output, Realtek Speakers and Voicemod Microphone are 48kHz 2ch float, mask 0x3. Realtek Microphone Array is **4ch float, channel mask 0x0**; its stereo conversion is untested. The BlackShark headset from November 6 was not connected.

**Not verified:** a human listening test (see procedure in KNOWN_LIMITATIONS.md); microphone speech; Discord/game app isolation in real use; long sessions with real clock drift (measured device drift here was about +5 to +9ppm over 10–20s); editor import. A direct FFmpeg AAC 192k encode of the synthetic tone WAV (without REPLAYX) showed 16 phase steps and −39.6dB THD+N on one channel; REPLAYX clips did not show this, but the native AAC encoder's behaviour on real content has not been characterised.

**Artifacts (built November 7, 13:16 local, from the source above):** `src-tauri/target/release/replayx.exe` 13,978,624 bytes, SHA-256 `B471E25D41917D953C3B71DBC921604637775341AEED5C83006E4DFA4D5DC265`; installer `src-tauri/target/release/bundle/nsis/REPLAYX_0.1.3-alpha.1_x64-setup.exe` 59,333,476 bytes, SHA-256 `9FE2538058D05F27639C643B25331DD8E77D2CD7EC11B3822F31985B4B11DAC2`. Both embed FileVersion/ProductVersion 0.1.3-alpha.1. Bundle resources were staged by `prepare-bundle.ps1` from the checksum-pinned runtime. The installer has **not** been installed, launched or used to record; no REPLAYX installation was present. The older `REPLAYX_0.1.2-alpha.1_x64-setup.exe` (59,363,198 bytes, November 7 09:54) and 0.1.0 installers remain on disk and contain the crackle defect.

Video note from these runs: release CLI at 2560x1600 → 1920x1080 with the CPU scaling fallback dropped 40/826 and 25/814 frames at 30fps; GPU scaling probe still fails. Not an audio issue; not a gameplay benchmark.

---

# Current repair validation — 0.1.2-alpha.1 (November 7, 2026)

- Replaced the Defender-quarantined master snapshot (Trojan:Win32/Wacatac.C!ml) with BtbN LGPL stable-branch build `n9.0.2-47-gb204e94e90-20261106`. Archive SHA-256: `858225c38c2a86d2407ac9239d5af85f5b7ca721f0886084fb6ee111539c8208`. Official FFmpeg download page lists BtbN: https://ffmpeg.org/download.html .
- Windows Defender custom scan of the extracted replacement reported **no threats**, signature 1.463.169.0, real-time protection enabled. No exclusions, protection changes or quarantine restore. Scan log: `tools/ffmpeg-stable-defender-scan.log`. Both replacement executables launched successfully.
- Corrected a real DPI bug: DXGI initially reported 2048 x 1280 for a 2560 x 1600 mode. Querying in a scoped per-monitor-aware thread now reports full physical dimensions. Microsoft documents DPI virtualization in https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgioutput-getdesc .
- Real desktop test after correction: `test-output/capture-test-20261107-093814/report.json`. Full 2560 x 1600 source, 1920 x 1080 H.264/NVENC output, CPU scaling fallback, 10.047s saved clips, two audio tracks, 1,396 WASAPI packets, zero queue gaps, zero reported frame drops, 29.72 reported FPS. Two overlapping saves passed full decode verification. Decoded audio RMS 0.05415; monotonic video/audio timestamps; decoded frame visually inspected. This is desktop capture, **not fullscreen Valorant validation**; source content can itself contain letterboxing.
- The first successful real run (`capture-test-20261107-093404`) preceded the DPI correction and reported 11 drops; it is not evidence of correct full-source geometry. Initial restricted execution denied Desktop Duplication; the explicitly scoped test outside the sandbox succeeded.
- Synthetic media test `self-test-20261107-093454`: 3 tracks, 100.997/89.317 dB track separation, 9 pulse/flash events within 10ms, monotonic timestamps. Physical microphone/app isolation and editor compatibility remain unverified.
- Defaults now use Ctrl+Shift+F10/F9/M/F8. The complete legacy default set migrates; custom shortcuts remain. One unavailable shortcut no longer unregisters the others.
- Automated regression suite: **33 passed**, zero failed/ignored; includes DPI-context restoration, legacy shortcut migration and distinct shortcut parsing. Frontend production build, rustfmt and Clippy with warnings as errors passed. The complete check ran outside the sandbox after npm failed internally in the restricted cache. The retired development runtime cache/archive was removed. Installer and final native UI observations are recorded below once completed. Public release, gameplay/stretch-driver qualification, long-session sync and installer upgrade/uninstall qualification remain outside these repair results.

---

# Historical validation — 0.1.1-alpha.1
November 6–7, 2026. This is an alpha hardening build, not a verified Windows beta or production release. Nothing was pushed or publicly released.

## Current source checks

- Final `scripts/check.ps1` run on 0.1.1-alpha.1: **30 passed**, zero failures; rustfmt, TypeScript/Vite and Clippy all passed. npm ci reported zero known audit vulnerabilities at that time.
- TypeScript/Vite production build: passed.
- Clippy, all targets and desktop features with warnings as errors: passed after fixing lint findings.
- Coverage includes source bounds/overflow, stretched canvases, negative monitor origins, mode versus identity changes, rotated-display rejection, settings migration, storage/progress policies, missing-runtime error state, shortcut-warning persistence, snapshot concurrency/rollback, instance locking and Windows job child termination.
- The retention test rotates 1,800 filesystem segments with one hour of nominal timestamps and overlapping snapshots. It is **not** a real-time capture soak.
- The updated native UI and capture paths have not been visually/runtime verified. Compilation does not establish hardware behavior.

## Current blocker

Launching the bundled FFmpeg returned: “Operation did not complete successfully because the file contains a virus or potentially unwanted software.” No protection was disabled and no alternate copy was executed to evade the block. Fresh video/NVENC capture, encoded multitrack media self-tests, FFmpeg decode verification and installed recording tests remain blocked pending resolution of the runtime's security/provenance issue. A Defender history query did not provide the detection classification. WMI hardware queries returned Access denied.

## Earlier alpha evidence — not current-version retests

| Test | Observed result | Limit |
| --- | --- | --- |
| Real desktop + NVENC + audible WASAPI output | `capture-test-20261106-144915`: 8.136s MKV, 768 x 480 H.264, 2 tracks, 992 packets, nonzero peak, overlapping saves verified | Short desktop session with CPU scaling; no gameplay |
| Microphone attempt | `capture-test-20261106-145242`: 819 packets, 2 tracks, peak zero | Intelligible speech **not verified** |
| Synthetic multitrack | `self-test-20261106-145146/media-verification.json`: 3 tracks, 90.29/96.68 dB isolation, 9 pulse/flash events within 10ms | Does not verify real-device synchronization or editor playback |
| Native alpha UI | Launched and enabled replay before user stopped UI testing | Current changes not retested |
| GPU scaling | D3D11 texture error `80070057`; CPU scaling/NVENC fallback worked | GPU scaling success unverified |
| User's recording report | User says a recorded video works | Resolution, tracks and stretched behavior unspecified |

## Required physical matrix

The user confirmed **Valorant fullscreen**; NVIDIA scaling location remains unknown. Inspect decoded saved files with recognizable game/HUD edges and audio/visual cues.

| Scenario | Acceptance evidence | Status |
| --- | --- | --- |
| Fullscreen 1280 x 960 and 1440 x 1080 via Windows Display Settings | 1920 x 1080, square pixels, full stretched game, no adjacent desktop, correct edges | Pending |
| Both modes via NVIDIA Full-screen scaling on GPU | Same visible appearance; record actual GPU/CPU scaling path | Pending |
| Both modes via NVIDIA Full-screen scaling on display | Same checks; record driver-reported source dimensions | Pending |
| Native → 4:3 → native while recording/replaying | Recovering state, same-display bounded restart, fresh replay and separate verified recording parts | Pending |
| Alt-Tab, minimize, monitor reconnect, sleep/resume | No false healthy state or unintended display; preserved usable footage | Pending |
| Audible system output + spoken microphone | Audible mix and isolated sources; start/middle/end sync | Pending |
| Independent game + Discord + microphone | Separate tracks without system duplication; process restart handled visibly | Pending |
| Default-device change, selected device unplug/replug, permission denial | Explicit error/stop where needed; successful refresh/reselection/restart | Pending |
| MKV in installed video editor | Record editor/version; all tracks selectable, isolated and synchronized; MP4 remux separately checked | Pending |
| 30-minute replay with overlapping saves | Stable memory/disk budget and valid saves; continuous progress | Pending |
| Bounded test-volume exhaustion, encoder termination, app crash | Timely error, no hidden encoder, retained segments recover/decode | Pending |
| Clean-profile installed app, upgrade and uninstall | Correct resources/paths, launch and real replay/audio save | Pending |

Use a disposable bounded volume for full-disk tests, not the system drive. Unit policy tests do not prove physical filesystem failure handling.

## Installer evidence

Verified present: `src-tauri\target\release\bundle\nsis\REPLAYX_0.1.0_x64-setup.exe`, **63,247,309 bytes**, written November 6 at 14:55 local time.

SHA-256: `792a1e469dcce7b520462dd210e8285aeeb07e765683083da16d2c07cfe87e49`.

This is the **previous 0.1.0 installer**, not the current changes. Installation, installed launch/recording and uninstall/upgrade are unverified. No new installer was generated while the media runtime was blocked. Bundle preparation now checks both media executables before copying and includes privacy/license/validation documents. Corresponding-source/license review and signing remain distribution gates; see THIRD_PARTY_NOTICES.md.

## Hardware enumeration (current code)

Native DXGI enumeration succeeded without launching FFmpeg: DISPLAY1 reported an active and desktop mode of **1440 x 1080 at 250 Hz**, origin 0,0, unrotated. WASAPI enumeration found BlackShark V3 X, Voicemod and Realtek outputs, plus Voicemod and BlackShark V3 X microphones. These confirm enumeration/geometry reads, not successful capture. JSON evidence is in `test-output/validation-0.1.1-alpha.1/displays.json` and `devices.json`.

## Final source-check result

`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check.ps1` completed successfully after the final cleanup regression: 30 tests passed in 1.50s, zero failed/ignored, rustfmt passed, TypeScript/Vite passed, Clippy with warnings as errors passed. npm ci audited 74 installed packages and reported zero known vulnerabilities. A prior concurrent npm reinstall hit a loaded Tauri CLI file lock; after the release build exited, the complete reinstall/check succeeded. No elevated install or security bypass was used.

## Current real WASAPI audio checks

The existing WASAPI producer was exercised directly, without launching FFmpeg, using the new `audio-test` CLI diagnostic and independently checked PCM16 WAV headers/duration.

- BlackShark V3 X system output: **10 seconds**, 1,003 real packets, zero queue overruns, peak 0.98675 and RMS 0.19033. Strong nonzero real output captured. Evidence: `test-output/audio-test-system-20261106-235750-24267021/report.json` and `capture.wav`.
- BlackShark V3 X microphone: **10 seconds**, 1,004 real packets, zero queue overruns, peak 0.000915 and RMS 0.0000442. Device capture succeeded, but this is near-silent and does not establish intelligible speech. Evidence: `test-output/audio-test-microphone-20261106-235822-6d3cd6bd/report.json` and `capture.wav`. The initial sandboxed microphone call returned Access denied; the scoped diagnostic outside the sandbox succeeded. Windows security protections were unchanged.
- These were separately captured WAVs, not a multitrack MKV. Physical A/V sync, game/Discord isolation, editor playback and device unplug/recovery remain unverified.

## Updated application executable

The optimized 0.1.1-alpha.1 application built successfully with `npm run tauri -- build --no-bundle`. Path: `src-tauri/target/release/replayx.exe`; size: 13972992 bytes; SHA-256: `2287394decb5ec33d39300016150a9467fd3a664a61ff9e9295a7d80c726e144`. Native launch and video recording of this new build are unverified. The NSIS installer remains the older artifact described above. No recording process was started by the release build.
# Local 0.3.1-alpha.7 validation - November 11, 2026

Production-only follow-up found and fixed missing TLS-provider initialization
before the Releases API request. The corrected native release completed a real
GitHub check with no newer signed channel release; Rust 160/160 and Clippy passed
again. Artifact hashes and final build details are in release-notes/LOCAL_BUILD_0.3.1-alpha.7.md.

Final additional checks: 132/132 responsive text cases (including update popup,
130% text), 24/24 scrolling cases, and 9/9 media checks on each of the short
fixture and end-of-soak clip. Short-fixture isolated audio separation measured
101.0/89.3 dB; max generated A/V offset 10 ms. End-of-soak max offset 30 ms.
15-minute soak, second encoder start/stop, two concurrent saves and full decode
passed; see PERFORMANCE.md and `test-output/nov11-soak/summary.json`.

This continues the recovered work checkpoint `71e03ca` on
`codex/performance-0.2.1`. No push, merge, tag or publication was performed.

- 160 Rust tests passed with desktop features; scratch registry test requires
  execution outside the restricted sandbox. Rustfmt and Clippy with warnings
  denied passed. TypeScript/Vite build and 50 frontend tests passed.
- Native debug app updater integration: 42/42 checks passed against isolated
  localhost fixtures, including signatures, wrong key/version, tampering,
  interrupted transfers, HTTP errors, timeout, offline mode, progress,
  dismissal persistence and mic synchronization. Evidence:
  `test-output/update-e2e-Uyx6Tp/results.json`. Installation was refused by
  design. Dummy signed payloads were never executed.
- Four motion-rich desktop hardware recordings completed. See PERFORMANCE.md
  for decoded unique motion, frame pacing, proxy FPS and methodological limits.
  An initial sandbox attempt returned access denied; the authorized desktop
  captures outside the sandbox succeeded. No protection setting was changed.
- The inherited alpha.6 installer failed independent signature verification:
  its unsigned rebuild had replaced the signed artifact. Alpha.7 is generated
  separately; manifest generation now verifies signature and signed version.
- Native installation/restart preservation is NOT verified. Testing NSIS with
  another directory on this host would still share installer registration with
  the working installation. Use a disposable Windows VM for that acceptance.
- Physical gameplay FPS, fullscreen stretched transitions, real microphone,
  Discord sharing, editor multitrack playback and real A/V sync remain manual
  acceptance items in MANUAL_ACCEPTANCE.md. No production-readiness claim.

Historical entries below retain their original scope and dates.
# Local alpha.8 performance milestone - November 11, 2026

The Fortnite FPS gap remains unresolved; no under-5% gameplay qualification or
ShadowPlay-equivalence claim. See engineering/PERFORMANCE_ALPHA8.md for the complete audit,
experiments and limits, and engineering/FORTNITE_ABA.md for manual acceptance.

- 161 Rust tests passed; the opt-in release polling benchmark was executed
  separately. Rustfmt/Clippy passed, as did 51 frontend tests, TypeScript/Vite,
  four read-only CSV-analyzer tests and 42 native isolated updater/mic checks.
- Six hardware codec/region cases passed with actual NVENC H.264/HEVC/AV1;
  12 clips fully decoded. 4:3 region SAR/DAR verified. This did not change
  Windows/NVIDIA display modes or validate physical fullscreen gameplay.
- 15-minute synthetic replay soak passed with two other NVENC workloads,
  concurrent saves (805/805 ms), full decoding and no remaining buffer MKVs.
  Memory averages 458.62 -> 461.26 MB; peak 528.69 MB; fixture CPU 6.21%.
- End-of-soak media checks: three tracks, 87.38/106.13 dB synthetic isolation,
  max generated A/V offset 30 ms, monotonic packet timestamps.
- NV12 D3D11 scaling and CUDA derivation probes failed on this hardware;
  direct D3D11/NVENC passed. Those unavailable paths are limitations, not
  passing feature tests. The media runtime was not modified.
- Production quality, lookahead, audio processing, game detection and default
  GPU priority remain unchanged. Lower-priority/AQ-off/lookahead-off defaults
  were rejected based on measurements or motion regressions.
