# Independent review of REPLAYX Alpha.9

Review date November 12, 2026, using the client's date. Reviewed local commit
`051f935` on `codex/performance-0.2.1`. The initial working tree was clean.
The implementation comparison used `1210803..051f935`, as specified by the
handoff. Earlier WGC work was inspected as supporting architecture context.

**NEEDS FIXES BEFORE RELEASE**

The settings-preservation defect below blocks publication. The additional
confirmed behavior issues need correction or an explicit product decision.
The installer still needs approved installation and Alpha.8-to-Alpha.9 update
validation after the fixes. Application source was left intact throughout this
review. Only this report and ignored audit fixtures/logs were created.

## Confirmed findings

### P1 — Settings recovery can destroy the original or an earlier backup

Locations are `src-tauri/src/model.rs` lines 429–447 and 451–482. Subsequent
writes include `src-tauri/src/engine.rs` lines 465, 499 and 548.

`load_or_recover` returns ordinary writable defaults even when `fs::rename`
fails. A later settings save atomically replaces the original file without a
successful backup. Backup filenames also have only seconds of uniqueness, and
Windows rename replaces an existing destination.

Both cases were reproduced against the actual public `Settings` implementation
using an independent Rust harness under `test-output/alpha9-settings-review/`.
No user settings were touched. Evidence is in that directory's `review.log`.

- Write invalid bytes, open the file with read/write sharing and delete sharing
  disabled, and call `load_or_recover`. Rename fails and defaults are returned.
  Release the handle and save those defaults. The original bytes are overwritten.
- Recover one invalid file, write another invalid file and recover again in the
  same second. Both calls claim the same backup filename. Only one backup exists,
  containing the second file. The first backup's bytes are lost.

Use a unique backup name with no-replace semantics. Make failed preservation
an explicit state that prevents writes to the original, or use a separate
temporary profile until preservation succeeds. Distinguish parse/validation
failure from transient read or migration-write failures before replacing valid
custom settings with defaults. Keep normal atomic saves.

Add regression tests for a sharing violation followed by a later save, two
recoveries within one second, an existing backup, migration persistence failure,
and successful preservation of the original bytes. The current test near
`model.rs:712` covers one malformed file and a missing file, leaving these
failure paths uncovered. The data-directory lock is acquired in `Engine::new`
after settings loading, so it does not protect this rename operation itself.
The GUI's single-instance plugin helps ordinary repeated launches.

### P2 — Retry changes a failed recording request into replay buffering

Location is `src/ErrorNotice.tsx:18`, with distinct engine entry points at
`src-tauri/src/engine.rs:574` and `:577`.

Every retry executes `go('start_replay')`. Start a manual recording, cause its
start to fail, then use its error card's Retry action. That action enables replay
instead of starting the requested recording. `onAction` also dismisses the toast
after the command wrapper returns, including when the wrapper handled a failure.

Carry the failed operation with its error, or label the available action
explicitly as enabling replay. Keep the error actionable until the intended
operation succeeds. Add a component/integration regression that fails
`start_recording`, clicks Retry and checks the invoked command and resulting
Recording state. Existing mapping tests only inspect action descriptors.

### P2 — Packet starvation is presented as confirmed microphone permission denial

Locations are `src/errors.ts:41`, `src-tauri/src/engine.rs:2430`, and
`tests/errors.test.ts`'s microphone test.

The permission rule matches the word `permission` anywhere in the message.
The engine's packet-starvation error suggests checking microphone permission
among several possible causes. Passing that actual engine string through
`describeError` produces “Microphone access is blocked” and a statement that
Windows is refusing access. Missing packets also arise from device changes,
producer failures or stalls, without an access-denied result. The existing test
asserts the misleading classification and therefore passes.

Use typed error causes, or require an actual denial code/cause for the permission
category. Keep packet starvation as a microphone/device error with permission
checks offered conditionally. Add separate tests for starvation with advisory
permission wording, explicit access denial, unplugged hardware and startup
timeout. The direct Node reproduction was run during this review.

### P2 — A genuine quick game relaunch suppresses its new-session notice

Locations are `src-tauri/src/overlay.rs:35–64` and its test
`launcher_hand_off_and_quick_restart_continue_the_session` near line 957.

