<p align="center">
  <img src="assets/branding/banner.svg" alt="REPLAYX — Capture everything. Miss nothing." width="100%">
</p>

<p align="center">
  <b>A free, open-source game recorder for Windows.</b><br>
  REPLAYX quietly keeps the last moments of your game, so when something great happens,<br>
  one shortcut saves it as a clip. No account, no ads, nothing uploaded.
</p>

<p align="center">
  <a href="https://github.com/PrithviGhale/REPLAYX/releases/download/v0.3.1-alpha.9/REPLAYX_0.3.1-alpha.9_x64-setup.exe"><img alt="Download for Windows" src="https://img.shields.io/badge/Download%20for%20Windows-v0.3.1--alpha.9-8b5cf6?style=for-the-badge&logo=windows&logoColor=white"></a>
  <br>
  <sub><a href="https://github.com/PrithviGhale/REPLAYX/releases/tag/v0.3.1-alpha.9">Release notes and checksums</a> · <a href="https://github.com/PrithviGhale/REPLAYX/releases">All releases</a></sub>
</p>

<p align="center">
  <a href="https://github.com/PrithviGhale/REPLAYX/releases"><img alt="Latest version" src="https://img.shields.io/github/v/release/PrithviGhale/REPLAYX?include_prereleases&label=version&color=8b5cf6"></a>
  <img alt="Windows 10 and 11, 64-bit" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4">
  <a href="https://github.com/PrithviGhale/REPLAYX/actions/workflows/check.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/PrithviGhale/REPLAYX/check.yml?branch=main&label=build"></a>
  <a href="LICENSE"><img alt="MIT licence" src="https://img.shields.io/badge/licence-MIT-3ad29f"></a>
</p>

<p align="center">
  <img src="docs/images/showcase-home.jpg" alt="REPLAYX home screen with Instant Replay on, in front of the Clips library" width="100%">
  <br>
  <sub>The Alpha.9 interface, shown with sample data.</sub>
</p>

## Why REPLAYX

Everything in this list is in the current download (0.3.1-alpha.9):

- **Never miss the moment.** Instant Replay keeps the last 30 seconds to 30 minutes on disk. Press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> and the moment is a clip, usually within a second.
- **Hardware recording.** NVIDIA, Intel and AMD encoders; H.264, HEVC or AV1; 720p to 4K output.
- **Clips sorted by game.** 140 games are recognised automatically, each with its own icon, in a library you can search, sort and export to MP4.
- **Clean voice, separate tracks.** The Voice Enhancer removes background noise. Game, chat and microphone each get their own track.
- **Playtime tracking** per game, by day, week or month.
- **Private by design.** Everything stays on your PC: no account, no telemetry, no uploads.

## New in 0.3.1-alpha.9

Alpha.9 is an alpha prerelease. Times and sizes below were measured on one development laptop (Intel Core i9-13900H with Intel and NVIDIA graphics, Windows 11). Results vary with the PC, game and settings. Installed-app and additional hardware tests remain unfinished. See the release notes for details.

- **Faster start.** With Windows Graphics Capture, Intel Quick Sync and system sound only, the first Instant Replay or Record after opening REPLAYX reached recording in about 1.2 s instead of about 3.3 s. Later starts took about 1.1 s.
- **Smaller clips.** *Balanced* at 1080p60 targets an average of 15 Mb/s on every encoder. Re-encoding one 60-second Fortnite clip with REPLAYX's settings averaged 15.8 Mb/s on Intel Quick Sync and 15.2 Mb/s on NVIDIA NVENC. That's about 223–233 MB per 2 minutes as Windows shows sizes, similar to NVIDIA's own recorder, at a VMAF quality score of 92.7. Very detailed scenes can briefly use up to 23 Mb/s.
- **Replay length slider** from 30 seconds to 30 minutes, showing how much space a saved replay takes.
- **Errors you can fix in one click**, settings that are never lost, clear status messages you can close, a collapsible sidebar, and a lighter footprint in the tray.

Details: [alpha.9 release notes](docs/release-notes/v0.3.1-alpha.9.md).

<p align="center">
  <img src="docs/images/showcase-replay.jpg" alt="Alpha.9 Instant Replay page: replay length slider with the size of a saved replay, and the disk budget" width="49%">
  <img src="docs/images/showcase-library.jpg" alt="Alpha.9 Clips library with thumbnails, filters and sorting, in front of the Games page" width="49%">
  <br>
  <sub>The Alpha.9 interface, shown with sample data.</sub>
</p>

[See every feature →](docs/guide/FEATURES.md)

## Install

