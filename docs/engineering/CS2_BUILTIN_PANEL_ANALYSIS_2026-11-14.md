# CS2 on the built-in panel: what the four-configuration run shows (November 14, 2026)

**Data:** `test-output/cs2-session-20261114-111632`, a real CS2 run by the
owner.
- PresentMon frame times.
- Telemetry once a second: per-process CPU and GPU engines, CPU effective
  clock, NVIDIA clocks and power.
- The recorder's own counters.

**Recorder:** `1f87874`, test recorder (`replayx-cli capture-test`), 1920 ×
1080 at 60 FPS, H.264 Balanced, silent.

**Display:** built-in 2560 × 1600 at 240 Hz, driven by the Intel GPU. CS2
renders on the RTX 4070.

## Per-phase data (45 s each, A = recorder off)

| Phase | CS2 FPS | 1% low | CPU busy / frame | NVIDIA GPU busy / frame | CPU clock (% of base) | NVIDIA power | Recorder |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| A1 | 428.3 | 204 | 1.84 ms | 1.87 ms | **127.3** | 69 W | — |
| B1 WGC (normal) | 331.0 | 149 | 2.39 ms | 2.00 ms | **123.6** | 63 W | 1x, never switched to 2x; 11 duplicates |
| A2 | 376.3 | 188 | 2.11 ms | 1.93 ms | 109.0 | 68 W | — |
| B2 WGC pinned 1x | 277.3 | 133 | 2.85 ms | 2.07 ms | 95.0 | 55 W | 64 duplicates |
| A3 | 382.6 | 190 | 2.07 ms | 1.92 ms | 108.3 | 66 W | — |
| B3 WGC 12 ms floor | 276.7 | 131 | 2.86 ms | 2.07 ms | 97.2 | 53 W | 14 duplicates |
| A4 | 383.0 | 192 | 2.07 ms | 1.93 ms | 108.6 | 67 W | — |
| B4 WGC + NVENC | 269.5 | 130 | 2.95 ms | 2.11 ms | 99.2 | 56 W | 24 duplicates |
| A5 | 383.8 | 184 | 2.07 ms | 1.92 ms | 108.8 | 67 W | — |

Intel GPU engines while recording:

| Engine | Quick Sync phases | NVENC phase |
| --- | --- | --- |
| DWM 3D | 13.6–14.1% | 14.8% |
| FFmpeg 3D | 10.8–11.0% | 22.7% |
| FFmpeg video (VDEnc encode) | 16.0–18.0% | — |
| NVENC on NVIDIA | — | 21.3% |

**FFmpeg CPU:** 1.4–1.5% of the whole CPU (2.95% in the NVENC phase).

**Presentation:** CS2 stayed on "Hardware Composed: Independent Flip" for 100%
of frames in every phase, recording or not.

## Answers

1. **Why the CPU clock was 123.6% in B1 and 95–99% later.**
   - B1 ran while the CPU was still in its short turbo window (PL2).
   - A1 (127.3%) and B1 (123.6%) are the only phases above 110%. From A2
     onward every recorder-off phase sits at 108–109% and every recording
     phase at 95–99%. This is the sustained limit (PL1).
   - It is **benchmark order**, not the configuration.
2. **Power limits, thermals, frequency.**
   - The step at A1 → A2 is the turbo window ending: CPU clock −14%, CS2
     −12% with the recorder off.
   - NVIDIA temperatures (73–78 °C) and power stayed steady.
   - **Within the sustained limit, recording lowers the CPU clock by 9–12%**
     (108–109% → 95–99%). During turbo it lowered it only 3% (127 → 124%).
     The Intel GPU's work comes out of the same package power budget as the
     CPU's cores. It costs CPU clock once that budget is the limit.
3. **Did WGC switch to its doubled rate?**
   - No. `recovery_sessions` is 0 in every recording phase.
   - Captured frames were about 80/s at 1x: 1056–1161 discarded per phase,
     as expected for a 12.5 ms floor on 240 Hz.
   - The 2x hypothesis does not explain this run.
