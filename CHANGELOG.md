# Changelog

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
