# Focused Alpha.9 re-review

Reviewed November 13, 2026, at `2e52a53` on `codex/performance-0.2.1`.
The expected commit was present and the initial working tree was clean.
Implementation review covered `cf89b88` and `910bd56` against `051f935`.

**NEEDS FIXES — Specific regressions or incomplete corrections**

The serial settings-preservation failures are corrected. A concurrent-writer
case still loses settings, and the new notification suppression still misses a
confirmed new session. Those corrections are required before this candidate
can proceed with a passing review to final manual installation/update validation.

## Status of the five original findings

| Original finding | Re-review result |
| --- | --- |
| P1 settings preservation | Partially resolved. Serial collisions, failed rename with safe copy, unreadable files, failed preservation and migration-save failure have meaningful coverage. Concurrent modification can still invalidate the preservation decision. |
| Retry starts the wrong operation | Resolved in the reviewed implementation. Engine intent and command context identify replay, recording and combined retries. Ordered execution stops on failure. Local guards prevent double-click runs, and failed retries retain their notice. |
| Microphone starvation reported as denial | Resolved for the reviewed cases. Explicit denial evidence selects the permission category. Starvation, disconnection, busy devices, startup timeout and advisory permission wording have separate coverage. |
| Genuine game relaunch suppressed | Partially resolved. Ordinary quick relaunch after a longer session works. A new process detected within three seconds of the initial notice is silently retained without ever receiving a notice. |
| Technical details in simple errors | Resolved for the original reproductions. Unknown technical messages receive a generic summary, settings problems are structured, and raw details use the expandable disclosure. |

## Confirmed P1 — Copy fallback can authorize overwriting newer valid settings

Locations are `src-tauri/src/model.rs:608` (`set_aside`), `:665` (backup-only
verification), `:477` (writable recovery classification),
`src-tauri/src/engine.rs:487` (`report_settings_problem`) and `:1853` (`persist`).

The fallback copies the bytes previously read from the original. It verifies
the backup against that snapshot but does not establish that the original still
contains those bytes. A concurrent writer can replace the source contents with
new valid settings. Recovery returns `Damaged`, allowing defaults to overwrite
the newer settings through the normal engine persistence path. The backup
contains only the older damaged bytes.

This was reproduced using the actual library with isolated files. The harness
is `test-output/alpha9-rereview/src/main.rs` and the final evidence is
`test-output/alpha9-rereview/review-final.log`.

1. Write damaged settings and hold a read handle allowing read/write sharing
   while denying delete sharing. This forces the copy fallback.
2. Start recovery. A second thread waits for backup creation, then writes valid
   custom settings with FPS 30 to the original.
3. Recovery completes and reports `allows_saving() == true`.
4. Construct an isolated Engine, report that recovery problem, and call its
   normal `set_settings` entry point. Persistence succeeds.
5. The newer valid bytes are absent from both the original and all backups.

The probe used a 32 MiB damaged fixture to make the interleaving reproducible.
This establishes a race and its consequence without estimating its frequency
for ordinary settings files. It ran successfully twice, including the final
normal-engine-entry-point reproduction. Serial recovery also preserved both
distinct original files in the independent harness.

The smallest safe correction is to keep recovery read-only whenever the source
snapshot cannot be protected through backup and transition to writable defaults.
An implementation that permits writes needs source identity/content validation
under appropriate sharing/locking, with no unchecked modification window before
replacement. A comparison followed by an unlocked save would leave another
race. Acquire the REPLAYX profile lock before migration/recovery to serialize
concurrent REPLAYX startups. Test concurrent external writers separately.

Add a regression that changes the source after backup creation begins and
asserts that the changed valid bytes either remain current, are preserved
separately, or cause writes to be refused. Include concurrent migration/startup.
The current tests cover serial snapshots and static sharing violations, so they
pass while this interleaving fails.

## Confirmed P2 — FLAP suppression permanently consumes a new session

