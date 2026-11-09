<p align="center">
  <img src="docs/images/banner.svg" alt="REPLAYX — Capture everything. Miss nothing." width="100%">
</p>

<p align="center">
  <a href="https://github.com/PrithviGhale/REPLAYX/releases"><img alt="Download for Windows" src="https://img.shields.io/badge/Download-Windows%20installer-8b5cf6?style=for-the-badge&logo=windows&logoColor=white"></a>
</p>

<p align="center">
  <img alt="Windows 10 and 11, 64-bit" src="https://img.shields.io/badge/Windows-10%20%7C%2011%20x64-0078d4">
  <img alt="MIT licence" src="https://img.shields.io/badge/licence-MIT-3ad29f">
  <img alt="Status: alpha" src="https://img.shields.io/badge/status-alpha-7262ff">
  <img alt="No account, no telemetry" src="https://img.shields.io/badge/no%20account-no%20telemetry-22d3ee">
</p>

<p align="center">
  <b>REPLAYX</b> is a free, open-source Instant Replay and screen recorder for Windows gamers.<br>
  It keeps the last moments of your game on disk, so the play you didn't expect is one shortcut away,<br>
  with clean microphone audio, separate tracks, clips sorted by game and your playtime at a glance.<br>
  No account, no ads, no telemetry. Everything stays on your PC.
</p>

<p align="center">
  <a href="#download">Download</a> ·
  <a href="#getting-started">Getting started</a> ·
  <a href="#features">Features</a> ·
  <a href="#screenshots">Screenshots</a> ·
  <a href="#faq">FAQ</a> ·
  <a href="docs/KNOWN_LIMITATIONS.md">Known limitations</a>
</p>

<p align="center">
  <img src="docs/images/home.png" alt="REPLAYX home screen: Instant Replay on with 43 of 60 seconds buffered, capture details (display, 1080p60 output, NVIDIA NVENC), health checks, audio levels and recent clips" width="900">
</p>
<p align="center"><sub>Screenshots show the real REPLAYX interface with sample data. On your PC, games show their own icons.</sub></p>

> [!IMPORTANT]
> **REPLAYX is alpha software.** Capture, saving and the clip library are tested on real hardware (see [VALIDATION](docs/VALIDATION.md)), but it has been used on few machines so far, and builds are not code-signed yet, so Windows SmartScreen may warn. Read [Known limitations](docs/KNOWN_LIMITATIONS.md) before relying on it for something you can't replay.

## Download

