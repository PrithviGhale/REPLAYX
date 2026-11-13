# Known limitations — 0.3.1-alpha.8

REPLAYX is **alpha** software. New Fortnite screenshot averages suggest roughly 13% overhead; the earlier ~45% report and the new samples are not controlled benchmarks. The real-game overhead remains unresolved. Current synthetic measurements do not establish real-game improvement or Discord compatibility. Physical microphone, editor multitrack playback and fullscreen stretched transitions require manual testing. Evidence is in [VALIDATION.md](../development/VALIDATION.md) and [the alpha.8 investigation](../engineering/PERFORMANCE_ALPHA8.md).

- Detailed GPU-fixture A/B/A runs generally measured about 5-8% overhead with current quality settings. This does not qualify Fortnite or establish the under-5% goal.
- The bundled D3D11 NV12 scaler fails on this GPU; direct BGRA-to-NVENC works. Exact resizing can therefore fall back to CPU processing. Smooth mode may use native pixels plus aspect metadata instead of encoding exact 1920x1080 pixels.
- Live per-frame capture/conversion/encoder latency and unique source FPS remain unavailable. Driver encoder statistics and fixture GPU draw timings are different measurements.

## Updates and the microphone switch (0.3.1-alpha.7)

- **In-app updates start with the first signed release.** v0.3.1-alpha.6 and older contain no update channel, so they must be updated once by hand (run the new installer). From the first release that carries `latest.json`, installed copies find later versions themselves.
- **Not yet tested end to end with a real installation:** checking, downloading, signature verification, rejections and the interface are tested against a local test server with the real signing key, but replacing an installed copy through the installer and restarting has not been done yet; do this in a disposable Windows VM with two signed versions.
- **No automatic rollback.** If an installation fails part-way, run the installer from the Releases page again; settings and clips are not touched by the installer.
- Update discovery considers the 100 most recent releases on this repository. GitHub failures now report a failed check and retry with backoff. Actual installer replacement/restart remains unverified; no isolated Windows VM was available.
- Updates are signed with REPLAYX's update key, but the installer is still not code-signed for Windows (Authenticode), so SmartScreen may still warn on a manual download.
- **Microphone switch:** Off mutes the microphone in recordings at once (also mid-recording, with a 10 ms fade so there is no click) and keeps its track, so audio tracks stay aligned. While capture is running REPLAYX keeps the microphone device open so it can be switched back on instantly; nothing from it is written while it is off. To keep the device closed entirely, turn off *Record microphone* on the Audio page. A microphone removed from the capture there can only be switched back on while capture is stopped.

## Audio devices (0.3.1-alpha.6)

- A device that changes while capture starts is retried for about four seconds; one that changes during capture is reconnected through REPLAYX's bounded recovery (up to three restarts). A specifically selected microphone that stays unplugged ends capture with a message after those attempts; choose another device in **Audio**. Each reconnection starts a new replay buffer and splits a recording into verified parts.

## New in 0.3.1-alpha.1

- **Playtime** is *foreground* time: REPLAYX can't tell whether you are actively playing, only which window is in front. Controller input doesn't count as activity for the idle limit (Windows reports keyboard and mouse only); set the limit to *Never* if you play with a controller. Time while REPLAYX isn't running is never counted.
- **Game detection** recognises 140 games by executable (plus unknown games in Steam, Epic, Xbox, Riot, EA, Ubisoft and GOG folders by folder name). Games launched from elsewhere with an unlisted executable count as desktop time; add a correction in Settings.
- **Game artwork** is the game's own icon: from the executable REPLAYX saw in front, or found among installed games (Windows' installed-apps list, which covers Steam, Roblox and most installers, and Epic Games manifests), or your own image. Microsoft Store/Xbox games and games installed by copying a folder show a drawn tile until REPLAYX sees them running. Cover art from online services is not used.
- Alt+F10 can only be used when no other program owns it; REPLAYX never takes a shortcut away from another app.

## New in 0.3.0-alpha.1

- **120 fps is not smooth yet in our benchmark.** With REPLAYX's GPU-heavy test window (about 900 FPS, full screen, 250 Hz display), 120 fps recordings repeated 43-45% of frames, the same with and without audio and unchanged by longer encoder look-ahead (45%) or oversampled capture (38%); 60 fps under the same load repeated 0.56%. Not yet tested with a real game at 120 fps. Use 60 fps until this is resolved.
- **Voice Enhancer** quality has been measured with synthetic speech and noise only. Whether it makes a real headset sound better is a listening test still to be done, and it has not been compared with Voicemod on the same recording.
- Noise suppression removes steady noise (fans, hum, hiss, room noise) well; short sounds such as keyboard clicks and mouse clicks are only partly reduced.
- If you record a virtual microphone (for example Voicemod's), REPLAYX processes audio that is already processed. Choose your real microphone, or the **Off** preset.
- The microphone track is about 11 ms later than with processing off (RNNoise frame plus limiter look-ahead); far below what is noticeable as lip-sync error.
- Voice settings (other than volume and mute) can only be changed with capture stopped; Test mic applies them live.
- The splash can only appear once Windows has created the app's WebView: about 0.35 s after launch normally, 1.7–2.4 s on the first launch after a restart or update.

## New in 0.2.2-alpha.1

- **Codecs.** AV1 needs a GPU with an AV1 encoder (NVIDIA RTX 40 and newer, Intel Arc/Core Ultra, AMD RX 7000). On hybrid laptops the codec must be encodable by the GPU that drives the display at that moment: with the desktop on an Intel GPU without AV1, an AV1 setting records H.264 (the app shows a warning). AV1 and HEVC files do not play in some older players, phones, TVs and editors; HEVC needs the HEVC Video Extension in some Windows apps.
- **Constant quality** is used on NVENC only; Quick Sync, AMD and software encoders use VBR at the preset rate. File sizes in constant quality depend on the scene: the app's estimate is typical gameplay plus the cap for the worst case.
- **HEVC** was not smaller than H.264 at equal quality on the RTX 4070 with REPLAYX's settings; it is offered for compatibility, not as a size saving.
- **Overlay.** Not shown over exclusive full-screen games (Windows does not show desktop windows there). Exclusion from capture needs Windows 10 version 2004 or later; on older Windows the overlay is not shown at all. It is positioned on the monitor of the window in front; with several monitors it follows that window.
- **Microphone processing** is light on purpose (no AI models): noise suppression reduces steady noise such as fans, not keyboard clicks or voices in the room. Changing boost or processing needs capture stopped; volume and mute work live.
- **Tray.** Quit asks first while a recording or a save runs; Windows shutdown or a forced end of the process skips that (finished segments are recoverable as before).

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
- (Superseded: in-app updates are implemented; see *Updates and the microphone switch* above.)
- Before a public release, the FFmpeg corresponding source and the notices of its bundled libraries must be packaged with the release (see THIRD_PARTY_NOTICES.md). H.264/AAC patent questions are not resolved by the software license.
