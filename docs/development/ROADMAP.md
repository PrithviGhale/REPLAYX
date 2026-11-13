# Roadmap

REPLAYX is in **alpha**. The order below follows what users reported and what measurements show, not a calendar. Items move only when they are measured on real hardware ([VALIDATION.md](VALIDATION.md)).

## Now: 0.2.x alpha

- [x] Disk-backed Instant Replay with overlapping saves, recording alongside replay
- [x] Hardware encoding (Intel Quick Sync incl. low-power VDEnc, NVIDIA NVENC, AMD AMF) with automatic path selection
- [x] Separate audio tracks (system, apps, microphone), drift-corrected WASAPI timeline
- [x] Game detection, per-game clip folders, game-detected notice
- [x] Explicit output size (1920 × 1080 means 1920 × 1080), quality presets
- [ ] **Game FPS impact measured in real games** on hybrid (Intel display + NVIDIA) laptops and on desktops, with the fixes that numbers call for
- [ ] 120 fps recording validated in real gameplay
- [ ] Installer install/upgrade/uninstall tested and documented
- [ ] Code-signed releases and signed in-app updates

## Next: 0.3 beta

- Windows Graphics Capture for single-window capture (game window only, no desktop)
- HEVC and AV1 hardware encoding where available (smaller files at the same quality)
- Automatic device following when audio devices change
- In-app trimming of saved clips without re-encoding
- Per-game settings (frame rate, quality, audio sources)
- Measured A/V sync report in the Performance page

## Later: 1.0

- Stable settings and file layout with migration guarantees
- Accessibility review (keyboard, screen reader, contrast) of every page
- Localisation
- Documented, signed update channel with release notes in the app

## Not planned

- Accounts, cloud upload, telemetry or ads
- Anything that injects into games or interacts with anti-cheat systems

Have an idea? Open a [feature request](https://github.com/PrithviGhale/REPLAYX/issues/new/choose).
