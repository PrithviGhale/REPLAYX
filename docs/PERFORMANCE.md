# Performance evidence and pending benchmark

## 0.3.1-alpha.7: November 11 validation

**15-minute synthetic replay soak:** `test-output/nov11-soak/summary.json`.
1080p60 NVENC, 60-second retention, internal generated video and three audio
tracks. Two additional NVENC sessions started and ended successfully. Both
overlapping saves finished during the second session: 691/664 ms to ready,
25 ms save-job handover. Both files fully decoded, with no decode errors;
no buffer MKVs remained after shutdown. One-minute average resident memory
459.85 MB near the beginning, 461.94 MB near the end; peak 522.33 MB including
saves. Recorder tree CPU averaged 6.56% of all cores. Internal synthesis and
CPU scaling contribute to this number; it is not a desktop/game overhead
measurement. Some automated checks overlapped this soak. End-of-soak output
was 1920x1080 H.264, 60.021 s. Generated flash/pulse sync over 59 events had
30 ms maximum offset; this does not validate physical audio sync. The static
flashing fixture intentionally repeats pictures and cannot qualify smoothness.

The user's real-game snapshots (~211 FPS off, ~117 on) remain unresolved.
No real-game improvement is claimed. Earlier proxy numbers are not an upper
bound on real games; CPU/GPU saturation, presentation and hybrid copies differ.

Controlled desktop capture of our silent GPU proxy, RTX 4070 Laptop driving
1920x1080 at 250 Hz, H.264 NVENC Balanced 1080p60 with WASAPI system loopback,
1200 shader iterations, 10 s warmup + 25 s capture per run:

| Configuration | Proxy FPS off/on | Overhead | 1% low off/on | Decoded repeats |
| --- | --- | --- | --- | --- |
| Existing high GPU priority, run 1 | 205.9 / 199.2 | 3.3% | 172.4 / 164.3 | 0.00% |
| Existing high GPU priority, run 2 | 204.0 / 198.1 | 2.9% | 159.4 / 164.3 | 0.19% |
| Normal priority, run 1 | 207.0 / 198.3 | 4.2% | 173.9 / 155.5 | 0.00% |
| Normal priority, run 2 | 205.6 / 199.1 | 3.2% | 172.6 / 144.6 | 0.09% |

Evidence: `test-output/overhead/nov11-hardware/all.json`. These are recovered
alpha.6 capture-engine measurements, not an alpha.7 versus alpha.6 improvement.
The selected path was D3D11 texture input to NVENC, without CPU scaling/readback.
No quality/preset/priority change was justified. Quick Sync is not the primary
candidate on this NVIDIA-driven display; an Intel-driven hybrid display needs
its own comparison. FFmpeg process + CLI CPU was about 0.57% of machine capacity
in run 1 (Windows performance counters). This excludes the native GUI.

Unique motion was decoded from the source's frame stamp. Near-zero repeats do
not mean perfect pacing: approximately 6.7% of source-clock advances exceeded
25 ms. This variability and physical gameplay smoothness remain unqualified.
Output timestamps alone are not evidence of unique 60 FPS motion.

Confirmed code findings: the GPU scaling fallback can round-trip BGRA through
system memory; exact scaling can therefore be costly, while the smooth D3D11
path avoids it. NVENC lookahead intentionally buffers video to prevent audio
holdback from starving the grabber; removing it without replacing that scheduling
mechanism risks the previously measured stutter regression. Keep quality and
lookahead unchanged pending a reproducible game trace.

The public NVIDIA guidance supports GPU-resident resource reuse and avoiding
copies; NVENC's dedicated engine does not make preprocessing free:
[NVENC programming guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvenc-video-encoder-api-prog-guide/index.html).

Alpha.7 removes an audio byte-buffer allocation per 10 ms chunk and applies
mute/gain after the bounded queue. This fixes stale queued microphone gain;
it is not evidence of a game-FPS fix. New audio queue telemetry and a redacted
report make future failures measurable. Video source FPS, video queue depth,
GPU usage and per-frame latency are explicitly unavailable live.

Repeat using `scripts/overhead-bench.py`; real-game and Discord instructions are
in [MANUAL_ACCEPTANCE.md](MANUAL_ACCEPTANCE.md).