Get the latest installer, `REPLAYX_<version>_x64-setup.exe`, from the [**Releases**](https://github.com/PrithviGhale/REPLAYX/releases) page. Each release lists the installer's SHA-256 checksum in `SHA256SUMS.txt`; you can compare it in PowerShell:

```powershell
Get-FileHash .\REPLAYX_*_x64-setup.exe
```

The installer is per-user (no administrator rights needed), about 60 MB, and includes the FFmpeg runtime REPLAYX records with. Your clips are never deleted when you uninstall.

## What's new in 0.3.1

- **New look:** the REPLAYX eye logo on the app, installer, taskbar and tray.
- **Games and playtime:** see how long you play each game, per day, week or month, with charts and a page per game.
- **Clips library:** *All clips* or *By game*, with sorting, filters, search and each game's own icon.
- **Pop-ups:** a short bar slides in when a recording starts and when a replay or recording is saved.
- **Open / hide shortcut:** <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> brings REPLAYX up over anything and sends it back to the tray.
- **Lighter on games:** hidden in the tray, REPLAYX draws nothing, and saving never competes with your game for the CPU.

The full list is in the [CHANGELOG](CHANGELOG.md).

## Features

### Instant Replay
Turn it on before you play. REPLAYX keeps a rolling window of the last 30 seconds to 30 minutes **on disk** (not in memory), bounded by a disk budget you choose. Press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> and the last moments are saved as a clip; buffering carries on, and overlapping saves are fine. Clips are joined from finished 2-second segments without re-encoding, so a 60-second replay is usually ready about a second after you press save. Presets (*Short clips*, *Balanced*, *Long sessions*) show how much disk each one needs.

### Recording
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F9</kbd> starts and stops a normal recording. If Instant Replay is running, the recording joins it instantly, without restarting capture or leaving a gap. Recordings survive crashes: an interrupted session can be recovered from the Instant Replay page.

### Quality and codecs
- **Exact output size:** 720p, 1080p, 1440p, 4K, your display's shape or custom. 1920 × 1080 always saves a 1920 × 1080 file; stretched 4:3 gameplay can be recorded full-frame.
- **Presets:** Performance, Balanced, High quality or a custom bitrate, with a *Recommended* tag for your hardware. On NVIDIA GPUs the presets use constant quality with a size cap (measured: same picture as before for about 12% less space).
- **Codecs:** H.264 (default, plays everywhere), HEVC and AV1 (about 25% smaller than H.264 at the same measured quality). Only codecs your PC can encode are offered, and REPLAYX never switches codec by itself.
- **Hardware encoding:** NVIDIA NVENC, Intel Quick Sync (including its low-power encoder) and AMD AMF, chosen by real probes, with a CPU fallback. A storage estimate shows MB per minute, GB per hour and the recording time left on your drive.

### Microphone and Voice Enhancer
Your microphone is part of every setup and is recorded on **its own track**. The **Voice Enhancer** cleans it up with REPLAYX's own real-time processing:

| Preset | What it does |
| --- | --- |
| **Off** | Your microphone exactly as Windows delivers it. |
| **Natural** | Removes rumble and a little background noise; sounds like your mic, only cleaner. |
| **Clear** (default, recommended) | Removes background noise, makes words easier to understand, evens out levels gently. |
| **Broadcast** | A fuller, steadier "streamer" voice: warmth, presence, stronger compression, softer S sounds. |
| **Noise-Free** | The strongest background-noise removal, quiet between words. |

Noise suppression (Off, Light, Balanced, Strong) uses **RNNoise**. In our tests on speech mixed with fan noise it removed about 10, 17 and 25 dB of the noise while leaving the speech practically unchanged, using about 1% of one CPU core. **Test mic** lets you hear yourself live, exactly as you will be recorded, and switch between *Enhanced* and *Original* at the same loudness. **Compare** records one short take (kept in memory only) so you can hear both versions of the same words. Fine-tune gives you recording gain, a rumble filter, a soft gate, compressor, EQ and de-esser, and you can save your own presets. REPLAYX never changes your Windows microphone level.

### Separate audio tracks
System sound, individual apps (game, Discord, music) and your microphone each get their own track, plus a full mix on track 1 so every player plays everything. New microphones and headsets are picked up automatically when you plug them in.

### Clips library and game detection
REPLAYX recognises 140 games (Fortnite, Roblox, Valorant, Minecraft, Counter-Strike 2, Apex Legends, Call of Duty, Overwatch 2, League of Legends, Genshin Impact and many more, plus games in Steam, Epic, Xbox and other store folders) by process identity only. It never injects into or reads from games. Clips are saved as `Videos\REPLAYX\<Game>\<Game>_2026-11-07_21-30-12.mkv`.

Browse **All clips** or **By game**, sort by date, game, length, size or type, filter by game, type and dates, and search. Rename, change the game, export to MP4 or delete to the Recycle Bin. Each game shows **its own icon**, taken from the game installed on your PC (Steam, Epic, Riot, Roblox and others) or from the game's executable once you've played it. Nothing is downloaded.

### Games and playtime
The **Games** page shows how long you played each game today, yesterday, this week, this month, over the last 7 or 30 days, all time or any dates you choose, with a daily chart, your most-played games and a page per game with its sessions and clips. It counts only time while the game is the window in front and REPLAYX is running, skips idle stretches (no keyboard or mouse for 15 minutes, adjustable) and sleep, and stays on your PC. It's on from the start; pause it, exclude games, export it to CSV, clear it or turn it off in **Settings**.

### Status overlay and pop-ups
Small icons in a corner of your screen show Instant Replay, your microphone and a red dot while recording (with an optional timer), over games, apps and the desktop. You see them while you play; Windows keeps them **out of your recordings** (verified on captured frames). Corner, opacity and icons are configurable.

A short **pop-up** slides in at the top-right for about three seconds: *Recording started* (once video is actually being written), *Instant Replay saved* and *Recording saved* with the game and length, or *Save failed*. Silent, click-through, never steals focus, never in your clips, and it can be turned off.

### Runs quietly in the background
Closing the window keeps REPLAYX running in the **system tray** (Windows' hidden icons, the **^** next to the clock): Instant Replay, recording, shortcuts and playtime keep working. Click the tray icon or press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> to bring it back; right-click for the menu (Instant Replay, Save replay, Record, microphone, Clips, Settings, Quit). It asks before quitting during a recording or save.

While hidden, REPLAYX draws nothing and checks its status only every 2 seconds. The once-a-second game check costs about 17 µs (≈0.002% of one CPU core), and everything after capture (joining clips, checks, thumbnails) runs below normal priority, so your game always comes first. Recording itself costs about 3–4.5% game FPS at 1080p60 on the test laptop ([measurements](docs/PERFORMANCE.md)).

## Screenshots

<p align="center">
  <img src="docs/images/replay.png" alt="Instant Replay page: replay length presets with disk estimates, custom length, buffer progress and disk budget" width="49%">
  <img src="docs/images/video.png" alt="Video settings: display, resolution, frame rate, codec, quality presets and a storage estimate" width="49%">
</p>
<p align="center">
  <img src="docs/images/clips.png" alt="Clips library: all clips with game, date, length, resolution and size, filters and sorting" width="49%">
  <img src="docs/images/clips-by-game.png" alt="Clips by game: one tile per game with its clip count and total length" width="49%">
</p>
<p align="center">
  <img src="docs/images/audio.png" alt="Audio page: microphone with the Voice Enhancer presets, noise suppression and Test mic, plus game and Discord on their own tracks" width="49%">
  <img src="docs/images/games.png" alt="Games page: 9 h 42 min played in the last 7 days, a daily chart, most played games and game tiles" width="49%">
</p>
<p align="center">
  <img src="docs/images/settings-overlay.png" alt="Settings: status overlay with a live preview, pop-up switch and playtime tracking" width="49%">
  <img src="docs/images/hotkeys.png" alt="Settings: keyboard shortcuts, including Open / hide REPLAYX" width="49%">
</p>
<p align="center">
  <img src="docs/images/performance.png" alt="Performance page: 59.9 fps encoded, 0 dropped frames, recorder CPU and memory, video pipeline and per-source audio health" width="49%">
  <img src="docs/images/tour.png" alt="First-run tour: step 1 of 12, Welcome to REPLAYX" width="49%">
</p>

## Requirements

- Windows 10 or 11, 64-bit. Per-app audio tracks need Windows 10 build 20348 or later (or Windows 11); the overlay's exclusion from recordings needs Windows 10 version 2004 or later.
- A GPU with hardware video encoding (NVIDIA GTX 900 series or newer, Intel HD 500 series or newer, AMD RX 400 series or newer) is recommended; otherwise REPLAYX encodes on the CPU. AV1 needs an NVIDIA RTX 40, Intel Arc/Core Ultra or AMD RX 7000 GPU. Development and testing use an Intel Iris Xe + NVIDIA RTX 4070 Laptop.
- Microsoft Edge WebView2 Runtime: included with Windows 11; on Windows 10 the installer downloads it if missing.
- Free disk space for the replay buffer (the default 2 GB budget holds about 15 minutes at 1080p60 Balanced with typical audio).

## Getting started

1. **Install** with `REPLAYX_<version>_x64-setup.exe`. Because builds are not code-signed yet, SmartScreen may say *Windows protected your PC*; choose **More info → Run anyway** only if the file's SHA-256 matches the published checksum.
2. **Take the tour.** REPLAYX opens with a one-minute tour (skip it with <kbd>Esc</kbd>; replay it any time from Settings).
3. **Set up your mic.** In **Audio**, choose your microphone and a Voice Enhancer preset; click **Test mic** to hear yourself.
4. **Play.** Turn **Instant Replay** on (top right). When something happens, press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd>; a pop-up confirms the save.
5. **Watch.** Open **Clips** to watch, rename, export or delete, and **Games** to see your playtime. Uninstalling never deletes your clips in `Videos\REPLAYX`.

### Keyboard shortcuts

| Action | Default |
| --- | --- |
| Save replay | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> (Alt+F10 if no other app uses it) |
| Start / stop recording | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F9</kbd> |
| Instant Replay on / off | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F8</kbd> |
| Mute microphone | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> |
| Open / hide REPLAYX | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> |

All shortcuts work in games and can be changed in **Settings → Hotkeys**, including shortcuts another app swallows (pick them from *Common…*). If another program already owns a shortcut, REPLAYX says so and keeps your previous one working.

### Where things are

| Page | What you do there |
| --- | --- |
| **Home** | Capture status, health checks, audio levels, recent clips |
| **Instant Replay** | Replay length, disk budget, recovery of interrupted sessions |
| **Video** | Display, resolution, fill mode, frame rate, encoder, codec, quality, storage estimate |
| **Audio** | Microphone and Voice Enhancer, sound sources, tracks and audio quality |
| **Clips** | All clips or by game: watch, rename, change game, export to MP4, delete |
| **Games** | Playtime per game and period, daily chart, per-game sessions and clips |
| **Performance** | Live encoded frame rate, dropped frames, encoder speed, CPU, memory and audio health |
| **Settings** | Clips folder, game detection, overlay and pop-ups, playtime, tray, start with Windows, shortcuts, updates, tutorial |

## FAQ

**Is it safe with anti-cheat games?** REPLAYX records the screen through Windows (Desktop Duplication) and audio through WASAPI, like the Xbox Game Bar. It never injects into games, reads their memory or hooks their graphics. Game detection uses only the name of the program in front.

**Does it slow down my games?** While recording, about 3–4.5% FPS at 1080p60 in our measurements; less at lower settings. With capture off it does almost nothing. See [Performance](docs/PERFORMANCE.md).

**Where are my clips?** `Videos\REPLAYX\<Game>\` by default (change it in Settings). They're `.mkv` files, which survive crashes and keep every audio track; export any clip to MP4 from the library.

**Does anything leave my PC?** No. No account, telemetry, analytics or uploads. Playtime, settings and clips stay local. See [PRIVACY](PRIVACY.md).

**Why does Windows warn about the installer?** The builds aren't code-signed yet. Check the SHA-256 against the release's `SHA256SUMS.txt` before running it.

**How do I quit it completely?** Right-click the tray icon → **Quit**.

## Troubleshooting

| Problem | What to try |
| --- | --- |
| My microphone is silent | Check **Windows Settings → Privacy & security → Microphone**, pick the right device in **Audio**, and use **Test mic**. If you use Voicemod or another virtual mic, choose your real headset instead, or REPLAYX processes already-processed audio. |
| My voice sounds too processed | Choose **Natural** or lower noise suppression to **Light**; compare with **Test mic → Original**. |
| Games lose FPS while recording | Use 60 fps and the **Performance** or **Balanced** preset; the **Performance** page shows if the encoder falls behind. |
| A shortcut does nothing | Another app may own it (NVIDIA's overlay uses Alt+F10). Pick another one in **Settings → Hotkeys**. |
| I can't find REPLAYX after closing it | It's in the tray: click the **^** next to the clock, or press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd>. |
| The overlay doesn't show in a game | Windows can't show desktop windows over exclusive full-screen games. Use borderless or windowed mode. |
| SmartScreen blocks the installer | Builds aren't code-signed yet. Compare the SHA-256 with the published checksum before choosing *Run anyway*. |
| Something else | See [Known limitations](docs/KNOWN_LIMITATIONS.md) or [open an issue](https://github.com/PrithviGhale/REPLAYX/issues) with your Windows version, GPU and what you did. |

## How it works

```mermaid
flowchart LR
    D["Display<br/>(Desktop Duplication)"] --> S["GPU scale + colour convert"]
    S --> E["Hardware encoder<br/>H.264 / HEVC / AV1"]
    A["WASAPI audio<br/>system · apps · mic"] --> T["Drift-corrected timeline"] --> V["Voice Enhancer<br/>(microphone)"] --> M
    E --> M["2 s MKV segments<br/>(disk ring buffer)"]
    M -->|Save replay / Stop| C["Join without re-encoding<br/>+ playback check"] --> L["Clips library<br/>by game"]
    G["Foreground game<br/>(process identity)"] -.-> L
    G -.-> P["Playtime<br/>(local only)"]
```

Capture and encoding run in a separate FFmpeg process managed by the Rust engine; audio capture and voice processing run in the engine's own threads and never block video. The interface is a Tauri 2 window that never waits on the engine. Details: [ARCHITECTURE](docs/ARCHITECTURE.md), measurements: [PERFORMANCE](docs/PERFORMANCE.md), evidence for each claim: [VALIDATION](docs/VALIDATION.md), audio research: [RESEARCH](docs/RESEARCH.md).

**Technology:** Rust · Tauri 2 · React 19 + TypeScript · FFmpeg (LGPL build, pinned and checksum-verified) · WASAPI · DXGI Desktop Duplication · RNNoise (nnnoiseless) · Win32.

## Build from source

Windows 10/11 x64, Node 24, Rust 1.98 (MSVC) and WebView2. Full instructions are in [BUILDING](docs/BUILDING.md).

```powershell
git clone https://github.com/PrithviGhale/REPLAYX.git
cd REPLAYX
npm ci
powershell -ExecutionPolicy Bypass -File scripts/setup-media.ps1   # pinned, checksum-verified FFmpeg (LGPL)
powershell -ExecutionPolicy Bypass -File scripts/check.ps1         # format, tests, Clippy, frontend build
npm run desktop                                                    # run in development
npm run tauri build                                                # NSIS installer in src-tauri/target/release/bundle
```

### Project layout

```
src/                 React interface (pages, splash, tour, components, status store)
src-tauri/src/       Rust engine: capture pipeline, audio, Voice Enhancer, encoding, library, overlay, games, playtime
src-tauri/src/bin/   replayx-cli (engine and evaluation tools) and replayx-gameproxy (benchmark workload)
scripts/             Build, release and measurement scripts
tests/               Frontend unit tests
dev/                 UI preview with mock data, screenshots and headless layout checks
docs/                Architecture, building, performance, validation, research, roadmap, releasing, branding
```

## Roadmap

Highlights: code-signed builds and in-app updates, per-game profiles, a clip trimmer, more GPUs measured, and comparisons of the Voice Enhancer against real headset recordings. The full list is in [ROADMAP](docs/ROADMAP.md).

## Contributing

Bug reports, measurements from your hardware and pull requests are welcome. Start with [CONTRIBUTING](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md). Report security issues privately ([SECURITY](SECURITY.md)).

## Privacy

REPLAYX has no account, telemetry, analytics or uploads. Recordings, settings, playtime and microphone tests stay on your PC; the microphone is only opened while you record with it or test it. See [PRIVACY](PRIVACY.md).

## Licence

REPLAYX is released under the [MIT licence](LICENSE). It uses FFmpeg under the LGPL (shipped as separate, replaceable libraries; the corresponding source is attached to each release), RNNoise via nnnoiseless (BSD-3-Clause) and other open-source components listed in [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md). H.264, HEVC and AAC may be subject to patents in some countries; the software licence does not grant patent rights. Game names and icons belong to their owners; REPLAYX shows icons from games installed on your PC and bundles none.
