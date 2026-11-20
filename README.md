<p align="center">
  <img src="assets/branding/banner.svg" alt="REPLAYX — Capture everything. Miss nothing." width="100%">
</p>

<p align="center">
  <b>A free, open-source game recorder for Windows.</b><br>
  REPLAYX quietly keeps the last moments of your game, so when something great happens,<br>
  one shortcut saves it as a clip. No account, no ads, nothing uploaded.
</p>

<p align="center">
  <a href="https://github.com/PrithviGhale/REPLAYX/releases/download/v0.4.0/REPLAYX.exe"><img alt="Download for Windows" src="https://img.shields.io/badge/Download%20for%20Windows-v0.4.0-8b5cf6?style=for-the-badge&logo=windows&logoColor=white"></a>
  <br>
  <sub><a href="https://github.com/PrithviGhale/REPLAYX/releases/tag/v0.4.0">Release notes and checksums</a> · <a href="https://github.com/PrithviGhale/REPLAYX/releases">All releases</a></sub>
</p>

<p align="center">
  <a href="https://github.com/PrithviGhale/REPLAYX/releases"><img alt="Latest version" src="https://img.shields.io/github/v/release/PrithviGhale/REPLAYX?label=version&color=8b5cf6"></a>
  <img alt="Windows 10 and 11, 64-bit" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4">
  <a href="https://github.com/PrithviGhale/REPLAYX/actions/workflows/check.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/PrithviGhale/REPLAYX/check.yml?branch=main&label=build"></a>
  <a href="LICENSE"><img alt="MIT licence" src="https://img.shields.io/badge/licence-MIT-3ad29f"></a>
</p>

<p align="center">
  <img src="docs/images/showcase-home.jpg" alt="REPLAYX 0.4.0 Capture overview with Instant Replay on, in front of the Clips library" width="100%">
  <br>
  <sub>REPLAYX 0.4.0, shown with sample data.</sub>
</p>

## New in 0.4.0

- **Redesigned workspaces** keep recording and Instant Replay controls available from Capture, Library, Studio and Settings.
- **Guided setup and What's New** explain the controls and show features added since your previous version.
- **Crosshair Studio** supports presets, customized overlays, and a visibility shortcut in compatible games.
- **Recording and replay fixes** improve FFmpeg audio timing across segments and preserve completed footage when a recording fails to stop cleanly.
- **Windows setup** upgrades per user and removes retired diagnostic tools while keeping settings and recordings.
- **Native V2** is available as an experimental option on compatible hardware. FFmpeg is the default.

See the [release notes and checksums](https://github.com/PrithviGhale/REPLAYX/releases/tag/v0.4.0) and [known limitations](docs/guide/KNOWN_LIMITATIONS.md).


## Why REPLAYX

- **Never miss the moment.** Instant Replay keeps the last 30 seconds to 30 minutes on disk. Press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> to save a clip without re-encoding.
- **Hardware recording.** NVIDIA, Intel and AMD encoders; H.264, HEVC or AV1; 720p to 4K output.
- **Clips sorted by game.** 140 games are recognised automatically, each with its own icon, in a library you can search, sort and export to MP4.
- **Clean voice, separate tracks.** The Voice Enhancer removes background noise. Game, chat and microphone each get their own track.
- **Playtime tracking** per game, by day, week or month.
- **Private by design.** Everything stays on your PC: no account, no telemetry, no uploads.

[See every feature →](docs/guide/FEATURES.md)

## Install

1. Download **`REPLAYX.exe`** with the button above.
2. Run it. It installs for your account only, without administrator rights.
3. Builds aren't Windows code-signed yet. Compare the file with the SHA-256 shown on the release page. If SmartScreen or Windows Security blocks it, stop and review the warning with security protections enabled.

Requires Windows 10 or 11 (64-bit). A GPU with hardware encoding is recommended. [Full setup guide →](docs/guide/GETTING_STARTED.md)

## How Instant Replay works

1. **Turn it on before you play.** REPLAYX keeps a rolling buffer of your screen and sound on disk, in short pieces, dropping the oldest first.
2. **Press save** right after something happens. REPLAYX joins the last moments into one clip without re-encoding and checks that it plays.
3. **Find it in the Library**, filed under the game you were playing. Recording keeps going, so you can save again moments later.

Normal recordings run alongside and are saved the same way.

## Capture and encoders

| | |
| --- | --- |
| **Capture** | Desktop Duplication (default) or Windows Graphics Capture (experimental, *Video → Capture method*); full display or a region |
| **Encoders** | NVIDIA NVENC, Intel Quick Sync, AMD AMF, with a software fallback; *Auto* tries hardware encoding based on the selected display and available GPU paths |
| **Codecs** | H.264, HEVC, AV1 (where the GPU supports them) |
| **Output** | 720p to 4K, 30 to 144 FPS; Native, Fit, Crop and Stretch (4:3 stretched games) |
| **Audio** | Up to six sources: system sound, individual apps, microphone (with Voice Enhancer); one track each plus a full mix |

## Shortcuts

| Save replay | Record | Replay on / off | Mute mic | Open / hide |
| :---: | :---: | :---: | :---: | :---: |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F9</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F8</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> |

These global shortcuts can be changed in **Settings → Hotkeys**.

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
> **Hardware compatibility varies.** FFmpeg is the default, and Native V2 is experimental. See the [known limitations](docs/guide/KNOWN_LIMITATIONS.md), and please report anything that goes wrong.

REPLAYX is [MIT-licensed](LICENSE). It ships an LGPL build of FFmpeg, whose source is published with every release, and other open-source components listed in the [third-party notices](THIRD_PARTY_NOTICES.md).