## 0.3.0-alpha.1 — voice processing and startup

**Voice Enhancer** (release build, `replayx-cli voice-eval`, 20 s of speech): 0.5–1.0% of one CPU core per microphone (0.02% with the Off preset); added latency 11 ms with noise suppression on (RNNoise frame 10 ms + limiter look-ahead 1 ms), 1 ms without. Noise removed on fan noise: Light 9.5 dB, Balanced 17.0 dB, Strong 25.0 dB, Noise-Free preset 28.6 dB; hiss: 10.0 / 18.4 / 28.0 / 32.3 dB. Clean speech level changed by ≤0.3 dB through noise suppression. Before (FFmpeg `afftdn` as shipped): 2.2 dB on the same kind of noise.

**Startup** (`dev/startup-bench.mjs`, tray-only launch, warm, 4 runs each, medians, measured back to back): WebView page start 340 ms (0.2.2-alpha.2) vs 342 ms (0.3.0-alpha.1); interface mounted 455 vs 427 ms; app ready (settings loaded and the engine answered) 483 ms. The splash adds no delay: it paints from the first HTML byte (before, the window showed only its dark background until the interface mounted). First launch after building: 1.7–2.4 s to page start in both builds (WebView and disk caches cold).

## 0.2.2-alpha.1 — quality per GB and codecs

Method: `scripts/quality-bench.py` encodes the same footage with each configuration (NVENC p5, tune hq, spatial AQ, lookahead 20, no B-frames, 2 s GOP, exactly as REPLAYX does) and scores it against the source with FFmpeg's libvmaf (VMAF v0.6.1, plus SSIM and PSNR). Two sources: 20 s of the user's real Roblox recording (itself H.264 at 16 Mb/s, which slightly favours H.264) and a 12 s lossless synthetic game-like scene with noise foliage, a fractal, gradients, a minimap and a HUD (much harder to compress). Results: `test-output/quality/*/results.json`.

Real Roblox gameplay, 1080p60:

| Configuration | Mb/s | MB/min | VMAF | VMAF 5% low | SSIM |
|---|---|---|---|---|---|
| H.264 VBR 16 (0.2.1-alpha.3 Balanced) | 16.66 | 124.9 | 97.89 | 92.29 | 0.9959 |
| **H.264 CQ 28 (new Balanced)** | 14.65 | 109.9 | 97.61 | 92.53 | 0.9954 |
| H.264 CQ 30 | 11.86 | 89.0 | 95.65 | 89.93 | 0.9926 |
| H.264 VBR 10 | 10.49 | 78.7 | 93.27 | 84.35 | 0.9896 |
| HEVC CQ 28 | 14.20 | 106.5 | 97.29 | 92.64 | 0.9951 |
| HEVC CQ 30 | 11.25 | 84.3 | 95.59 | 90.05 | 0.9930 |
| **AV1 CQ 34 (new Balanced, cap 18)** | 12.47 | 93.5 | 97.65 | 93.00 | 0.9939 |
| AV1 CQ 36 | 10.52 | 78.9 | 96.64 | 91.59 | 0.9923 |
| AV1 CQ 38 (new Performance) | 8.81 | 66.1 | 95.18 | 89.84 | 0.9902 |

Synthetic hard scene: H.264 VBR 16.55 Mb/s → VMAF 84.0; H.264 CQ 28 → 22.0 Mb/s (under its 24 Mb/s cap), 86.1; AV1 CQ 34 capped at 18 → 18.5 Mb/s, 86.7; HEVC CQ 30 → 16.9 Mb/s, 83.1 (worse than H.264 at equal rate).

Conclusions: constant quality with a cap beats a fixed average (same picture for 12% less on real gameplay; more bits where a scene needs them, never above the cap). AV1 needs about 25% fewer bits than H.264 for the same VMAF. HEVC on this GPU with these settings is not meaningfully smaller than H.264, so the app does not promise smaller HEVC files. H.264 stays the default because it plays everywhere.