The process identity correctly includes creation time. However, the game-name
grace map suppresses any same-game process appearing within 15 seconds after
termination, including a genuine close/relaunch or crash/relaunch. The existing
test explicitly asserts no notice for a new process at time 35 after the previous
processes ended at time 30. The relaunch test that expects a notice waits until
time 60.

This conflicts with the requested per-genuine-session behavior and the release
note that closing and relaunching shows the notice again. Restrict hand-off
coalescing to a recognized launcher/client relationship or obtain agreement to
the grace-period behavior and document it. Add quick relaunch and PID reuse
within the grace interval to the acceptance tests.

### P2 — Some simple error surfaces still expose technical details directly

Locations are `src/errors.ts:76–80`, `src-tauri/src/model.rs:440–445`,
`src-tauri/src/desktop.rs:1353–1354` and `src/pages/Home.tsx`'s notice banner.

Unknown errors retain paths, decimal OS errors and exception text in their first
sentence. For example, `Failed at C:\\Private\\name\\session.db (os error 5)`
is displayed unchanged as the simple message. Settings-recovery notices embed
`Technical: {e:#}` and are rendered directly as banners, bypassing the expandable
technical-details component. These are local UI disclosure and clarity issues.

Use a generic unknown summary and preserve raw details in the disclosure. Model
settings recovery as structured notice data with separate technical details.
Add fixtures for paths, decimal OS errors, JSON parse failures and nested error
chains. Keep the original diagnostic text available for troubleshooting.

## Audio assessment

The ten-second retry window is a bounded mitigation for transient initialization
failures. Each retry creates a fresh enumerator/client and rereads the default
endpoint when the source uses the default. Explicit devices stay pinned to their
selected IDs. Failed clients leave scope before the next attempt. The backoff
caps at one second and readiness waits at most fourteen seconds on the caller.
The retry budget does not bound a blocking driver/COM call inside `open()`.

