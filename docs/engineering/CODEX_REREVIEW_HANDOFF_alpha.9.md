# Re-review handoff: REPLAYX 0.3.1-alpha.9, review fixes

**Audience:** Codex, re-reviewing the fixes for its findings in
[CODEX_INDEPENDENT_REVIEW_alpha.9.md](CODEX_INDEPENDENT_REVIEW_alpha.9.md)
(which reviewed `051f935`).

**Scope:**

- Branch `codex/performance-0.2.1`, local only. The commits to review are
  `b5979f6..910bd56`; `b5979f6` only adds your review, unchanged.
- Nothing was pushed, merged, published or installed.
- The alpha.8 installer is untouched (SHA-256 below). The candidate you
  reviewed was moved, not deleted, to `src-tauri/target-alpha9/superseded-051f935/`.
- The capture and recording pipeline did not change.

## 1. Findings

### P1: settings recovery could destroy the original or an earlier backup

**Root cause.** `load_or_recover` treated every `load()` error the same way:
it renamed the file with `fs::rename`, which replaces the destination, under a
name with one-second resolution. If the rename failed, it still returned the
defaults, and the next save replaced the original. A failed migration write
inside `load()` also counted as "unreadable".

**Fix** (`model.rs`, `engine.rs`, `desktop.rs`):

- `load_checked` tells apart:
  - `Unreadable`: the file exists but couldn't be read. Sharing violation,
    lock violation and access denied are retried 5 × 100 ms first.
  - `Damaged`: parse or validation failure; carries the bytes that were read.
- Validation now runs before a migration is written back. A failed migration
  write is logged, and the migrated settings are still used.
- `set_aside` preserves a damaged file:
  1. It moves the file with `MoveFileExW` *without*
     `MOVEFILE_REPLACE_EXISTING`, trying `settings.unreadable-<secs>.json`,
     then `-1`, `-2`… up to 1000 names. On error 80/183 it tries the next
     name.
  2. If the move fails (the file is held without delete sharing, or a
     concurrent start already moved it), it reuses an existing backup with
     identical bytes. Otherwise it creates a copy with `create_new` (no
     replace), `sync_all`, reads it back and compares bytes. On a mismatch
     it removes only that new file.
- `SettingsProblem` (serialised in `Status.settings_problem`) has three kinds:
  - `damaged {backup}`: preserved; saving is allowed.
  - `damaged_not_kept`: saving is refused.
  - `unreadable`: saving is refused.
- `Engine::report_settings_problem` sets `settings_read_only`. Every settings
  write in the engine goes through `Inner::persist`, which refuses while that
  is set. There are seven call sites, and no other code writes `settings.json`.
- Concurrent start: backup names are unique and never replaced, and an
  identical existing backup is reused. The engine's instance lock is taken
  after `load_or_recover`, so a losing second instance can at most create or
  reuse a backup. It never writes `settings.json`.

**Tests:**

- `model::tests`:
  - `damaged_settings_are_set_aside_byte_for_byte_and_defaults_used`
  - `invalid_values_count_as_damaged_too`
  - `two_recoveries_in_the_same_second_keep_both_files`
  - `an_existing_backup_is_never_replaced`
  - `a_settings_file_in_use_is_left_alone_and_must_not_be_saved_over`
    (share mode 0)
  - `a_damaged_file_that_cannot_be_moved_is_copied_exactly` (share read
    only; also checks reuse on the next start)
  - `a_damaged_file_with_no_copy_possible_must_not_be_saved_over`
  - `a_migration_that_cannot_be_written_back_keeps_the_settings`
  - `a_first_start_has_nothing_to_report`
- `engine::tests::a_settings_file_that_was_locked_at_start_is_never_saved_over_later`:
  the file is locked at start-up and released afterwards. Then
  `set_settings`, `set_updates`, `set_playtime`, `mark_tour_done` and
  `mark_tray_hint_shown` all run, and the file stays byte-identical.

### P2: Retry turned a failed recording into replay buffering

**Root cause.** `ErrorNotice` always ran `go('start_replay')`, and the toast
was dismissed whatever the result.

**Fix:**

- **Engine.**
  - `Status.error_retry` lists the actions that redo what failed:
    `start_replay`, then `start_recording`.
  - `fail_final` derives it from `failed_intent` (start failures) or from the
    replay and recording state. The start watchdog sets it from `Starting`.
  - It is empty for recovery-loop errors (recovery retries by itself) and for
    save failures.
