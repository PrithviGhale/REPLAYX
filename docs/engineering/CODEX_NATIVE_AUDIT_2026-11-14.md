# Independent native-prototype audit

## Scope and state

Work was restricted to the performance worktree on branch `performance/capture-replay-stutter-optimization`, starting at clean commit `0f1f003`. That commit includes Claude's CPU fix `c157076`. The main repository and crosshair branch were excluded from changes. No interactive benchmark, game launch, audio capture, installation, merge or push was performed. New edits remain uncommitted.

Claude completed a development-only WGC to fixed-function NV12 to Media Foundation hardware H.264 path, a CFR selector, synthetic correctness checks and owner-run CS2 measurements. Audio, container muxing, segments, replay retention, recovery and cross-vendor qualification remain future work.

## Latest partial CS2 run

Source data is `test-output/cs2-session-20261114-194122/summary.json`. The run stopped on loss of game focus. Seven phases completed, including two existing-recorder phases and one native phase. The unfinished second native phase is excluded. Each recorded phase below is compared with its own two neighboring recorder-off baselines.

| Phase | Game FPS | FPS cost | 1% low FPS | P99 | Recorder CPU, whole machine |
| --- | --- | --- | --- | --- | --- |
| Existing recorder B1 | 255.53 | 29.16% | 124.02 | 7.780 ms | FFmpeg 1.58% plus CLI 0.27% |
| Native B2 | 285.84 | 20.56% | 138.94 | 6.911 ms | 0.95% |
| Existing recorder B3 | 255.72 | 29.17% | 120.75 | 7.899 ms | FFmpeg 1.65% plus CLI 0.27% |

Off baselines stayed between 359.55 and 362.02 FPS. Native recovered about 30 FPS in this one completed phase. This is promising evidence, with only one post-fix native observation. It does not establish repeatability or a ShadowPlay-level result. Detailed recomputation and per-clip results are in `test-output/codex-native-review/partial-review.json`.

All three inspected completed-phase clips fully decoded with zero errors. Existing-recorder replays contained 3,480 frames at 1920x1080. The native file contained 3,422 decoded frames at 1920x1080. These are different recording windows. Decode success, matching bitrate and FPS counters cannot establish equal picture quality or motion smoothness in the still game scene.

## CPU fix and compiled executable

The source identifies the process once before recording. Its loop uses foreground-window checks at most four times per second. The old repeated process snapshot is absent from the recording loop.

A standalone build initially failed because the Windows dependency omitted `Win32_System_Ole`, required by VARIANT and ICodecAPI settings. That feature declaration was added. Standalone tests and a fresh optimized executable now build without depending on feature unification from the desktop target.

The rebuilt executable's `--bench-guard 32` measured 16.11 ms CPU per process snapshot and 0.0003625 ms wall time per foreground check. Foreground CPU time rounded to zero at the process CPU timer's resolution. This confirms the expensive check and the replacement path in the rebuilt executable. It does not measure new gameplay overhead.

Rebuilt executable SHA-256 is `1ae2f9dbad4e5a78550414f54ca036017740b4cb939c92cf77802bfc63eddfd0`. Original and rebuilt fingerprints and logs are retained under `test-output/codex-native-review/`. The original executable's exact historic source provenance cannot be established from its timestamp alone.

## Correctness and resource findings