4. **Why pinned 1x had more duplicates.**
   - B1 and B2 had the same capture settings (B1 never switched to 2x).
   - The difference is CS2's frame rate: 331 FPS in turbo against 277 FPS
     sustained.
   - A 12.5 ms floor is exactly three 240 Hz refreshes. When CS2 delivers
     no new frame at one of those refreshes, the capture slips to a fourth
     (16.7 ms) and an output slot can be missed.
   - The 12 ms floor run, at the same 277 FPS, had **14** duplicates
     against 64. That supports this explanation. As a share of the about
     3,300 recorded frames per phase, that is about 1.9% repeated frames
     against 0.4%: a smoothness fix worth keeping, at no FPS cost.
5. **Is Quick Sync versus NVENC fair?** It is the only NVENC path on this
   display. Frames must be captured and scaled on the Intel GPU first:
   - Intel 3D load doubled (11% → 22.7%) for the scaling and the download
     to system memory;
   - the scaled frames then cross to NVIDIA.

   NVENC therefore cannot take work off the Intel GPU here, which is the
   resource that matters. It was 2.7% slower than Quick Sync at the same
   power state. The comparison is fair for this display; it says nothing
   about NVIDIA-driven displays.
6. **Why A1 was 428 FPS and later off phases about 380.** The turbo window
   (answer 2): CPU clock 127% → 108–109%, and CPU busy per frame 1.84 →
   2.07 ms.
7. **Scene and settings against VProf.**
   - The recorder-off FPS here (376–428) is far above VProf's 281.9, so this
     is not the same scene. VProf's benchmark moves the camera; this run
     held still.
   - Absolute FPS cannot be compared; the relative cost can: VProf −33.8%;
     this run, sustained phases, −26.9% to −29.7%; B1 in turbo −17.7%.
   - VProf's excluded-frame counts were not available, so that difference
     is not accounted for.

## The bottleneck

Recording makes **CS2's CPU work slower**:
- CPU busy per frame rose 2.07 → 2.85–2.95 ms (+38–42%) in the sustained
  phases;
- the NVIDIA GPU's time per frame rose only 7%;
- REPLAYX's own CPU use is about 1.5%.

The cause is the work the capture puts on the **Intel GPU**, which shares the
CPU's package power budget, last-level cache and memory controller:

| Work on the Intel GPU while recording | Engine load | Controllable by REPLAYX? |
| --- | --- | --- |
| DWM rendering the screen for capture | about 14% (3D) | No (Windows) |
| WGC copies, about 80/s at 2560 × 1600, scaled in `gfxcapture`'s shader | Part of FFmpeg 3D, 11% | Partly: rate only |
| VPP colour conversion, run on all ~80 frames/s although 60 are kept | Part of FFmpeg 3D, 11% | **Yes**: drop surplus first; fixed-function scaler |
| Encoding (VDEnc, fixed-function) | 16–18% (video) | Already the lowest-power encoder |

**Two parts of the cost.**
- **Package power:** in the sustained limit, about −10% CPU clock.
- **Memory and cache contention:** the rest of the +38% CPU time. Even in
  turbo, where the clock barely fell, recording added 30% CPU time per frame.

Both scale with the Intel GPU's work per second, so the lever is less Intel
GPU work per captured second, without changing the 1080p60 output.

## Next test (one hypothesis, about 12 minutes)

**Hypothesis.** Dropping the ~20 surplus frames/s before VPP, and running VPP
on the fixed-function scaler (SFC) instead of the 3D engine, lowers FFmpeg's
Intel 3D load. That gives back CPU clock and CPU time per frame in the
sustained power state. The output stays the same: 1080p60, same bitrate,
frame nearest each slot.

- `wgc_early_drop=1`: drop before VPP, using the same keep-nearest rule as
  the 2x path. Opt-in (`db9507e`).
- `wgc_early_drop=1,vpp_scale_mode=1`: plus SFC. It ran in the bundled
  FFmpeg on this Intel GPU in an offline encode.

**Run (owner, CS2 on the built-in display, same spot, admin PowerShell):**

```powershell
cd "C:\Users\Prithvi\Desktop\VS CODE\replayx-perf"
py -3 scripts\bench\fortnite-session.py --game cs2 --warmup 150 --cap 0 `
  --configs wgc-auto,wgc-early-sfc,wgc-early,wgc-auto `
  --presentmon "..\REPLAYX\test-output\fortnite-live-alpha8\PresentMon-2.6.0-x64.exe"
```

