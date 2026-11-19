# REPLAYX 0.4.0-rc.2 validation

The unsigned x64 build is ready for an approved installation in a disposable Windows guest. Public release qualification remains open.

| Area | Result | Evidence |
| --- | --- | --- |
| Automated checks | PASS | Rust 284 passed and 8 ignored in the standard run. Strict Clippy and rustfmt passed. Frontend 76 passed, TypeScript and Vite passed. Python 56 passed and 21 skipped. |
| Intel Native V2 | PASS | Eight generated GPU frames encoded on Intel Iris Xe with Quick Sync. The desktop app's opt-in generated-media lifecycle passed normal recording, two concurrent replay saves, clip library, and the injected runtime failure recovery. A separate test passed startup fallback for unsupported settings. |
| NVIDIA Native V2 on this host | FAIL | On the Intel-driven panel with the RTX 4070 display output inactive, the NVIDIA adapter created the D3D11 device and enumerated its hardware H.264 transform. `IMFActivate::ActivateObject` returned `0x8000FFFF` before the D3D11 manager or codec settings were supplied. All three opt-in NVIDIA application modes reproduced the failure and entered FFmpeg fallback. FFmpeg NVENC independently encoded four generated frames on the same RTX 4070 with driver 621.24. This isolates the failing operation. The underlying driver or hybrid-mode cause remains undetermined. |
| GPU selection | PASS | Native tries eligible transforms for the exact capture adapter and reports activation failures. Tests cover subsequent candidates and exhaustion for Intel, NVIDIA and AMD. Existing FFmpeg fallback and recording defaults are retained. |
| Crosshair on controlled desktop | PASS | DDA and WGC both reproduced the hidden baseline exactly with capture exclusion enabled. The intentionally capturable control changed the targeted crop in both paths. The overlay verifies Windows capture exclusion before showing. No game, interactive benchmark or in-game overhead test ran. |
| MP4 and synthetic audio | PASS | An independent decode checked three normal/replay files of 360 frames each. Each had three audio tracks, keyframes at frames 0, 120 and 240, and maximum 60 FPS presentation-grid error of 0.334 ms. Audio ended 5.17 ms before video in the recording and 16.17 ms after video in the replays. |
| Failure recovery | PASS / FAIL | Injected failure preserved a fully decodable 329-frame clip with all three audio tracks. Audio continued 346.5 ms past the final video timestamp. Normal saves and replay did not exhibit that tail. |
| NSIS build and QA staging | PASS | The app and installer report 0.4.0-rc.2. The staged native worker hash matches the compiled binary. The installer retains the per-user mode and includes branded artwork for setup and uninstall. Clean, Alpha.4, Alpha.9 and rc.1 upgrade kits passed hash, version and read-only mapping checks. |
| Installation and upgrade | BLOCKED | No install or uninstall ran. Windows Sandbox is unavailable on this host. A disposable Windows guest with WebView2 and approval is needed. |
| Signing and updater | BLOCKED | Credential validation and the build hooks are prepared. The current outputs are unsigned. Authenticode trust, timestamps and a signed updater round trip still need qualification. |
| Physical and game tests | BLOCKED | Device audio synchronization, game overlay behavior and recording performance need an authorized physical session. No Radeon adapter is present. |

The standard, opt-in, and hardware evidence is saved under `test-output` in `rc2-rust.log`, `rc2-native-integration.log`, `rc2-adapter-probes.json`, `rc2-clip-audit.json` and `rc2-crosshair-exclusion/result.json`. Installer and build metadata are recorded in `rc2-artifacts.json`.

- Installer — `src-tauri/target/release/bundle/nsis/REPLAYX_0.4.0-rc.2_x64-setup.exe` (60,427,537 bytes)
- Installer SHA-256 — `99FBDDD69114EAAD0007F077EEB60C91ED8F090AB5883D61D3568437E9B19635`
- Windows app — `src-tauri/target/release/replayx.exe`
- Installer kits — `test-output/installer-qa/rc2-clean`, `rc2-alpha4-upgrade`, `rc2-alpha9-upgrade` and `rc2-rc1-upgrade`

See [Windows installer qualification](INSTALLER_QUALIFICATION.md) for the guest test and signing procedures.
