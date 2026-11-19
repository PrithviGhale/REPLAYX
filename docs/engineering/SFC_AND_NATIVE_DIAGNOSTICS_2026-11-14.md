# Intel SFC verification and native-pipeline diagnostics (November 14, 2026)

Branch `performance/capture-replay-stutter-optimization`. Production defaults
are unchanged. Everything here was verified from the terminal, without
capture or games.

## Stage 1: `wgc-sfc` (Quick Sync VPP on the fixed-function scaler)

**What it is.** `REPLAYX_VIDEO_TUNING=vpp_scale_mode=1`, opt-in. It adds
`:scale_mode=1` to the Quick Sync `vpp_qsv` conversion. Nothing else changes:

| Unchanged | Where it lives |
| --- | --- |
| Frame selection (re-timing) | `frame_timing` |
| CFR output and its drop/duplicate counters | FFmpeg's output step |
| The 2x starvation switch, recovery | `wgc_missing_slots`, `begin_recovery` |
| Bitrate, resolution, 60 FPS, colour matrix | Encoder and VPP arguments |

The values accepted are FFmpeg's documented 0–5; anything else is ignored.

**Unit test** (`sfc_changes_only_the_quick_sync_conversion`). For every scaler
and canvas mode, the SFC filter graph equals the default one except
`:scale_mode=1` inside `vpp_qsv`.

**Offline hardware test** (`scripts/bench/sfc-offline.py`).
- The real Intel chain: D3D11 BGRA → regrid → hwmap to QSV → `vpp_qsv` NV12
  BT.709 → `h264_qsv` low power → 60 FPS CFR, with an AAC audio track.
- Both modes run on identical synthetic input; 12 s each.

| Delivery pattern | Output frames | Drop / duplicate counters | Video and audio timestamps | Same source frame in every slot |
| --- | --- | --- | --- | --- |
| 80/s regular | 721 = 721 | 240/0 = 240/0 | identical | yes (min 42.2 dB) |
| 80/s ±3 ms jitter | 721 = 721 | 239/0 = 239/0 | identical | yes (42.2 dB) |
| 80/s with a 1 s gap mid-run | 721 = 721 | 218/58 = 218/58 | identical | yes (42.2 dB) |
| 50/s (starving) | 720 = 720 | 0/120 = 0/120 | identical | yes (40.8 dB) |
| 240 Hz refresh pattern | 841 = 841 | 119/0 = 119/0 | identical | yes (42.2 dB) |

- **Control:** the same output one frame apart scores at most 28.4 dB. The
  35 dB threshold separates "same frame" from "different frame".
- **Colour quality** (earlier, on real CS2 frames, against a software BT.709
  reference): default 61.3 dB, SFC 61.5 dB PSNR.

**Conclusion.**
- SFC changes how Quick Sync converts each frame, and nothing about which
  frames are recorded or how they are counted.
- A/V sync is identical: the timestamps are identical.
- It needs the CS2 A/B below and an independent code review before it can
  become a default.

## Stage 2: native capture-to-encode diagnostics (`replayx-wgc-profile`)

A development binary only; it does not touch the recorder.

**Capture settings.**
- `--stage` runs at the recorder's capture interval (`--interval-us 12500`).
- One frame is chosen per 60 FPS slot (the newest delivered).
- No encoder.

| Stage | Work per chosen frame |
| --- | --- |
| `none` | Release the frame at once: Windows' capture cost alone |
| `copy` | One full-resolution GPU copy (the earlier capture-only phase) |
| `convert` | One fixed-function D3D11 VideoProcessor pass: 2560 × 1600 BGRA → 1920 × 1080 NV12, BT.709 limited range |
| `convert-xfer` | `convert`, then the NV12 frame to the NVIDIA GPU through a non-blocking readback ring and an upload (the portable path) |
| `--mode window` | Any stage, capturing the game's window instead of the display |

**Self-test** (`replayx-wgc-profile --self-test`, no capture):

| Check | Result |
| --- | --- |
| Six solid colours through the converter | Exact BT.709 limited values (black 16/128/128, white 235/128/128, red 63/102/240, green 173/42/26, blue 32/240/118, grey 126/128/128) |
| Capture textures read by the video processor | Directly; no extra copy |
| NV12 frames on the RTX 4070 | Byte-identical after the transfer |

**Per-stage time** (synthetic frames on this laptop; not game impact):

| Stage | Time per frame | Where |
| --- | --- | --- |
| Full-resolution copy | 0.80–0.84 ms | Intel 3D/copy engine |
| Fixed-function scale + convert | 1.62–1.64 ms | Intel video-processing engine |
| Transfer to NVIDIA (system memory) | 1.4–2.1 ms | CPU (memcpy and upload) |

**Reading.**
- The fixed-function pass takes longer than a copy, but runs on a different
  engine. In CS2, moving VPP to that engine gave CPU clock back (+2.4% and
  +3.3% FPS), so time on that engine is not the cost that matters most.
- **The portable transfer is not free.** At 60 FPS it takes about 9–13% of
  one CPU core, which a CPU-bound game such as CS2 may feel. Only the game
  test can tell.
- A true GPU-to-GPU copy (cross-adapter heap) would avoid the CPU. It is
  harder to build and is worth building only if `convert-xfer` otherwise
  looks promising.

**Not proven by these tests.** Game impact. That needs the CS2 sessions below.

## CS2 sessions (owner runs them; each needs the screen)

Built-in display, same spot, administrator PowerShell, in
`C:\Users\Prithvi\Desktop\VS CODE\replayx-perf`. Add to each command:

```
--presentmon "..\REPLAYX\test-output\fortnite-live-alpha8\PresentMon-2.6.0-x64.exe"
```

**A. SFC confirmation (about 11 minutes).** Alternating baselines:

```powershell
py -3 scripts\bench\fortnite-session.py --game cs2 --warmup 150 --cap 0 --configs wgc-auto,wgc-sfc,wgc-auto,wgc-sfc
```

Success, for both SFC phases against both neighbouring baselines:
- higher FPS than the baseline spread;
- the same duplicate counts.

**B. Native-pipeline ceiling (about 15 minutes).**

```powershell
py -3 scripts\bench\fortnite-session.py --game cs2 --warmup 150 --cap 0 --configs native-none,native-convert,native-convert-xfer,native-none-window,native-convert-window,wgc-auto
```

**Decision rules.**
- **If `native-convert` is within about 3 points of `native-none`, and well
  below `wgc-auto`:** a native path can recover most of what REPLAYX adds. A
  rewrite is worth planning.
- **If `native-convert-xfer` adds little to `native-convert`:** encoding on
  NVENC with a system-memory transfer is viable. If it adds a lot, a GPU-to-GPU
  copy is required first, or Quick Sync stays the encoder on Intel-driven
  displays.
- **If the window stages are clearly below the monitor stages:** window
  capture of the detected game is the bigger lever.
- **If none of this beats today's path meaningfully:** no rewrite. Document
  the floor.