**Background cost while you play (0.3.1-alpha.3).** With capture off, REPLAYX does one check a second (which program is in front, which game it is, keyboard/mouse idle time): about 17 µs per check on this laptop (`replayx-cli tick-cost`), ≈0.002% of one CPU core. Playtime writes a small checkpoint at most once a minute. Hidden in the tray or minimised, the window's page is marked hidden (no drawing, a status check every 2 s). Everything after capture (segment checks, joining, playback checks, thumbnails, export) runs below normal priority; the capture and encoder processes keep normal CPU and high GPU priority, which the measurements below show is what keeps recordings free of repeated frames.

Game-FPS impact and smoothness (`scripts/overhead-bench.py`, the GPU-heavy game proxy full screen on the RTX 4070, system audio captured, Balanced 1080p60, 20 s per run, one run each):

| Codec | Game FPS without → with REPLAYX | Cost | 1% low | Repeated frames in clip |
|---|---|---|---|---|
| H.264 CQ 28 | 972.4 → 943.8 | 2.9% | 551.5 → 526.1 | 0.0% |
| HEVC CQ 28 | 982.5 → 939.2 | 4.4% | 559.3 → 524.7 | 0.0% |
| AV1 CQ 34 | 957.9 → 929.3 | 3.0% | 536.7 → 510.3 | 0.14% (one frame) |

The proxy runs uncapped near 1000 FPS here, so these percentages are an upper bound for a game at a normal frame rate; earlier runs at ~185 FPS cost 3–7%. Not yet measured in a real game.

Colour: NVENC's RGB→YUV conversion was measured by encoding solid colours from system-memory BGRA, CUDA frames and Direct3D 11 textures with H.264, HEVC and AV1: always BT.601 matrix, limited range (red → Y 81, Cb 90, Cr 239). Quick Sync's video processor converts to BT.709 (red → Y 61, Cb 102, Cr 239).

## 0.2.1-alpha.3 — game-FPS impact (measurement in progress)

**What was measured.** With 0.2.1-alpha.2 buffering at 120 fps (Quick Sync, 12 Mb/s, 2560×1600 → 1728×1080) on a near-static desktop, Windows' per-engine GPU counters (`\GPU Engine(*)\Utilization Percentage`, 8 × 1 s) showed REPLAYX's FFmpeg at **34.4% of the Intel Iris Xe 3D engine** and **19.4% of its video engine**; REPLAYX's own window added 8.2% of the 3D engine. The user's report: Roblox 150–200 FPS without recording, 80–100 FPS with it.

**Method for the comparison** (`scripts/overhead-bench.py`, needs a free desktop): `replayx-gameproxy` renders a heavy shader uncapped on the RTX in a borderless full-screen window, so its frames take the same Optimus path as a game. Its own frame times give FPS, 1% and 0.1% lows before and during a real REPLAYX capture of it. GPU engine use is sampled in the same window. The saved clip is analysed frame by frame (`gameproxy-check.py`: repeated frames, freezes, frame-advance percentiles). Each configuration runs back to back with a no-capture control for thermal drift.

| Configuration | Game FPS (avg / 1% low) | Intel 3D engine | Repeated frames in clip |
| --- | --- | --- | --- |
| No capture (control) | pending | — | — |
| Quick Sync, shader-assisted (0.2.1-alpha.2 path) | pending | pending | pending |
| Quick Sync, low-power VDEnc (0.2.1-alpha.3 default) | pending | pending | pending |
| Quick Sync VPP → NVENC (hybrid) | pending | pending | pending |

**NVIDIA-driven display, measured** (RTX 4070 Laptop driving the panel at 1920×1080@250 Hz, `replayx-gameproxy` at about 185 FPS, 60 fps recording, 25 s runs; game FPS before → during):

| Path | Repeated frames in the recording | Game FPS |
| --- | --- | --- |
| Zero-copy D3D11 → NVENC (no resize), normal / high GPU priority | 0.14% / 0.0% | −3.5% / −4.5% |
| Smooth: display pixels, marked to play at the chosen shape | 0.0–0.6% (5 runs) | −3% to −7% |
| CUDA copy path (resize through system memory), various priorities | 3.6–86% (7 runs) | −3% to −24% |

