# Performance investigation: continuation of Codex's work (November 11, 2026)

**Bottom line: the Fortnite FPS problem is not shown to be fixed.** No gameplay
frame times were measured, because PresentMon needs an administrator token, and
Fortnite was not in the foreground during this session. The synthetic
measurements below explain *where* REPLAYX's overhead comes from. They found no
change that measurably reduces it. No production capture or encoder code
changed, so there is **no 0.3.1-alpha.9**. Alpha.8 remains the current and
rollback build.

## 1. What Codex completed

The following were already committed as `2b7f0f3` and `970ac3b` (alpha.8):

- Reduced replay polling cost.
- GPU experiments: priority class, lookahead, AQ and split graphs.
- Signed alpha.8 artifacts.

Codex also had uncommitted work in progress. It is described in
[docs/engineering/FORTNITE_LIVE_PROFILE.md](FORTNITE_LIVE_PROFILE.md):

- A live Fortnite resource profile. It covered NVIDIA device telemetry and
  process CPU/memory only; no frame times were measured.
- An audit of the pinned FFmpeg sources (`vsrc_ddagrab.c`, `nvenc.c`,
  `ffmpeg_enc.c`).
- `replayx-dda-profile`, which runs Desktop Duplication alone and compares two
  ReleaseFrame policies.
- p95, variance, spike and GPU-wait metrics in `compare-frametimes.py`.
- A live-profile collector with tests.
- A partial fix to the updater end-to-end test.

## 2. Recovered work

All of Codex's uncommitted files were kept unchanged in intent, then verified
and committed locally:

| Commit | Content |
| --- | --- |
| `6bdf107` | Updater start-up race fix, and the updater e2e readiness fixes (Codex's `untilUI` plus mine) |
| `882bc20` | Codex's profiling tools (`dda_profile.rs`, `live-game-profile.py`, frame-time metrics, tests), plus presentation-mode reporting and the input-invalidation check |
| (this commit) | Foreground guard for screen-capturing test scripts, a software-fallback benchmark config, and this report |

Nothing was reset, discarded, pushed or merged.

## 3. The two updater test failures

**Symptoms (alpha.8).** In `dev/update-e2e.mjs`, "status at start" failed. In
some runs, the red update dot and the pop-up were missing after a check.

**Root cause 1, in the test.** The test treated "the WebView debug target
exists" as "the app is ready". The page actually stays on `about:blank` until
the Vite dev server answers. Measured on three runs:

| Run | Debug target appeared | App page loaded |
| --- | ---: | ---: |
| 1 | 40 s | 69 s |
| 2 | 3 s | 65 s |
| 3 | 3 s | 66 s |

The old loop gave up quietly and continued, so the first checks ran against an
empty page.

- **Fix:** wait (5 minutes at most) until the page is the app, the Tauri bridge
  exists and the navigation has rendered.
- Codex's `untilUI` waits for the rendered condition instead of a fixed 600 ms
  sleep; it is kept.
- The test now also stops immediately if `tauri dev` exits. Before this, a
  stale dev server holding port 1420 made it wait silently for 20 minutes.

**Root cause 2, in the app.** `startUpdates()` subscribed to `update-status`
events and read the status snapshot at the same time. An event arriving between
the two could be missed. A slower snapshot could also overwrite a newer event,
for example "available" replaced by an older "checking". The result was no dot
and no pop-up.

- **Fix:** `UpdateStatus` carries a monotonic `revision`, increased by the
  engine on every change and kept across resets.
- The window subscribes first, then reads the snapshot.
- The window never applies a status older than the one shown.
- A unit test covers out-of-order delivery.

**Result.** 42/42 checks passed on each of 3 consecutive full runs. The test
timeouts were not raised and no assertion was removed.

## 4. Research findings

- **Desktop Duplication and composition (measured here).** As soon as a
  duplication session acquires frames, Windows stops showing the full-screen
  fixture on a hardware overlay plane. DWM composes every frame instead:
  100% "composed" in every capture run, against 100% "overlay" once settled
  without capture. The data comes from `IDXGISwapChainMedia::GetFrameStatisticsMedia`,
  which the game proxy now samples every 32 presents.
- **ReleaseFrame timing.** Microsoft recommends releasing just before the next
  acquisition. FFmpeg's ddagrab releases early. In this measurement, holding
  the frame gave no FPS benefit. It also blocks the acquiring thread for about
  1.2 ms on average, against 0.09 ms with early release. The GPU copy span is
  tiny either way: 0.043 ms (hold) against 0.056 ms (early).
- **NVENC.** The encoder works on its own VideoEncode engine (about 20% of the
  engine at 1080p60). FFmpeg's own 3D work is about 1.1%. Synchronous bitstream
  retrieval (`enableEncodeAsync=0`) did not show up as a measurable game-FPS
  cost, because capture without any encoder costs the same.
- **Display adapter.** On this date the only active display was 1920x1080 at
  250 Hz, driven by the RTX 4070. All GPU counters were on one adapter. The
  Optimus (Intel-composited) configuration from earlier sessions did not apply.
  Results may differ on the Intel-driven panel.
- **Not available within the rules.** The two ways to avoid composition are
  in-process game capture hooks (what OBS "game capture" does, needing code
  injection into the game; forbidden here and risky with anti-cheat) and
  NVIDIA's own capture interfaces (NvFBC is not offered to third-party apps on
  GeForce GPUs). Windows Graphics Capture also captures from the DWM composition
  pipeline. It was not measured, so no claim is made about it.

