> **2026-11-16 follow-up:** [Measured motion improvement, physical audio, 90-second retention and remaining counterexamples](NATIVE_V2_VALIDATION_2026-11-16.md). The opt-in candidate has clean 600/480-frame recording/replay evidence and 0.370% recorder CPU. Mixed-desktop and capture-burst cases remain limitations; defaults are unchanged. Results below are historical.

> **Latest follow-up:** [Live phase-lock validation and bounded-recovery candidate](NATIVE_V2_PACING_LIVE_2026-11-15.md). Earlier NVIDIA 60/59.94 app/replay tests passed, a 30 FPS boundary defect and redundant-capture sealing were fixed, and the final candidate is rebuilt. Final GPU/app/audio acceptance is pending while the user is gaming. Defaults remain unchanged. The results below are historical.

# Native V2: 60 FPS pacing, CFR against VFR (November 15, 2026)

Branch `performance/capture-replay-stutter-optimization`, worktree
`replayx-perf`. Continues `NATIVE_SMOOTHNESS_2026-11-15.md` (Codex).

**The defaults are unchanged.**
- The production recorder stays FFmpeg.
- The native engine stays alpha-only (`REPLAYX_NATIVE_ALPHA=1`), with `clocked`
  pacing on NVIDIA and `legacy` on Intel.
- The two experiments can only be switched on explicitly:
  - `REPLAYX_NATIVE_TIMESTAMP_TEST=phase-lock` (new);
  - `REPLAYX_NATIVE_TIMESTAMP_TEST=capture-vfr` (Codex).

**Nothing was run on screen.**
- No game was launched.
- No window, overlay or full-screen test was used.
- Every result below comes from recorded traces, modelled traces or generated
  GPU textures.

## 1. Where Codex stopped

Codex's working tree (17 modified files, 49 new entries) was backed up first
(zip and diff, outside the repo), then checked rather than assumed.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | clean |
| `cargo clippy --features desktop --all-targets -D warnings` | clean |
| `cargo test --features desktop` | 204 passed, 6 opt-in ignored; native worker 28 passed |
| Release build of all binaries | builds |
| Codex's last logs | app tests 5/5 passed (17:08); the CFR failure evidence (`native-grid-v2/exact-failure`); three VFR app runs |

**No partially applied edit or broken build was found.** Codex's last
attempted step was the CFR investigation. Its conclusion stands:
- the 60 FPS grid **failed** phase-independent smoothness;
- timestamp-preserving VFR was added as an explicit experiment.

## 2. Why the 60 FPS grid repeats and skips

The grid shows, in output slot *n* (PTS exactly *n*/60 s), the newest
capture at or before its deadline. Today the deadline is the slot's grid time
counted from the **first capture**.

From Codex's 14 saved real traces, which have per-capture source frame IDs:

- **New pictures arrive in a cluster.** At 250 Hz they arrive about −1.7 to
  +6 ms around each 16.67 ms slot (compositor vblank quantization plus
  jitter). The rest of the period is almost empty.
- **The first capture is always a new picture.** So every deadline sits on an
  arrival time, inside the cluster. Whether that is harmless or a failure is
  chance: it depends on where in its jitter window the first picture landed.
  - Codex's failing run: 71 repeats and 72 lost pictures.
  - Another run: 0.
- **Most fixed phases are fine; a band is terrible.** Across the moving
  traces, the median fixed phase loses 0–9 pictures and the worst band
  77–148. **A fixed
  offset fails** on other traces: half a period loses 78 and 98 pictures on
  two of them.
- **Redundant captures hide the gap.** About half the captures repeat the
  previous picture (other screen activity). They arrive 4, 8, 12 and 16 ms
  after each new picture, filling the empty part of the period. This is why a
  timestamp-only phase estimate failed for Codex.

## 3. Phase-locked 60 FPS CFR (experimental): `bin/native/phase.rs`

Each slot's deadline is its grid time plus an offset. The offset is chosen
from the new pictures that arrived before the previous slot's deadline. This
is causal: a sealed slot is never re-decided.

**Output PTS are not touched.** They stay exactly *n*/60 s.

**Rule with change hints** (each capture says whether it is a new picture):
1. For each of 128 deadline phases, count how many of the last 60 new
   pictures that phase would have lost (two inside one slot).
2. If the current phase would have lost max(1, best ÷ 4) more than the best
   phase, move to the best phases' widest margin.