Locations are `src-tauri/src/overlay.rs:48` and `:65–69`.

The new process is appended to `sessions` before the three-second announcement
guard is checked. When that guard suppresses the notice, future observations
return early because the process is already tracked. Expiration does not cause
the missed announcement to appear.

The same independent harness exercised the public `Announcer` with distinct,
known process identities and deterministic elapsed times.

| Observation | Actual result |
| --- | --- |
| Process A first detected at time 0 | Notice emitted |
| A confirmed dead at time 1 | Session removed |
| Distinct process B detected at time 2 | Notice suppressed |
| B remains foreground at time 10 | Notice still absent |
| Control case, A closes at time 100 and B starts at time 102 | New notice emitted |

The failure interval is measured from the previous announcement. It does not
affect every relaunch within three seconds of closing a mature game session.
It does violate the immediate-relaunch requirement for the demonstrated known
identity sequence. No game was launched or manipulated during the probe.

Bypass game-name suppression when a distinct known creation identity replaces a
confirmed-dead process. Apply any uncertainty guard to the same uncertain
identity, or queue a pending announcement until its guard expires. Add cases at
one and two seconds and check a later observation. The new relaunch tests use
four or more seconds, or six seconds from the first notice, leaving this gap.
The code comment assuming no genuine game window can return this quickly is
not a reliable condition for the implementation.

## Other focused checks and limitations

- All seven engine persistence call sites use `persist`. Unreadable and
  unpreserved settings produce a read-only session. A successful subsequent
  restart can load settings again. The identified bypass is an incorrect
  writable classification after a concurrent change.
- Backups use a no-replace move and `create_new` copies. Collision suffixes and
  the existing-backup tests address the original same-second overwrite.
  Backup verification and migration validation improve the failure paths.
- Retry operations propagate through `error_retry`, the command failure context,
  `retryInOrder`, the component guard and conditional toast dismissal. Unknown
  failures without an operation context have no Retry. The helper tests exercise
  order, early failure and double calls. There remains no automated React click
  test of the actual Home/Toast components. That is a coverage limitation.
- Permission classification and sanitization tests use the original engine
  strings and original path/parser/OS-error fixtures. Settings recovery technical
  text is separated from its summary. The updated audio release note accurately
  identifies missing physical-headset verification.
- Process liveness now uses one handle, and missing creation metadata can fall
  back to PID-only identity. Denied queries count as alive, preserving ordinary
  focus-change deduplication. Actual protected-process behavior and PID reuse
  while identity remains unavailable were not reproduced. The handoff describes
  a possible permanently stale PID-only session. Treat that as an unverified
  limitation requiring bounded reconciliation or further qualification.
- The release note's promise of immediate relaunch notification still exceeds
  the confirmed behavior above. Revise it with the fix. The claim that a problem
  stays visible until Retry works is supported for failed retries, although
  existing toast overflow and explicit dismissal can still remove notices.

## Independent validation

| Check | Result |
| --- | --- |
| Rust with desktop features and workspace TEMP/TMP | 185 passed, 0 failed, 1 filtered registry test, 1 ignored opt-in microbenchmark |
| Frontend | 61 passed |
| TypeScript and production frontend build | Passed through `npm run build` |
| Python | 9 passed |
| Formatting | Passed |
| Clippy, desktop/all targets, `-D warnings` | Passed |
| Independent filesystem and process-identity probes | Serial originals preserved. Concurrent valid settings lost. Distinct process notice suppressed permanently in the two-second case. |

The registry test was excluded because it is sandbox-restricted, as established
in the initial review. The ignored polling microbenchmark was not run. Native
updater E2E, physical audio/screen capture, graphical benchmarks and installer
operations were not run under this re-review's authorization limits. Existing
Rust tests used isolated synthetic sources. No physical hardware result is
claimed from them.