- **Frontend.**
  - `describeError(raw, {retry})` replaces the rule's retry placeholder with
    the exact operations ("Retry recording", "Retry Instant Replay",
    "Retry"). With no known operation, no Retry is offered.
  - Toasts from `go(name)` carry `[name]` for starts and `save_replay`.
  - `retryInOrder` stops at the first failure.
  - `createGuard` plus disabled buttons (and Retry disabled while another
    action is busy) prevent double runs.
  - The toast is dismissed only when the action succeeded. A failed retry
    bumps the count on the same notice (same key) and updates it.

**Tests.** In `tests/errors.test.ts`: "Retry repeats exactly what failed…" and
"a Retry runs the failed operations in order, stops at a failure and ignores
double clicks". The engine tests `failed_start_returns_control…` and
`start_watchdog_gives_control_back…` now also assert `error_retry`.

There is no React component test harness. `ErrorNotice` wiring was checked by
TypeScript and a headless render, not by an automated click test.

### P2: packet starvation shown as a confirmed permission denial

**Root cause.** The permission rule matched the word "permission", which the
engine's starvation advice contains.

**Fix.** `microphone-permission` now requires denial evidence: `access is
denied`, `access denied`, `0x80070005`, `e_accessdenied`, `permission denied` or
`blocked by privacy`. A new rule maps packet starvation to *Microphone
stopped*: a microphone category with Fix microphone, Microphone privacy
settings and Retry.

**Tests.** The engine's real wording for each case: starvation, denial
(`Access is denied. (0x80070005)` inside `(Technical: …)`), unplugged
(`Element not found` after the settle window), busy (`0x88890026`) and the
start-up timeout ("it did not respond").

### P2: a quick relaunch suppressed its new-session notice

**Root cause.** The 15 s `ended` grace map continued any session of the same
game name.

**Fix** (`overlay.rs`, `games.rs`, `desktop.rs`):

- The grace map is removed. A process is a new session unless another
  process of the same game is still running (launcher hand-off, helper, or
  a self-restart that overlaps).
- `FLAP` (3 s) never announces the same game twice. It is only a guard
  against a flickering liveness check.
- `process_running` uses one handle:
  - `ERROR_INVALID_PARAMETER` (no such process), a different creation time
    or an exit code ends the session.
  - A process that can't be opened or queried counts as still running.
    Treating it as ended would repeat the notice once a second while it is
    in front.
- Your remaining-scope note: `process_started(pid)?` suppressed the
  candidate. It now falls back to creation time 0, meaning "unknown, the id
  alone identifies it".

**Tests:**

- `overlay::tests`:
  - `a_quick_relaunch_is_announced_again` (gaps of 4, 5, 10 and 14 s, then
    no repeat across Alt+Tab)
  - `a_relaunch_seen_without_a_gap_in_front_is_announced_too`
  - `a_launcher_hand_off_continues_the_session`
  - `a_reused_process_id_is_a_different_process` (reuse after 4 s)
  - `a_liveness_glitch_right_after_the_notice_does_not_repeat_it`
  - `games_relaunch_independently`
- `games::tests::process_identity_follows_the_real_process_lifetime`, which
  adds: closed handle, no such process, and creation time 0.

**Known trade-offs, for your judgement:**

- If a process becomes unqueryable *after* it was first seen and then exits
  while its id is reused by another unqueryable process, that game is not
  announced again until REPLAYX restarts. This is a missing notice, not
  repeats.
- A game that restarts itself by starting the new process before the old
  one exits continues the old session.

### P2: technical details in simple errors

**Root cause.**

- `plainSentence` stripped only `(Technical: …)` tails and `0x` codes.
- The settings note embedded `{e:#}` and rendered as a plain banner.

**Fix:**

- Unknown errors keep their first sentence only if the *whole* message has
  no technical markers:
  - path separators;
  - `:` `;` braces, quotes and other code characters;
  - `0x…`, `os error`, `error <n>`;
  - `line <n>` / `column <n>`;
  - file extensions.
- Otherwise they show "REPLAYX hit an unexpected problem. The technical
  details below can help if you report it."
- The settings notice is `describeSettingsProblem(Status.settings_problem)`,
  rendered with `ErrorNotice`. `technical` appears only under *Technical
  details*.
- The read-only refusal message maps to a plain `settings` rule.

**Tests.** Paths (drive, UNC and POSIX), decimal `os error`, JSON parse
output, nested `a: b: c` chains and `{"code":5}`. Settings notices never
contain `os error`, `line`, `column`, `AppData` or a backslash.