3. Otherwise keep the deadline away from where pictures arrive: a density
   cost with a 4.2 ms triangle. It moves only on a clear difference, and only
   if a gap exists at all.

**Without hints** (older Windows), every capture counts. Then:
- only the density rule applies;
- it uses a 1 ms triangle, because redundant captures fill most phases.

**Why a move is safe.** Moving inside an empty gap crosses no arrival, so it
adds no repeat or skip.

**The offset stays bounded,** in [−T/4, 5T/4):
- Leaving that range wraps by one period. That is exactly the one repeat a
  59.94 Hz source needs per 1001 frames, or the one skip a 60.06 Hz source
  needs.
- The A/V offset is therefore bounded: a picture's capture time is within
  ±1.25 frames of its PTS. Today's grid gives −1 to 0 frames.

**Holds when there is nothing to lock to:**
- sources faster than 60 Hz;
- 50 Hz sources;
- static pictures;
- sources with no structure.

**Cost.** Memory is fixed: 60 arrivals. The work is at most about 12,000
arithmetic operations per output slot, with no waits and no GPU work; see §8.

**Where the hints come from:**
- **Windows 11 24H2 and later:** WGC dirty regions
  (`GraphicsCaptureDirtyRegionMode::ReportOnly`). A capture counts as new when
  its dirty regions cover at least 5% of the captured area, so the cursor, a
  caret or a clock does not count.
- **Older systems:** the timestamps-only rule above.
- **Offline replays:** the decoded source frame IDs. This models perfect
  dirty regions.

**Not yet verified:** whether real dirty regions behave like that model on
this PC (§10).

## 4. Evidence for the phase-locked grid

### 4a. GPU encode, then decode, on the 14 recorded traces

`scripts/bench/phase-lock-gpu-check.py`:
- Each real timeline is replayed through the worker's actual recording path:
  - generated D3D11 textures carrying the source frame ID as a 24-bit
    pattern;
  - fixed-function conversion;
  - the vendor's hardware H.264 encoder.
- FFmpeg then decodes every frame's ID.
- **Every run passed the integrity checks:**
  - decoded IDs equal the scheduler's choices, and equal the pure scheduler
    (`--pacing-replay`);
  - frame count; keyframes every 120; no B-frames;
  - rational 60 FPS timestamps;
  - 0 leaked encoder surfaces.

Pictures lost / repeated, counted from the **decoded** IDs, NVIDIA RTX 4070:

| Trace (source) | Grid today | Phase lock, change hints | Phase lock, timestamps only |
| --- | ---: | ---: | ---: |
| app-mp4-clocked (60 Hz motion) | 88 / 89 | **0 / 1** | 0 / 1 |
| app-mp4-transport-final (Codex's failing run) | 72 / 71 | **0 / 0** | 2 / 2 |
| trace-unthrottled | 82 / 83 | **0 / 0** | 0 / 0 |
| timestamp-vfr-app2 | 34 / 34 | **1 / 1** | 11 / 11 |
| trace-baseline (irregular 12–36 ms source) | 24 / 56 | **12 / 43** | 47 / 78 |
| d3d-clocked | 2 / 2 | 2 / 2 | 0 / 0 |
| timestamp-vfr-app3 | 2 / 3 | 1 / 3 | 0 / 1 |
| trace-clocked | 1 / 2 | 0 / 1 | 6 / 7 |
| app-mp4-clocked-ties | 0 / 0 | 1 / 2 | 4 / 4 |
| d3d-clocked-final | 0 / 0 | 0 / 0 | 5 / 5 |
| d3d-30fps (30 fps source; 240 repeats are correct) | 0 / 240 | 0 / 240 | 0 / 239 |
| d3d-static, timestamp-vfr-app1 (static) | 0 | 0 | 0 |
| d3d-legacy-before (starved source, pre-fix) | 36 / 106 | 36 / 106 | 35 / 104 |
| **Total lost** | **341** | **53** | **110** |

**Intel Quick Sync** (Iris Xe), the same 14 traces with and without hints:
- every integrity check passes;
- **every decoded count is identical to NVIDIA's (28 / 28).**

The scheduling is vendor-independent, and both hardware encoders reproduce
it exactly.

**Reading.**
- **With change hints:** 341 → 53 lost pictures. 36 of the 53 are the starved
  pre-fix source, which loses the same in every mode. It is never worse than
  today by more than one picture.
- **Timestamps only** (Windows before 11 24H2): 341 → 110. That is better
  overall, but **worse than today on 4 of the 14 traces** (by 4–23 pictures).
  Without change hints, redundant captures hide the gap.

### 4b. Phase sweep and modelled traces

`scripts/bench/cfr-scheduler-eval.py`: the same selection rule, every
deadline policy, replayed on the traces.

**Real traces:**
- Today's grid loses 0–88 pictures, depending on where its first capture
  happened to land.
- A fixed offset of half a period loses 78 and 98 on two traces.
- The phase lock with hints loses 0–2 on every trace except the starved one.

**Modelled WGC traces:**
- 3 random phases each.
- Display clock 40 ppm off.
- Present jitter.
- 1% late frames.
- 0–90% redundant captures.

| Case (displays) | Grid today, lost | Best **fixed** phase | Phase lock, hints |
| --- | ---: | ---: | ---: |
| 60 Hz source, 165 Hz | 1,703–1,783 | 0 | **0** |
| 60 Hz, 144 Hz, 50% redundant, 1% late | 285–2,711 | 0 | 15–17 |
| 60 Hz, 240 Hz, 90% redundant | 303–356 | 0 | **0** |
| 59.94 Hz, 250 Hz, 50% redundant (10 min) | 1,390–1,459 | ~1,200 | **2–5** |
| 60.06 Hz, 250 Hz (10 min) | 1,010–1,121 | ~1,010 | **36** = the rate difference (0.06/s) |
| 30 fps, 144 Hz (uneven holds) | 122–1,437 | 0 | **0** |
| 50 fps, 250 Hz | 1–58 | 0 | **0** |
| 144 / 300 fps sources | the same in every mode (downsampling) | | holds |

- **Drifting rates.** No fixed phase can follow a 59.94 or 60.06 Hz source.
  The lock wraps exactly once per beat, which is the minimum.
- **Remaining weakness: 1% late frames at 144 Hz.** They cost 15–17 pictures
  in 2 minutes, where the best fixed phase loses 0.

### 4c. Rust port against the reference model

`scripts/bench/phase-lock-crosscheck.py`:
- Inputs: 14 real traces plus one modelled trace of each case, with and
  without hints.
- **46 of 46 runs pick the same capture in every slot as the Python model.**
  Moves and wraps are identical too.

### 4d. Unit tests: `grid.rs` and `phase.rs`

- **Settling.** A 60 Hz source on 60, 144, 165, 240 and 250 Hz displays at 4
  phases settles at least 2 ms from every arrival, in ≤ 3 moves.
- **Holding.** 50, 144 and 300 Hz sources hold the deadline.
- **Drift.** A 59.94 Hz source wraps 6–8 times in 120 s, with the offset
  bounded.
- **Memory** stays bounded.
- **Causality.** The phase-locked grid matches an independent oracle at the
  deadline each slot used. Slots stay consecutive and picks never go
  backwards.
- **Today's failure, reproduced and fixed.** A fixture where today's grid
  produces more than 30 repeat/skip events: the lock has 0 after its first
  second.
- **Unchanged captures** never move the deadline.
- **Every existing grid test is unchanged and passes.**

## 5. Timestamp-preserving VFR (Codex's experiment), checked

| Question | Finding |
| --- | --- |
| Order, missing updates | All 481 / 361 unique source updates in order, 0 wrong decoded IDs (Codex, `app-vfr3`) |
| Timestamps, segment joins | Error at most 0.105 ms; adjacent segment starts cannot move (Codex's join test) |
| 59.94 / 60 Hz, phases, drift | Unit tests (`timeline.rs`): every capture kept at 60, 59.94, 120, 144, 240 and 250 Hz and every phase; a day without rounding accumulation |
| AVC metadata | `fixed_frame_rate_flag = 0` is already written (`h264_metadata`); VUI timing 120000/1000; no reordering, no B-frames. Nothing to clear |
| MP4 | `r_frame_rate` **250/1**, average **94.6 FPS** in Codex's desktop run. Durations 4–20 ms, multiples of the 4 ms vblank |
| **Encoder workload** | **VFR encodes every capture.** Generated 4 s: 250 Hz → 1,000 frames (250/s), 144 Hz → 576 (144/s). The 60 FPS grid encodes 240 for both (60/s). The desktop run encoded 758 frames in 8 s, 277 of them repeated pictures |
| A/V | Capture timing kept exactly (the most accurate of the three). Codex's NVIDIA audio-marker test passes (§9) |
| Editing compatibility | Not tested here. Common editors (Premiere Pro, DaVinci Resolve, Vegas) are known to handle VFR poorly (audio drift, conforming) |

**Verdict.** Accurate, but not a 60 FPS recording. On a high-refresh
display it does **2.4–4.2× the conversion and encoder work** of 60 FPS in
games running above 60 FPS, REPLAYX's main case. That is the GPU work the
native engine exists to reduce. VFR also spends the 15 Mb/s on up to four
times as many frames.

**It stays an explicit experiment, not a default candidate.**

## 6. CFR against VFR

| | 60 FPS grid today | **Phase-locked 60 FPS (new)** | Timestamp VFR |
| --- | --- | --- | --- |
| Pictures lost (Codex's failing run, GPU-decoded) | 72 | **0** | 0 (encodes all captures) |
| Smoothness at ~60 Hz | Depends on luck | Avoidable events gone (tables above) | Keeps capture cadence, including duplicates |
| Timing accuracy | Picture is 0–16.7 ms older than its PTS | Picture within ±1.25 frames of its PTS (bounded) | Exact |
| Output | 60 FPS CFR | 60 FPS CFR, PTS exactly n/60 | 60–250 FPS variable |
| Encoder workload | 60/s | 60/s | Up to the display refresh (250/s here) |
| Editing compatibility | CFR | CFR | VFR (risk) |
| Segments, replay | Proven (Codex) | Same muxing and PTS as today (§9) | Proven (Codex) |
| CPU for scheduling | ≈ 0 | < 0.02% of one core (§8) | ≈ 0 |

## 7. A pre-existing timing quantization (both engines)

- **The segments are Matroska,** which stores millisecond timestamps.
- **So both recorders' 60 FPS MP4s carry 16/17 ms frame durations:** ±0.33 ms
  around 16.67 ms. This includes today's FFmpeg recorder, which uses the same
  segment format.
- **It is not new and not native-specific.** It is invisible next to
  display-refresh quantization.
- **Possible fix later:** MPEG-TS (90 kHz) or fragmented-MP4 segments.

## 8. CPU and GPU

**Scheduling cost.** `--pacing-replay` on a modelled 10-minute 59.94 Hz trace
(92,971 captures, 35,999 slots), whole process CPU including start-up and
JSON parsing:

| Mode | CPU |
| --- | ---: |
| `clocked` | 125–156 ms |
| `phase-lock`, with change hints | 94 ms |
| `phase-lock`, timestamps only | 109 ms |

- **The lock's cost is below this measurement's resolution.** That bounds it
  at under 3 µs per output slot, under 0.02% of one core.
- **No waits, threads or GPU work were added.** The worker's timed
  condition-variable wake-up is unchanged; it now targets the shifted
  deadline.

**What was not changed:**
- **Codex's host transport fix is untouched.** The blocking video socket took
  normalized CPU from 5.15% to 0.44%; `configure_video_transport` is not
  modified.
- **No readback was added.**
- **GPU work per output frame is the same as today's grid:** one
  fixed-function conversion, one encode.

**No game FPS claim is made.** That needs a controlled benchmark with the
owner (§10).

## 9. Tests

| Suite | Result |
| --- | --- |
| `cargo fmt --check`; `cargo clippy --features desktop --all-targets -D warnings` | clean |
| `cargo test --features desktop` (library) | 204 passed, 7 opt-in ignored (the 6 app tests below + an explicit microbenchmark) |
| Native worker unit tests (`--bin replayx-native-proto`) | 35 passed: Codex's 28 + 7 new phase-lock tests |
| Opt-in synthetic app integration (`app_integration`, `--ignored`) | **6 passed:**<br>- lifecycle with concurrent replay saves, recording, MP4 with 3 audio tracks, library;<br>- runtime failure keeps the recording;<br>- unsupported settings fall back to FFmpeg;<br>- NVIDIA A/V markers across segments for `clocked`, `capture-vfr` and the new `phase-lock`: marker error 0.4 ms in all three, the right picture at 0.5 s |
| Phase-lock app MP4 | `r_frame_rate` 60/1, 360 frames, 3 audio tracks. Its completion check requires the worker to report `clocked-phase-lock-experimental` |
| GPU encode/decode matrix (§4a) | NVIDIA and Intel, with and without hints, 14 traces each: all integrity checks pass; Intel's decoded counts are identical to NVIDIA's |
| Encoder backpressure (40 ms delayed surface release) | NVIDIA and Intel, 3 traces each: pass, surface waits exercised, decoded results identical to the runs without delay |
| Rust port against the reference (§4c) | 46 / 46 identical |
| Frontend (`npm run test:frontend`), `tsc --noEmit` | 71 / 71, clean |
| Python (`unittest discover -s tests`, `REPLAYX_NATIVE_TEST=1`, `REPLAYX_NATIVE_NVIDIA_TEST=1`) | 53 passed, 5 skipped (AMD hardware and opt-ins) |

**No test was weakened.** One assertion was adjusted, and it is in my own new
fixture: the fixture's own "today's grid fails here" threshold, from more than
100 to more than 30 events (it produced 47).

## 10. Not verified (needs the owner or other hardware)

1. **Live capture with real dirty regions.** On Windows 11 24H2+ the worker
   asks WGC for dirty regions and counts a capture as new above 5% of the
   area. Not yet seen live:
   - whether redundant captures really report small or empty regions;
   - whether `ReportOnly` mode costs anything.

   Codex's on-screen motion check (`native-app-motion-check.py`, a moving
   test window) with `REPLAYX_NATIVE_TIMESTAMP_TEST=phase-lock` answers both.
   About 2–5 minutes on screen, **with your approval**.
2. **Timestamps-only fallback on older Windows.** Measured offline (§4a): better
   overall, worse on 4 of 14 traces. Not tested on Windows 10.
3. **Late frames at 144 Hz** (§4b) still cost about 0.2% of pictures (15–17 in 7,200).
4. **VRR / G-SYNC** displays, a display mode change during recording, AMD:
   not tested.
5. **Long sessions** with phase lock: Codex's 90 s soak used `clocked`.
   Offline, 10-minute modelled runs only.
6. **No game FPS measurement.** The phase lock does the same GPU work as
   `clocked`, so no FPS change is expected, but none is claimed.

## 11. Recommendation and next milestone

**Recommendation.**
- **CFR, not VFR.** Phase-locked 60 FPS removes today's phase-dependent
  repeat/skip pairs in every recorded and modelled case where they are
  avoidable. It keeps exact 60 FPS output, the same encoder workload,
  segments and A/V.
- **VFR stays an explicit experiment.** It is accurate, but encodes up to the
  display refresh rate.
- **Keep the defaults as they are.**

**Next milestone, in order, each needing your approval:**
1. **On-screen acceptance** of `phase-lock`: Codex's motion check at 60 Hz,
   59.94 Hz and 30 fps, on the external 250 Hz and the built-in 240 Hz
   panels. Pass: no repeated or skipped pictures after the first second,
   identity-checked in the decoded MP4. The report's `phase_lock.change_hints`
   must read `wgc-dirty-regions`.
2. **If it passes, make `phase-lock` the native engine's pacing on NVIDIA**
   (the native engine stays alpha-only). That is an explicit default change
   for you to decide.
3. **The controlled CS2 comparison** Codex prepared, today's recorder against
   native NVENC, with `phase-lock` as the native pacing.

## 12. What changed in this step (uncommitted, on top of Codex's work)

**New:**
- `src-tauri/src/bin/native/phase.rs`: the phase lock, with 4 unit tests.
- `scripts/bench/`:
  - `cfr-scheduler-eval.py`: the reference model and the offline evaluation;
  - `phase-lock-crosscheck.py`: Rust against the model;
  - `phase-lock-gpu-check.py`: GPU encode, then decode.

**Changed:**
- `bin/native/grid.rs`:
  - an optional lock with a per-slot deadline;
  - `push_hinted`;
  - 3 tests.

  Without a lock the behaviour is unchanged, and so are its existing tests.
- `bin/native_proto.rs`:
  - the `--pacing phase-lock` mode;
  - dirty-region hints;
  - `--phase-hints`;
  - the `--pacing-replay` command;
  - report fields (`phase_lock`, `deadline_hns`).
- `engine_native.rs`:
  - `REPLAYX_NATIVE_TIMESTAMP_TEST=phase-lock`, alpha-only;
  - its completion check;
  - one opt-in NVIDIA A/V test.

**Evidence:** `test-output/native-v2-resume/`.