The added Rust tests use actual Windows sharing behavior, real bytes, fixed
timestamps and a refused migration-save destination. They substantially improve
coverage of the original defects. The blocked-copy test uses exhausted fixture
names to force failure. It does not exercise arbitrary mid-copy faults. The
game tests exercise several real state transitions but miss the demonstrated
under-three-second transition. Neither concurrency reproduction above appears
in the new test set.

Logs are `.tmp-alpha9-rereview-*.log`. The independent harness and its fixtures
are under ignored `test-output/alpha9-rereview/`. Library compilation generated
a cache hard-link fallback warning, and the isolated harness generated a source
lint warning. The application's Clippy check passed separately.

## Candidate artifacts

The current installer is
`src-tauri/target-alpha9/release/bundle/nsis/REPLAYX_0.3.1-alpha.9_x64-setup.exe`.
Its independently verified size is **61,379,005 bytes** and SHA-256 is
`4ef312ffd77499e20a363e10bd621b4a0714e7ee94bb166a8d21197b335039a1`.

The detached updater signature verifies for exactly `0.3.1-alpha.9` using
REPLAYX's public key. `latest.json` contains the same signature and expected
Alpha.9 download URL. SHA256SUMS.txt matches the full digest. Package manifests,
Cargo.lock, frontend version and Tauri version agree. The release executable's
FileVersion and ProductVersion both read `0.3.1-alpha.9`. Authenticode reports
NotSigned, consistent with the notes.

The superseded installer exists under
`src-tauri/target-alpha9/superseded-051f935/` and retains SHA-256
`d19553ece521a91e7e1ce06d558ba5a1ac6eacd5825b477aec3423d22b57e7d1`.
It is outside the current bundle directory. The inspected manifest/release
scripts select nonrecursive installer files from an explicit bundle directory,
so those selectors do not include the superseded candidate. A manual publisher
must still select the current artifact by its full path and digest. Subsequent
code fixes require a rebuilt/resigned candidate and regenerated metadata.

## Handoff decision

Three original findings are resolved in the reviewed implementation. Settings
preservation and new-session notification handling remain incomplete in the
confirmed cases above. Correct those cases and add their regression tests before
requesting a passing final-validation verdict.

Application code was not edited. This review created only this report and
ignored local test fixtures/logs. No commit, push, merge, installation or
publication was performed.

## Authorized implementation follow-up — November 13, 2026

After the review, the owner requested fixes for both confirmed defects.

- `set_aside` now distinguishes a moved original from a copied snapshot.
  `DamagedCopied` recovery keeps the engine's existing persistence guard active.
  Its UI message identifies the saved backup and explains that restarting after
  releasing the original will load current settings or finish recovery. Both
  new copies and reused copies follow this rule. Successful move-based recovery
  continues to support saving defaults.
- The announcement guard records the previous process identity with its time.
  A distinct identity bypasses that guard. Same-process liveness glitches retain
  suppression, and launcher handoffs retain their existing session behavior.
- Added regressions exercise a newer valid source after copying, all normal
  settings write paths, backup reuse, successful saving after restart, completion
  of recovery after releasing a damaged original, one-second relaunch, two-second
  PID reuse and later focus changes. The frontend test checks the copy-only
  recovery message and diagnostic separation.

Validation passed with 188 Rust tests, 62 frontend tests, nine Python tests,
TypeScript/production frontend build, formatting and Clippy with `-D warnings`.
The sandbox-restricted registry test remained filtered and the opt-in benchmark
remained ignored. Rust emitted incremental-cache hard-link fallback warnings.
Application lint checks passed.

The independent concurrent-writer harness was rerun against the fixes. Normal
engine persistence refused the destructive write and the newer valid settings
survived. The distinct process received its two-second notice. Evidence is
`test-output/alpha9-rereview/fixed-results.log`.

The installer reviewed above predates these implementation changes. Rebuild and
resign it, regenerate its manifest/checksums, and perform the approved manual
installation/update validation before distributing a candidate containing them.
No installer or hardware-capture test was performed during this fix.