- The 150 s warm-up is recorder off and not measured. It lets the CPU reach
  its sustained limit before the first phase, so every phase is compared in
  the same power state.
- Today's path runs first and last, so drift is visible.

**Success criteria.**
- The candidate lowers FFmpeg's Intel 3D load.
- CS2's CPU busy time and FPS improve beyond the two baseline phases' spread.
- Duplicates stay at or below the baseline.
- The saved clips decode at 60 FPS.

If neither candidate helps, the remaining cost is DWM's capture rendering and
the encoder, which REPLAYX cannot reduce without lowering the output.

## Follow-up: early drop + Intel SFC (`wgc_early_drop=1,vpp_scale_mode=1`)

### Real CS2, warmed up (`cs2-session-20261114-115824`, owner)

Recorder-off phases: 300.1–302.6 FPS.

| Recording phase | CS2 FPS | Cost vs neighbouring off phases | CPU clock | CPU busy / frame | FFmpeg Intel 3D | Intel video processing |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Normal WGC (1st) | 227.8 | −24.4% | 94.9% | 3.51 ms | 11.2% | — |
| Early drop + SFC | 232.5 | **−23.2%** | 97.6% | 3.46 ms | 7.6% | 7.2% |
| Early drop only | 228.2 | −24.4% | 95.8% | 3.50 ms | 11.2% | — |
| Normal WGC (2nd) | 226.2 | −24.9% | 94.8% | 3.56 ms | 11.1% | — |

**Video lag.** It peaks at 107–144 ms in every configuration and returns
to 0. This is pipeline depth, not growing lag; the encoder stayed at ≥0.97×
real time.

**Clips.** All 3,480 frames, 58.0 s, 60 FPS.

**Colour conversion quality.** 300 frames of the CS2 clip were converted
offline; both against a software BT.709 reference:

| Conversion | PSNR | SSIM |
| --- | ---: | ---: |
| Default VPP | 61.3 dB | 0.9996 |
| SFC | 61.5 dB | 0.9996 |

**Counters.** FFmpeg's drop and duplicate counters do not see frames the
`fps` filter selects. They read 0 for early drop, so pacing must be measured
from the frames themselves (below).

### Synthetic pacing test (built-in 2560 × 1600 at 240 Hz)

Test workload: CPU-bound, about 440 FPS, frames stamped with their render
time. Two runs each, shuffled. Recording: 1080p60, H.264 Balanced, Quick
Sync low-power.

| | Normal WGC | Early drop + SFC |
| --- | --- | --- |
| Game FPS cost (A/B/A) | 1.51%, 1.71% | 1.25%, 1.51% |
| P99 frame time while recording | 2.71–2.72 ms | 2.69–2.75 ms |
| Clip frames / rate | 1320 at 60.0 (both runs) | 1320 at 60.0 (both runs) |
| Repeated frames (from timestamps) | 0.0%, 0.0% | 0.0%, 0.0% |
| Longest freeze | 0 ms | 0 ms |
| Uneven motion (advance > 1.5 slots) | 19.7%, 23.6% | 19.9%, 21.4% |
| Game-time advance per frame, p50 / p95 / max | 13.75 / 25.5–25.75 / 27.25–28.0 ms | 13.75 / 25.5 / 27.25–27.5 ms |
| Captures discarded after conversion | 20.7/s | 0 (selected before conversion) |

**Pacing is the same.** Repeats, freezes and the motion-advance distribution
match. Early drop hides nothing: the timestamps prove each recorded frame is
a distinct game frame.

The uneven motion (about 20% in both) belongs to WGC at 1x on a 240 Hz
panel. Captures land on 12.5 ms or 16.7 ms refreshes, and a 60 FPS slot
sometimes takes a frame 1.5 slots newer. It does not come from early drop.

This synthetic workload costs only about 1.5% to record. It cannot reproduce
CS2's 24%, so it validates quality, not the FPS gain.

### Verdict