User recording before the fix (Fortnite 1440×1080 → 1920×1080, CUDA copy path): 76% repeated frames, 14 new pictures per second median, freezes up to 2.9 s (`scripts/clip-smoothness.py`). Remaining frame-to-frame variation in Smooth recordings follows the game's own frame pacing: at about 170–190 FPS, a 60 fps recording mixes 2- and 3-game-frame steps.

**Audio starvation (the stutter users saw), measured** with system-audio loopback and the game proxy at about 180 FPS:

| Pipeline | Repeated frames |
| --- | --- |
| Silent (all earlier smoothness tests) | 0.0–0.6% |
| With audio, one shared filter graph (0.2.1-alpha.3 before this fix) | 57% |
| Separate video/audio graphs | 33% |
| NVENC lookahead only | 58% |
| Audio timestamps from wall clock / muxer interleave 0 | 55–56% |
| **Separate graphs + NVENC lookahead 20 (default)** | **0.0–0.28%** |

During real Roblox play, Desktop Duplication delivered 133 new frames/s, WGC window capture 122 and WGC monitor 124. The app pipeline with audio kept 19 of them per second; silent it kept 60. A GeForce recording of Fortnite on the same laptop: 1440×1080, 16.5 Mb/s, about 30 new pictures per second (43% repeated).

**Real game.** Roblox, same place and camera, Roblox's own frame-rate display (Shift+F5): REPLAYX closed, replay at 60 fps, replay at 120 fps, recording at 1080p60, saving a replay while playing. Results will be recorded here as reported, labelled as user measurements.

## 0.2.1-alpha.2 — capture start

Measured with `replayx-cli start-bench silent` (real capture of the 2560×1600 display to 1728×1080 60 fps, Quick Sync zero-copy) and in the real window with `scripts/ui-record-check.mjs`. Times are from the start request.

| Start | Command returns | First frame encoded | *Recording* state |
| --- | --- | --- | --- |
| First start for this display (capture paths probed) | 1.1–2.4 s | 1.75–3.0 s | 1.8–3.0 s |
| Same session, again (cached path) | 8–9 ms | 654–668 ms | 678–795 ms |
| After a relaunch (path remembered on disk) | 9–20 ms | 403–681 ms | 406–812 ms |
| Record while Instant Replay runs | 0 ms | already encoding | immediate |

Peak CPU during a start 1.1–4.7% of all cores; peak memory of REPLAYX plus its encoder 267–292 MB. The bench above ran with a 0.25 s encoder progress interval; the released build uses 0.5 s (see VALIDATION.md), so it reports the first frame, and switches to *Recording*, up to 0.5 s after it happens. The start itself is not slower.

Before (0.2.1-alpha.1): every first Record after launching the app probed capture paths and checked the media runtime again; launch-to-first-finished-segment was the same as this build's cold start (3.40–3.66 s against 3.47–3.81 s, `scripts/start-latency.py`, 3 interleaved runs each). Its in-app timings were not instrumented and are not available. In the real window, 0.2.1-alpha.1 froze instead of reaching *Recording* in 2 of 2 valid runs.


## 0.2.1-alpha.1

**Smoothness (decoded motion, not FPS counters).** A silent full-screen pattern encodes a 60 Hz tick in on-screen bits; `scripts/motion-check.py` reads the tick from every decoded frame (validated against an independent numpy implementation). 20 s runs, 2560×1600 @ 240 Hz → 1920×1080, Quick Sync zero-copy:

| Capture and timing | Repeated pictures | Longest freeze |
| --- | --- | --- |
| `ddagrab` + max(previous+slot) re-grid (0.2.0) | 7.17% | 53 frames |
| `ddagrab`, no re-timing | 1.08% | 7 frames |
| `ddagrab` + slot rounding (0.2.1) | 0.83%, all in the first 0.3 s | 8 frames (startup only) |
| `gfxcapture` (Windows Graphics Capture) + rounding | 15.7% | 6 frames |

Full app pipeline (capture → segments → saved replay), 28 s: startup 0.3 s of repeats, then **6 repeated and 7 skipped pictures in 1,659 frames (0.36%)**, isolated single-frame slips. The 0.2.0 "0 dropped frames" figure below came from FFmpeg's counter on an almost static desktop and did not show this.

