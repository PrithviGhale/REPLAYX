# Changelog

## Unreleased

Not published. Built and tested locally; see docs/VALIDATION.md.

- **In-app updates from GitHub Releases.** REPLAYX checks for a newer version shortly after it starts, then every 6 hours, and when you press *Check Now*. A violet dot on **Settings** and a badge on the **Updates** tab show when one is available, and a pop-up offers *Update Now* / *Not Now* with the release's highlights (dismissed versions are not offered again for three days; in the tray a small notice appears once instead, never over a game). Downloads show real progress in bytes; the package is verified against REPLAYX's publisher key before it can install. Modes: **Manual**, **Notify me** (default) and **Auto update**, with switches for checking, the schedule, notifications, automatic download and automatic install (which waits for an idle PC, no game in front, and nothing recording or saving). An install never interrupts a recording, a save, an export or a recovery; Instant Replay alone is paused, and switched back on after the restart. Settings, clips and playtime are kept.
- **Signed releases.** A dedicated update signing key; the release workflow signs the installer, verifies the signature and its version against the built-in public key, and publishes `latest.json`. Releases without a valid signature cannot be drafted. Older or unsigned packages, packages signed by another key, and packages offered under a different version than they were signed for are refused by the app.
- **Microphone switch on the home page.** The Audio card's Microphone row has an On/Off switch: Off mutes the microphone in recordings immediately, also in the middle of a recording, without restarting capture, with a short fade instead of a click; other sources and the microphone's device, volume and Voice Enhancer settings are untouched. The tray menu's microphone line (now clickable), the mute shortcut, the overlay icon and the Audio page all show and change the same state.
- Development builds can preview every update screen with clearly labelled test data and never install anything; `dev/update-e2e.mjs` tests the updater against a local test server.

## 0.3.1-alpha.6 — Audio device changes, clips folder

User-facing notes: [docs/release-notes/v0.3.1-alpha.6.md](docs/release-notes/v0.3.1-alpha.6.md).

- **Fixed: "Audio initialization failed … 0x88890004".** Windows returns this code (*device invalidated*) when an audio device is switching just as REPLAYX opens it: a headset connecting, a Bluetooth profile change, waking from sleep. REPLAYX now waits and retries up to five times over about four seconds, each time with the then-current default device. If a device changes **during** capture, REPLAYX reconnects through its bounded recovery instead of stopping (footage so far is kept; a recording continues as a new verified part). The error text is now plain language instead of `Ok(Err(…))`.
- **Fixed: the Clips folder button opened Documents.** The default clips folder was stored as `…\Videos/REPLAYX`; Explorer reads `/REPLAYX` as a command-line switch and fell back to Documents. Paths are now stored with Windows separators, existing settings are repaired automatically, and the button opens exactly the clips folder.
- A voice-processing speed test no longer fails when the PC is busy (it now keeps the best of three timings).
- Playtime tracking (on by default, off in **Settings → Playtime tracking**) re-measured: about 21–25 µs per once-a-second check (≈0.002% of one CPU core) and one small file write at most once a minute.

## 0.3.1-alpha.5 — Reliable Start with Windows, release pipeline, new project page

User-facing notes: [docs/release-notes/v0.3.1-alpha.5.md](docs/release-notes/v0.3.1-alpha.5.md). The recording engine is unchanged from 0.3.1-alpha.4.

- **Fixed: a blank Windows Terminal window at sign-in for developers.** *Start with Windows* registered whichever REPLAYX executable was running, so running a development (debug) build pointed the startup entry at it; debug builds are console programs, so Windows opened a terminal for it at every sign-in. Now only the installed app (a release build with its uninstaller beside it) registers itself, it re-points a stale entry at itself, it removes the entry when the setting is off, and development builds never register and clean up an entry left by an older development run. The installed app was not affected (it is a windowed program).
- **Start with Windows** now always matches the setting: turning it off removes the startup entry right away, and the installed app repairs an entry that points somewhere else.
- **Project page and documentation:** a short new README with a direct download; detailed guides moved to `docs/` (Getting started, Features, FAQ, Troubleshooting, Privacy); branding files in `assets/branding/`; `CODE_OF_CONDUCT.md` and `SECURITY.md` in `.github/`.
- **GitHub Actions work from a clean checkout:** the source checks prepare the FFmpeg bundle folder themselves, the pinned FFmpeg download is retried and cached, and the release workflow builds test builds without creating a release, never rebuilds an existing release, and attaches the FFmpeg corresponding-source package to every draft.