## 5. Confirmed bottleneck (synthetic)

**The cost is the Desktop Duplication session itself, which forces DWM
composition of the game's frames. It is not NVENC, FFmpeg scaling or encoder
settings.**

Fixture: `replayx-gameproxy`, full-screen, about 900 FPS. The "A/B/A drop"
compares the capture window with the mean of the windows before and after it.
Only runs with control drift within 5% and no user input count as valid.

| Configuration (encoder?) | Valid runs | A/B/A FPS drop per valid run | Mean | Fixture presentation during B |
| --- | ---: | --- | ---: | --- |
| No capture (control) | 3 | −1.9, 3.5, −5.3 | −1.3% | overlay (settling) |
| Duplication only, early release (no) | 2 | 3.3, 8.9 | 6.1% | 100% composed |
| Duplication only, hold frame (no) | 2 | 4.9, 4.2 | 4.6% | 100% composed |
| REPLAYX default: H.264 NVENC (yes) | 4 | 4.9, 3.1, 4.6, 5.6 | 4.5% | 100% composed |

Run-to-run noise on the control is about ±4%. That is why the policy
difference (early against hold) is not significant: both are within noise of
full capture.

The GPU draw time of the fixture's own frames did not change (0.73 ms in every
window). So the game's rendering is not slowed. The loss is in getting frames to
the screen.

GPU counters, one profiling run per configuration (separate from the FPS runs):

| | No capture | Duplication only | Default H.264 capture |
| --- | ---: | ---: | ---: |
| DWM 3D engine | 0% | 0.98% | 1.19% |
| DWM CPU | 0.26% | 1.76% | 2.09% |
| FFmpeg 3D engine | — | — | 1.13% |
| FFmpeg VideoEncode engine | — | — | 19.8% |

DWM's work appears as soon as duplication starts, with or without an encoder.

Fallback encoders cost more than the default NVENC path. These are single
valid runs, so they are indicative only:

| Encoder | A/B/A FPS drop |
| --- | ---: |
| QSV | 7.2% |
| CPU scaling plus NVENC | 8.2% |
| OpenH264 (software) | 12.0% |

The default stays NVENC.

## 6. Optimizations made

**No production capture or encoder change was made, because none was supported
by evidence:**

