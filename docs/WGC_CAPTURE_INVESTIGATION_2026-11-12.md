# Windows Graphics Capture vs Desktop Duplication (November 12, 2026)

**Scope.** Every number in this report was measured on the laptop's
**built-in display**:

- 2560x1600 at **240 Hz**.
- Driven by the **Intel** iGPU (vendor 0x8086), in Optimus mode.
- The synthetic game renders on the RTX 4070 Laptop GPU.
- AC power, Windows 11 build 26300.

The display, GPU mode, refresh rate and power settings were not changed. The
external 250 Hz NVIDIA-driven monitor was **not** tested in this round.
Results there may differ: the November 11 report measured about 4–6% for
Desktop Duplication on that display.

**No real game was measured.** The Fortnite FPS problem is **not** shown to be
fixed. See section 9.

## 1. Summary

| | Desktop Duplication (current default) | Windows Graphics Capture (new, opt-in) |
| --- | ---: | ---: |
| Light high-FPS game (about 690 FPS), recording | **35.0% ± 2.4** FPS loss | **3.3% ± 1.0** |
| CPU-limited game (about 327 FPS), recording | **7.4% ± 1.2** | **1.65% ± 0.2** |
| GPU-limited game (about 108 FPS), recording | 0.9% ± 0.8 | 0.6% ± 0.5 at 1x; 1.2% ± 0.2 at 2x |
| Windowed game, capture only | **19.2% ± 3.0** | −0.1% ± 0.6 (window), 2.9% (monitor) |
| Recording smoothness, light game | 1.8–12.6% repeated frames | 0% repeated frames |

All figures are synthetic, with 95% confidence intervals over 3–5 valid runs
per configuration, in randomized order.

**Root cause on this hardware:** Desktop Duplication makes DWM produce a
duplicated desktop surface **for every desktop update**:

- about 500 per second with a full-screen game,
- 240 per second for a windowed one,
- all on the Intel GPU, at 2560x1600.

DWM's Intel 3D engine use goes from **0% to about 62%**, and the
NVIDIA-rendered game cannot present faster than the Intel GPU keeps up.
**NVENC and Quick Sync are not the cause.** Capture without any encoder costs
the same.

WGC lets the capturing app limit updates (60 or 120 per second), so DWM's load
stays at about 10–14%.

**Decision.** WGC is integrated as an **experimental, opt-in** capture method:

- Setting: Capture → Capture method.
- It falls back to Desktop Duplication automatically if it cannot start.
- **The default remains Desktop Duplication** until WGC is verified on real
  games and on the NVIDIA-driven external display.
- No version bump.

## 2. What was built

| Piece | Purpose |
| --- | --- |
| `src-tauri/src/bin/wgc_profile.rs` (`replayx-wgc-profile`) | Isolated WGC capture with no encoder. Details below. |
| `REPLAYX_VIDEO_TUNING=capture_api=wgc` | Benchmark switch: REPLAYX's real pipeline with FFmpeg's `gfxcapture` (the pinned n9.0.2 build includes it) instead of `ddagrab`. |
| Setting `capture_api` (`"dda"` default, `"wgc"`) | User-facing opt-in. Details below. |
| `replayx-gameproxy --windowed 1 --topmost 1` | 1280x720 windowed workload. Topmost keeps the fixture above other windows when Windows refuses it the foreground. |
| `scripts/overhead-bench.py` | Additions listed below. |
| `scripts/capture-matrix.py --capture-api wgc` | Codec and region matrix through WGC. |

`replayx-wgc-profile` uses only documented WGC/Direct3D 11 APIs:

- `GraphicsCaptureItem` for a monitor (`CreateForMonitor`) or a window
  (`CreateForWindow`).
- A free-threaded 2-buffer `Direct3D11CaptureFramePool`.
- `MinUpdateInterval`.
- Each sampling slot takes the newest frame and makes one GPU copy; there is
  no CPU readback.
- It handles resizes (`Recreate`) and `Closed`, and reports the frame
  interval, the latency from Windows' timestamp, the GPU copy span and empty
  slots.
- The Windows capture border is on by default.

The `capture_api` setting:

- Uses `gfxcapture` with the same D3D11 BGRA frames, filters and encoder
  settings as `ddagrab`.
- Is part of the remembered and cached video-path keys.
- Falls back to Desktop Duplication when no WGC path starts, with the reason
  on the Performance page.
- Exposes `Status.capture_api`.

