# Local alpha.8 build - November 11, 2026

Unpublished. Branch `codex/performance-0.2.1`; baseline `825edae`, implementation
`2b7f0f3`. See PERFORMANCE_ALPHA8.md for measurements and FORTNITE_ABA.md for the
manual comparison. The real Fortnite overhead remains unresolved.

## What changed

Replay maintenance now reads only newly appended manifest bytes. Large-history
polling benchmark: 429.141 -> 26.456 ms per 1,000 polls. This does not establish
a game-FPS gain. Better GPU-path diagnostics, A/B/A tools, GPU draw timestamps,
clock/power collection and a read-only PresentMon CSV analyzer support follow-up.
Video quality, lookahead, audio handling and default GPU priority are preserved.
AQ-off, lookahead-off and lower-priority alternatives did not justify a default
change. No capture/NVENC rewrite or modified media runtime was introduced.

## Actual validation

- 161 Rust tests, 51 frontend tests, four Python tests and 42 isolated native
  updater/microphone checks passed (258 core checks), plus the separately run
  release polling benchmark. TypeScript, rustfmt and Clippy passed.
- Six hardware cases passed for H.264/HEVC/AV1 and 4:3 regions; 12 saved clips
  fully decoded. Smooth 4:3 files had correct 16:9 display aspect metadata;
  exact 1920x1080 scaling used CPU fallback. Actual display-mode transitions
  and physical gameplay appearance were not tested.
- 15-minute synthetic replay soak passed with concurrent NVENC workloads,
  two saves ready in 805/805 ms, clean full decoding and buffer cleanup.
  Resident means 458.62 -> 461.26 MB; peak 528.69 MB; synthetic CPU 6.21%.
  This includes CPU generation/scaling and is not game overhead.
- Nine end-of-soak media checks passed: three tracks, synthetic separation
  87.38/106.13 dB, max generated A/V offset 30 ms and monotonic timestamps.
- Detailed-scene default-quality A/B/A results generally fell around 5-8%
  overhead, near-zero repeated decoded frames, and increased GPU draw time.
  Short-session before/after comparisons did not establish an improvement.
  CPU readback is absent from the measured direct path; exact attribution of
  GPU cost among copies, conversion and driver scheduling is still unavailable.
- D3D11 NV12 scaling and direct CUDA mapping were unavailable in hardware
  probes; the direct D3D11-to-NVENC path passed. These failures were retained
  and documented, not hidden by claiming every tested path worked.

## Artifacts

Executable: `src-tauri/target/release/replayx.exe`

Installer: `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.8_x64-setup.exe`

Adjacent assets: `.exe.sig`, `latest.json`, `SHA256SUMS.txt`.
Final signed build succeeded (exit 0). Independent signature/signed-version verification passed; executable, installer and manifest match alpha.8. Installer size: 61257218 bytes.

Installer SHA-256: 1f420d0729454393cf17d7229d642eccd2894029bf513d91cec2e3aa5c7f8186

Executable SHA-256: e75d0ccb279858169344185aa1b86c65f9885fcdd69ca7273ba77134dc392d6e

The installer is not installed or published. A standalone review instance uses
`REPLAYX_DATA_DIR=test-output/alpha8-ready`, with capture off, mic muted and a
test clips folder. Normal startup uses the existing user profile; close the
isolated review instance before launching a normal copy.

## Remaining acceptance

Run Fortnite A/B/A with frame-time CSVs and comparable NVIDIA settings; account
for the 360 FPS cap. Confirm physical fullscreen 1280x960/1440x1080 transitions,
Discord streaming, real microphone/system audio listening and editor playback.
Unique source FPS and per-frame capture/conversion/encode latency remain
unmeasured live. No ShadowPlay parity or production-readiness claim is made.

No push, merge, tag, release publication, installation, security-setting change,
game/account interaction, or deletion of user recordings/settings occurred.

Native release UI verification passed: version alpha.8, Home mic muted, capture Stopped, and Performance navigation/diagnostics visible. The isolated review copy remains open.