This strategy follows Microsoft's invalid-device recovery guidance. A longer
window alone cannot establish that Bluetooth, USB, virtual-device switching or
active-session recovery works on physical hardware. Those tests are release
qualification gaps. [Microsoft recovery guidance](https://learn.microsoft.com/en-us/windows/win32/coreaudio/recovering-from-an-invalid-device-error)

The endpoint-list watcher emits changes using ID/kind/name comparisons. It does
not explicitly route an existing stream when only the default endpoint changes.
An old endpoint that remains valid and produces silence can evade a
packet-health check. Treat this as an inherited, unverified routing risk.
Exercise a default switch while the old endpoint remains connected. Compare the
recorded audio with the selected output and inspect all independent tracks.
Windows exposes a dedicated default-endpoint notification for this case.
[Microsoft default-device notification](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immnotificationclient-ondefaultdevicechanged)

Cancellation is checked between attempts and during the capture loop. A start
waiting inside `spawn` lacks direct user-cancellation propagation into its
readiness wait. Test Stop and shutdown during repeated initialization failures,
including several enabled sources, and verify that late-prepared feeds are
disposed. No physical device failure or driver hang was induced in this review.

## Sound implementation and remaining scope

- Windows Settings destinations use a fixed native allowlist. Arbitrary URLs
  and command strings are rejected. Navigation targets for sound sources,
  microphone, display, encoder and output folder exist in the page components.
- Ordinary Alt+Tab deduplication, independent game tracking, process creation
  identity, bounded session cleanup and cheap liveness checks are implemented.
  Unavailable process metadata and the separate foreground sampling calls still
  need protected-process/focus-race qualification. The handoff's assertion that
  denied process access can only add a notice is too strong because
  `process_started(pid)?` can also suppress the foreground candidate entirely.
- The idle-loop early exit occurs after tray, playtime, notice and overlay work.
  Engine capture monitoring remains independent. Hidden active captures still
  get resource sampling. Low WebView memory mode uses a best-effort interface
  cast and resets to Normal on restoration. The stated 367-to-178 MB result is
  a single-machine observation. It was not independently remeasured here.
- Error controls use buttons, dismiss labels and alert/status roles. Anchor
  navigation respects reduced motion and moves keyboard focus. GDI notice
  code restores selected objects, deletes temporary resources and uses
  `SWP_NOACTIVATE`. Windows animation preference affects slide distance.
  Visual layout, screen-reader behavior, repeated restore cycles and
  multi-monitor notice placement remain manual tests.
- The Alpha.9 range does not change the recording pipeline in `engine.rs` or
  `media.rs`. Audio initialization does change, so previous capture results
  alone cannot qualify device recovery for this candidate.

## Independent validation

| Check | Actual result |
| --- | --- |
| Rust, desktop features | Initial run 171 passed, 2 failed, 1 ignored. One failure came from the sandbox temporary path containing `#!sandbox`, which FFmpeg could not resolve in the remux test. The other was the denied scratch-registry test. |
| Rust rerun with workspace TEMP/TMP | 172 passed, 0 failed, 1 ignored, with only `startup_entry_round_trips_in_a_scratch_key` excluded. No Windows registry permissions were changed. |
| Formatting | Passed |
| Clippy, desktop/all-targets, `-D warnings` | Passed |
| Frontend tests | 57 passed |
| TypeScript and production frontend build | Passed through `npm run build` |
| Python tests | 9 passed |
| Targeted settings harness | Both preservation failures reproduced |
| Native updater/microphone E2E | Blocked before application launch. `tasklist` precheck returned “The user name or password is incorrect” in the sandbox. The handoff's 42/42 result was not independently reproduced. |
| Existing installer updater signature | Verified against the built-in public key and exact Alpha.9 version |
| Installation / real update | Not performed |
| Graphical capture matrix / soak / physical devices | Not run under this audit's permission limits |

Logs are `.tmp-alpha9-independent-*.log`. The harness and fixture output are
under ignored `test-output/alpha9-settings-review/`. The harness used only local
fixtures and the existing library. Its build created an isolated target cache.

## Artifact and repository audit

Expected version `0.3.1-alpha.9` matches package manifests, frontend version,
Tauri configuration and the release executable's FileVersion/ProductVersion.
The Alpha.8 updater public key is unchanged. The detached signature verifies,
and `latest.json` embeds the same signature and names the expected Alpha.9 URL.

Installer path
`src-tauri/target-alpha9/release/bundle/nsis/REPLAYX_0.3.1-alpha.9_x64-setup.exe`

Size is 61,403,275 bytes. Independently computed SHA-256 matches SHA256SUMS.txt
and the handoff
`d19553ece521a91e7e1ce06d558ba5a1ac6eacd5825b477aec3423d22b57e7d1`.

The executable is `src-tauri/target-alpha9/release/replayx.exe`.
Authenticode reports the installer as NotSigned. Updater signing is valid.
Installation, installed runtime contents, launch after installation and a real
Alpha.8 update remain unverified. Any code correction requires rebuilding and
resigning the candidate, followed by regenerated checksums and manifest.

A static scan found zero missing relative Markdown file targets. Anchor targets
and remote HTTP availability were not comprehensively checked. README download
links intentionally target the published Alpha.6 release. No tag, push or release
was created. The release notes overstate quick-relaunch behavior and claim the
physical Bluetooth error is resolved despite the handoff's missing reproduction.
Revise those claims before publication.

The pinned LGPL FFmpeg runtime, third-party notices, source packaging scripts
and release workflow source-package gate are present. The local corresponding
source tar exists under `release/ffmpeg-source/` at 1,255,055,360 bytes. Source
availability is therefore more complete than “not done” suggests in the handoff.
Verify its fetch/checksum report and include the exact corresponding package
when distributing the fixed build. Source completeness/rebuild reproducibility
and installed license files were not independently certified in this audit.

## Handoff to Claude

1. Fix preservation failure and backup collisions before publishing.
2. Correct retry intent and permission classification, isolate technical text,
   and resolve genuine-relaunch behavior against the stated requirement.
3. Add the specific failure-path regressions above. Preserve bounded retries,
   existing audio alignment and recording recovery behavior.
4. Rerun automated checks and native updater tests in a permitted environment.
   Then qualify device changes, manual/replay start-stop, A/V sync, concurrent
   saves and Alpha.9 recording stability. Graphical runs need owner approval.
5. Rebuild/resign the corrected candidate. With permission, install in a
   disposable environment, test Alpha.8 updates and verify deployed FFmpeg and
   licensing files. Complete source-package checks before publication.

No publication, installation, recording deletion, user-settings modification or
application-code fix was performed by this review.
