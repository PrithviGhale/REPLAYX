# Why the first Fortnite test differed from the synthetic results (November 12, 2026)

Inputs:

- Your Fortnite screenshots on the built-in 2560x1600 240 Hz display:

  | State | FPS readings |
  | --- | --- |
  | Off | 330 / 360 / 359 |
  | NVIDIA Instant Replay on | 359 / 353 / 357 / 306 |
  | REPLAYX on | 243 / 276 / 255 |

- The diagnostics JSON you copied.
- The test build's own settings and remembered video path. These were read
  only; nothing was changed.

All measurements below are synthetic, on the same display, with 3 valid runs
per configuration. **The Fortnite overhead itself is still unmeasured**:
screenshots are instantaneous readings, not frame-time averages.

## 1. What actually ran: verified

| Evidence | Value |
| --- | --- |
| `settings.json` | `capture_api: "wgc"`, **`encoder: "h264_nvenc"`**, codec h264, 1920x1080 at 60, Balanced |
| Remembered path (`video-path.json`) | Key `v5|8086|…|h264_nvenc|…|wgc|…target-wgc\release\media\ffmpeg.exe` → **`QsvHybrid` + `h264_nvenc`**, no fallback detail |
| Your diagnostics | scaling "qsv with CPU transfer", encoder input "system NV12", capture adapter Intel |

**WGC (`gfxcapture`) was running. There was no fallback.**

The `backend: desktop-duplication` field was **misleading, not a fallback**.
It printed `Status.capture`, the *capture source* setting, whose value for
"record the display" is the legacy string `"desktop-duplication"`. It never
reported the capture API.

The pipeline was:

1. WGC frame on the Intel GPU (2560x1600, up to 120/s; 60 kept).
2. Intel VPP scales it to 1920x1080 NV12.
3. **`hwdownload` copies the frame into system memory.** The CPU waits for the
   Intel GPU on every frame.
4. h264_nvenc uploads the frame to the **NVIDIA GPU, which Fortnite renders
   on**, and encodes it.

My earlier benchmarks used `encoder: auto`. On this display that picked
**Quick Sync zero-copy**, with everything on the Intel GPU. **So the Fortnite
test ran a different pipeline from the one measured at 3.3%.**

## 2. Diagnostics fixed

The diagnostics JSON now reports what ran, not the source setting:

| Field | Meaning |
| --- | --- |
| `captureSource` | `display` or `window` |
| `captureMethod` | The API that ran: `windows-graphics-capture` or `desktop-duplication` |
| `requestedCaptureMethod` | The setting |
| `captureFilter` | `gfxcapture` / `ddagrab` / `gdigrab` / `synthetic` |
| `captureFallback` | true when WGC was requested but Desktop Duplication recorded |

The engine exposes `Status.capture_filter` and `Status.capture_fallback`.
The fallback reason is shown on the Performance page, with a "WGC could not
start" badge. It is never copied into the shareable JSON, which excludes
FFmpeg error text by design. A test covers both the WGC and the fallback case.

## 3. The frame path, and the alternatives measured

CPU-limited game, about 313 FPS: similar to Fortnite at 330–360. NVENC
selected, or Auto for Quick Sync. Same output (1920x1080, 60 FPS, H.264).
3 runs each in randomized order; every run valid.

| Path | Game FPS drop | 1% low drop | p99 frame time | Clip repeated / jumps |
| --- | ---: | ---: | --- | --- |
| No capture | −0.9 ± 1.0% | — | 3.95 → 3.69 ms | — |
| DDA → Intel VPP → system memory → NVENC (alpha.8 with NVENC) | 9.5 ± 1.3% | 18% | 3.86 → 4.63 ms | 0% / 7.7% |
| **WGC → Intel VPP → system memory → NVENC (your test)** | **5.55 ± 0.21%** | 16% | 3.93 → 4.22 ms | 0.05% / 4.1% |
| WGC → **NVIDIA** device → GPU scale → NVENC zero-copy (new) | **29.9 ± 1.1%** | 57% | 3.92 → 8.30 ms | 0.05% / 3.0% |
| **WGC → Quick Sync zero-copy (Encoder: Auto)** | **4.64 ± 0.14%** | 8% | 3.85 → 4.10 ms | 0% / 7.0% |

**A GPU-resident NVENC path exists but is worse here.**

- FFmpeg's `gfxcapture` accepts a D3D11 device on the NVIDIA adapter
  ("Using provided hw_device_ctx"), scales on the GPU, and NVENC encodes the
  D3D11 frames directly. It worked.
