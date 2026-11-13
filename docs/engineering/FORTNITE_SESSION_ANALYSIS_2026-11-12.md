# Fortnite session analysis and the resulting fix (November 12, 2026)

**Source data:**

- `test-output/fortnite-session-20261112-191500/`: PresentMon 2.6.0 CSVs per
  phase, per-process GPU-engine counters, the CPU effective clock, NVIDIA
  telemetry and recorder status.
- Build `target-wgc3` (commit `7bbaa8e`).
- Built-in 2560x1600 240 Hz panel driven by the Intel GPU; Fortnite renders
  on the RTX 4070 Laptop GPU.

## 1. Measurements

Each recording phase is compared with the **mean of the replay-off phases on
either side of it**. The off phases drifted from 341 FPS to 314 FPS over the
session (heat), so comparing with a single baseline would mislead.

| Phase | FPS | 1% low | p99 | CPU busy per frame | GPU busy per frame | GPU wait | CPU clock (% of base) | Intel GPU: DWM 3D / FFmpeg 3D / video |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| A1 off | 341.0 | 207.8 | 4.43 | 2.58 ms | 2.53 ms | 0.29 | 134.7 | 0 / 0 / 0 |
| B1 adaptive WGC + QSV | 271.7 | 149.7 | 6.21 | 3.22 ms | 2.70 ms | 0.72 | 123.0 | 12 / 14 / 15 % |
| A2 off | 328.0 | 200.2 | 4.59 | 2.66 ms | 2.56 ms | 0.35 | 124.4 | 0 / 0 / 0 |
| B2 2x WGC + QSV | 216.6 | 139.8 | 6.75 | 4.05 ms | 2.92 ms | 1.21 | 99.6 | 21 / 22 / 15 % |
| A3 off | 315.8 | 201.8 | 4.64 | 2.75 ms | 2.60 ms | 0.42 | 113.8 | 0 / 0 / 0 |
| B3 WGC + NVENC | 243.0 | 147.3 | 6.28 | 3.59 ms | 2.85 ms | 0.94 | 102.3 | 14 / 16 / 0 % (NVENC on NVIDIA) |
| A4 off | 314.4 | 199.8 | 4.67 | 2.77 ms | 2.60 ms | 0.43 | 113.0 | 0 / 0 / 0 |

Against the mean of the neighbouring off phases:

| Recording phase | FPS loss | CPU busy per frame | GPU busy per frame | CPU clock |
| --- | ---: | ---: | ---: | ---: |
| B1 adaptive WGC + QSV | **18.8%** | **+23%** | +5% | −5% |
| B2 2x WGC + QSV | **32.7%** | **+49%** | +8% | −16% |
| B3 WGC + NVENC | **22.9%** | **+30%** | +3% | −10% |

**Fortnite's own CPU use** was about 18% of all threads in every phase, so it
was not short of CPU time.

**Excluded:** one PDH sample put Fortnite's 3D engine at about 3x10^13 %. That
is a counter error.

## 2. What the evidence shows

**Confirmed (measured in Fortnite):**

1. **Composition is not involved.** Fortnite stayed in **Hardware Composed:
   Independent Flip for 100% of frames** in all seven phases. The
   composition hypotheses from the synthetic tests do not apply to Fortnite.
2. **Fortnite is CPU-bound, and recording makes its CPU work per frame
   slower:**
   - CPU busy per frame rose 23–49%.
   - GPU busy per frame rose only 3–8%.
   - The added GPU wait is the GPU waiting for the CPU.
   - Fortnite's share of CPU time did not change, so the same work simply
     takes longer.
3. **The slowdown follows how much work the recorder puts on the Intel GPU.**
   The integrated GPU shares the CPU cores' power budget, last-level cache
   and memory controller. Ordering the phases by Intel load, FPS loss and
   CPU-busy increase gives the same order every time: B1 < B3 < B2.

**Supported by reproduction, not directly measured in Fortnite.**
Cache and memory-bandwidth counters need ETW or administrator rights. Two
pieces of evidence:

- **Clock speed explains only part of it.** In B2 the CPU clock fell 16%,
  but CPU time per frame rose 49%.
- **A memory-bound synthetic game reproduces Fortnite.** One with dependent
  random reads over a 64 MB buffer, larger than the cache, plus two
  streaming threads reproduces Fortnite's ordering and magnitudes:

  | Configuration | Memory-bound synthetic game | Fortnite |
  | --- | ---: | ---: |
  | B1 adaptive WGC + QSV | 14.6–15.3% | 18.8% |
  | B3 WGC + NVENC | 16.7–17.8% | 22.9% |
  | B2 2x WGC + QSV | 20.4–21.1% | 32.7% |

  In that game the CPU clock barely moved. Every earlier synthetic game did
  only arithmetic, which a slower clock affects but cache or memory
  contention does not, and none of them reproduced the Fortnite losses.

So the remaining cost is best explained by **contention in the shared
cache and memory system** with the Intel GPU's full-resolution frame traffic,
plus a smaller **clock reduction** from the shared power budget. Every
captured frame moves the 2560x1600 desktop (about 16 MB) several times
across Windows' capture copy, FFmpeg's `gfxcapture` pass, VPP scaling and
the encoder.

## 3. Lower bound of public-API capture on this laptop

