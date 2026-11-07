# Performance evidence and pending benchmark

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
