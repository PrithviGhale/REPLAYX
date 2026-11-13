# Build and validation on Windows

Requirements: Windows 10/11 x64, Node 24/npm, Rust MSVC toolchain (developed with 1.98.0), Visual Studio C++ build tools and Windows SDK, and WebView2. Run from the project root in PowerShell.

```powershell
npm ci
powershell -ExecutionPolicy Bypass -File scripts/check.ps1
npm run desktop
```

`check.ps1` runs TypeScript/Vite, rustfmt, all desktop-feature Rust tests and Clippy with warnings treated as errors. Hardware/media tests are separate and cannot be replaced by these checks.

## Media runtime

`scripts/setup-media.ps1` downloads the LGPL shared FFmpeg archive pinned in `tools/ffmpeg-lock.json` and verifies its SHA-256. The lock points at the immutable dated release `autobuild-2026-11-06-12-50`, and the script refuses floating `latest` URLs. If the download is unavailable, pass a copy of the same file with `-ArchivePath`. To move to a newer build, use `-RefreshTag autobuild-YYYY-MM-DD-HH-MM`; it marks the lock as *not reviewed* until you scan it, run the media tests, and update the `reviewed` field (a unit test enforces this). A checksum proves agreement with the lock, not that a binary is safe.

The earlier FFmpeg development snapshot was quarantined as Trojan:Win32/Wacatac.C!ml. Version 0.1.2-alpha.1 replaces it with the SHA-256-pinned BtbN 9.0 LGPL stable-branch build. The extracted replacement passed a Windows Defender custom scan with real-time protection enabled before execution. No exclusion, restore or protection-disable action was used. Review and scan future dependency changes before execution.

## Local builds

Compile the application without making an installer:

```powershell
npm run tauri -- build --no-bundle
```

After the media-runtime issue is resolved and its preparation checks pass:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/prepare-bundle.ps1
npm run tauri -- build
```

Executable: `src-tauri\target\release\replayx.exe`. NSIS installers: `src-tauri\target\release\bundle\nsis\`. These paths alone do not establish successful installation or recording. Check the artifact's version/date against [VALIDATION.md](VALIDATION.md).

## UI preview without the native app

```powershell
npx vite build --config dev/vite.preview.ts
npx vite preview --config dev/vite.preview.ts
powershell -File dev/cdp-probe.ps1 -Url "http://127.0.0.1:1430/dev/preview.html?state=replay#home" -Screenshot home.png
```

The preview uses Tauri's `mockIPC` with fixture data (`?state=replay|rec|off|empty`). It renders headlessly, so no window opens.

## Hardware commands

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin replayx-cli -- displays
cargo run --manifest-path src-tauri/Cargo.toml --bin replayx-cli -- devices
cargo run --manifest-path src-tauri/Cargo.toml --bin replayx-cli -- capture-test 60 system stretch1080
cargo run --manifest-path src-tauri/Cargo.toml --bin replayx-cli -- capture-test 60 microphone stretch1080
cargo run --manifest-path src-tauri/Cargo.toml --bin replayx-cli -- capture-test 1800 system stretch1080
npm run test:media
node scripts/verify-media.mjs
```

`displays` and `devices` only enumerate Windows hardware; they do not launch FFmpeg. Capture tests save two overlapping replays while buffering continues, verify/decode them and write status samples/reports under `test-output`. Samples do not contain frame-level latency or GPU utilization. Speak into the selected microphone and play audible content for real tests; silence or packet counts alone are not usable-audio verification. Use the GUI for device selection and independent game/Discord sources. Exit the GUI before CLI tests to avoid competing capture workloads; test data directories are isolated.

## CI, updates and distribution

`.github/workflows/check.yml` performs source checks on Windows when this project is eventually hosted. It has no publishing step and does not claim real GPU/audio coverage. It has not run on a hosted runner yet.

Updates remain disabled. For a future signed build, set compile-time `REPLAYX_UPDATE_URL` to `https://github.com/OWNER/REPO/releases/latest/download/latest.json` and `REPLAYX_UPDATE_PUBKEY` to the Tauri public verification key. Keep the private signing key outside the repository and use Tauri's signing tooling. Both endpoint and signatures are required; installation is blocked during capture, recovery and saves. Signing/update rollback and installed updates need an end-to-end test before enabling user updates. Do not publish artifacts until licensing and validation gates pass.

Audio analysis without listening: `replayx-cli device-formats` lists each endpoint's mix format without opening a stream. `audio-test 10 application PID` captures one process via process loopback, isolating a test stimulus from other PC audio, and `capture-test 20 app:PID stretch1080` does the same end to end. `audio-test` also writes `capture.f32`, the exact producer output. Analyse with `python -I scripts/bench/audio-tone.py FILE f32` (known stereo tone; its docstring gives the stimulus command) or `python -I scripts/bench/audio-clicks.py FILE f32` (any content). Decode clip tracks with `ffmpeg -i clip.mkv -map 0:a:N -f f32le out.f32`. The scripts use only the Python standard library.

Independent real-audio diagnostic (does not launch FFmpeg): `cargo run --manifest-path src-tauri/Cargo.toml --bin replayx-cli -- audio-test 10 system DEVICE_ID` or `audio-test 10 microphone DEVICE_ID`. Obtain IDs from `devices`; omit ID to use the Windows default at start. It writes a local 48kHz stereo PCM16 WAV and packet/level report. Near-zero levels do not prove speech works; it does not test encoded tracks or A/V sync.