**WGC capture alone** costs **8.7–9.3%** of the memory-bound game's FPS. That
is one full-screen copy per delivered frame, with no scaling and no encoding.
This is Windows producing full-resolution capture frames on the Intel GPU. No
WGC-based recorder can go below it on this display configuration.

NVIDIA Instant Replay captures inside NVIDIA's driver, on the NVIDIA GPU,
without the Intel GPU's frame traffic. That interface is not available to
third-party applications on GeForce.

## 4. What changed, and why

Measured on the memory-bound game, 2 runs per configuration:

| Configuration | FPS loss | Repeated frames |
| --- | ---: | ---: |
| B1 (unscaled, 16 ms floor) | 14.6–15.3% | **9.6–10.3%** (starving) |
| Unscaled, 12.5 ms floor | 16.9–17.2% | 0% |
| **Scaled in gfxcapture, 12.5 ms floor (new default)** | 13.6–14.0%, and 15.0% in a later batch | **0%** |
| Scaled in gfxcapture, 2x | 16.7–17.5% | 0% |
| B2 (unscaled, 2x) | 20.4–21.1% | 0% |

1. **Scale in `gfxcapture`** (`Tuning::wgc_scale`, now on). Its full-screen
   shader pass runs on every frame anyway. It now writes the 1920x1080
   image, so VPP reads 1080p instead of 2560x1600. At a 16 ms floor this cut
   the cost from 14.9% to 11.5%.
2. **A 12.5 ms update floor** (was 16 ms). At 16 ms, a game running just
   below the 240 Hz refresh was delivered at about 57/s, and **10% of
   recorded frames were repeats**: this was the case for B1. At 12.5 ms,
   repeats are 0%. The encoder keeps the frame nearest each 60 FPS slot.
3. **The adaptive 2x switch** is unchanged. It is still needed for games
   between 60 and about 80 FPS, and was verified again: a 100 FPS game
   switched, and its clip had 1.0% repeats.
4. **Performance page and diagnostics.** With WGC, surplus captures are
   skipped by design: about 12,000 in 10 minutes at 80/s → 60/s. The page now
   shows **Repeated frames** (the real starvation measure) instead of
   "Dropped frames", and diagnostics label the drop counter.
5. **Benchmark harness.** A memory-bound workload (`--mem-reads`, `--mem-mb`,
   `--mem-workers`); a 10 s fixture start tolerance; and a fixture that is
   always stopped on refusal (a leftover fixture once blocked a run).

**Net effect.** At about the same game-FPS cost as the adaptive build you
tested, recordings no longer repeat 10% of frames. Against the old 2x
capture, the cost is about 6 points lower. **A game-FPS improvement over the
B1 build is not demonstrated:** the batches measured 13.6–15.1%, against
14.6–15.3%. No Fortnite measurement of the new build exists.

## 5. Quality and reliability (new defaults)

| Test | Result |
| --- | --- |
| Memory-bound game | 0% repeated frames; frame "jumps" (uneven motion steps) 10.7–12% |
| High-FPS game (about 640 FPS) | 1.2% FPS loss (1 valid run; the other exceeded control drift), 0% repeats |
| GPU-heavy game (about 100 FPS, 60 s) | Switched to 2x as designed; clip 1.0% repeats |
| Codec matrix through WGC | 6/6 (H.264/HEVC/AV1, 4:3 regions, exact policy); every clip decodes clean |
| 10-minute WGC soak | Details below |
| Scaling quality (static test pattern against a Lanczos reference) | gfxcapture scaling SSIM 0.674 / PSNR 22.2 dB, against Intel VPP 0.642 / 21.6 dB. Not worse. The absolute values are low because the pattern window was itself scaled by Windows' 125% display scaling. |
| Rust | 164 passed (1 ignored); Clippy and rustfmt clean |
| Frontend | 53/53; TypeScript clean |
| Python | 9/9 |
| Updater e2e | 42/42 |

The 10-minute soak results:

- 36,062 frames at 59.99 FPS; 0% repeated; no freeze.
- Saves ready in 918 ms; both clips decode clean.
- Encoder at least 0.981x real time; video lag at most 101 ms; no recovery.

**Trade-off:** motion cadence is less even than at 2x. Jumps were 11–14%,
against 2–4% at 2x and 7–8% for Desktop Duplication in earlier reports.
Repeated frames, which are visible as stutter, are what the change removes.

## 6. Remaining options, ordered by expected effect

1. **Run the display from the NVIDIA GPU.** On this laptop that is the MUX
   "Ultimate"/dGPU mode in Armoury Crate, or an external monitor on the
   NVIDIA outputs. The capture work then runs on the NVIDIA GPU, away from
   the CPU's cache and memory controller. On November 11, with the external
   NVIDIA-driven 250 Hz display, Desktop Duplication cost about 4–6%
   (synthetic). This is a user setting. REPLAYX has not changed it and will
   not.
2. **An external NVIDIA-driven monitor test** of the current build, with the
   same session script.
3. Within WGC on the Intel-driven panel, the remaining REPLAYX-side work is
   about 5 points above the about 9% capture floor:
   - the 12.5 ms floor's surplus frames (about 80/s), and
   - VPP colour conversion.

   An adaptive floor that targets exactly 60 deliveries per second needs
   Windows to deliver frames on demand, which `gfxcapture` does not support.