- Early drop + SFC keeps 1080p60, bitrate, pacing and colour quality.
- It lowered the Intel 3D load by a third and gave a small real-CS2 gain:
  1.4 points of the ~24% cost, in one run.
- Recommended as the Quick Sync path's conversion once one more CS2
  comparison confirms the gain. It stays opt-in until then.
- It does not change the larger picture: most of the cost is Windows'
  compositor and the capture itself. The capture-only phase measures that.
  Its first attempt failed because the probe refused CS2; fixed in
  `e425d8d`.

## Capture-only run (`cs2-session-20261114-125212`, owner)

Warmed up. The five recorder-off phases: 360.2–365.7 FPS.

| Phase | CS2 FPS | Cost | CPU clock | CS2 CPU busy / frame | Intel 3D: DWM + REPLAYX | Intel video engines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Off (mean of 5) | 362.8 | — | 106–109% | 1.55–1.62 ms | 0 | 0 |
| **Capture only** (probe: 77.8 deliveries/s, one 2560 × 1600 GPU copy each) | 297.6 | −18.0% | 104.2% | 2.34 ms | 25.8 + 8.6 | — |
| Normal WGC recording (2 runs) | 256.4 / 258.0 | −29.1% | 95.7–95.8% | 2.94–2.97 ms | 16.7 + 11.1 | 16.6% encode |
| Early drop + SFC | 265.8 | −26.7% | 98.8% | 2.81 ms | 17.4 + 7.7 | 17.0% encode + 7.4% SFC |

### 1. How representative the capture-only phase is

**It approximates the first stage of REPLAYX's path, not a pure Windows
floor.**
- The probe takes WGC frames at the recorder's rate (77.8/s against about 74/s
  for `gfxcapture`).
- Each frame gets one full-resolution GPU copy on the Intel GPU: 0.96 ms, 8.6%
  of the 3D engine.
- REPLAYX's `gfxcapture` instead runs one shader pass per frame that reads the
  full frame and writes 1080p. The read traffic is similar; the write is
  smaller.

**DWM's load differs between the phases.**
- DWM's Intel 3D load was 25.8% under the probe against 16.7% under the
  recorder.
- CS2 also ran faster under the probe (297 against 257 FPS).
- DWM's capture work therefore appears to follow the screen's update rate,
  not only the delivery rate. That is a hypothesis; it is not separated yet.

**18% is not proven to be the floor.**
- A consumer that touches no pixels was not measured.
- Window capture (`CreateForWindow` on the detected game) was not measured in
  a real game.
- Windows' share alone may be lower than 18%.

### 2. What REPLAYX adds beyond capture

Full recording costs **11 points more** than capture only (29.1% against
18.0%). Its signature is power:
- The CPU clock fell about 3 points with capture only, and about 12 points
  with full recording.
- CS2's CPU time per frame: 2.34 ms against 2.95 ms.

The work behind it, all on the Intel GPU in the CPU's package:

| Work | Engine | Per frame |
| --- | --- | --- |
| `gfxcapture` shader scaling | 3D | Every delivered frame (about 74/s) |
| VPP BGRA → NV12 conversion | 3D | Every delivered frame, including ~14/s discarded later |
| VDEnc encode at 60 FPS | Video | 16.6% |

FFmpeg itself used about 1.4% of the CPU. There are no CPU copies and no
cross-adapter transfers on this path (Quick Sync zero-copy).

**Moving VPP to the fixed-function scaler (SFC):**
- Intel 3D load fell by 3.4–3.5 points.
- The video-processing engine took 7.4 points.
- The CPU clock rose about 3 points.
- CS2 gained +2.4% and +3.3% in two warmed runs.
- Fixed-function units do the same conversion at lower power. The contention
  really drops; this is not only load moving between engines.

### 3. Early drop: not a default candidate (code review and offline test)

**Offline test.**
- Input: about 80 deliveries/s with ±3 ms jitter and a 1 s gap, through
  today's path and through early drop.
- Today's path: 1201 output frames, 60 duplicates (exactly the gap).
- Early drop (`fps=60:round=near`): 1200 frames, 563 dropped, **244
  duplicates**.

**Why.** Today's constant-rate output moves a spare capture into a
neighbouring empty slot; the `fps` filter cannot.