`scripts/overhead-bench.py` gains:

- WGC configurations, counterbalanced order and p95/stdev frame times.
- Input, foreground/coverage, GPU-clock and drift invalidation.
- The display configuration in every report.
- A fix for a quadratic median that stalled reports at very high frame rates.

## 3. Method

The fixture is a D3D11 flip-model game proxy (`replayx-gameproxy`) on the
NVIDIA GPU. It presents to the Intel-driven panel, as a game does on an
Optimus laptop. Each run has three phases:

- A1: no capture.
- B: capture or recording for 20 s.
- A2: capture stopped.

The FPS drop compares B with the mean of A1 and A2. A run is **invalid** if:

- A1/A2 drift exceeds 5%,
- keyboard or mouse input occurred,
- the GPU clock fell more than 10% during B, or
- the fixture did not cover the screen.

Presentation mode comes from `IDXGISwapChainMedia::GetFrameStatisticsMedia`.
PresentMon was not used: it needs an administrator token, which was not
obtained or bypassed.

Workloads:

| Batch | Fixture | Valid runs |
| --- | --- | ---: |
| Light high-FPS | full screen, about 690 FPS | 35/35 |
| CPU-limited | 3 ms CPU per frame, about 327 FPS | 20/20 |
| GPU-limited | heavy shader, about 108 FPS | 20/20 |
| Windowed | 1280x720, about 2370 FPS | 14/15 (one run had user input) |
| Oversampling follow-up | GPU-limited and light | 28/28 |
| Counters | one profiling run per configuration | 6/6 |

**Invalid or discarded runs:**

- Seven early smoke attempts were refused because Windows' focus-stealing
  protection kept VS Code or Opera in front. The guard stopped them before any
  capture, and they are kept as `refused-*` folders.
- The first windowed batch stalled in the report statistics; it was rerun in
  full.
- One windowed run had user input.

**Focus confound.** In the light, CPU and GPU batches Windows did not give the
fixture the foreground: it was topmost but not focused. In the oversampling
batches it was focused. Focus changed the absolute numbers, but not the
ranking:

| Fixture state | Desktop Duplication recording | WGC recording |
| --- | ---: | ---: |
| Not focused | 35.0% | 3.3% |
| Focused | 25.0% | 3.0% |

## 4. Results

The DDA configurations are the current REPLAYX path. In every batch both paths
used the same encoder (Intel Quick Sync low-power, GPU zero-copy) and the same
1920x1080 output, so **only the capture API differs.**

### Light high-FPS game (about 690 FPS, 5 runs each)

| Configuration | FPS drop | 1% low drop | p99 frame time | Composed during B | Clip repeated / jumps |
| --- | ---: | ---: | --- | ---: | --- |
| No capture | −0.45 ± 1.8% | — | 1.91 → 1.91 ms | 0% | — |
| DDA capture only | 24.0 ± 2.6% | 54% | 1.94 → 3.84 ms | 100% | — |
| **DDA + encoder (current)** | **35.0 ± 2.4%** | 65% | 1.88 → 5.15 ms | 100% | 1.8% / 8.2% |
| WGC monitor capture only | 1.9 ± 1.0% | 10% | 1.87 → 2.06 ms | 99% | — |
| WGC window capture only | 1.2 ± 2.5% | 18% | 1.89 → 2.18 ms | 99.5% | — |
| **WGC monitor + encoder** | **3.3 ± 1.0%** | 19% | 1.89 → 2.34 ms | 100% | 0% / 0.5% |
| WGC window + encoder | 2.8 ± 1.1% | 18% | 1.92 → 2.34 ms | 100% | 0% / 0.45% |

### CPU-limited game (about 327 FPS, 4 runs each)

| Configuration | FPS drop | 1% low drop | p99 frame time | Clip repeated / jumps |
| --- | ---: | ---: | --- | --- |
| No capture | −0.06 ± 0.1% | — | 3.28 → 3.25 ms | — |
| DDA capture only | 2.8 ± 0.25% | 6% | 3.30 → 3.52 ms | — |
| **DDA + encoder (current)** | **7.4 ± 1.2%** | 25% | 3.27 → 4.31 ms | 0% / 7.4% |
| WGC capture only | 0.7 ± 0.2% | 2% | 3.27 → 3.34 ms | — |
| **WGC + encoder** | **1.65 ± 0.2%** | 7% | 3.25 → 3.52 ms | 0% / 0% |