1. Download **`REPLAYX_0.3.1-alpha.9_x64-setup.exe`** with the button above.
2. Run it. It installs for your account only, without administrator rights.
3. Builds aren't Windows code-signed yet. Compare the file with the release's `SHA256SUMS.txt`. If SmartScreen or Windows Security blocks it, stop and review the warning with security protections enabled.

Requires Windows 10 or 11 (64-bit). A GPU with hardware encoding is recommended. [Full setup guide →](docs/guide/GETTING_STARTED.md)

## How Instant Replay works

1. **Turn it on before you play.** REPLAYX keeps a rolling buffer of your screen and sound on disk, in short pieces, dropping the oldest first.
2. **Press save** right after something happens. REPLAYX joins the last moments into one clip without re-encoding and checks that it plays.
3. **Find it in Clips**, filed under the game you were playing. Recording keeps going, so you can save again moments later.

Normal recordings run alongside and are saved the same way.

## Capture and encoders

| | |
| --- | --- |
| **Capture** | Desktop Duplication (default) or Windows Graphics Capture (experimental, *Video → Capture method*); full display or a region |
| **Encoders** | NVIDIA NVENC, Intel Quick Sync, AMD AMF, with a software fallback; *Auto* picks the most efficient one that works on your PC |
| **Codecs** | H.264, HEVC, AV1 (where the GPU supports them) |
| **Output** | 720p to 4K, 30 to 144 FPS; Native, Fit, Crop and Stretch (4:3 stretched games) |
| **Audio** | Up to six sources: system sound, individual apps, microphone (with Voice Enhancer); one track each plus a full mix |

## Shortcuts

| Save replay | Record | Replay on / off | Mute mic | Open / hide |
| :---: | :---: | :---: | :---: | :---: |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F9</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F8</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> |

All of them work in games and can be changed in **Settings → Hotkeys**.

## System requirements

- Windows 10 or 11, 64-bit (Windows Graphics Capture needs Windows 10 1903 or later)
- A GPU with hardware video encoding (NVIDIA, Intel or AMD) is strongly recommended; REPLAYX falls back to software encoding without one
- About 2 GB of free disk space for the replay buffer (adjustable)

## Building from source

```powershell
git clone https://github.com/PrithviGhale/REPLAYX.git
cd REPLAYX
npm ci
powershell -ExecutionPolicy Bypass -File scripts/prepare-bundle.ps1   # pinned, checksum-verified FFmpeg runtime
npx tauri dev                              # or: npx tauri build
```

Requires Node 24, the Rust MSVC toolchain, the Visual Studio C++ build tools with a Windows SDK, and WebView2. `scripts/check.ps1` runs everything CI runs. Details: [Building](docs/development/BUILDING.md).

## Repository layout

| Folder | What's inside |
| --- | --- |
| `src/` | The app's interface (React, TypeScript) |
| `src-tauri/` | The recording engine and desktop app (Rust) |
| `docs/` | [User guides](docs/guide/), [development docs](docs/development/), engineering reports and release notes ([index](docs/README.md)) |
| `scripts/` | Build and release scripts; benchmarks in `scripts/bench/` |
| `tests/` | Frontend and Python tests (Rust tests live next to the code) |
| `dev/` | Development preview and screenshot tools |

## Known limitations

- On laptops whose screen is driven by the integrated GPU, recording can noticeably lower the FPS of CPU-heavy games (about 19% measured in Fortnite with Windows Graphics Capture and Quick Sync). Screens driven by the dedicated GPU are much less affected. Investigation: [engineering reports](docs/README.md#engineering-reports).
- Exclusive full-screen games can hide REPLAYX's on-screen notices (recording itself is not affected).
- Builds are not code-signed yet.

More: [Known limitations](docs/guide/KNOWN_LIMITATIONS.md).

## Learn more

[Documentation](docs/README.md) · [FAQ](docs/guide/FAQ.md) · [Troubleshooting](docs/guide/TROUBLESHOOTING.md) · [Changelog](CHANGELOG.md) · [Report a bug](https://github.com/PrithviGhale/REPLAYX/issues/new/choose) · [Contributing](.github/CONTRIBUTING.md)

---

> [!NOTE]
> **REPLAYX is in alpha.** It works well on the PCs it has been tested on, but it hasn't been used on many yet. See the [known limitations](docs/guide/KNOWN_LIMITATIONS.md), and please report anything that goes wrong.

REPLAYX is [MIT-licensed](LICENSE). It ships an LGPL build of FFmpeg, whose source is published with every release, and other open-source components listed in the [third-party notices](THIRD_PARTY_NOTICES.md).
