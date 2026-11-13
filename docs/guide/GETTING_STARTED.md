# Getting started

## Requirements

- **Windows 10 or 11, 64-bit.** Per-app audio tracks need Windows 10 build 20348 or later (or Windows 11); keeping the overlay out of recordings needs Windows 10 version 2004 or later.
- **A GPU with hardware video encoding** is recommended: NVIDIA GTX 900 series or newer, Intel HD 500 series or newer, or AMD RX 400 series or newer. Without one, REPLAYX encodes on the CPU. AV1 needs an NVIDIA RTX 40, Intel Arc / Core Ultra or AMD RX 7000 GPU.
- **Microsoft Edge WebView2 Runtime:** included with Windows 11; on Windows 10 the installer downloads it if it's missing.
- **Free disk space** for the replay buffer. The default 2 GB budget holds about 17 minutes at 1080p60 *Balanced* with typical audio.

## Install

1. Open the [**Releases**](https://github.com/PrithviGhale/REPLAYX/releases) page and download `REPLAYX_<version>_x64-setup.exe` from the newest release. You don't need the `ffmpeg-corresponding-source` file; it's the source code of the bundled FFmpeg, published for licence reasons.
2. *(Recommended)* Check the download against `SHA256SUMS.txt` from the same release:
   ```powershell
   Get-FileHash .\REPLAYX_*_x64-setup.exe
   ```
3. Run the installer. It installs for your Windows account only and doesn't need administrator rights.
4. Builds aren't code-signed yet, so SmartScreen may say *Windows protected your PC*. Choose **More info → Run anyway** only if the checksum matched.

Uninstalling never deletes your clips in `Videos\REPLAYX`.

## First run

1. **Take the tour.** REPLAYX opens with a one-minute tour. Skip it with <kbd>Esc</kbd>; replay it any time from **Settings → Getting started**.
2. **Set up your microphone.** In **Audio**, choose your microphone and a Voice Enhancer preset (*Clear* is recommended), then click **Test mic** to hear yourself.
3. **Turn on Instant Replay** with the switch at the top right, and play.
4. **Save the moment.** Press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd>. A short pop-up confirms *Instant Replay saved*.
5. **Watch it.** Open **Clips** to watch, rename, export to MP4 or delete. **Games** shows your playtime.

## Keyboard shortcuts

| Action | Default |
| --- | --- |
| Save replay | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> |
| Start / stop recording | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F9</kbd> |
| Instant Replay on / off | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F8</kbd> |
| Mute microphone | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> |
| Open / hide REPLAYX | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> |

All shortcuts work in games and can be changed in **Settings → Hotkeys**. Shortcuts another program swallows (NVIDIA's overlay uses Alt+F10) can be picked from **Common…**; if another program owns one, REPLAYX tells you and keeps your previous shortcut working.

## Running in the background

Closing the window keeps REPLAYX running in the **system tray** (the **^** next to the clock): Instant Replay, recording, shortcuts and playtime keep working. Click the tray icon or press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd> to bring it back; right-click it for the menu, including **Quit**.

**Settings → Start with Windows** opens REPLAYX quietly in the tray when you sign in. Capture never starts on its own.

## Where things are

| Page | What you do there |
| --- | --- |
| **Home** | Capture status, health checks, audio levels, recent clips |
| **Instant Replay** | Replay length, disk budget, recovery of interrupted sessions |
| **Video** | Display, resolution, frame rate, encoder, codec, quality, storage estimate |
| **Audio** | Microphone and Voice Enhancer, sound sources, tracks and audio quality |
| **Clips** | All clips or by game: watch, rename, change game, export to MP4, delete |
| **Games** | Playtime per game and period, daily chart, sessions and clips per game |
| **Performance** | Live frame rate, dropped frames, encoder speed, CPU, memory, audio health |
| **Settings** | Clips folder, game detection, overlay and pop-ups, playtime, tray, startup, shortcuts, updates |

Next: [Features](FEATURES.md) · [FAQ](FAQ.md) · [Troubleshooting](TROUBLESHOOTING.md)