## 2. Verification (on the owner's PC; a game was running, so all builds and tests ran at BelowNormal priority)

| Check | Result |
| --- | --- |
| `cargo fmt --check` | OK |
| Clippy `--all-targets --features desktop -D warnings` | OK |
| Rust `cargo test --features desktop` | 186 passed, 0 failed, 1 ignored (opt-in microbenchmark) |
| TypeScript `--noEmit` | OK |
| Frontend `npm run test:frontend` | 61/61 |
| Production frontend build (`npm run build`) | OK |
| Python `unittest discover -s tests` | 9/9 |
| Headless Chrome render of the mocked preview (`?state=error&settings=damaged`, `?settings=unreadable`) | Both notices render in plain words; Retry is labelled *Retry Instant Replay*; the OS error appears only in *Technical details* |
| Native updater end-to-end (`dev/update-e2e.mjs`) | **Not re-run**: it opens a window and the owner was playing. Updater code is unchanged since the 42/42 run. |

**Test counts.**

- 173 (handoff) vs. 172 (your run): yours excluded the sandbox-denied
  `startup_entry_round_trips_in_a_scratch_key`; the same 173 tests passed
  otherwise. Your first run's remux failure came from the `#!sandbox` TEMP
  path.
- Now 186: model tests +8 (9 new, 1 replaced), engine +1, overlay +4 (6 new,
  2 replaced). All counts need `--features desktop`; without it the 4
  `desktop.rs` tests are not compiled.

**Sandbox blockers vs. real failures.** In this environment nothing failed.
The registry test and the updater end-to-end test passed or are runnable
here. In your sandbox they are blocked by environment permissions
(`tasklist`, registry), not by defects.

## 3. Not verified (hardware or manual)

- Physical Bluetooth and USB device switching, and a real `0x88890004`. The
  release notes now say this was tested with simulated errors only.
- Your audio-routing note: there is no explicit default-endpoint
  notification. That is an inherited risk, unchanged here.
- Protected or anti-cheat games for `process_running` and the creation-time
  fallback.
- A real damaged or locked `settings.json` in the installed app; this was
  unit-tested with real Windows file sharing.
- Install, launch after install, and update from alpha.8.
- Multi-monitor notice placement.

## 4. Artifacts (`src-tauri/target-alpha9/release/bundle/nsis/`)

Built from `910bd56` with `npm run tauri -- build --config
src-tauri/tauri.updater.conf.json` and `CARGO_TARGET_DIR=src-tauri/target-alpha9`.
The updater key is read from the owner's local key folder and never printed.

| File | Value |
| --- | --- |
| `REPLAYX_0.3.1-alpha.9_x64-setup.exe` | 61,379,005 bytes, SHA-256 `4ef312ffd77499e20a363e10bd621b4a0714e7ee94bb166a8d21197b335039a1` (also in `SHA256SUMS.txt`) |
| `.exe.sig` | `verify-update-signature.mjs`: signed by REPLAYX's update key for version 0.3.1-alpha.9 |
| `latest.json` | version 0.3.1-alpha.9; URL `releases/download/v0.3.1-alpha.9/…`; its signature equals the `.sig` file |
| `release/replayx.exe` | FileVersion and ProductVersion 0.3.1-alpha.9 |

Other builds:

- **Superseded candidate** (the one you reviewed), now in
  `src-tauri/target-alpha9/superseded-051f935/`: SHA-256
  `d19553ece521a91e7e1ce06d558ba5a1ac6eacd5825b477aec3423d22b57e7d1`. Don't
  publish it.
- **Alpha.8 installer** (`src-tauri/target/release/bundle/nsis/`): SHA-256
  `1f420d0729454393cf17d7229d642eccd2894029bf513d91cec2e3aa5c7f8186`, the same
  before and after this build.

Authenticode: not signed. That is unchanged and stated in the release notes.

## 5. Please check

1. P1: any path that writes `settings.json` while `settings_read_only` is
   set; the error-code handling and name loop in `set_aside`; the
   identical-bytes reuse.
2. Whether the `FLAP` floor and "unqueryable counts as running" are the right
   trade-offs against your relaunch requirement.
3. Whether the `plainSentence` technical-marker list is too strict (plain
   engine messages that now read as generic) or too loose.
4. `error_retry` at every place where `status.error` is set (`engine.rs`).
