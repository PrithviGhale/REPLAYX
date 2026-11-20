# Native capture-to-encode prototype (November 14, 2026)

Branch `performance/capture-replay-stutter-optimization`. Development binary
`replayx-native-proto` only. The recorder, its defaults and its FFmpeg path
are unchanged.

## Tests A and B (owner, CS2, built-in 2560 × 1600 at 240 Hz panel, warmed up)

### Test A: SFC alone (`cs2-session-20261114-140312`)

The recorder-off phases were 297.2, 306.7, 303.7, **268.0** and 297.7 FPS. A4
dipped about 10% below the others, so the "cost" figures next to it are not
reliable; the table uses absolute FPS.

| Recording phase | CS2 FPS | CPU clock | CPU busy / frame | FFmpeg Intel 3D |
| --- | ---: | ---: | ---: | ---: |
| Normal WGC #1 | 223.6 | 95.2% | 3.60 ms | 10.9% |
| SFC #1 | 223.8 | 96.9% | 3.60 ms | 7.3% (+8.6% video processing) |
| Normal WGC #2 | 220.9 | 95.0% | 3.64 ms | 11.0% |
| SFC #2 | 223.2 | 96.4% | 3.62 ms | 7.4% (+8.8%) |

**SFC alone: no repeatable FPS gain** (+0.6% on average, inside the spread).
It moves about 3.6 points of Intel 3D load to the video-processing engine and
raises the CPU clock by 1.5 points, too little for CS2 to show. The earlier
+2.4% and +3.3% came only with early drop **and** SFC together, which is
rejected (frame repeats).

**Conclusion: SFC should not become a default.**

### Test B: native stages, no encoder (`cs2-session-20261114-141639`)

The recorder-off phases drifted (281.0 to 305.6), so each phase is compared
with its two neighbours; absolute FPS is also shown.

| Phase | CS2 FPS | Cost vs neighbours | CPU clock | CPU busy / frame | Capture GPU load |
| --- | ---: | ---: | ---: | ---: | --- |
| Capture only, display | 272.6 | 6.3% | 101.2% | 2.93 ms | DWM 3D 30.8% |
| Capture only, game window | 279.8 | 5.2% | 102.8% | 2.81 ms | DWM 3D 25.1% |
| + fixed-function convert, display | 267.6 | 10.8% | 101.0% | 2.95 ms | DWM 19.7%, video processing 19.0% |
| + convert, window | 268.5 | 8.7% | 101.1% | 2.96 ms | DWM 17.0%, video processing 21.9% |
| + convert + transfer to NVIDIA | 256.1 | 15.3% | 100.1% | 3.12 ms | DWM 17.8%, video processing 14.4% |
| **Today's recorder (Quick Sync)** | **234.5** | **22.8%** (clean neighbours 303.7 and 304.1) | 94.5% | 3.44 ms | DWM 16.0%, FFmpeg 3D 9.7%, encoder 14.7% |

**Reading.**
- Capture alone costs about 5–6%. The earlier "18%" capture-only phase
  included a full-resolution GPU copy per capture. That copy is the expensive
  part, and today's FFmpeg path does the equivalent (`gfxcapture`'s shader
  pass) on every capture.
- Capture plus one fixed-function conversion of the chosen frames costs
  9–11%, against today's 23%, but with **no encoder yet**. Today's Quick Sync
  encode is part of the 23%, so this is not a like-for-like comparison.
- The transfer to NVIDIA through system memory adds about 4.5 points. On this
  display Quick Sync stays the encoder.
- Window capture is slightly cheaper (1–2 points), within the drift.

## Step 1: can Quick Sync take the converted D3D11 surface directly? Yes.

`replayx-native-proto --feasibility` (synthetic frames, nothing captured):

| Check | Result |
| --- | --- |
| Hardware H.264 encoders offered by Windows | Intel Quick Sync (two entries), NVIDIA |
| Input | The converter's D3D11 NV12 texture on the same device (`MFCreateDXGISurfaceBuffer`, DXGI device manager); **no CPU readback** |
| Settings accepted | Peak-constrained VBR, 15 Mb/s mean, 23 Mb/s max, GOP 120, no B-frames, low latency off: all 6 |
| Frames | 600 in, 600 out; keyframes at 0, 120, 240…; timestamps increasing |
| FFmpeg verification | Decodes cleanly: H.264 High, 1920 × 1080, no B-frames, limited range |
| Throughput | 395 FPS unpaced (6.6× real time) |
| At 60 FPS | 3.2 ms CPU per frame including the encoder's in-process threads; ~34 ms encode latency |
| Gap | The encoder does not write BT.709 colour tags into the stream. They must be added when muxing (FFmpeg's `h264_metadata` bitstream filter). |

## Step 2: frame selection identical to today's recorder

`native/select.rs` ports what today's chain does to WGC frames:
- re-timing to the nearest slot (`setpts`);
- FFmpeg's constant-rate output (`video_sync_process`, `fg_output_frame`).

**FFmpeg details the port depends on.** Both were found by reading the bundled
FFmpeg n9.0.2 source:
- gfxcapture frames get the graph frame rate's duration (1/80 s at the
  12.5 ms floor) at the filter output, which enables the "clip into the next
  slot" branch;
- timestamps are truncated to 100 ns integers and rescaled with 16 extra bits.

**Check** (`scripts/bench/native-select-check.py`). Numbered synthetic frames
with gfxcapture-like timestamps go through FFmpeg's real chain and through the
selector. The rows are delivery patterns; every one shows the same source
frame in every output slot, and the same counters:

| Delivery pattern | Output slots | Drop / duplicate counters |
| --- | ---: | --- |
| 80/s regular | 721 | 240 / 0 |
| ±3 ms jitter | 721 | 240 / 0 |
| 1 s gap | 721 | 218 / 58 |
| 50/s (starving) | 720 | 0 / 120 |
| 240 Hz pattern | 841 | 119 / 0 |
| Irregular | 721 | 240 / 0 |
| Exact 60/s | 720 | 0 / 0 |

This keeps today's selection, duplicate counting and the counter that drives
the 2x switch. It does not have the early-drop flaw.

**One measured deviation.** FFmpeg keeps every capture, so it can repeat a
capture it had dropped (a drop immediately followed by a gap). The native path
never converts dropped captures, so it repeats the last recorded frame
instead. That frame is one capture older; the pacing is the same. Every
occurrence is counted (`repeats_of_dropped_frames`); none occurred in any
test.

## Step 3: end-to-end prototype

**`NativeRecorder`.**
- Every delivered capture goes through the selector, in arrival order.
- Only chosen frames get one fixed-function conversion, into an 8-texture
  encoder ring; dropped captures are never converted.
- Repeats re-encode the last converted frame.
- Quick Sync encodes the D3D11 texture.
- Output slot *n* has timestamp *n* / 60 s (constant frame rate, as today).

**Offline check** (`scripts/bench/native-proto-check.py`).
- Each synthetic capture is a flat grey whose level encodes its index.
- The output is decoded with FFmpeg, and the raw luma of every frame is
  compared with the capture the selector chose.

| Pattern | Captures | Converted | Encoded = decoded | Duplicates | Drops | Wrong slots | Keyframes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 80/s regular | 640 | 481 | 481 | 0 | 160 | 0 | every 120 |
| ±3 ms jitter | 640 | 481 | 481 | 0 | 160 | 0 | every 120 |
| 1 s gap | 560 | 422 | 481 | 59 | 139 | 0 | every 120 |
| 50/s | 400 | 400 | 480 | 80 | 0 | 0 | every 120 |
| 240 Hz | 640 | 561 | 561 | 0 | 79 | 0 | every 120 |

**At 60 FPS (paced, two runs).**

| Measure | Value |
| --- | --- |
| CPU per output frame (including the encoder's in-process threads) | 2.9–3.3 ms |
| Conversion submit | about 1 ms |
| Encode submit | about 1.3 ms |
| Encode latency | about 38 ms |

For comparison, today's FFmpeg recorder used about 1.2–1.4% of the CPU in CS2,
roughly 4 ms per frame. **CPU is comparable; no CPU gain is claimed.** The
expected gain is Intel GPU work:
- no per-capture shader pass;
- no conversion of captures that are dropped;
- conversion on the fixed-function engine.

**`--record --pid PID`.** WGC capture of a running game (CS2, Fortnite or the
test workload, which must be in front):
- at the 12.5 ms interval;
- on the GPU that drives its display, found by matching the monitor rather
  than assuming Intel;
- frame-arrival driven.

It refuses any other process and stops if the game leaves the front.

## What the prototype does not do (before any comparison is read)

| Area | State |
| --- | --- |
| **Audio** | **Not implemented.** Video only. The CS2 comparison records today's path silent too (`silent` in the session settings), so the comparison is fair for FPS. A product version would pipe the H.264 stream into FFmpeg (`-c:v copy`) beside today's audio inputs, with a shared start time. A/V sync must then be measured (`scripts/bench/av-sync.py`). |
| Muxing, segments, Instant Replay buffer | Not implemented. Writes one raw H.264 file. Keyframes every 2 s already match the segmenting. |
| Colour tags | Missing in the stream; to be written at muxing. |
| 2x capture for starving games, recovery and fallbacks | Not implemented. The duplicate counters the 2x switch uses are computed identically. CS2 never triggered 2x. |
| Display resize or mode change | Changed-size captures are skipped and counted, not handled. |
| Other GPUs | The encoder is chosen by the display GPU's vendor (Media Foundation H.264 MFT). Tested only with Intel Quick Sync on this laptop; NVIDIA and AMD MFTs are not tested. |
| Window capture | Supported (`--mode window`); not part of the first comparison. |

## The CS2 comparison (owner runs it; needs the screen)

Built-in display, same spot, administrator PowerShell, about 11 minutes.
Today's complete recorder is compared with the native prototype, alternating.
Both record 1080p60 H.264 with real hardware encoding, silent.

```powershell
cd "C:\Users\Prithvi\Desktop\VS CODE\replayx-perf"
py -3 scripts\bench\fortnite-session.py --game cs2 --warmup 150 --cap 0 `
  --configs wgc-auto,native-record,wgc-auto,native-record `
  --presentmon "..\REPLAYX\test-output\fortnite-live-alpha8\PresentMon-2.6.0-x64.exe"
```

**Success, both native phases against both neighbouring baselines:**
- CS2 FPS clearly above the spread between the two baselines;
- CPU busy time per frame lower;
- the native report's `dup_frames` no higher than the baseline's
  `duplicated_frames`;
- `repeats_of_dropped_frames` near 0;
- `success: true`;
- the native `.h264` files decode cleanly at 60 FPS.

**If the gain is substantial,** the next steps, in order:
1. Audio (pipe into FFmpeg, measure A/V sync).
2. Segments and replay.
3. Colour tags.
4. 2x and fallbacks.
5. NVIDIA and AMD encoder tests.

Only then a recorder integration plan.

**If it is not substantial,** no rewrite. The data then shows that WGC capture
and encoding together set the floor on this display.