## 0.3.1-alpha.4 — Game icons, open/hide shortcut, text that always fits

Published on November 9, 2026 as the first public release (pre-release).

- **Game icons for installed games.** Games now show their own icon even before REPLAYX has seen them running: it looks them up in Windows' list of installed apps (Steam, Roblox and most installers register there) and in the Epic Games Launcher's manifests, and uses the game's own executable icon. Read-only and local; nothing is downloaded. On the test PC this found 9 games (Fortnite, Counter-Strike 2, VALORANT, Apex Legends, PUBG, Genshin Impact, F1 25, Roblox, Roblox Studio) in about 0.02 s.
- **New shortcut: Open / hide REPLAYX** (Ctrl+Shift+F11 by default, changeable in Settings → Hotkeys). Opens REPLAYX from anywhere, from the tray or over a game; press it again to hide it to the tray. If one of your shortcuts already uses Ctrl+Shift+F11, a free one is chosen instead.
- **Closing and the shortcut both hide REPLAYX to the tray** (Windows' hidden icons, the ^ next to the clock); it keeps running there with Instant Replay, shortcuts and playtime.
- **Fixed: text cut off in a smaller window.** With the window restored from fullscreen, the Replay length cards were squeezed side by side and their text ("Balanced", *Recommended*, the size and description) ran out of the card. The cards now wrap to fewer per row when space is short and the badge moves to its own line. Performance page values with long labels no longer spill out either. A new check (`dev/text-fit-check.mjs`) tests every page and Settings tab at six window sizes, also with 130% text, for text cut off or spilling out of its card; it passes.

## 0.3.1-alpha.3 — New logo, playtime on by default, lighter on games

Not published.

- **New logo** everywhere: the app and installer icons, taskbar, tray, Alt-Tab, the startup screen, the sidebar, About and the README banner.
- **Playtime tracking is on by default.** No sign-up step: the Games page counts your playtime from the start (existing settings are switched on once). It stays local; turn it off, pause it, exclude games, export or clear it in Settings.
- **Fixed: the hidden window kept working during games.** Hiding REPLAYX to the tray (or minimising it) hid the window but not its page, which kept refreshing up to 4 times a second and redrawing while you played. The page is now told it's hidden, so it slows to a status check every 2 seconds and draws nothing until you open it.
- **Saves and thumbnails no longer compete with your game for CPU.** Everything that runs after capture (checking footage, joining it into a clip, playback checks, thumbnails, MP4 export) now runs below normal priority, so Windows always serves the game first. Capture and encoding keep their priority, so recordings stay smooth.
- The Games page no longer re-reads your playtime history every 10 seconds while REPLAYX is hidden.
- Measured: the background check that powers playtime and game detection takes about 17 µs per second (≈0.002% of one CPU core).

## 0.3.1-alpha.2 — Pop-ups for recording started and clips saved

Not published.

- **New: capture pop-ups.** A short bar slides in at the top-right for about 3 seconds when a recording starts (*Recording started* · *Ctrl+Shift+F9 stops and saves it*, shown once video is actually being written, not when the button is pressed), when an Instant Replay is saved (*Instant Replay saved* · game · length · in Clips) and when a recording is saved (*Recording saved*). A failed save says *Save failed* instead. Red for recording, green for saved. Silent, click-through, never takes focus, and excluded from your recordings like the overlay. It appears for saves from hotkeys, the tray and the app alike. Turn it off in Settings → Status overlay → *Pop-up when recording starts or a clip is saved*.
- The game-detected notice, the tray hint and the new pop-ups now share one notice window instead of starting a thread each.

## 0.3.1-alpha.1 — Games and playtime, clip library, correct thumbnails, tray, Alt+F10

Not published. User-facing notes: [docs/release-notes/v0.3.1-alpha.1.md](docs/release-notes/v0.3.1-alpha.1.md).

- **Fixed: a clip showed another clip's thumbnail.** The thumbnail component kept the first image it loaded and skipped loading once it had one, so the details panel (one component reused for every selected clip) kept the previous clip's picture; a slow request could also overwrite a newer one. Each image is now tied to its clip (id, size, modification time), a different clip shows a placeholder until its own image arrives, and late answers for an old clip are ignored. The thumbnail cache no longer truncates long clip ids (two clips could share a cached image) and is invalidated when a clip is renamed, moved or replaced.
- **Better thumbnails:** the most representative of 30 frames about a third into the clip (not a black first frame), square pixels (Smooth-mode recordings are no longer squashed), 640 px wide.
- **Clips library:** *All clips* and *By game*. Cards show the thumbnail, game icon and name, date, duration, resolution, FPS, size and type. Sort by newest, oldest, game A–Z or Z–A, length, size or type; filter by game, type and date range; search by game or file name; your choices are remembered. Game tiles exist only for games you have clips of and update when clips are saved, deleted or moved.
- **Games and playtime (new, opt-in):** a Games page that tracks how long each game is the window in front while REPLAYX runs. Time in another window, sleep and idle stretches (no keyboard or mouse input, 15 minutes by default, adjustable or off) are not counted. Periods: today, yesterday, last 7 days, this week, last 30 days, this month, all time or custom dates; daily chart, most played, and a page per game (total, today, yesterday, week, month, last played, clips, footage, recent sessions). Stored only on this PC; pause, exclude games, export CSV or clear (to the Recycle Bin) in Settings. Nothing from before you turn it on is counted.
- **Game detection:** a structured catalog of 140 games (executable aliases, folder requirements for generic names such as THE FINALS' `Discovery.exe`) compiled into the app, plus store-folder fallback for unknown games. Minecraft Java now files as *Minecraft Java Edition*.
- **Game artwork without downloads:** the icon of the game's own executable (high resolution), or an image you choose; otherwise a drawn tile.
- **Fixed: no tray icon after closing.** *Minimize to tray when closing* was off by default, so closing quit REPLAYX. It is now on by default (existing settings are switched once); clicking the tray icon opens REPLAYX, right-click shows the menu (Open REPLAYX, Instant Replay, Save replay, Start/Stop recording, microphone, Open Clips, Settings, Quit). The first time, a notice says *REPLAYX is still running in the background. Open it from the system tray.*
- **Alt+F10 (and any shortcut another program owns):** pick it from *Common…* even when the key press is swallowed (NVIDIA's Instant Replay uses Alt+F10). REPLAYX checks at once whether Windows will give it the shortcut; if another app owns it, it says *Alt+F10 is already used by another application. Change that application's hotkey or choose another shortcut.* and keeps your previous working shortcut. *Try again* re-registers after you free it.
- **Overlay:** about 19% smaller, a little more transparent; new options for the recording indicator and a recording timer.
- **Presets:** Instant Replay *Short clips*, *Balanced* (recommended) and *Long sessions* with real disk estimates; *Recommended* marks the video preset suited to your hardware and the *Clear* voice preset.
- **Startup:** smoother hand-off from the startup screen to the dashboard; branding is centralised so the approved logo (pending) can be applied in one pass (docs/BRANDING.md).

## 0.3.0-alpha.1 — Voice Enhancer, startup screen, first-run tour, release preparation

First public preview candidate (not yet published). User-facing notes: [docs/release-notes/v0.3.0-alpha.1.md](docs/release-notes/v0.3.0-alpha.1.md).

- **Fixed: noise suppression did almost nothing.** The FFmpeg `afftdn` filter as configured (noise floor −50 dB) removed only 2.2 dB of steady fan noise at headset levels, and the live *Test mic* played the raw microphone, so switching noise suppression on or off changed nothing you could hear. Replaced by REPLAYX's own processing (below), which the recording, Test mic and Compare all use.
- **Voice Enhancer.** Native real-time processing on each 10 ms microphone chunk in the capture thread (never the video path): 80 Hz high-pass → RNNoise noise suppression (via nnnoiseless, BSD-3) → expander held open by RNNoise's voice detection → EQ → compressor → de-esser → recording gain → look-ahead limiter. Presets *Off*, *Natural*, *Clear* (default), *Broadcast*, *Noise-Free* set real parameters; *Custom* and saved presets. Measured with text-to-speech and synthetic noise (no microphone used): Light/Balanced/Strong remove 9.5/17/25 dB of fan noise; speech level changes ≤0.3 dB; 0.5–1% of one CPU core; 11 ms added latency on the microphone track. Existing microphones move to the *Clear* preset.
- **Test mic** now hears the exact recording chain, applies changes live, and switches between *Enhanced* and *Original* at the same loudness. **Compare** records one take (explicit click, memory only) and plays original vs enhanced of the same words; changing settings re-processes the same take.
- **Fine-tune:** recording gain (REPLAYX only; the Windows hardware level is never changed, with a button to Windows sound settings), rumble filter, soft gate with threshold, compressor threshold/ratio, warmth / boxiness / presence / air EQ, de-esser, saved presets.
- **Fixed:** in-app audio playback was blocked by the window's security policy (`media-src` did not allow `data:`/`blob:`); comparisons now use `blob:` URLs, which are allowed.
- **Startup screen.** Painted from the first HTML byte with the REPLAYX mark, a subtle violet glow and progress line; shows only steps that are really running (interface, settings, engine check); disappears as soon as the app is ready (no minimum duration); respects reduced motion; cannot trap the window. Measured: no startup delay added (interface mounted 455 ms before vs 427 ms after, warm; app ready at 483 ms).
- **First-run tour.** Twelve short steps spotlighting the real components (Record, Instant Replay, Save replay, video, microphone, Voice Enhancer, overlay, clips, game detection, tray, settings). Dims the rest, follows scrolling and resizing, Back/Next/Skip, Esc and arrow keys, "Step n of 12"; never presses anything; remembered once finished or skipped; *Settings → Replay tutorial*.
- README rewritten with current screenshots; release notes, third-party notices (RNNoise/nnnoiseless) and release workflow prepared locally.
- Developer tools: `replayx-cli voice-eval`, `scripts/voice-eval.ps1`, `dev/startup-bench.mjs`.

## 0.2.2-alpha.2 — redesigned Audio page, icon overlay

- Audio page redesigned: microphone first, one-line sound-source strips, tracks and quality last.
- Microphone always present (no "add" step); Discord-style live **Test mic**.
- Status overlay redrawn as GeForce-style icon tiles (Instant Replay, microphone, recording dot), stacked in a corner, shown over every app while enabled, still excluded from capture.
- Audio devices plugged in or removed appear within about 2 s.

## 0.2.2-alpha.1 — quality per GB, codec choice, microphone setup, in-game overlay, tray

Not yet verified by a person in a real game: the microphone listening test, gameplay with the new presets and codecs, and the overlay over a full-screen game are still to do (see docs/VALIDATION.md). Everything below that says "measured" was measured on the development laptop (RTX 4070 Laptop GPU, Intel Quick Sync, Advanced Optimus).

- **Smaller files at the same quality.** NVENC presets now use constant quality capped at 1.5× the preset rate instead of a fixed average. Measured with VMAF on 20 s of real Roblox gameplay: Balanced H.264 CQ 28 averaged 14.7 Mb/s at VMAF 97.6, against 16.7 Mb/s and 97.9 for the previous 16 Mb/s VBR (12% smaller, same picture); in a very detailed synthetic scene the cap holds it at 24 Mb/s. Quick Sync, AMD and software encoders keep VBR at the preset rate. Game-FPS cost under a GPU-heavy test load: 2.9% (H.264), 0.0% repeated frames.
- **Codec choice: Automatic (H.264), H.264, HEVC, AV1.** Only codecs this PC can encode are offered (each is probed). Automatic always records H.264 because it plays everywhere; REPLAYX never switches codec by itself. If the GPU driving the display cannot encode the chosen codec (for example AV1 on a display run by an Intel GPU without AV1), capture uses H.264 and the app says so. Measured: AV1 CQ 34 reached VMAF 97.7 at 12.5 Mb/s (about 25% smaller than H.264 at equal quality), 3.0% game-FPS cost, 0.14% repeated frames. HEVC was within ±3% of H.264's size at equal quality on this GPU, so it gets no size discount; it is offered for devices that prefer it.
- **Fixed: colours were tagged with the wrong matrix on NVIDIA.** NVENC converts the captured RGB with the BT.601 matrix regardless of the colour tags (measured for H.264, HEVC and AV1 from system-memory, CUDA and Direct3D 11 frames). 0.2.1-alpha.3 tagged these files BT.709, so players that follow the tags showed shifted colours (pure green 192 decoded as 159). Files are now tagged BT.601 matrix with BT.709 primaries, limited range; Quick Sync files were already correct (converted to BT.709 in the video processor). A real-encode test guards this.
- **Storage estimate** on the Video page: per minute, per hour, a 10-minute replay, free space on the recording drive and recording time left, each as typical gameplay and the worst case at the cap.
- **Fixed: wrong sizes in the app.** The preset line used outdated rates (16/28/45 Mb/s instead of 10/16/25) and the Instant Replay disk budget used the Custom bitrate for every preset, under-estimating Balanced buffers by a quarter.
- **Microphone setup.** A dedicated Microphone card: device (with a *Disconnected* state when the chosen microphone is missing), on/off, its own track number, live volume and mute, a 0–24 dB boost that is always followed by a limiter (a test boosts a near-full-scale tone by 24 dB through the real FFmpeg chain and requires the result to stay between −12 and −0.3 dBFS), a warning when peaks reach the limiter, Reset to defaults, and optional light processing: rumble filter (on by default), gentle noise suppression, a soft expander between words and a compressor. The processing costs about 0.4% of one CPU core and is applied to the recording only; Windows' microphone level is never changed.
- **Test microphone.** Records only after you press *Start test*, can be stopped early, shows a live level meter, plays the processed result inside the app (the same processing the recording uses) and deletes the test recording when the window closes and at start-up.
- **In-game status overlay.** A small translucent pill in a corner of the display the game is on: Instant Replay (off, starting, on, saving, error), a red recording dot with timer that appears only once video is actually being written, and the microphone (off, muted, ready, live, error). Click-through, never takes focus, no injection or hooks, and excluded from capture (Windows display affinity), so it never appears in clips; the "game detected" notice is now excluded too. Settings: on/off, corner, opacity, compact, which items, games only or always. Like every desktop window it shows over windowed and borderless games, not exclusive full-screen ones.
- **Tray.** The menu shows the real state: Instant Replay on/off (check mark), Save replay (only while the buffer runs), Start/Stop recording with its time, the microphone state, Open clips folder, Settings, Quit. Quit (and closing the window when *Minimize to tray when closing* is off) asks first while a recording or a save is in progress; quitting stops capture, finalizes the recording and ends the encoder. The first time the window closes to the tray, a one-time notice says REPLAYX is still running.
- **Fixed: the app's content area did not scroll** with the mouse wheel or touchpad on any page taller than the window (21 of 21 page/size combinations; now 21 of 21 scroll, checked by `dev/scroll-check.mjs`).
- Developer tools: `scripts/quality-bench.py` (VMAF/SSIM/PSNR per encoder configuration on real or synthetic footage), `replayx-cli overlay-test` (proves overlay exclusion on Desktop Duplication frames), codec/preset configs in `scripts/overhead-bench.py`. The UI test scripts now use an anonymous headless Chrome: a new Edge profile signed in with the Windows account and synced the user's extensions into the test browser.

## 0.2.1-alpha.3 — lighter capture on gaming laptops, real 1920×1080, sharper presets, recovery fixes

Game-FPS and smoothness improvements in this build are **not yet measured in a real game**. The measurements so far, and how to run the comparison, are in [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

- **Fixed: stutter in every recording with sound.** Recordings with audio repeated 57–84% of their frames (a user's Roblox clip showed about 12 new pictures per second while claiming 60 fps). Silent test recordings were smooth, which is why earlier checks missed it. Cause: video and audio shared one FFmpeg filter graph, so waiting for real-time audio stalled the video chain. On top of that, the deliberately held-back audio (the crackle fix) trailed the video by more than FFmpeg's scheduling tolerance, so the desktop grabber was paused again and again. Video and audio now run in separate graphs, and NVENC holds about 1/3 s of video for rate-control lookahead, so video never runs ahead of audio. Measured with system audio under a ~180 FPS game load: 57% → 0.0–0.28% repeated frames. Desktop capture itself delivered 122–133 new frames per second during Roblox.
- **Smaller files, better picture.** The Balanced preset is now 16 Mb/s at 1080p60 (was 28), matching what NVIDIA's recorder produced on the same laptop (16.5 Mb/s). NVENC uses preset p5, tune hq, spatial AQ and lookahead to spend the bits better.
- **Fixed: wrong colour tags.** NVENC converts BGRA with BT.709 limited range but tagged the result full-range RGB ("GBR"), so players that trust the tags showed it wrong. Frames, bitstream and container are now tagged BT.709 limited.
- **Fixed: recordings that looked laggy, with the FPS seeming to jump around, when the NVIDIA GPU drives the display.** A user recording of Fortnite was 60 fps on paper but showed only about 14 new pictures per second (76% repeated frames, freezes up to 2.9 s). Resizing went through a CPU copy that stalled while the game kept the GPU busy, and the bundled FFmpeg has no GPU-only resize for these frames. The new default, **Smooth**, records the display's own pixels on the zero-copy path and marks the file to play at the chosen shape (for example 1440×1080 that plays as 1920×1080). The app says when it does this, and **Exact size** keeps the copy path. In a game-load benchmark Smooth repeated 0.0–0.6% of frames, against 3.6–86% for the copy path, and the capture process now runs at high GPU priority.
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
