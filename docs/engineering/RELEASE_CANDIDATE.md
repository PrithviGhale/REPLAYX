# REPLAYX 0.4.0-rc.1 validation

This workspace combines the Crosshair Studio snapshot with the working Native V2 snapshot. The source revisions and initial conflicts are recorded in test-output/integration-source.json. The candidate keeps the existing Windows product identity and per-user install mode for upgrade testing.

## Engine choice and recovery

The Video page offers FFmpeg, Automatic and experimental Native V2. FFmpeg stays the persisted default. Automatic uses FFmpeg and its existing display-aware scaler and encoder probes while Native gameplay and recovery qualification continues. Native uses the display adapter and its same-device hardware H.264 encoder. The worker rejects an adapter switch between preflight and startup. A rejected configuration or startup error tries FFmpeg once and shows the reason. Mid-session Native failures preserve completed footage through the existing recovery path.

For Native, use a supported SDR display with 1920×1080, 60 FPS, Stretch to fill and Balanced or Custom bitrate. Diagnostic variants use explicit capture settings. Audio follows the existing mix and separate track settings. Native Radeon uses the hardware Media Foundation path and still needs physical Radeon validation. FFmpeg probes AMF and other viable paths under its existing encoder policy.

Crosshair Studio uses a click-through Windows overlay. Use windowed or borderless mode and follow the game's overlay rules. Display affinity requests capture exclusion. If a game blocks or covers the overlay, leave it disabled for that game.

## Build the setup

Use the local pinned FFmpeg runtime and normal Tauri build prerequisites. Run from the candidate root after installing the repository's development dependencies in the usual build environment.

    powershell -NoProfile -File scripts/build-rc.ps1

The script builds and stages the native worker, then builds the Tauri app and the NSIS installer. The artwork and recorder runtime are included in setup. The standard package uses Windows' existing WebView2 Runtime and can download it if absent. Pass `-OfflineWebView2` to embed Microsoft's runtime installer for machines without network access. That larger build needs the matching runtime available to Tauri at build time. Validate setup in a disposable Windows environment before distribution.

The installer uses Tauri's stock NSIS navigation, directory, shortcut, progress, completion and upgrade flow with REPLAYX header and sidebar artwork. Stock NSIS pages use system window chrome and native controls. A signed custom bootstrapper could own rounded controls and reduced-motion-aware transitions, but would require maintaining cancellation, return-code handling, upgrades, silent installs and accessibility across two executables. The current candidate uses the supported NSIS artwork options.

## Authenticode and updates

Use an approved code-signing certificate with its private key available to the build account, Windows SDK SignTool and the certificate provider's RFC 3161 timestamp endpoint. Keep credentials out of source control and console logs. On a configured signing runner, use the following command with your certificate metadata.

    powershell -NoProfile -File scripts/build-rc.ps1 -CertificateThumbprint <thumbprint> -TimestampUrl <url> -SignToolPath <absolute-path>

The script stages and signs the worker, supplies the certificate and timestamp options to Tauri, and checks the app, staged worker and setup with Get-AuthenticodeSignature. Verify the same signatures on extracted or installed copies during qualification. A managed signing service can instead use Tauri's signCommand with a securely configured signer. Authenticode and the Tauri updater signature are separate. Create and validate updater metadata against the final installer bytes after Authenticode signing. SmartScreen reputation develops separately from signature validity.

Official references are the [Tauri NSIS configuration](https://v2.tauri.app/reference/config/#nsisconfig) and [Windows code signing guide](https://v2.tauri.app/distribute/sign/windows/).

## Approved gameplay and install checks

Open CS2 on the built-in display with the external display disconnected, enter a local practice match, and return to the same stationary view for each block. Tell the operator the current FPS cap and explicitly approve the benchmark. The operator runs the existing PresentMon command from an administrator session if Windows requests confirmation. The game remains open and foreground during warm-up and measurement.

    python scripts/bench/cs2_ab.py

The command above prints the offline plan. For the approved run, add --execute --approved-gameplay with an existing PresentMon path, the actual --cap, and a new --out directory under test-output. The first block takes 4 minutes 33 seconds plus offline clip checks. Resume with identical arguments plus --resume to reverse engine order. Both recording phases have neighboring OFF measurements. The FFmpeg phase explicitly uses steady 2x WGC and the Native phase uses phase-lock with the benchmark-only 6666 microsecond interval. Those results describe the measured profiles. Require valid clips and both orders before drawing a comparative conclusion.

In a disposable Windows profile or VM, validate clean setup, upgrade with prior settings, native worker placement, recording plus Instant Replay, system and microphone tracks, crosshair exclusion, and uninstall with retained clips. Test AMD hardware separately on an actual Radeon system. Validate updater behavior with a signed test feed before changing public release metadata.


## Results for this candidate

| Check | Status | Evidence |
| --- | --- | --- |
| Combined app, frontend and NSIS | PASS | Release build succeeded. Setup metadata identifies 0.4.0-rc.1 with the established per-user upgrade identity. Generated NSIS includes the media runtime and native worker and excludes diagnostic executables. |
| Rust, Python and frontend | PASS | The final serial Rust run passed 283 tests with eight ignored. Python passed 56 with 21 skipped. Frontend passed 76. TypeScript, rustfmt and strict Clippy passed. The earlier parallel Rust run timed out in the replay-stop latency assertion during other builds. The unchanged assertion passed serially. |
| Crosshair and generated Native capture | PASS | The 125% scale DDA and WGC controls matched the hidden baseline exactly when the crosshair was excluded. Native tests changed Crosshair settings while recording and replay were active. Four generated MP4s decoded end to end, including both concurrent replay saves and the preserved recording from an injected failure. They had three generated audio tracks, two-second keyframes and at most 0.334 ms of video grid error. |
| Generated NVIDIA integration on the current layout | FAIL | Three NVIDIA A/V cases fell back at startup with 0x8000FFFF. The previous performance worker reproduced the failure in its generated hardware probe. Both workers successfully encoded on Intel. |
| Gameplay and real-world recording | BLOCKED | Current inventory reports the 2560?1600 240 Hz built-in panel attached to Intel Iris Xe. The RTX 4070 has no active display. Fresh matched CS2 results, physical system/microphone sync and real crosshair overhead remain pending. |
| Radeon and deployment | BLOCKED | Radeon hardware validation, Authenticode, a clean installation, upgrade, uninstall and signed updater validation still require their respective hardware, credentials or isolated environment. |

The generated installer is 60,438,922 bytes. Its SHA-256 is `378F74626641E94F237099E2D52D59162B8EF5D9D8F6F9172B7F01B73B602F4F`. Windows reports the app, worker and installer as unsigned. The artifact and test logs are under test-output and src-tauri/target/release.

Keep Native explicit for this candidate. Known Native pacing limits include full-screen dirty metadata and uneven 30 FPS holds. A restarted FFmpeg buffer can begin a new replay history. The diagnostic CLI wait covers first-segment readiness, while the sustained FFmpeg benchmark profile and matched gameplay checks still need approval and execution.
