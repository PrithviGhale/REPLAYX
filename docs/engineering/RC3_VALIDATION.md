# REPLAYX 0.4.0-rc.3 qualification

For the later frontend version-label rebuild and current optical image, see the [final release checklist](FINAL_0.4.0_RELEASE_CHECKLIST.md). The hashes below describe the original rc.3 build, preserved under test-output/rc3-final-preflight/original-artifacts.

## Verdict

The local candidate passes build, automated reliability and static package checks. Public distribution remains on hold pending isolated installation and updater qualification. No setup or installed application was run during this qualification.

| Check | Status | Evidence |
| --- | --- | --- |
| FFmpeg recording and Instant Replay | PASS | Decoded a 600-frame 1080p60 recording and an 840-frame replay produced by the rc.3 CLI. Both encode the mix and two distinct tracks. All three AAC tracks maintained 21–22 ms packet spacing across segment joins. Video remained at 16–17 ms intervals. |
| Startup, Stop, recovery, save handling | PASS | The full Rust suite covered starts, switches, concurrent saves and cleanup. An injected manifest failure during Stop released the saving state and made the completed footage recoverable. Failed recording saves still attempt replay shutdown. |
| Crosshair Studio | PASS, automated | Confirmed settings round trips, shortcut checks, geometry and save order. A pending edit is flushed when leaving the page. Prior capture-exclusion controls passed on the tested desktop; no new in-game overlay test ran. |
| Native V2 and fallback | PASS, generated | Intel and NVIDIA generated tests passed on this machine. Six extra app tests exercised concurrent saves, forced failure recovery, A/V tracks and fallback. The Intel-driven hybrid NVIDIA activation limitation remains documented. FFmpeg remains the default. |
| Tooling and optimized package | PASS | 290 Rust tests passed with eight opt-ins excluded from that run. Six Native opt-ins were run separately and passed. Python passed 79 tests with six Radeon tests skipped. Frontend passed 77. Formatting, strict Clippy, TypeScript, Vite and optimized NSIS passed. |
| Package identity and payload | PASS, static | App and setup PE versions match rc.3. The generated NSIS uses currentUser and io.replayx.desktop. The staged Native worker matches the built worker. All 10 staged FFmpeg runtime binaries match the pin and appear in the setup script. |
| Clean install, upgrades and uninstall | BLOCKED | Five read-only test kits are staged for clean, Alpha.4, Alpha.9, rc.1 and rc.2 scenarios. An approved disposable Windows guest with WebView2 is required to run the tests and verify recordings and preferences survive. |
| Signed installer and updater | BLOCKED | The local setup is unsigned. Obtain the Windows signing input and use the existing updater signing key on the approved signing runner. Sign the final installer bytes, validate the manifest and run an installed update in a guest. rc.2 can follow the rc channel. Alpha.9 needs a manual move to this RC or can follow 0.4.0 stable. |
| Real device A/V smoke check and public release assets | BLOCKED | Verify the final candidate with a short ordinary desktop capture on real audio devices. Confirm the matching FFmpeg source archive for publication. No further FPS comparison is a qualification requirement. |

The first decode audit assumed a two-second keyframe interval. The software encoder inserted extra one-second keyframes. The rerun passed the correct check of a keyframe at the start and no interval exceeding two seconds.

The previous stationary comparison used two matched observations. Its exploratory interval crossed zero. The later motion unit had 11.34% OFF baseline spread in one bracket and remains excluded from comparative FPS conclusions. Those results stay with their original ledgers.

The installer is 60,456,024 bytes. SHA-256 is 33ED5884C80889E291D83A6F210C4DC596FCE5F39A3864E5EC6CBADD309E2B84. The build and package evidence is under test-output/rc3-qualification. See [release notes](../release-notes/v0.4.0-rc.3.md), [known limitations](../guide/KNOWN_LIMITATIONS.md) and [installer qualification](INSTALLER_QUALIFICATION.md).
