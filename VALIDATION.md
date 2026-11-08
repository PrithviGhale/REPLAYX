# Validation — 0.2.1-alpha.2 (November 7, 2026)

Same laptop (Iris Xe drives the 2560×1600 @ 240 Hz panel; RTX 4070 Laptop). Roblox, Steam and the Epic launcher were open in the background; capture tests ran after the user approved about 12 minutes of silent capture (no microphone, nothing played; one 15 s run with system-audio loopback was approved separately). All runs used throw-away data folders under `test-output/`; the user's settings, clips and sessions were not touched.

## The Record freeze

| Evidence | Result |
| --- | --- |
| Windows error reports for the user's freezes | `AppHangB1`, `replayx.exe` 0.2.1.0 at `src-tauri\target\release\replayx.exe` (the 0.2.1-alpha.1 build, not an installed copy), 18:20:53 and 18:21:40. A 0.2.0.0 hang is also recorded at 16:13:37. |
| User's data folder after the freezes | Each frozen session: a buffer folder full of 2 s segments (87 and 17) and an **empty** recording folder. Capture ran, but the engine's tick never reached the step that adds segments to the recording. |
| In-process reproduction (`games::tests::sampling_own_window_never_waits_for_its_blocked_owner_thread`) | `GetWindowTextW` on a window owned by the calling process blocks while the window's thread waits on a mutex, and completes once it is released. The new `window_process` returns immediately for REPLAYX's own windows. |
| Real window, 0.2.1-alpha.1 (`scripts/ui-record-check.mjs`, Record pressed in the page) | Froze in 2 of 2 valid runs: window stopped answering 1.4 s after Record (`IsHungAppWindow` true; force-closed after 6 s in one run, after more than 3 min in the other), status stuck on *Stopped*, recording folder empty. A third run lost the foreground at the click (another window was in front), so the deadlock could not occur; it stayed responsive. |
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
| Delete in the real Clips page (`scripts/ui-delete-check.mjs`, generated clips) | 10/10: the clicked clip is the one deleted, the dialog names it, it lands in the Recycle Bin from the test folder, the list updates, the other clips are untouched; a clip held open by another program is refused with Windows' reason and kept. |

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
- Isolated encoder-process CPU, 15 s at 60 fps, 2560×1600 → 1920×1080 (`scripts/bench-scaling.ps1`): CPU scale + NVENC **1.94 cores** (9.7% of the machine); Intel VPP + NVENC **1.07** (−45%); Intel VPP + Quick Sync zero-copy **0.56** (−71%); D3D11 + NVENC fails.
- Full pipeline, 30 s, 60 fps, system audio, two overlapping saves (`scripts/measure-capture.ps1`, whole-run CPU including probes and the save-verification decodes): CPU path 2.05 cores, 57 dropped / 2,665; hybrid 1.76, 36 / 2,788; zero-copy 1.17–1.25, 37–46 dropped per ~2,500 frames.
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

Evidence (analysis scripts: `scripts/audio-tone.py`, `scripts/audio-clicks.py`; a single inserted/dropped frame shifts tone phase by 7.5°/11.25°):

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