**Saving** (synthetic 1080p60 12 Mbps fixtures, `replayx-cli save-bench`; game running in the background at the time):

| Clip | 0.2.0 | 0.2.1 (segments probed during capture) |
| --- | --- | --- |
| 60 s replay (30 segments, 93 MB) | ≈6 s | 0.96 s |
| 10 min (300 segments, 913 MB) | ≈56–62 s | 5.0 s |

Real capture: a saved replay was ready **608 ms** after the save request. Synthetic recording: Stop handed over the clip in 706 ms (waiting for the in-flight segment), ready ≈1.0 s after Stop, with replay still encoding.

**Library** (`replayx-cli library-bench`, fixture clips): 100 clips 6.5 s → 46 ms (index reload); 1,000 clips 478 ms warm, 20.4 s for the one-time first index on 4 threads, which runs in the background while the list is already shown.

**Game impact** of 0.2.1 is not measured: no in-game FPS or frame-time comparison has been run. Overhead added by 0.2.1: one foreground-process query per second while capturing, and one small `ffprobe` per finalized 2 s segment at below-normal priority.


## 0.2.0-alpha.1 (Zephyrus M16: Iris Xe display GPU + RTX 4070 Laptop, 2560×1600 @ 240 Hz, game running)

| Path (1080p60) | Encoder process CPU, 15 s isolated | Whole capture-test CPU, 30 s | Dropped frames |
| --- | --- | --- | --- |
| CPU scaling + NVENC (0.1.x path) | 1.94 cores | 2.05 cores | 57 / 2,665 |
| Intel VPP + NVENC | 1.07 cores | 1.76 cores | 36 / 2,788 |
| Intel Quick Sync zero-copy | 0.56 cores | 1.17–1.32 cores | 37–46 (FFmpeg counter; see the 0.2.1 correction above) |

The "whole capture-test" figure includes REPLAYX, FFmpeg, startup probes and the full decode that verifies both saved clips, sampled once per second (`scripts/measure-capture.ps1`). Peak private memory summed across all of those processes was 0.8–1.2 GB. How it splits between the recorder and the save-verification decoders was not measured, so this is not a steady-state recorder figure. GPU utilization, encoder latency and in-game FPS impact were **not** measured.



Current-version gameplay performance is unmeasured because Windows blocked the media runtime. Earlier CLI CPU/memory values of zero were uninstrumented fields, not zero overhead.

The earlier real desktop report `test-output/capture-test-20261106-144915/report.json` measured NVENC with CPU scaling at 768 x 480: 29.65 reported FPS, 287 encoded frames, 9 output-synchronization drops, no WASAPI queue gaps. A concurrent saved replay was 8.136 seconds and 1,572,193 bytes. Retained buffer size at stop was 2,669,639 bytes over 10.821 seconds. This is not a 1080p gameplay benchmark.

The GUI now totals all recorder descendants, including nested WebView processes. CPU is normalized to logical-machine capacity; summed working memory may count shared pages multiple times. Frame/segment age measures liveness, not encoding latency. FFmpeg's drop counter does not cover every missed game-presented frame. The buffer write metric is retained bytes divided by retained seconds, not a disk I/O counter. GPU utilization and per-frame encode latency are unavailable.

For qualification, run identical fullscreen game scenes with recording off/on, at 1080p30/60 and both 1280 x 960 and 1440 x 1080 source modes. Record GPU/driver, Windows build, game version, refresh rate, NVIDIA scaling mode and actual scaler/encoder. Over 30 minutes collect CPU, memory, GPU encode/3D utilization, disk bytes, game frame-time traces, encoded FPS and drop counters. Save overlapping replays at minutes 1, 10 and 25; compare game average/1% low FPS and memory growth after warmup. Check physical audio/visual synchronization at start, middle and end.

At 12 Mbps, video alone is approximately 90 MB/minute, before AAC/container overhead. Hard-link snapshots pin bytes beyond the active ring budget until saves finish; filesystems without hard links copy them. Every segment is probed before concat and the output is fully decoded. That verification can compete with recording, especially for long/concurrent saves. Profile this workload before optimizing away checks. No current real soak or measured performance improvement is claimed.
