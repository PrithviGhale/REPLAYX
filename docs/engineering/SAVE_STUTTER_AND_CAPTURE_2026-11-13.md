# Instant Replay save stutter and capture overhead (November 13, 2026)

Branch `performance/capture-replay-stutter-optimization`, from `main`
(`eb905d0`, 0.3.1-alpha.9). Not merged, not released.

**All game-side numbers here are synthetic.** They come from
`replayx-gameproxy`, a game-like GPU workload, not from a real game. Real-game
tests (CS2, Fortnite) were not run: they need the owner at the PC. They are
listed in section 7.

## Setup

**Hardware.**
- Intel Core i9-13900H and RTX 4070 Laptop GPU, on AC power, Windows 11.
- **Display: an external 1920 × 1080, 250 Hz monitor driven by the NVIDIA
  GPU.** Earlier reports used the built-in Intel-driven panel.
- On this display, REPLAYX's automatic choice is **Windows Graphics Capture
  (`gfxcapture`) → NVENC**, with no scaling (Direct3D 11 zero copy). That
  matches the owner's saved settings: WGC, H.264, Auto encoder, 1080p60,
  Balanced, 2-minute replay.

**Tools.**
- **`replayx-cli save-stutter` with `scripts/bench/save-stutter.py`** (new).
  - It runs Instant Replay with those settings beside the workload.
  - It saves on a schedule: single saves, two saves 1 s apart, and three
    saves 0.4 s apart.
  - It does what the app does around a save: each save on its own thread, the
    "saved" notice, the in-game status overlay updated once a second, and
    optionally the thumbnail.
  - Every stage is logged on the same QPC clock as the workload's presents,
    so frame times line up with save stages.
  - Each save window runs from the request to 4 s after the clip is ready
    (the notice included). It is compared with the 20 s of capture just
    before the first save.
- **`overhead-bench.py`** for A/B/A FPS (off / on / off).
- **`replayx-cli save-bench`** for offline save timing: no capture and no
  window.

## 1. Save stutter

### What a save does

| Stage | Where | Cost |
| --- | --- | --- |
| Hand-over: read the segment list, hard-link the selected segments into a snapshot | Under the engine lock | **11–85 ms** (24 saves measured) |
| Remux the snapshot into one clip (`-c copy`, about 230 MB for 2 min) | Helper FFmpeg, below normal priority | Disk and memory bandwidth; little CPU |
| Validate: probe, then decode 1 s at the start and 1 s at the end | Helper processes | About 0.2–0.4 s CPU |
| Library index entry; "saved" notice; the app's toast | App | Small; the notice is a topmost window over the game |

Capture, the encoder and audio are never paused by a save. The new test
`rapid_repeated_saves_never_stall_capture_or_collide` checks this with three
saves back to back while the encoder keeps writing packets, then one more:
- the encoder process never changes (no restart);
- the frame count keeps rising;
- every clip is complete and separate;
- no snapshot folder is left behind.

### Measurements

**GPU-bound workload, about 900 FPS.** Worst frame in each save window, with
the capture-only baseline for comparison:

| Configuration | Baseline (capturing, no save) | Single saves | Back-to-back and triple saves |
| --- | --- | --- | --- |
| Before, notice off | 2.46 ms | 2.3–2.9 ms, 0–1 hitch | 2.1 ms, no hitches |
| Before, notice on | 3.2 ms | **8.7 / 5.4 / 5.4 ms**; worst frame when the notice appears or leaves | 2.7 ms |
| Before, notice and status overlay on | 2.7 ms | 5.4 / 2.3 / 2.1 ms | 2.7 ms |
| **After, notice and status overlay on** (3 runs) | 2.5–3.0 ms\* | **2.1–3.2 ms** (one 4.2 ms) | 2.5–3.3 ms |

\* Ignoring the separate 0.5 s stalls in section 3.

**CPU-bound workload, about 405 FPS** (2.2 ms of main-thread work per frame,
8 busy worker threads, like a CPU-limited shooter):

| Configuration | Baseline | Save windows | Hitches |
| --- | --- | --- | --- |
| Before | 3.31 ms worst, 405.9 FPS | 3.6–4.0 ms worst, 402–406 FPS | 0 |
| After | 3.34 ms worst, 408.0 FPS | 3.3–4.2 ms worst, 405–410 FPS | 0 |