1. **Encoder-surface reuse lacks an explicit retirement contract.** `NativeRecorder::deliver` cycles eight textures before conversion, while `MfEncoder::encode` submits samples asynchronously. There is no tracked-sample release or equivalent retirement check before an occupied slot is overwritten. NeedInput credits permit submission and do not prove a particular older surface is free. The optional sample-reuse experiment also changes timestamps on cached samples. Corruption was not observed in the offline tests, but the implementation needs explicit ownership handling and delayed-consumer stress tests before integration. Microsoft requires clients to wait for sample release unless the transform guarantees that it retains no input reference. [Input-sample ownership](https://learn.microsoft.com/en-us/windows/win32/api/mftransform/nf-mftransform-imftransform-processinput), [tracked samples](https://learn.microsoft.com/en-us/windows/win32/api/mfidl/nn-mfidl-imftrackedsample)

2. **Encoder input waits are unbounded.** The input-credit loop can call blocking GetEvent indefinitely. This can prevent recording timeout and foreground checks from running if the driver stops responding. The drain deadline is checked only when no event is available, so continuous unrelated events can also evade it. Use bounded, cancellation-aware waits and test a stalled transform. No such driver hang was reproduced in this audit.

3. **Recording memory grows with clip length.** Encoded output accumulates in a Vec and is written to disk at the end. At 15 Mb/s, ten minutes represents roughly 1.1 GB of compressed data before bookkeeping. A crash can lose the unwritten clip. Streaming and bounded buffering are required for a full recorder.

4. **Timing qualification requires muxing.** Raw H.264 has no container timestamps. FFprobe returned r_frame_rate 120/1 and avg_frame_rate 25/1 for the native file. Header inspection found progressive video, num_units_in_tick 1, time_scale 120 and fixed_frame_rate_flag 1, consistent with 60 FPS progressive timing. The raw probe fields do not establish a 120 FPS recording defect. Sample timestamps and muxed playback timing still need independent checks. [FFmpeg timing-field documentation](https://ffmpeg.org/ffmpeg-bitstream-filters.html#h264_005fmetadata)

5. **Prototype limitations are substantive.** Resizes are skipped, adaptation and fallback are absent, colour metadata needs muxing treatment, and audio/replay/recovery are unimplemented. The known repeat-of-dropped-capture approximation remains documented. Matching selector counters is not a proof for every arbitrary input trace or a stalled encoder.

## Changes made

- Added `--plan-only`, which prints the schedule without finding a game, starting telemetry or capturing.
- Reported countdown, warm-up, all settling and measurement time, with separate process-wait allowances.
- Added phase number, ten-second measurement progress, elapsed time, save-stage status and an explicit final DONE or STOPPED line after writing the summary.
- Used monotonic duration deadlines and bounded observation sleeps.
- Recorded completed/planned phases and actual elapsed duration in summary.json.
- Added normalized FPS, 1% low and P99 comparisons against both neighboring off baselines. Incomplete comparisons and unsuccessful native recordings are marked unusable.
- Added selected-executable preflight and SHA-256 fingerprints before warm-up.
- Reported nonzero recorder exits, waited for telemetry termination and preserved the summary when incomplete GPU telemetry rows are encountered. Interrupted runs now exit nonzero.
- Fixed the standalone Windows feature dependency.

The live benchmark path has not been exercised interactively after these edits. Its plan-only entry point and pure comparison/timing helpers were tested.

## Safe validation results

| Check | Result |
| --- | --- |
| Python unit tests | 15 passed, including 6 new timing/comparison cases |
| Native Rust selector unit tests | 2 passed |
| Standalone optimized native build | Passed after dependency fix |
| Selector against FFmpeg | 7 patterns passed, identical source slots and counters |
| Native synthetic encode/decode | 5 patterns passed, zero wrong slots and expected keyframes |
| Native Clippy with warnings denied | Passed |
| Rust formatting and diff whitespace | Passed |
| Existing partial-run videos | 3 fully decoded without errors |
| Interactive comparison | Not run |

## Recommended next step

The native path is worth further development as an experimental candidate. Its post-fix sample recovered about 8.6 percentage points of game FPS cost, while CPU use fell into the expected range. Full production integration is premature.

First implement explicit sample/surface retirement and bounded encoder waits, then stress delayed encoder consumption offline. Keep the existing recorder as the fallback. NVENC on the hybrid laptop remains an experiment requiring its own transfer and quality measurements.

After those fixes and explicit owner approval, repeat a counterbalanced comparison with neighboring off baselines. A prepared command is below. It has 7 minutes 15 seconds of timed work with a 60-second warm-up, plus startup, saving and analysis. Use the shorter warm-up only after the game is already in a sustained warmed state. With `--warmup 150`, timed work becomes 8 minutes 45 seconds.

```powershell
python scripts/bench/fortnite-session.py --game cs2 --seconds 30 --warmup 60 --cap 0 --configs wgc-auto,native-record,native-record,wgc-auto --plan-only
```

The command above only prints the plan. An approved live run would omit `--plan-only` and supply the existing PresentMon executable path. Keep the scene, resolution, display, output dimensions, bitrate and audio setting identical. Judge paired FPS cost, 1% lows, P99, baseline drift, per-process CPU and per-engine GPU load. Independently decode each clip and validate captured motion with a timestamped moving fixture in a separately approved session. Record uncertainty when differences are within baseline spread.
