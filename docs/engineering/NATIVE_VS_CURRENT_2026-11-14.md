# Native prototype against today's recorder in CS2 (November 14, 2026)

Data: `test-output/cs2-session-20261114-185453`, recorded by the owner.

**Conditions.**
- CS2 on the built-in 2560 × 1600 240 Hz panel, warmed up.
- Phases alternate: today's WGC recorder, the native prototype
  (build `7fbeac8`), each run twice.
- Both record 1080p60 H.264 through Intel Quick Sync, silent.

## 1. Telemetry, each phase against its two neighbouring recorder-off phases

The off phases ran at 336.3, 327.6, 330.3, 337.6 and 337.5 FPS.

| Phase | CS2 FPS | FPS cost | 1% low cost | P99 vs off | Frames > 8.33 ms | > 16.67 ms (max) | CPU clock | CS2 CPU busy / frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Today #1 | 233.2 | 29.7% | 30.8% | +2.69 ms | 140 | 0 (12.6 ms) | 93.4% | 3.29 ms |
| Native #1 | 242.4 | 26.3% | 25.6% | +1.91 ms | 35 | 0 (9.9 ms) | 95.7% | 3.18 ms |
| Today #2 | 244.1 | 26.9% | 27.3% | +2.33 ms | 65 | 0 (9.9 ms) | 93.9% | 3.11 ms |
| Native #2 | 253.9 | 24.8% | 28.5% | +1.90 ms | 23 | **2 (45.0 ms)** | 99.1% | 3.00 ms |

**Recorder resources**, both runs of each:

| | Today | Native |
| --- | --- | --- |
| Recorder CPU (whole 20-thread CPU) | FFmpeg 1.04–1.10% + CLI 0.27% | **4.8%** (95% of one core) |
| DWM Intel 3D | 16.4–16.7% | 17.3–17.6% |
| Recorder Intel 3D | 12.0% | **0** |
| Intel encoder (video engine) | 16.8–17.1% | 14.1–14.2% |
| Intel video processing (conversion) | — | 12.4–12.6% |

**Reading.**
- **FPS.** Native was cheaper in both pairs, by −3.4 and −2.1 points of cost.
  But the two runs of today's recorder differ by 2.8 points themselves, so
  two pairs do not establish it. Both kinds of phase improved from the first
  pair to the second (drift).
- **Smoothness.** Native was better in both pairs:
  - P99 frame-time rise: 1.9 ms against 2.3–2.7 ms;
  - frames over 8.33 ms: 23–35 against 65–140.
- **1% lows.** No consistent difference: native 25.6% and 28.5%, today 30.8%
  and 27.3%.
- **The 45 ms frames.** Native #2 had two frames, 22.6 s and 23.2 s into the
  phase: 25.4 ms and 45.0 ms. Both were CS2's own CPU work (CPU busy 24.2 ms
  and 43.6 ms; GPU busy and Present normal). No other phase has a frame over
  16.67 ms. The cause is unproven. A candidate in the prototype is described
  below and has been removed; the next run checks whether such frames recur.

## 2. Why the prototype used 95% of one core, and the fix

`replayx-native-proto --bench-guard` (terminal only):
- The record loop's per-capture "game still in front" check built a process
  snapshot through `sysinfo`: **12–15 ms of CPU per call.**
- The loop ran it on every wake-up, about 60–74 times a second: about 0.9 of
  one core. That accounts for essentially all of the 95%.
- Enumerating every process also takes system-wide kernel locks 60+ times a
  second. That is a plausible link to the 45 ms frames, not proven.

**Fix (`c157076`).**
- The process is identified once at start; a process's executable does not
  change.
- The loop only checks which window is in front: two window-manager calls,
  about 0.06 µs, at most four times a second.

**The capture-only probe phases of Test B never had this cost.** That probe
checked the process once and the foreground only through window calls.

### CPU that remains

Synthetic, paced at 60 FPS, CPU per output frame, several runs while the PC
was in use (noisy):

| Part | CPU per frame |
| --- | --- |
| Whole prototype process | 1.6–3.9 ms (10–23% of one core) |
| Recording thread: conversion submit | 0.13 ms (one outlier 1.1) |
| Recording thread: encoder `ProcessInput` | **1.0–1.3 ms** |
| Recording thread: wait for an input request, wrap the texture | about 0 |
| Recording thread: collect output | 0–0.16 ms |
| Recording thread: synthetic frame drawing (test only) | the rest |

- `ProcessInput` is the Intel driver submitting work to its media engine.
  FFmpeg's Quick Sync path pays the same kind of cost.
- **Tried, no effect beyond noise:**
  - reusing one sample per ring texture (`--reuse-samples`);
  - binding the ring for the video encoder (`--encoder-bind`).