A hitch is a frame longer than 2.5 × the baseline median.

**Reading.**
- The save pipeline (snapshot, remux, validation) caused no measurable frame
  time change on either workload.
- The one save-related cost found was the notice. Its slide-in moved and
  re-ordered a topmost window over the game every 16 ms for about 0.5 s,
  and each window change cost a 5–9 ms frame. Fading in place removes it.
- The "several seconds of lag" reported in real games was **not reproduced**
  by any save here. See section 3 for a possible match.

### Changes

1. **The saved notice fades in place.** The window is placed once (topmost)
   and only its opacity changes. Before, it slid in and drifted out with a
   `SetWindowPos(HWND_TOPMOST)` every 16 ms.
2. **Helper processes use low memory priority**
   (`SetProcessInformation(ProcessMemoryPriority)`). This applies to the save
   remux and checks, probes, thumbnails and export. Their file pages, such as
   a 230 MB copied clip, are the first to leave memory, never the game's.
   Documented API; it applies on every Windows PC.
3. **EcoQoS (power throttling) only for work nothing waits for:** the
   per-segment probe that runs every 2 s during play, and thumbnails. On
   hybrid Intel CPUs this keeps them on efficient cores at low clocks.
   - It was first applied to the save path too. That made clips ready after
     1.5–2.1 s instead of 0.5–0.9 s, with no measurable FPS benefit, so it
     was taken off the save path.
   - Offline save timing, 2-minute replay, two interleaved runs each: original
     2.33 / 2.38 s; final 2.31 / 2.50 s.
4. A test reads the helper settings back from Windows: below normal,
   EcoQoS, low memory priority.

**Tried and not kept.** Raising the in-game status overlay only when another
window covers it, instead of every 3 s. It showed no benefit, and while
investigating section 3 it was briefly suspected of causing the stalls.

## 2. Capture overhead (recording on vs off)

A/B/A with the GPU-bound workload. Each run is 30 s of capture between two
capture-off phases; the drop is against the "before" phase.