### GPU-limited game (about 105 FPS, 4 runs each)

| Configuration | FPS drop | Clip repeated / jumps |
| --- | ---: | --- |
| No capture | −0.04 ± 0.3% | — |
| DDA + encoder (current) | 0.9 ± 0.8% (0.4 ± 0.6% in the follow-up) | 0–0.04% / 6.5–7.1% |
| WGC + encoder, 1x (60/s) | 0.6 ± 0.5% (0.0 ± 0.3%) | **12–15.5%** / 12.7–17.5% |
| **WGC + encoder, 2x (120/s)** | 1.2 ± 0.2% | **0.39%** / 1.1% |

### Windowed game (1280x720, about 2370 FPS, 3 runs each)

| Configuration | FPS drop | 1% low drop | Composed before capture |
| --- | ---: | ---: | ---: |
| No capture | 0.0 ± 0.5% | — | 100% |
| DDA capture only | **19.2 ± 3.0%** | 49% | 100% |
| WGC monitor capture only | 2.9 ± 9.2% (2 valid) | 18% | 100% |
| WGC window capture only | −0.1 ± 0.6% | 0% | 100% |
| WGC window + encoder | 1.0 ± 5.0% | 0.5% | 100% |

The full-screen DDA recording was not run windowed: it would save the user's
other windows around the fixture.

### Oversampling, light game (about 559 FPS, focused, 4 runs each)

| Configuration | FPS drop | Clip repeated / jumps |
| --- | ---: | --- |
| DDA + encoder (current) | 25.0 ± 1.2% | **12.6%** / 15.1% |
| WGC 1x | 3.0 ± 0.8% | 0% / 0.1% |
| WGC 2x | 6.45 ± 0.85% | 0% / 4.7% |

### GPU engines (one profiling run each)

The game renders on NVIDIA. DWM and the encoder run on the Intel GPU that
drives the panel.

| Configuration | DWM Intel 3D | FFmpeg Intel 3D | FFmpeg Intel video | Game NVIDIA copy engine | FPS drop |
| --- | ---: | ---: | ---: | ---: | ---: |
| No capture | 0% | — | — | 87% | −0.9% |
| DDA capture only | **62%** | — | — | 67% | 32% |
| DDA + encoder | **68%** | 16% | 26% | 54% | 41% |
| WGC monitor capture only | 14% | — | — | 86% | 0.7% |
| WGC window capture only | 13% | — | — | 84% | 1.9% |
| WGC + encoder | 10% | 14% | 18% | 80% | 5.4% |

### Capture probes

| Measure | Value |
| --- | --- |
| DDA desktop updates per second | 524 (light), 318 (CPU), 106 (GPU), 240 (windowed), matching the game's present rate or the refresh cap |
| DDA GPU copy | 0.6–0.87 ms |
| WGC delivered rate | 60.0/s at the 60/s limit, except 53.2/s for the about-108 FPS game (137 of 1200 slots empty), which is the reason for 2x |
| WGC latency, Windows timestamp → frame taken | p50 9–15 ms, p95 13–19 ms (one 60 Hz sampling slot at most) |
| WGC GPU copy | about 1.0 ms (2560x1600), 0.22 ms (1280x720 window) |
| WGC resizes / errors | 0 / 0 |
| WGC settings | Border and `MinUpdateInterval` settings were accepted in every run |

## 5. Root cause: confirmed and hypothesised

**Confirmed (this hardware, synthetic workloads):**

1. The encoder is not the bottleneck. Desktop Duplication **without an
   encoder** costs 24% (light) and 19% (windowed) of FPS, and adding the
   encoder adds roughly 11%. WGC with the same encoder costs 1.7–3.3%.
2. Both APIs make DWM compose the game: overlay → composed in 99–100% of
   samples. **Composition alone is not the cost.** WGC causes the same
   composition at a fraction of the cost, and the windowed game was composed
   even without capture, yet DDA still cost 19%.
3. While DDA is active, DWM's Intel 3D load rises from 0% to about 62%. DDA
   reports one desktop update per game present (about 500/s), capped at the
   refresh rate when windowed (240/s).
4. Limiting WGC to 60 updates per second leaves Windows missing the slot
   whenever the game's frame interval does not divide 16.7 ms. A 108 FPS game
   arrived at 53/s and 12–15% of recorded frames were repeats. Capturing at
   120/s and keeping the nearest frame per slot fixes it (0.14–0.7% repeats),
   at an extra cost of about 1–3.5 percentage points.