- But Windows must copy every full 2560x1600 frame (about 16 MB, up to 120/s)
  across adapters onto the game's GPU. That costs far more than copying one
  scaled 3 MB NV12 frame through system memory.
- "No CPU copy" did not mean "cheaper". This path stays in the code as a
  development switch (`REPLAYX_VIDEO_TUNING=wgc_gpu=1`) and is **never chosen
  automatically**. It is unmeasured on an NVIDIA-driven display, where no
  cross-adapter copy is needed.

**GPU-heavy game** (99% NVIDIA GPU busy, about 102 FPS):

- WGC → Quick Sync: 0.9 ± 0.6% FPS drop, 1.3% repeated frames.
- **WGC → VPP → system memory → NVENC fell behind:**
  - Encoder speed **0.75x** (about 45 FPS), video lag growing to 4.4 s, and
    **89.6% repeated frames** in the clip.
  - In the second run, the backlog ended in FFmpeg "Cannot allocate memory"
    and the capture stopped.
  - Uploading each frame into a GPU the game keeps fully busy starves the
    encoder.
  - Your Fortnite diagnostics showed 59.89 encoded FPS, so the encoder kept up
    in that scene. The failure mode is real, but it is not proven to be the
    cause of your FPS readings.

The Performance page now explains this path when it is active and suggests
Encoder: Auto. Your NVENC choice is not overridden.

## 4. Capture-side measurement

| Measured | Where |
| --- | --- |
| WGC delivered rate, source frame interval, Windows-timestamp → taken latency, empty slots, GPU copy span | `replayx-wgc-profile` (development probe) |
| Repeated frames and frame-advance jumps in the saved clip | `scripts/bench/gameproxy-check.py` (benchmarks) |
| Recorder output FPS, output-grid drops/duplicates, encoder speed, video lag | The app (Performance page) |

Not measured in the app, and still `null` in diagnostics: source FPS, source
drops, GPU usage and per-frame latency.

With WGC at 2x, FFmpeg's `fps` filter selects frames before the output stage.
So **output-grid duplicates under-report source starvation**: they stay near 0
even when frames repeat. Use encoder speed and video lag; a lag that keeps
growing means the encoder is falling behind. Recorder FPS is the encoded
output and is never game FPS.

## 5. Why 3.3% synthetic vs about 25% in the Fortnite screenshots

**Confirmed differences:**

1. **A different pipeline.** NVENC through system memory, against Quick Sync
   zero-copy. The synthetic cost of your path is 5.55% (CPU-limited), not
   3.3%. It also collapses under a saturated NVIDIA GPU.
2. **Screenshots are single instants.** The baseline readings varied between
   330 and 360 against a 360 cap. A cap hides headroom, and a few readings
   cannot give an average or a 1% low.

**Plausible, not proven** (needs PresentMon in Fortnite):

3. **NVIDIA GPU contention.** With NVENC selected, every recorded frame is
   uploaded to and encoded on the GPU Fortnite renders on. The synthetic
   CPU-limited game barely uses that GPU, so it cannot show this cost.
4. **CPU contention.** REPLAYX used 9.86% of all CPU threads in your
   diagnostics: the readback, upload and WebView interface. Fortnite at
   330–360 FPS is CPU-bound.
5. **Presentation.** Both capture APIs moved the synthetic game from overlay to
   DWM composition. Fortnite's actual present mode under each recorder is
   unknown.

The 1464 MB "resident memory" is the whole REPLAYX process tree, including the
WebView2 interface processes, not FFmpeg alone. The interface-less engine soak
holds about 460 MB.

## 6. What to test next in Fortnite

Use the new build (section 7). Run the A/B/A procedure in
[FORTNITE_WGC_ABA.md](FORTNITE_WGC_ABA.md) with:

- **B1: Capture method = Windows Graphics Capture, Encoder = Auto.** That is
  Quick Sync, the cheapest and most robust path measured here.
- B2: NVIDIA Instant Replay.
- Optionally B3: WGC + NVENC, to see the NVENC cost in the real game.

The copied diagnostics now prove which path each phase ran.

## 7. Builds

| Build | Path | Status |
| --- | --- | --- |
| New test build | `src-tauri\target-wgc2\release\replayx.exe` | Accurate diagnostics, Performance hint, WgcGpu off |
| Previous WGC test build | `src-tauri\target-wgc\release\replayx.exe` | Unchanged |
| Alpha.8 | installer and `target\release\replayx.exe` | Unchanged |

None of these is installed. Only one REPLAYX can run at a time.
