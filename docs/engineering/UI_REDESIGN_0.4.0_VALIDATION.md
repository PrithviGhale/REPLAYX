# REPLAYX 0.4.0 redesign validation

The redesign worktree starts at the qualified `35b20b6` source. The prior stable branch, signed updater artifact and GitHub draft remain the fallback.

## Implementation

- The horizontal Capture, Library, Studio and Settings workspaces have addressable pages with Back and Forward history. The capture dock stays visible while browsing clips and changing pages.
- The Overview uses a rolling replay timeline, capture configuration, device meters and recent clip links. Recording, Audio and Preferences use separate work areas. Crosshair Studio keeps the preview alongside the editor.
- The logo animation uses finite CSS motion and exits when the startup code signals readiness. Reduced motion removes transitions and animations. Visibility and focus changes pause animation when the window is away.
- A native install-origin check distinguishes a fresh profile from existing or unreadable settings. The frontend keeps separate completion and dismissal outcomes by tour and version. An upgrade from pre-0.4 settings offers all three 0.4 features. Existing 0.4 pre-redesign settings offer the workspace changes. Malformed settings suspend automatic prompts.
- The release manifest holds versioned feature IDs, improvements and fixes. Fix-only releases show a short summary. The tour waits for an idle foreground application, uses native dialog focus management, and measures anchors only when their layout changes.
- FFmpeg remains default. Native V2, the existing audio pipeline and updater endpoints retain their qualified implementation.

## Offline evidence

| Check | Result |
| --- | --- |
| Rust tests with the desktop feature | 291 passed, 8 ignored |
| Formatting and strict Clippy | Passed |
| Frontend tests, TypeScript and optimized Vite build | 81 tests passed, build passed |
| Offline Python regressions | 64 passed, 21 opt-in hardware cases skipped |
| Browser UI tests with fixture IPC | 41 passed. Tested workspace history, keyboard navigation, replay length persistence, recording and replay actions, clip selection and playback, Crosshair settings, reduced motion, update-only and fix-only tours, unavailable anchors and hidden animation. |
| Generated FFmpeg recording and Instant Replay | One recording and three replay outputs fully decoded with three audio tracks each. The independently analyzed replay had monotonic timestamps, video and audio packet gaps at most 17 and 22 ms, respectively, and seven flash/pulse offsets within 10 ms. The isolated test tracks had 101 and 89.3 dB separation. |
| Optimized Windows NSIS build, product version and updater signature | Passed. The local installer is Windows unsigned. Its Tauri signature verifies against the embedded public key for version 0.4.0. The installer inputs match the pinned runtime. |
| Existing unsigned exception | Correctly rejects the redesigned installer. Its approval applies to the qualified fallback’s exact bytes. |

The first generated FFmpeg test timed out during the cold Rust build, and the same test passed on rerun. Browser review caught overflowing audio controls, missing spacing in status rows and compact capture labels disappearing. The final layout corrects all three.

## Frontend performance

The comparison served optimized frontend files and identical fixture data from the two worktrees in the same version of headless Chrome. Each run alternated four before/after pairs. The second run followed the Windows build and signing. A subsequent compact-width refinement restored the text labels on the capture buttons and passed the same browser regression suite. Values are medians from that run. Startup is page readiness. GPU-process CPU measures host CPU time consumed by Chrome’s GPU process. The working set sums the headless Chrome processes and includes shared memory.

| Measure | Qualified frontend | Redesign |
| --- | ---: | ---: |
| Cold frontend readiness | 64.5 ms | 43.5 ms |
| Warm frontend readiness | 18.5 ms | 17.0 ms |
| Renderer CPU over two idle seconds | 7.56 ms | 4.81 ms |
| GPU-process CPU over two idle seconds | 10.77 ms | 7.97 ms |
| JS heap | 4.38 MiB | 4.33 MiB |
| Summed Chrome working set | 1,034 MiB | 1,038 MiB |
| Navigation through two animation frames | 23.0 ms | 25.9 ms |
| Longest animation frame in navigation sample | 20.8 ms | 24.9 ms |
| Programmatic clip scroll sample | 6.6 ms | 5.5 ms |
| Hotkeys tab interaction sample | 7.5 ms | 9.9 ms |

An earlier comparison found about 76 ms of renderer CPU and 91 ms of GPU-process CPU over two idle seconds. Thumbnail placeholders continued pulsing outside the Overview scroll viewport. The new static placeholders keep lazy loading and eliminated the infinite animations in the idle test. Page transitions still carry a small frame-time cost in the controlled headless sample. The clip scroll comparison uses equivalent fixture data and each layout’s own scroll extent.

The browser data cannot establish hardware GPU utilization, installed WebView2 startup, native tray memory usage, device capture, physical Crosshair exclusion or in-game overhead. No gameplay benchmark was run for the redesign.

## Installer release gate

1. Approve the exact redesigned unsigned installer, app and worker SHA-256 values before using an additional narrowly scoped exception. Retain the old qualification pins for the draft fallback.
2. Approve a private upload and a fresh run of the existing GitHub-hosted installer matrix. Run clean, Alpha.4, Alpha.9, rc.1 and rc.2, followed by the Alpha.9 updater and restart scenario on the same Tauri-signed bytes. The private ZIP includes the pinned legacy installers and generated fixture. Its enclosed update manifest is a local qualification fixture.
3. On isolated interactive Windows, open the app once with a fresh profile and once with existing settings. Exercise the welcome/update tours, tray restore, record/stop, Instant Replay save, microphone and system audio, and Crosshair exclusion in the saved clip. The hosted matrix covers silent setup and preservation, while the interactive pass covers the physical device and WebView2 behaviors.
4. Review the design and evidence before replacing the existing draft or activating the production updater.

To repeat browser validation, build the redesign and qualified source with `npm run build` and `node ./node_modules/vite/bin/vite.js build --config dev/vite.preview.ts`. From the redesign run `node dev/redesign-check.mjs <path-to-qualified-source>`. The harness launches a new headless Chrome profile and retains its screenshots and measurements under `test-output`.