- **Not found:**
  - busy-waiting: the waits block on an event or condition variable;
  - per-frame allocations that matter: the sample and buffer wrappers cost
    about 0;
  - texture churn: one input view in total, so WGC-style textures reuse it.

**Expected after the fix:** about 0.5–1.2% of the whole CPU, comparable to or
below today's 1.3–1.4%. To be confirmed in the next run.

## 3. The recorded frames, measured independently

Each clip was decoded the same way and examined, rather than trusting either
recorder's counters:
- consecutive frames compared;
- the per-frame picture change at 480p.

| Clip | Frames | Identical to previous | Near-repeats (< 15% of median change) | Spread of change (CV) | Bitrate |
| --- | ---: | ---: | ---: | ---: | --- |
| Today #1 | 3480 (58 s replay window) | 0 | 0 | 0.44 | 14.9 Mb/s |
| Native #1 | 3422 (whole recording) | 0 | 0 | 0.39 | 15.1 Mb/s |
| Today #2 | 3480 | 0 | 0 | 0.44 | 14.9 Mb/s |
| Native #2 | 3422 | 0 | 0 | 0.40 | 15.1 Mb/s |

**Reading.**
- **No repeated frames in any clip,** by either measure.
- **The same bitrate,** so quality at the same size is comparable.
- **Limitation:** a still CS2 scene changes little between frames, so this
  cannot prove even pacing. Only the timestamped test workload measures that
  exactly (section 5).
- **Counters.** The native selector's counters are proven equal to FFmpeg's
  for identical capture times. In the game, each run captured its own frames,
  so the counter values are not directly comparable:
  - today: 854 / 918 dropped, 16 / 16 duplicated;
  - native: 774 / 787 dropped, 1 / 2 duplicated.

## 4. Where the native path's remaining cost is

From Tests B and C (absolute FPS alongside cost; the runs drifted):

| Stage | Cost | Note |
| --- | ---: | --- |
| WGC capture only | 5–6% | Windows: DWM renders frames for capture |
| + fixed-function conversion of the 60 chosen frames | 9–11% | No encoder |
| + NV12 frames to the NVIDIA GPU through system memory | 15% | No encoder |
| **Native with Quick Sync encoding** | 25–26% | Includes the 95%-core bug |
| Today's recorder | 27–30% | |

**Adding the Intel encoder to native capture and conversion raised the cost
from about 10% to about 25%.** Part of that was the CPU bug; the rest is the
Quick Sync encoder working on the Intel GPU, inside the CPU's power and
memory budget.

**On a hybrid laptop, the evidence-based lever is to encode on the NVIDIA
GPU.**
- The transfer to NVIDIA cost about 4.5 points in Test B, far less than the
  Intel encoder appears to.
- NVENC's power is the discrete GPU's, not the CPU package's.
- Today's FFmpeg NVENC option could not show this: it converts with the Intel
  3D engine (22.7%) and copies through system memory in FFmpeg.

## 5. Plan: focused, each step measured before the next

1. **Re-measure the fixed prototype.**
   - **What:** no new features. Warmed CS2, three alternating pairs, about
     15 minutes:
     `--warmup 150 --configs wgc-auto,native-record,wgc-auto,native-record,wgc-auto,native-record`
   - **Success:**
     - The mean paired cost difference exceeds twice the spread of the
       today-to-today differences.
     - Native recorder CPU is at or below today's (≤ 1.4% of the CPU).
     - No CS2 frames over 16.67 ms beyond the off phases' level.
     - P99 at or below today's.
2. **Native with NVENC on hybrid laptops** (about 1 day of development,
   offline first).
   - **What:** Intel fixed-function conversion → NV12 readback ring →
     upload to an NV12 texture on the NVIDIA GPU → NVIDIA's H.264 encoder
     (Media Foundation), same rate control.
   - **Offline success:**
     - the same synthetic checks (0 wrong slots, keyframes, timestamps);
     - CPU per frame measured; the transfer's CPU was 1.4–2.1 ms per frame
       in the self-test.
   - **CS2 success:**
     - its cost is at least 5 points below today's in every pair;
     - the transfer's CPU does not raise CS2's CPU busy time;
     - pacing and duplicates are equal.
3. **Only if step 2's transfer CPU shows up in CS2:** a GPU-to-GPU
   cross-adapter copy (D3D12 cross-adapter heap) instead of system memory.
4. **Pacing validation** of the winning variant with the timestamped test
   workload (about 5 minutes on screen, with approval):
   - 0% repeated frames;
   - the same motion advance as today.
5. **Only then** audio (pipe into FFmpeg, measure A/V sync with
   `av-sync.py`), segments and replay, colour tags, 2x and fallbacks, and
   tests on NVIDIA-only and AMD systems.

**Stop rule.** If step 1 shows no gain beyond the spread and step 2 does not
reach 5 points, the native path does not pay for a rewrite on this hardware,
and the report says so.
