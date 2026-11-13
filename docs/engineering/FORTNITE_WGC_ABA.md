# Fortnite A/B/A: REPLAYX capture paths vs NVIDIA Instant Replay

A short, manual procedure, about 25 minutes. REPLAYX does not start Fortnite,
change its settings or interact with it. Screenshots of an FPS counter are
not enough: they show one instant. This procedure records frame times.

## Before you start (once)

1. **Display:** the built-in 2560x1600 240 Hz panel, as configured now.
   Laptop on AC. Close other heavy apps, including browser video.
2. **Fortnite:** use the same Creative island, spot and camera direction for
   every phase. Stand still, or repeat one short identical route. Keep
   graphics settings and the frame cap unchanged throughout.
   - If FPS sits at the cap with replay off, the cap hides part of the
     overhead. Measuring with the cap is fine, but every phase must use the
     same cap.
3. **PresentMon:** start a terminal **as administrator** (your choice; REPLAYX
   never asks for it). Use the official console app, for example
   `PresentMon-2.x-x64.exe`.
4. **REPLAYX test build:** `src-tauri\target-wgc3\release\replayx.exe`. Quit the
   installed REPLAYX (and any other test build) first.
5. **NVIDIA Instant Replay:** set it to 1080p60 if possible, so it is
   comparable with REPLAYX's 1920x1080 at 60 FPS.

## One measurement (60 s)

```powershell
PresentMon-2.x-x64.exe --process_name FortniteClient-Win64-Shipping.exe --output_file PHASE.csv --timed 60 --terminate_after_timed
```

Change `PHASE` for each phase. Between phases, toggle the recorder, then wait
**10 seconds** before starting PresentMon.

## Sequence (repeat twice)

| Phase | Recorder state |
| --- | --- |
| A1 | all recorders off |
| B1 | **REPLAYX replay on**: Capture method = Windows Graphics Capture, **Encoder = Auto** (Quick Sync on this display) |
| A2 | all recorders off |
| B2 | **NVIDIA Instant Replay on** (REPLAYX off) |
| A3 | all recorders off |
| B3 (optional) | REPLAYX replay on with WGC and Encoder = NVIDIA NVENC, to measure the NVENC path in the real game |

During each REPLAYX phase, open REPLAYX → Performance → **Copy diagnostics**
once and save the JSON next to the CSV. It records:

- `captureMethod`, `captureFilter`, `captureFallback`,
- `scaling`, `encoderInput`,
- `encoder`.

These confirm what actually ran.

## Compare

```powershell
py -3 scripts/bench/compare-frametimes.py A1.csv B1.csv A2.csv --process FortniteClient-Win64-Shipping.exe --cap 360
py -3 scripts/bench/compare-frametimes.py A2.csv B2.csv A3.csv --process FortniteClient-Win64-Shipping.exe --cap 360
```

These report sustained FPS, 1% lows, p95/p99 frame times, control drift and a
frame-cap warning. A result is valid only if A-phase drift is within 5% and the
scene did not change. Also look at PresentMon's `PresentMode` column in each
CSV, for example "Hardware: Independent Flip" against "Composed: Flip".

Send the CSVs and diagnostics JSONs. They show:

- REPLAYX's overhead compared with NVIDIA's on the same scene,
- which capture path REPLAYX ran,
- whether a presentation-mode change accompanied the loss.