- Holding the duplication frame (Microsoft's recommendation) showed no gain.
- Encoder settings: not changed. Bitrate, lookahead, AQ and preset were left
  as they are, because the encoder is not the bottleneck.
- Removing ddagrab's internal copy would save about 0.02 ms of GPU per frame
  (Codex's probe). That is not meaningful, and the pinned FFmpeg binary
  cannot be patched safely.

Changes that were made:

- The updater race fix (section 3).
- Measurement safety and validity:
  - The benchmark and codec-matrix scripts now **refuse to record unless the
    test window is in the foreground**. During this session Windows' foreground
    lock left VS Code in front, and three fallback runs recorded the editor
    instead of the fixture. Those runs were discarded and re-run.
  - Runs with keyboard or mouse input are marked invalid.
  - Presentation modes are reported.
  - A software-fallback config was added.

## 7. Before/after numbers

| Item | Before | After |
| --- | --- | --- |
| Updater e2e | 2 failing checks (alpha.8) | 42/42 × 3 consecutive runs |
| Synthetic capture overhead | 5.0–7.7% (Codex, alpha.8) | 3.1–5.6% today, mean 4.5% |
| Fortnite FPS | unmeasured | unmeasured |

The synthetic overhead row is the **same capture code**. The difference is
run-to-run and day-to-day variation, **not an improvement**.

## 8. Tests

**Passed:**

- Rust: 161 passed, 1 ignored (an opt-in polling microbenchmark).
- Clippy with `-D warnings`.
- `cargo fmt --check`.
- Frontend: 52/52.
- TypeScript (`tsc --noEmit`).
- Frontend production build.
- Python: 9/9 (`test_frametimes`, `test_live_profile`).
- Updater e2e: 42/42 × 3.
- 15.7-minute synthetic replay soak with two concurrent NVENC sessions:
  - 54,074 frames at 60.01 FPS, 0 dropped.
  - Both saves ready in 718 ms; both clips decode clean.
  - Memory 458 → 461 MB; no leftover buffer files.
- `record-self-test`: recording stop hands over in about 1 s, replay keeps
  running, and the concurrent save is ready in 3.4 s.
- `save-bench`.
- Fallback encoders (QSV, CPU-scale plus NVENC, OpenH264): working clips with
  0% repeated frames.
- GPU capture with NVENC H.264: clips at 1920x1080, 0.0–0.14% repeated frames.
- Codec matrix behind the fixture: 6/6 cases.
  - H.264, HEVC and AV1 on NVENC with the GPU zero-copy path.
  - 1280x960 and 1440x1080 regions.
  - The "exact" policy falls back to CPU scaling, the same as alpha.8.
  - Saves took 0.8–1.4 s. Both clips of every case decode clean.

**Not run:**

- Physical A/V sync. The flash/beep stimulus must play through the speakers,
  and the user was away.
- Microphone tests.
- Discord or NVIDIA replay coexistence.
- Live Fortnite.

**Discarded as invalid:**

- The first fallback runs (the fixture was not in front).
- The first two matrix attempts (`numpy` was unavailable to the Python used;
  re-run with `D:\Python\Python313\python.exe`).

## 9. Remaining limitations

- **No gameplay frame times exist.** Synthetic results are at about 900 FPS on
  a light fixture. The composition cost per frame is small in absolute terms
  (about 0.05 ms), but it can matter more at very high frame rates, and with
  Fortnite's own presentation path. Neither was measured.
- Whether Fortnite itself loses independent flip or overlay presentation while
  REPLAYX runs is unknown. PresentMon's `PresentMode` column answers this
  directly.
- The Intel-driven panel configuration was not tested today.
- Avoiding composition entirely needs a different capture method (injection
  hooks or private NVIDIA APIs). Both are out of bounds.

## 10. Executables

- Alpha.8 app (unchanged, rollback):
  `src-tauri\target\release\replayx.exe` (17:38) and
  `src-tauri\target\release\bundle\nsis\REPLAYX_0.3.1-alpha.8_x64-setup.exe`.
- Diagnostic tools built today:
  `src-tauri\target\release\replayx-cli.exe`, `replayx-gameproxy.exe` and
  `replayx-dda-profile.exe`.
- The app source contains the updater fix. It is **not** in any installer yet.
  It needs a new build, which is a release decision.

## 11. Manual Fortnite benchmark

Follow [FORTNITE_ABA.md](FORTNITE_ABA.md). In short:

1. Run PresentMon **as administrator** (your own decision), targeting
   `FortniteClient-Win64-Shipping.exe` only.
2. Use the same Creative scene and settings throughout. Warm up for 2 minutes.
3. Record three phases, 60 s each, one CSV per phase:
   - A1: REPLAYX replay off.
   - B: replay on.
   - A2: replay off.
4. Repeat the whole sequence three times.
5. Compare with
   `py -3 scripts/compare-frametimes.py A1.csv B.csv A2.csv --process FortniteClient-Win64-Shipping.exe --cap 360`.
6. **Also check PresentMon's `PresentMode` column.** If A1/A2 show "Hardware:
   Independent Flip" or "Hardware Composed" and B shows "Composed: Flip", the
   composition effect measured here applies to Fortnite too.

## 12. Git state

- Branch `codex/performance-0.2.1`, local only.
- Commits after alpha.8 (`970ac3b`): `6bdf107`, `882bc20` and the commit
  adding this report.
- `main` was not touched. Nothing was pushed, merged, published or installed.
