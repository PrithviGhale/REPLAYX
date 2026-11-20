# EcoQoS and WGC update interval: A/B (November 14, 2026)

Branch `performance/capture-replay-stutter-optimization`. **No default was
changed.** Both candidates are opt-in `REPLAYX_VIDEO_TUNING` switches.

**Synthetic results only.** These numbers come from a game-like workload, not
from CS2. The real-game check is in the last section.

## Setup

**Machine.** i9-13900H with an RTX 4070 Laptop GPU. An external 1920 × 1080,
250 Hz monitor driven by the NVIDIA GPU. Windows 11 build 26300.

**Workload.** `replayx-gameproxy` full screen, CS2-like and CPU-bound:
- 2.2 ms of main-thread work per frame;
- 8 busy worker threads;
- a light GPU load;
- about 442 FPS with nothing recording.

**Recorder.** `replayx-cli capture-test`: Windows Graphics Capture, 1920 × 1080
at 60 FPS, H.264, Encoder Auto (NVENC here), Balanced, silent.

**Method.**
- `scripts/bench/overhead-bench.py`, three configurations × three repeats, in
  shuffled order.
- Each run is 10 s off, then 30 s recording, then 15 s off.
- The FPS loss is measured against both "off" phases (A/B/A).
- All 9 runs were valid: drift between the two "off" phases was 0.05–0.43%.

**Configurations.**
- **Baseline:** today's WGC path, with a 12.5 ms minimum update interval.
- **EcoQoS:** `capture_eco=1`.
- **Floor 15 ms:** `wgc_floor_us=15000`.

## Results (mean of 3 runs; individual runs in brackets)

| Metric | Baseline (12.5 ms) | EcoQoS | Floor 15 ms |
| --- | --- | --- | --- |
| Game FPS loss while recording (A/B/A) | 1.14% (1.15 / 1.19 / 1.07) | 1.14% (1.15 / 1.01 / 1.25) | 1.16% (1.04 / 1.07 / 1.38) |
| Game P99 frame time while recording | 2.51 ms | 2.53 ms | 2.51 ms |
| Game 1% low while recording (FPS) | 377–390 | 378–385 | 382–387, one run 323 |
| Frames captured and then discarded | **20.8/s** | 20.8/s | **2.6/s** |
| Recorded clip: repeated frames | 0.0% | 0.0% | 0.0% |
| Recorded clip: uneven motion ("jumps", > 1.5 frame intervals) | **14.8%** | 16.4% (one run 20.1%) | **4.4%** |
| Recorded clip: frames and frame rate | 1320 at 60.0 | 1320 at 60.0 | 1320 at 60.0 |
| Output frame-rate grid (duplicates, 30 s) | 16 | 18–19 | 15–16 |
| Clips decoded and validated | 3 / 3 | 3 / 3 | 3 / 3 |
| 0.5 s presentation stalls in the workload | 0 | 0 | 0 |

## Reading

- **Game FPS.** No configuration changed the game's FPS or P99 frame time
  beyond run-to-run variation. On this workload the whole recording cost is
  only about 1.1%. That is far below the 10.2% measured in CS2, so this
  synthetic test cannot show the CS2 gap closing. Only the real-game run
  below can.
- **EcoQoS for the recorder (`capture_eco=1`): rejected.** It gave no FPS
  gain, and the recorded motion got slightly less even (16.4% jumps against
  14.8%; one run 20.1%).
- **Floor 15 ms (`wgc_floor_us=15000`): better recordings, same FPS.**
  - On this 250 Hz display it captures every 4th refresh (16 ms) instead of
    every 3rd (12 ms).
  - Wasted captures fell from 20.8/s to 2.6/s: 87% less capture work thrown
    away.
  - Recorded motion became much more even: 4.4% jumps against 14.8%.
  - It kept 0% repeated frames and a full 60 FPS. The extra smoothness is
    real, not hidden duplication.
  - One run's 1% low during recording was 323 FPS, while its P99 frame time
    was normal (2.54 ms). That looks like a single disturbance; the real-game
    run must confirm there is no regression in lows.
- **Floor 15 ms cannot simply become the default.**
  - On a 240 Hz display, 4 refreshes equal one 60 FPS slot (16.67 ms). This
    is why 12.5 ms was chosen: with a 16 ms interval, a ~228 FPS game on a
    240 Hz panel recorded 10% repeated frames.
  - The general fix is an interval chosen from the display's refresh rate:
    the largest number of refreshes that stays clearly under one output
    slot. It is not implemented yet; it should follow the CS2 result.

**Recommendation.** Keep the baseline as the default. Verify `wgc_floor_us=15000`
in CS2 on the 250 Hz monitor. If it holds there (FPS, 1% lows, smooth clip),
implement the refresh-rate-based interval. That way each display gets the
benefit only where it is safe. Drop EcoQoS for the capture.

## Real CS2 verification (owner runs it)

About 6 minutes, no REPLAYX app running.

**What it does.**
- It alternates recorder off and recording, 45 s each: baseline, then floor
  15 ms.
- It never launches, focuses, or sends input to CS2.
- It stops at once if CS2 leaves the front.
- It records only through REPLAYX's test recorder, silent.

**Steps.**
1. Quit REPLAYX (tray icon → Quit). Start CS2 and load the same offline map
   and spot each time, for example a practice map against no bots. Keep the
   video settings and FPS cap unchanged.
2. Open PowerShell with **Run as administrator**. PresentMon frame times need
   it; this is your decision. Without it, drop `--presentmon` to get
   CPU and GPU telemetry only.
3. Run:

   ```powershell
   cd "C:\Users\Prithvi\Desktop\VS CODE\replayx-perf"
   py -3 scripts\bench\fortnite-session.py --game cs2 --configs wgc-auto,wgc-floor15 --cap 0 `
     --presentmon "..\REPLAYX\test-output\fortnite-live-alpha8\PresentMon-2.6.0-x64.exe"
   ```

4. Within the 15 s countdown, switch to CS2. Stand still, or repeat one short
   identical movement, until it prints the results.

**What comes back.** `test-output\cs2-session-<time>\summary.json` with, per
phase:
- CS2 average FPS, 1% low, P95/P99 frame time and presentation mode;
- per-process CPU and GPU-engine use;
- NVIDIA clocks and power;
- the recorder's captured, discarded and duplicated frames.

The recorded test clips stay under `test-output\` and can be deleted
afterwards. Send me the folder name and I will compare it with the 372.4 →
334.6 FPS baseline.