**Not seen on screen.** The synthetic pacing test delivered frames very
regularly, so it did not show this.

**The counter is blind as well.** Early drop's duplicates never reach FFmpeg's
duplicate counter. The engine's switch to 2x relies on that counter for games
between 60 and 120 FPS, so starvation could put repeats into clips unnoticed.

**Also rejected:** a `select` variant (keep the first frame of each slot) gave
the same 244 duplicates.

**Early drop alone gave no CS2 gain.** The gain belongs to SFC, which leaves
frame selection exactly as today. The new candidate is `wgc-sfc`
(`vpp_scale_mode=1` alone). Quality is verified: 61.5 dB PSNR against a
BT.709 reference, the same as default VPP. Pacing is unchanged by
construction. It still needs one CS2 confirmation run.

### 4. Architecture: what a native capture-to-encode pipeline could recover

**Ceiling.** A REPLAYX-owned pipeline could remove most of the 11 points
REPLAYX adds:
- WGC capture in REPLAYX;
- one fixed-function pass that scales and converts to NV12 (D3D11
  VideoProcessor, the same hardware as SFC);
- this pass runs only on frames chosen with CFR-equivalent selection (fill
  empty slots from neighbours), never on surplus captures;
- encoding through a hardware encoder.

**What it cannot remove.** Not Windows' capture work, and probably not all of
the 18%.

**Hybrid laptops.** The scaled 3 MB NV12 frame can cross to the NVIDIA GPU
over a GPU-to-GPU copy, so NVENC encodes on the discrete GPU. The encoder's
power then leaves the CPU's package, which is today's main lever.
- Today's NVENC option cannot do this: FFmpeg moves the frame through system
  memory with more Intel work (22.7% 3D), which is why it lost to Quick Sync.

**Estimate.** Unproven: about half to two-thirds of the 11 points, which puts
recording at about 22–24% on this panel instead of 29%.

**Known Windows limits.**
- WGC requires DWM to render capture frames; that showed as 17–26% Intel 3D
  load in these runs.
- NVIDIA's in-driver capture (NvFBC) is not available to applications on
  GeForce.
- On a display driven by the Intel GPU, the capture always happens there.

## Recommendation: the next highest-impact work

### 1. Measure the native pipeline's ceiling before building it

Small: about 1–2 days, a development probe only.

**What.** Extend `replayx-wgc-profile` with the native stages:
- (a) capture only, no GPU work;
- (b) plus a D3D11 VideoProcessor scale and NV12 conversion of 60 chosen
  frames/s;
- (c) plus a GPU-to-GPU copy of each NV12 frame to the NVIDIA GPU;
- (d) window capture of the game instead of the monitor.

**How to measure.** One warmed CS2 session, about 15 minutes, with phases
(a)–(d) beside today's recorder.

**Success.**
- If (b) and (c) stay within about 3 points of (a), a native pipeline can
  recover most of the 11 points.
- If (d) is clearly below (a), window capture of the detected game is the
  bigger lever.

**Risk.** Low; nothing in REPLAYX changes.

### 2. If confirmed, build the native video path

Large: about 2–4 weeks.

**What.** REPLAYX-owned WGC capture → VideoProcessor → Media Foundation
hardware encoder (vendor-neutral: Quick Sync, NVENC and AMF all ship as
encoder MFTs) → H.264 bitstream piped to FFmpeg. FFmpeg keeps audio, muxing,
segments and replay.

**Risks.**
- A/V sync.
- Per-vendor encoder MFT behaviour.
- Cross-adapter interop on hybrid systems.
- Keeping today's path as the fallback.

**Success, against today's recorder in the same warmed session.**
- CS2 recording cost within ~3–5 points of capture-only.
- 0% repeated frames and the same motion advance in the timestamped
  workload.
- A/V offset within ±1 frame (`scripts/bench/av-sync.py`).
- The same PSNR/VMAF at the same bitrate.
- Clean fallback on AMD, Intel-only and NVIDIA-only systems.

### 3. Meanwhile

Confirm `wgc-sfc` in one short CS2 run (two phases), then consider it as the
Quick Sync default.