| Path on the NVIDIA-driven display | FPS loss per run |
| --- | --- |
| **Desktop Duplication** (the default for new installs), Auto → NVENC | **19.9%, 20.8%, 23.4%** |
| **Windows Graphics Capture** (owner's setting), Auto → NVENC | −0.5%, 4.0%, 5.8%, −3.8%, 3.7%, 4.3%, 4.6%, 6.8% |
| WGC, capture GPU priority off (`gpu_priority=0`) | 2.2%, 8.7%, 6.9% |
| WGC, NVENC spatial AQ off (`nvenc_aq=0`) | 4.4%, 7.7%, 4.3% |
| WGC, status overlay on vs off (capture-only FPS) | 917 / 904 on, 906 / 909 off |

**Findings.**
- **On this display, Desktop Duplication costs about 5× more than WGC.** This
  matches the Intel-driven panel in the November 12 report (19–35% against
  about 4%). Two GPU configurations now show the same thing.
- **Capture forces composition.** Before capture, 100% of the workload's
  frames were presented on a hardware overlay plane; during capture, 100%
  were composed by DWM. Both public capture APIs do this, and REPLAYX
  cannot avoid it without driver-level capture.
- **The recorder's CPU use is small.** FFmpeg used about 8% of one core
  (0.4% of this CPU); REPLAYX itself about 0.5% of one core. CPU contention
  is unlikely to explain a 10% loss in CS2. DWM's CPU could not be read; it
  is a protected process.
- **Rejected:** the capture GPU priority and NVENC adaptive quantization make
  no measurable difference, even though the game and the capture share the
  GPU here. The run-to-run spread is about ±2.5 points.

**Owner's decision needed: default capture method for new installs.**
- WGC costs the game much less FPS.
- Its clips have less even motion: 11.8–14.7% of frames advance by more than
  1.5 frame intervals, against 7.6–7.7% with Desktop Duplication.
- WGC falls back to Desktop Duplication automatically if it cannot start.
- WGC can hide its yellow capture border only on Windows 11 and Server 2022
  (build 20348 and later), so a change should be limited to those.
- **Nothing was changed.** Existing settings must never be switched silently.

The CS2 numbers (372.4 → 334.6 FPS, 10.2%) are larger than this workload's
about 4%. Two possible reasons:
- CS2 is CPU-limited at that frame rate, and forced composition affects it
  differently.
- It may be the 0.5 s stalls below.

Only a PresentMon run in CS2 can tell (section 7).

## 3. Open: 0.5 s stalls with Windows Graphics Capture

Partway through this session, workload frames of **about 500–535 ms** began to
appear while WGC capture was running.

**The pattern:**
- they come in clusters of 2–3, about 0.73 s apart;
- clusters repeat about every 60 s, not tied to when capture started;
- they appear inside and outside save windows.

| Condition | Runs | Long frames (> 50 ms) |
| --- | --- | --- |
| Before this started (WGC, earlier in the session) | 9 | 0 stalls (one 50 ms frame as capture started) |
| WGC, original code (`main` plus test tools) | 2 | 23, 19 |
| WGC, new code | 2 | 13, 15 |
| WGC, new code, status overlay off | 2 | 14, 5 |
| Desktop Duplication | 1 | 0 |
| No capture, workload alone | 2 | 0, 0 |

**What this shows.**
- The stalls need WGC capture to be running.
- They occur with the original code too, so they were **not caused by these
  changes**.
- They are not caused by the status overlay or by the helper settings.
- They started without any change to REPLAYX: something on this PC began
  doing a once-a-minute action that stalls presentation while WGC captures.

A cluster is 1–1.5 s of stutter, so it could be what players feel as
"several seconds of lag". If it lands near a save, it looks like a save stall.

**Next step:** an ETW trace with Windows Performance Recorder (`wpr -start
GeneralProfile -start GPU`) during a stall would show which process acts every
60 s. WPR needs an administrator terminal, which is the owner's decision.
Nothing was installed or elevated.

## 4. Quality and integrity

- Every saved clip passed REPLAYX's own validation: duration within
  tolerance, probe, and decode at the start and end. That is 24+ clips in the
  synthetic runs.
- In all A/B/A runs, both capture paths had 0.0% repeated frames and no
  freeze in the clip.
- No encoder, bitrate, frame-rate or resolution setting was changed.
- The 60 FPS output is never lowered.

## 5. Tests

| Check | Result |
| --- | --- |
| Rust (`cargo test --features desktop`) | 194 passed, 1 ignored (opt-in) |
| Clippy `-D warnings`, rustfmt | Clean |
| TypeScript, frontend tests | Clean; 71 passed |
| Python (`tests/test_frametimes.py`, `tests/test_live_profile.py`) | 9 passed |
| New: `rapid_repeated_saves_never_stall_capture_or_collide` | Passes; hand-over bound 2 s for a busy test machine (11–85 ms measured) |
| New: `helpers_run_in_the_background_and_keep_their_output` | Passes |

## 6. Files

- `src-tauri/src/overlay.rs`: the notice fades in place.
- `src-tauri/src/media.rs`:
  - `BackgroundOutput` (`background_output`, `low_memory_output`);
  - `background_process`;
  - helper call sites;
  - test.
- `src-tauri/src/engine.rs`: the repeated-save test.
- `src-tauri/src/bin/cli.rs`: `save-stutter`.
- `scripts/bench/save-stutter.py` (new).
- `scripts/bench/overhead-bench.py`: GPU-priority and AQ configurations;
  fixed script path.
- `scripts/bench/fortnite-session.py`: fixed script path.

## 7. Not done: needs the owner's display or approval

1. **CS2 and Fortnite with PresentMon**, off/on/off. This includes saves
   during play: single, back to back, three in a row, during a recording,
   with the microphone on. Use the existing `fortnite-session.py` procedure,
   or PresentMon by hand. It takes over the screen.
2. **The 60 s WGC stall** (section 3): an ETW trace, which needs an
   administrator terminal.
3. **Re-measure the notice and saves after the final EcoQoS change.** The
   notice code did not change after it was measured; the save path was
   re-timed offline only.
4. **Crosshair Studio (other branch):** a long run, more than 2 minutes,
   with the crosshair shown over the workload. Its earlier 12 s runs were
   too short to catch a once-a-minute event.
5. **Saves with a slow disk** (HDD or a full SSD) and on a CPU with fewer
   cores. Not available here.
6. **The default capture method decision** (section 2).
