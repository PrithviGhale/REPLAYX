# Recording overhead: root-cause investigation and fix (November 12, 2026)

**Hardware:**

- RTX 4070 Laptop GPU renders the game.
- The Intel Iris Xe drives the built-in 2560x1600 240 Hz panel (Optimus).
- i9-13900H, AC power.

**Measurements** are A/B/A runs: no capture → capture → no capture. They use
the synthetic game fixture (`replayx-gameproxy`) in several workload shapes,
with the drift, input and coverage checks described in
[WGC_CAPTURE_INVESTIGATION_2026-11-12.md](WGC_CAPTURE_INVESTIGATION_2026-11-12.md).

**Bottom line:**

- **This work found and removed one large, measured cost in REPLAYX's WGC
  recorder.** The previous build captured at twice the output rate, and each
  captured frame costs full-screen GPU copies. The new default captures once
  per output frame and switches itself to 2x only when that would drop
  frames. Synthetic overhead in a high-FPS game fell from **6.8–8.6% to
  4.1–4.2%**, with 0% repeated frames.
- **The cause of the 25–35% loss you see in Fortnite is not established.** No
  synthetic workload reproduced it; the largest was 9.6%. A single real
  Fortnite session with the prepared script is needed to measure it (section 6).
- **No Fortnite improvement is claimed.**

## 1. Confirmed causes of recording overhead (synthetic, this laptop)

| # | Finding | Evidence |
| --- | --- | --- |
| 1 | **Desktop Duplication is expensive on an Intel-driven panel with an NVIDIA-rendered game.** While duplication is active, DWM produces a duplicated surface for every desktop update: about 500/s full screen, 240/s windowed. | DWM's Intel 3D load went 0% → 62%. Game FPS loss: 19–35% (24% with capture only, no encoder). |
| 2 | **WGC capture itself is nearly free. The cost is per delivered frame.** | WGC capture-only probe: −0.2% to 2.2%. Each delivered frame costs a Windows capture copy plus a full-screen shader pass in FFmpeg's `gfxcapture` (source audited: 2-frame pool, consumes everything delivered). |
| 3 | **The previous WGC build delivered 120 frames/s for a 60 FPS recording**, which doubles finding 2. | Raw FFmpeg chain, WGC + Quick Sync, about 550 FPS game: **5.75% at 120/s vs 2.5% at 60/s**. Of a 7.5% total, VPP scaling and Quick Sync encoding added about 1%. |
| 4 | **At 1x, Windows' update floor starves games between 60 and 120 FPS.** | About 95 FPS game: 18% repeated frames at 1x, 2.4–2.9% at 2x. The FFmpeg duplicate counter shows it clearly at 1x (about 240 extra duplicates in 20 s). |
| 5 | **NVENC on an Intel-driven display is costly and fragile.** Frames take Intel VPP → system memory → upload to the game's GPU. | 5.55% FPS loss (CPU-bound game); 9.6% (heavy CPU and GPU). With the NVIDIA GPU saturated it fell to 0.75x real time, repeated 89.6% of frames, and once ran out of memory. |
| 6 | **"Zero-copy" WGC → NVIDIA → NVENC is worse.** Windows copies every full-size frame across adapters onto the game's GPU. | 29.9% FPS loss. Kept as a development switch only. |

## 2. Rejected hypotheses

| Hypothesis | Result |
| --- | --- |
| Raised capture GPU priority competes with the game | WGC + Quick Sync with priority off: 3.4%, against 3.2% with it on. No difference. |
| DWM composition by itself caps games paced by a frame-latency wait | Waitable-swapchain game: 288 FPS before capture and 282 FPS with capture, with 100% composition. Not capped near 240. |
| Composition caps games with a 2-buffer flip chain | 2-buffer game: about 550 FPS during capture, with 100% composition. Not capped. |
| Composition (overlay → DWM) is the main cost | Both APIs compose 100% of frames. WGC capture-only costs about 0–2%. The windowed game is composed with no capture at all, yet DDA still costs 19% there. |
| Encoder settings (bitrate, lookahead, AQ) | Earlier reports: not the bottleneck. Capture without an encoder costs the same with DDA. |
| DDA ReleaseFrame timing | November 11: no gain. |
| CPU/iGPU shared power budget | **Inconclusive.** A heavy multi-thread CPU workload showed the CPU clock drifting from 115% to 92% of base even without capture, and the workload ended up GPU-bound. Not reproduced, not excluded. |

## 3. Code changes in this round

1. **Adaptive WGC capture rate.**
   - Files: `media.rs` (`wgc_rate_multiple`, `Tuning::wgc_floor_us`) and
     `engine.rs` (`wgc_missing_slots`, `Pipeline::wgc_adaptive`).
   - WGC now captures at 1x with a 16 ms update floor. A 16.7 ms floor
     sometimes slipped one 240 Hz refresh.
   - The engine watches FFmpeg's output-grid duplicates in 10 s windows. Two
     consecutive windows with 3–35% duplicates mean missed slots, as opposed
     to a static screen or a game below 60 FPS. The session then switches to
     2x (`Settings::wgc_boost`, runtime only) through the existing bounded
     recovery, which keeps the replay buffer.
   - Overrides: `REPLAYX_VIDEO_TUNING=wgc_oversample=2` forces 2x;
     `wgc_floor_us` changes the floor.
