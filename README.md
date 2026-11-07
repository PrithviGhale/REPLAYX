# REPLAYX

**Capture everything. Miss nothing.**

REPLAYX is a free, open-source Instant Replay and screen recorder for Windows. It keeps the last moments of your desktop or game on disk, so you can save them with a hotkey. It works with no game running, records apps' audio on separate tracks, and handles stretched-resolution gameplay.

- **No account, no ads, no telemetry.** Everything stays on your PC.
- **Instant Replay** for the last 30 seconds up to 30 minutes, saved with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd>. Buffering continues after a save, and overlapping saves are fine.
- **GPU capture path:** it picks the most efficient working route on your hardware (zero-copy D3D11 or Intel Quick Sync, then a GPU-scaled transfer to NVENC/AMF, then the CPU) and tells you which one it used.
- **Stretched 4:3 gameplay:** *Stretch to fill* records a 1440×1080 game as full-frame 1920×1080, the way it looks on your screen. You can also preserve the aspect ratio, crop to fill, or keep the native shape.
- **Separate audio tracks:** game, Discord, browser/music and microphone each get their own track, plus a mix on track 1 for normal players.
- **Honest status:** a health checklist and a Performance page show real measurements, and saved clips are fully decoded before they count as saved.

> **Status: alpha (0.2.0-alpha.1).** Core capture is measured and tested on real hardware (see [VALIDATION.md](VALIDATION.md)), but the redesigned app, the installer and stretched Valorant are still awaiting hands-on testing. Read [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) before relying on it.

## Quick start

1. Open **REPLAYX** and click **Enable replay** (top right, or on Home). The ring fills as history builds up.
2. Play or work normally. When something happens, press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F10</kbd> or click **Save replay**.
3. Open **Clips** to play, rename, export to MP4, or delete it (deleted clips go to the Recycle Bin).

Click **Record** to record from now on; stop it with the same button or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F9</kbd>. Clips are saved to `Videos\REPLAYX` as MKV files, which survive crashes and keep every audio track.

### Stretched resolution (e.g. Valorant 1440×1080)

Go to **Video**, then choose **1080p** and **Stretch to fill**. When the game switches the display mode, REPLAYX detects it, restarts capture on the new mode, and checks that the encoded frame is exactly 1920×1080. The desktop can't leak into the frame.

### Game, Discord and microphone on separate tracks

In **Audio**, turn off **System output**, then add **Application…** for the game, again for Discord, and add your **Microphone**. Use **Test** to check each source. In a video editor, each source appears as its own track.

## Building from source

Windows 10/11 x64, Node 24, Rust 1.98 (MSVC) and WebView2. Full details are in [BUILDING.md](BUILDING.md).

```powershell
npm ci
powershell -ExecutionPolicy Bypass -File scripts/setup-media.ps1   # pinned, checksum-verified FFmpeg (LGPL)
powershell -ExecutionPolicy Bypass -File scripts/check.ps1         # format, tests, Clippy, frontend
npm run desktop                                                    # run in development
```

## Documentation

[ARCHITECTURE.md](ARCHITECTURE.md) · [VALIDATION.md](VALIDATION.md) · [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) · [PERFORMANCE.md](PERFORMANCE.md) · [PRIVACY.md](PRIVACY.md) · [SECURITY.md](SECURITY.md) · [CONTRIBUTING.md](CONTRIBUTING.md) · [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) · [CHANGELOG.md](CHANGELOG.md)

## License

REPLAYX is MIT licensed. It runs FFmpeg (LGPL-3.0-or-later build) as a separate program; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