**Hypotheses (not proven):**

- On a display driven by the same GPU that renders the game (the external
  NVIDIA monitor), the DDA penalty is much smaller (about 4–6% measured on
  November 11), and WGC's advantage may shrink accordingly.
- Real games with frame-rate caps behave like the CPU-limited or GPU-limited
  cases, depending on where they sit.
- Fortnite on the built-in display probably suffered from this Intel DWM
  saturation if it was recorded with REPLAYX at high FPS. **Not measured.**

## 6. Production changes (all opt-in; default unchanged)

1. **The `capture_api` setting** (`model.rs`):
   - Defaults to `"dda"`, so existing users see no change.
   - The UI control is under Capture → Capture method, shown only for display
     capture.
   - Its hint says it is experimental, falls back automatically, and that
     Windows may show its border.
2. **The WGC source** (`media.rs`):
   - Uses `gfxcapture` with the display's `HMONITOR` (`geometry::monitor_handle`)
     and the same crop rectangle as `ddagrab`.
   - Keeps the capture border on (`display_border=1`). REPLAYX does not
     request borderless capture, because Microsoft requires user consent
     (`GraphicsCaptureAccess.RequestAccessAsync(Borderless)`) for that.
   - Captures at 2x the output rate with `fps=…:round=near`
     (`Tuning::wgc_oversample`; `wgc_oversample=1` for experiments).
3. **Fallback** (`engine.rs`): if no WGC capture path starts, the start
   repeats path selection with Desktop Duplication and records why in the
   scaling detail. The capture API is part of both video-path cache keys
   (remembered key bumped to v5), so a failing WGC path is never reused for
   Desktop Duplication, or the reverse.
4. **Diagnostics:** `Status.capture_api` is shown on the Performance page as
   "Capture method", with an "experimental" badge.
5. **CLI:** `capture-test … capture_api=wgc` for automated validation.

Unchanged:

- Desktop Duplication code.
- Encoder settings (bitrate, lookahead, AQ, preset).
- Audio, replay buffer, saving, game detection and the updater.

## 7. Quality validation

### Production path

| Check | Result |
| --- | --- |
| User-facing setting (`capture_api=wgc`, engine, not the benchmark switch) | Engine reported `capture_api: wgc`, Quick Sync zero-copy, 59.6 FPS, **0% repeated frames**, 1.4% jumps, 4.97% FPS drop (one valid run; the second exceeded control drift) |
| Fallback (WGC pointed at a window that does not exist) | Both runs recorded through Desktop Duplication. The Performance detail read: *"Windows Graphics Capture could not start (… Failed to find capture source); using Desktop Duplication"* |
| Codec matrix through WGC (`capture-matrix.py --capture-api wgc`) | **6/6** (details below) |
| Same matrix through Desktop Duplication | **6/6**, the same paths and sizes. WGC saves 3.0–3.7 s, DDA 2.1–3.9 s (this display uses Intel VPP and a transfer to NVENC) |

The codec matrix cases were:

- H.264, HEVC and AV1 on NVENC.
- 1280x960 and 1440x1080 (4:3) regions.
- Exact policy.

Every case produced the requested codec and 1920x1080 output, and both saved
clips of every case decode cleanly.

The first matrix attempts (both APIs) stopped at case 5 of 6. The fixture's
115 s lifetime was too short for this display's slower saves, and the guard
refused to record an uncovered screen. The lifetime was raised to 260 s and
both reruns passed.

### Regression (engine changes affect every start)

| Test | Result |
| --- | --- |
| Rust | 162 passed, 1 ignored (opt-in microbenchmark); Clippy `-D warnings` clean; rustfmt clean |
| Frontend | 52/52; TypeScript `--noEmit` clean |
| Python | 9/9 |
| Native updater e2e | 42/42 |
| `record-self-test` | Stop handed over in 1.07 s; replay continued; concurrent save ready in 3.4 s |
| `save-bench` | Passed |
| 15-minute synthetic soak with two concurrent NVENC sessions | Details below |

The soak results:

- 54,106 frames at 60.01 FPS, 0 dropped.
- Saves ready in 860 / 837 ms; both clips decode clean.
- Memory 458 → 461 MB; no leftover buffer files.
- Both extra encoders exited 0.

The soak and self-tests use the synthetic source. They validate the engine,
buffer and saving after the start-path change, not WGC itself.

**Not tested:**