2. **WgcGpu path** (`d3d11va` device on the NVIDIA adapter, `gfxcapture`
   scaling, NVENC zero-copy). Implemented and measured slower, so it is behind
   `wgc_gpu=1` only.
3. **Accurate diagnostics.** `captureMethod`, `requestedCaptureMethod`,
   `captureFilter` and `captureFallback` in the copied JSON;
   `Status.capture_filter` and `capture_fallback`; Performance-page badges, and
   an explanation of the Intel-to-NVENC transfer path.
4. **Measurement tools.**
   - The game fixture gained `--waitable`, `--buffers`, `--cpu-work`,
     `--workers`, `--windowed` and `--topmost`.
   - The benchmark gained raw-FFmpeg stage configurations, per-phase CPU clock
     telemetry, and focus and coverage guards.
   - `scripts/fortnite-session.py` is the single consolidated real-game
     measurement.

## 4. Before and after (synthetic; not game results)

| Workload | Previous WGC build (2x) | New default (adaptive) |
| --- | ---: | ---: |
| High-FPS game (about 550 FPS), WGC + Quick Sync | 6.8–8.6% FPS loss, 0% repeated, 2–6% jumps | **4.1–4.2%**, 0–0.14% repeated, 0–0.14% jumps |
| GPU-heavy game (about 95 FPS) | 0.05–0.3%, 2.4–2.6% repeated | 1.0%. Switched to 2x after about 26 s; final clip 2.9% repeated |

Desktop Duplication (alpha.8 default) on this display: 19–35%. Not changed:
DDA stays available and remains the default capture method until WGC is
verified in real games.

## 5. Quality and reliability

| Test | Result |
| --- | --- |
| Rust | 164 passed, 1 ignored (opt-in benchmark); Clippy `-D warnings` and rustfmt clean |
| Frontend | 53/53; TypeScript clean |
| Python | 9/9 |
| Codec matrix through WGC (adaptive) | 6/6: H.264/HEVC/AV1, 1280x960 and 1440x1080 regions, exact policy; all clips decode clean |
| 10-minute WGC soak (adaptive, Quick Sync) | Details below |
| Adaptive switch | One bounded restart (about 2 s); replay buffer preserved; the clip saved afterwards was clean |
| Updater end-to-end | 42/42 |

The 10-minute soak results:

- 36,033 frames at 59.99 FPS; 13 output-grid drops; 21 duplicates (start-up).
- No false switch to 2x.
- Saves ready in 1000 / 927 ms; both clips decode clean.
- Clip: **0% repeated frames**, no freeze, 0% jumps.
- Encoder speed never below 0.975x; video lag at most 127 ms.

**Limitations:**

- The switch to 2x leaves a gap of about 2 s in the replay buffer. A recording
  in progress at that moment is closed and continues as a new part (the
  existing recovery behaviour). It happens at most once per session.
- Audio was not part of these video tests (silent). Audio capture is
  independent of the capture API.

## 6. The one real-game test that remains

From the repository root, with Fortnite already running in the same
Creative scene:

```powershell
py -3 scripts/fortnite-session.py --presentmon test-output\fortnite-live-alpha8\PresentMon-2.6.0-x64.exe --cap 360
```

- Run it from an **administrator** terminal if you want frame times. That is
  your decision; without it, drop `--presentmon` and you get resource
  telemetry only.
- Quit the REPLAYX app first.
- After the 15 s countdown, switch to Fortnite and stand still for about 9
  minutes.

It records these phases automatically, 45 s each:

- A1 off,
- **B1 new adaptive WGC + Quick Sync**,
- A2 off,
- **B2 the previous 2x WGC**,
- A3 off,
- **B3 WGC + NVENC**,
- A4 off.

It writes `summary.json` with, per phase:

- Fortnite FPS, 1% lows and p95/p99 frame times (PresentMon),
- CPU effective clock,
- per-process GPU engines (Fortnite's NVIDIA 3D/copy, DWM's Intel 3D, FFmpeg),
- NVIDIA power and clocks,
- the recorder's actual path.

It never touches the game, and it stops at once if Fortnite loses focus.
Compare NVIDIA Instant Replay separately by toggling it yourself in the same
scene. Those numbers decide whether the remaining Fortnite gap is the
recorder's GPU work (it should shrink from B2 to B1), the CPU clock (it
should show in the CPU-clock column), or presentation (PresentMon's
`PresentMode`).

## 7. Limits that this work cannot remove

- **Public capture APIs make DWM compose the captured screen.** Both WGC and
  Desktop Duplication do. A full-screen game then leaves the overlay or
  independent-flip path for as long as REPLAYX captures.
- **NVIDIA Instant Replay captures inside NVIDIA's driver** and does not force
  composition. The equivalent interfaces (NvFBC) are not available to
  third-party applications on GeForce, and in-game hooks are excluded by
  design (anti-cheat).
- On this Optimus configuration, every REPLAYX frame is captured from the
  Intel-driven display:
  - Quick Sync keeps that work on the Intel GPU.
  - NVENC adds a transfer to the game's GPU.
  - Neither path can read the game's frames before they cross to the Intel
    GPU, as NVIDIA's own recorder can.
