# Local validation build: 0.3.1-alpha.7

Not published. Branch: `codex/performance-0.2.1`.
Recovery checkpoint: `71e03ca`; implementation: `9ec609e`; production TLS fix:
`471555e`.

## Launch and artifacts

The portable executable is `src-tauri/target/release/replayx.exe`.
An isolated instance was launched using `REPLAYX_DATA_DIR=test-output/alpha7-ready`;
capture is off, mic muted, output goes to that fixture's clips folder. Launching
the executable normally uses normal application data; the running isolated copy
is intended for review without changing existing settings.

Installer: `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.7_x64-setup.exe`
(61,263,728 bytes). Adjacent `.exe.sig`, `latest.json`, `SHA256SUMS.txt` exist.
Executable, installer ProductVersion and manifest are all `0.3.1-alpha.7`;
bundled FFmpeg executable matches the pinned runtime hash. JSON has no BOM.

Installer SHA-256:
`ec415adf500ddd591321f967ef3e5b17088a620ad4e50de6bfdac3d99e7591da`

Executable SHA-256:
`320a1819dd06ef08e2f1dcb74e370a937f79ef8e83444d97bbcb6dc48160e94d`

The first NSIS generation succeeded; the combined Tauri command then failed at signing
because its build step requires `TAURI_SIGNING_PRIVATE_KEY`, rather than the
key-path variable understood by the standalone signer. The completed installer
was signed separately using the existing local key. Independent cryptographic
verification and signed-version validation passed before manifest generation.
The subsequent production-channel test exposed a missing TLS provider before
release discovery; it was fixed and the release rebuilt. The corrected native
app contacted GitHub successfully and reported no newer signed channel release.
No key/password was printed or committed. Authenticode is not provided.
The installer was not run: this host has no disposable Windows VM, and another
NSIS target directory would still share registration with the working install.

## Results and limits

- 160 Rust tests, 50 frontend tests and 42 native updater checks passed (252).
  TypeScript/Vite, rustfmt and Clippy with warnings denied passed.
- 132 responsive text cases and 24 scroll cases passed. Each media acceptance
  run passed 9 checks; the short and end-of-soak fixtures were both tested.
- Native release UI opened at the correct version. Homepage mic switch is
  visible and muted; Settings and Updates navigation work. Update popup and
  badges were tested in the real debug app against signed local fixtures.
- 15-minute synthetic replay soak passed with two additional NVENC sessions,
  overlapping saves ready in 691/664 ms, full decode and buffer cleanup.
  Memory averages 459.85 -> 461.94 MB; peak 522.33 MB; fixture CPU 6.56%.
  End-of-soak generated A/V offset max 30 ms. No physical microphone was opened.
- Final hardware desktop capture used actual H.264 NVENC and D3D11 zero-copy,
  1920x1080, system WASAPI loopback, no error, zero decoded repeats. Saves were
  ready in 547/550 ms. The silent loopback delivered no audible content, so this
  is not a listening test. Its proxy FPS was 224.1 -> 204.6 (8.7%), with installer
  compression overlapping; it cannot establish a before/after improvement.
- Controlled recovered-engine proxy runs measured 2.9-3.3% overhead with the
  existing priority versus 3.2-4.2% at normal priority. No priority, lookahead,
  preset or quality reduction was justified. Motion advances still varied.

The reported ~45% loss in real games remains unresolved. GPU readback on fallback
scaling paths is a confirmed potential cost, but not a demonstrated cause of that
report. Fixed defects include automatic updates interrupting replay, update/start
races, prerelease discovery, misleading failure messages, stale signatures and
queued microphone gain. See PERFORMANCE.md, VALIDATION.md and
MANUAL_ACCEPTANCE.md for evidence and remaining gameplay, stretched-resolution,
Discord, physical audio, editor and installed-update acceptance.

Nothing was pushed, merged, tagged or published. Existing installation, recordings
and settings were not used as update-test targets.

Final corrected Tauri signed build completed successfully (exit 0), including NSIS and updater signature generation.
