# Review handoff: REPLAYX 0.3.1-alpha.9 release candidate

**Audience:** an independent reviewer (Codex) checking this release candidate
before it is published.

**Scope:**

- Branch `codex/performance-0.2.1`, local only.
- Commits after `1210803` (WGC border), listed in section 4.
- Nothing has been pushed, merged, published or installed.
- The alpha.8 installer (`src-tauri/target/release/bundle/nsis`) and earlier
  test builds (`target-wgc*`) are untouched. The candidate builds into
  `src-tauri/target-alpha9`.

## 1. What changed, and where to look

| Area | Files | Decision / risk to review |
| --- | --- | --- |
| Friendly errors | `src/errors.ts`, `src/ErrorNotice.tsx`, `src/navigation.ts`, `src/context.tsx` (toasts), `src/pages/Home.tsx`, `tests/errors.test.ts` | Details below. |
| Windows Settings links | `desktop.rs` (`open_windows_settings`, `windows_settings_uri`) | Only five fixed `ms-settings:` URIs; anything else is refused (unit test). Confirm no path lets the UI pass an arbitrary URI. |
| Game-session notice | `overlay.rs` (`Announcer`, `ProcessId`), `games.rs` (`process_started`, `process_running`), `desktop.rs` call site | Details below. |
| Notice redesign | `overlay.rs` (`draw_card`, `show`) | Details below. |
| Idle cost | `desktop.rs` sampler loop (`page_hidden()`, tooltip on change, `continue` before process scans), audio-device watcher interval, `set_page_visible` (WebView2 `MemoryUsageTargetLevel`), `src/store.ts` (5 s hidden poll, `page-hidden` class), `style.css` | Details below. |
| Audio settle | `audio.rs` (`DEVICE_SETTLE` = 10 s, backoff capped at 1 s; ready timeout = settle + 4 s) | Details below. |
| Settings recovery | `model.rs` (`Settings::load_or_recover`), `desktop.rs` setup | An unreadable file is renamed `settings.unreadable-<time>.json`, never deleted. The app starts with defaults and shows a notice. Check that nothing writes the defaults over a file that merely failed to rename. |
| UI fixes | `src/pages/Performance.tsx`, `Games.tsx`, `Clips.tsx`, `style.css` | The `.stat-grid` rules moved to `.game-stats`. Check the Games page layout. |
| Version | `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `src/api.ts` | 0.3.1-alpha.9 in all seven places. |
| Repository | `docs/engineering/`, `docs/release-notes/`, `README.md`, `CHANGELOG.md`, `scripts/updater-manifest.ps1` (`-BundleDir`, default unchanged) | References updated by script. Check for broken relative links. |

**Friendly errors.** Rule-based mapping from raw engine messages (substring
rules, the most specific first). Review:

- whether any important message falls into the wrong category: rule order,
  and the broad `capture` and `display` words;
- whether the Retry action (`start_replay`) is appropriate for every
  category that offers it.

**Game-session notice.** A session is keyed by process id plus creation time.
A second process of the same game, or a restart within 15 s, continues the
session. Review:

- `process_running` uses `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`.
  Protected or anti-cheat processes may refuse it; they would then count as
  ended. The effect would be an extra notice, never a missing one.
- The liveness checks are cheap and run once a second for a few tracked
  processes.

**Notice redesign.** GDI only, on the notice thread; the window is
click-through, never activated, and excluded from capture. Review:

- GDI object lifetimes in `draw_card` (every brush, font and pen deleted;
  the old objects reselected);
- `GradientFill` usage;
- the monitor chosen through `MonitorFromWindow(GetForegroundWindow())`.

**Idle cost.** Review:

- the `continue` skips only the CPU/memory sampling, which is the last part
  of the loop;
- the WebView2 cast to `ICoreWebView2_19` is best-effort: errors are
  ignored, and older runtimes keep the normal level;
- the window restore was verified, but only once, on this PC.

**Audio settle.** A device that keeps reporting change codes is retried with
the then-current default. Review the start latency in the worst case (about
14 s before the error) against the 60 s start watchdog.

## 2. Measurements (this PC: i9-13900H, RTX 4070 Laptop, Iris Xe)

| What | Before | After | How |
| --- | --- | --- | --- |
| `replayx.exe` CPU, idle in the tray | 1.92 s/min | 0.39–0.56 s/min | `--background`, isolated data folder, 60 s window |
| Process tree CPU, idle | 4.7% of one core | 2.7% | same |
| Process tree memory, idle | 367 MB | 178 MB | same (WebView2 memory target Low) |

Recording performance (WGC, Quick Sync, NVENC) did not change in this round.
For the real-game evidence and its limits, see
`FORTNITE_SESSION_ANALYSIS_2026-11-12.md`. The Fortnite overhead on the
Intel-driven panel (about 19% with WGC + Quick Sync) is **not** fixed.

## 3. Tests

Final results are in section 5. Not verifiable here:

- physical Bluetooth headset switching and a real `0x88890004` reproduction
  (the mapping and the settle logic are unit-tested; the hardware path is
  not);
- microphone permission prompts;
- exclusive full-screen games;
- multi-monitor placement of the notice (one monitor at a time was
  connected);
- the actual installer install and update from alpha.8. Install the alpha.9
  installer only with the owner's approval.

## 4. Commits

- `370aab9` Version 0.3.1-alpha.9 (local release candidate)
- `9c97a6d` UI fixes: Performance metric cards, capture line, empty library action
- `6de04d6` Open with defaults instead of failing silently when settings are unreadable
- `157b88b` Move dated engineering reports and local build notes out of the docs root
- `c879da5` Make an idle REPLAYX in the tray nearly free
- `c7a684f` Redesign the game-detected and capture notices
- `bf33dea` Friendly, actionable errors and a longer audio-device settle window
- `cbec044` Announce a detected game once per game session, not per focus change

## 5. Final verification

| Check | Result |
| --- | --- |
| `cargo fmt --check` | OK |
| Clippy, `-D warnings`, all targets | OK |
| Rust | 173 passed, 0 failed, 1 ignored (opt-in polling microbenchmark) |
| TypeScript `--noEmit` | OK |
| Frontend | 57/57 passed |
| Frontend production build | OK |
| Python (`tests/`) | 9/9 |
| Native updater end-to-end (`dev/update-e2e.mjs`, local test server, nothing installed) | 42/42 |
| 15-minute synthetic replay soak with two concurrent NVENC sessions | Details below |
| Signed installer build (`tauri build --config src-tauri/tauri.updater.conf.json`, `CARGO_TARGET_DIR=src-tauri/target-alpha9`) | OK: `REPLAYX_0.3.1-alpha.9_x64-setup.exe` (61,403,275 bytes) and `.sig` |
| `verify-update-signature.mjs` | Signed by REPLAYX's update key for version 0.3.1-alpha.9 |
| `updater-manifest.ps1 -BundleDir …` | `latest.json` points at `releases/download/v0.3.1-alpha.9/…` |
| SHA-256 (installer) | `d19553ece521a91e7e1ce06d558ba5a1ac6eacd5825b477aec3423d22b57e7d1` |

The soak results:

- 54,075 frames at 60.01 FPS, 0 dropped.
- Saves ready in 607 / 605 ms; both clips decode clean.
- Memory 461 → 463 MB; no leftover buffer files.

**Not re-run** (capture code unchanged in this round):

- The GPU codec matrix and the WGC soak. Their last results are in
  `FORTNITE_SESSION_ANALYSIS_2026-11-12.md` and
  `RECORDING_OVERHEAD_ROOT_CAUSE_2026-11-12.md`.
- Graphical benchmarks need the owner's approval.

**Not done:**

- Installing the installer, or a real update from alpha.8.
- The FFmpeg corresponding-source package. The local copy in
  `release/ffmpeg-source/` matches the pinned runtime (b204e94e90) and can be
  attached.
- Publishing.

**Artifacts** (`src-tauri/target-alpha9/release/bundle/nsis/`):

- `REPLAYX_0.3.1-alpha.9_x64-setup.exe`
- `.exe.sig`
- `latest.json`
- `SHA256SUMS.txt`