- Physical audio sync (needs a stimulus played through the speakers).
- Microphone recording and device changes.
- Discord sharing.
- Display-mode changes during WGC capture.
- Device loss (GPU reset).
- Multi-monitor setups.
- Real games.

Audio is captured independently of the video API. WGC changes only the video
source.

Recording smoothness through WGC (from the benchmark clips; 1920x1080, 60 FPS
CFR):

- 0% repeated frames in the light and CPU-limited workloads.
- 0.14–0.7% repeated frames in the GPU-limited workload at 2x.
- Desktop Duplication on this display repeated 0–12.6% of frames, depending on
  load.

## 8. Compatibility and limits

- **Windows.** `gfxcapture` and the setting need Windows 10 1903 or later
  (monitor/window interop). `MinUpdateInterval` (frame-rate limiting) is in
  the Windows 11 24H2 (26100) API set and was accepted here (build 26300).
  Earlier builds would deliver frames at the composition rate. That is not
  measured.
- **Border.** Windows draws a capture border while WGC runs. REPLAYX keeps it.
- **Protected content** (DRM video, some secure windows) appears black under
  WGC, as with Desktop Duplication.
- **Window capture** follows one window. A game that recreates its window
  (mode change) ends the capture item; the recorder then restarts as for any
  capture failure. Not tested with a real game.
- **The external NVIDIA-driven monitor, AMD or Intel-only systems,
  multi-monitor setups and exclusive full screen** are **not tested**.
- **Stretched 4:3** with display-mode changes: only the region crop was tested
  (codec matrix). A real mode change while recording through WGC is not
  tested.
- **Discord screen sharing alongside WGC** is not tested. Both use public
  capture APIs.

## 9. What still needs real-game testing

1. Fortnite (and one GPU-heavy title) on the **built-in display**, with
   REPLAYX replay on Desktop Duplication, then on WGC, using the A/B/A
   procedure in [FORTNITE_ABA.md](FORTNITE_ABA.md). Use PresentMon as
   administrator, and compare `PresentMode` and frame times.
2. The same on the **external 250 Hz monitor**.
3. Watch for the Windows capture border, stretched-resolution mode changes,
   alt-tab, and game window recreation while recording through WGC.
4. Only after 1–3: decide whether WGC becomes the default on hybrid laptops
   whose display runs on the integrated GPU. That is where this data shows the
   largest benefit.

## 10. Testing the build locally

The build is **not installed**. It is a local build in a separate folder, so
the alpha.8 installer and `target\release\replayx.exe` are unchanged:

`src-tauri\target-wgc\release\replayx.exe` (0.3.1-alpha.8 version number,
built from `82572c8`)

1. Quit the installed REPLAYX (tray icon → Quit). Only one copy can run.
2. Start the exe above. It uses your normal settings and clips folder.
3. Capture → **Capture method** → *Windows Graphics Capture (experimental)*.
   Start replay. The Performance page should show "Capture method: Windows
   Graphics Capture".
4. Compare with *Desktop Duplication* in the same game and scene. Use
   [FORTNITE_ABA.md](FORTNITE_ABA.md) with PresentMon as administrator for
   real numbers.
5. To go back, quit this build and start the installed REPLAYX. Alpha.8
   ignores the new setting, so your other settings and clips are unaffected.

## 11. Recommended next steps

1. **Adaptive WGC rate:**
   - Start at 1x.
   - Switch to 2x only when the delivered rate falls below the output rate,
     which the probe measures.
   - This would keep 1x's cost (3.0% instead of 6.45% in the light game)
     without the repeats.
2. **Repeat the comparison on the NVIDIA-driven external display.** If WGC is
   also no worse there, prefer it everywhere; otherwise choose by display
   adapter (WGC when the display runs on a different GPU than the game).
3. **Window capture of the detected game** (`CreateForWindow` on the
   foreground game's HWND) gave the lowest overhead in the windowed workload.
   It needs focus/minimize handling before production.

## 12. Git state

Branch `codex/performance-0.2.1`, local only. Commits after `853cfbc`:

| Commit | Content |
| --- | --- |
| `5de575e` | WGC prototype, probe and benchmark tooling |
| `82572c8` | Opt-in WGC capture method, 2x oversampling, fallback, diagnostics and UI |
| (this commit) | This report | Nothing was pushed, merged, published or
installed. The alpha.8 installer and `replayx.exe` (17:38 build) are
unchanged.
