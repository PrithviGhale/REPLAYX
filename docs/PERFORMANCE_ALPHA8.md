# Alpha.8 performance investigation (local, unpublished)

## Scope and status

The user's Fortnite snapshots suggest a remaining cost (~13.2% for REPLAYX,
~20.8% with both recorders), but they are not sustained measurements. The
near-360 FPS ceiling can conceal NVIDIA's cost. No Fortnite improvement or
ShadowPlay-equivalent performance is claimed. No game or account was opened.

Baseline was clean `825edae` on `codex/performance-0.2.1`, version alpha.7.
All files, recordings, settings and installed application were preserved.

## Pipeline audit

| Stage | Actual implementation / consequence |
| --- | --- |
| Desktop acquisition | FFmpeg `ddagrab`, Desktop Duplication, D3D11 textures on the selected display GPU. The measured display is NVIDIA-driven, 1920x1080 at 250 Hz. This is not a native WGC callback implementation. |
| Window/legacy source | `gdigrab` uses a CPU path. Do not generalize desktop D3D11 results to this source. |
| Frame rate | Default acquisition is configured FPS (60), not game/display FPS. Timestamp gridding precedes expensive processing. Explicit oversampling is a profiling option, not the production default. |
| Direct NVENC path | GPU BGRA textures passed to NVENC, without an explicit CPU readback. The existing label “zero-copy” does not mean there is no driver-internal conversion or GPU work. |
| CUDA scaling fallback | `hwdownload BGRA -> hwupload_cuda -> scale_cuda`: a system-memory round trip exists here. It is not the measured direct path. |
| Intel paths | QSV maps to Intel VPP without CPU transfer on a suitable Intel-driven display; hybrid copies only scaled NV12 to the other encoder. Neither was tested as the primary path on this NVIDIA-driven panel. |
| Stretch/native | Smooth mode may encode native pixels with sample-aspect metadata. Exact scaling can require the slower transfer path. Existing geometry validation, recovery and aspect behavior are preserved. |
| Encoding | Actual H.264 NVENC, p5/HQ Balanced, spatial AQ, lookahead 20, no B-frames, capped CQ. Some preprocessing shares rendering resources. No quality setting was reduced in production. |
| Threading | Separate audio/video filter graphs, bounded filter queues, atomic progress at 0.5 s. CPU normal and GPU HIGH for capture by default; helper work below normal. Background UI/status work is not per-frame. |
| Replay | Encoded two-second MKV segments on disk, retention by time/bytes. Recording can share the existing replay encoder. Saves remux encoded packets and use independent jobs; thumbnails are asynchronous. |
| Health | Frame/segment/audio-write age, backpressure and encoder lag checks remain active. Device/geometry recovery, process jobs and orphan cleanup remain intact. |

## Confirmed findings and implemented improvement

The segment list grows throughout a session. Previously every maintenance poll
read and split its entire history, then skipped already handled rows. Alpha.8
uses a byte cursor: only new bytes are read; the cursor advances after each
validated segment. Partial rows are retried, disappearance/truncation produces
an error, and pending manifest bytes have a 1 MiB bound. This avoids work growing
with session age without changing encoded output, retention or save behavior.

Release microbenchmark, 20,000 historical rows / 500,000 bytes, 1,000 polls:
full scan 429.141 ms; incremental EOF poll 26.456 ms (about 94% less polling
wall time). This is a maintenance microbenchmark, not a game-FPS improvement or
a per-frame GPU latency measurement. Evidence: `.tmp-alpha8-manifest-bench.log`.

Hardware scaler probes: direct D3D11/NVENC succeeded; bundled `scale_d3d11`
failed to create its output texture for same-size NV12 and resized NV12.
Native NV12 render-target creation succeeded with ArraySize=1 and failed with
ArraySize=10, with and without VIDEO_ENCODER binding. This is consistent with
the scaler's array-pool allocation, not evidence that NV12 itself is unsupported.
Direct CUDA derivation failed as not implemented. Evidence:
`test-output/alpha8-gpu-probes/`. No runtime binary patch, driver change or
unvalidated replacement FFmpeg build was used.

Diagnostics previously replaced real scaling labels with “unavailable” because
their parentheses/arrow failed the redaction regex. Known paths now map to a
fixed allowlist; reports include known input format, adapter vendor, desktop
pixels and confirmed GPU priority without raw errors or private identifiers.

## Hardware attribution results so far

The detailed moving fixture adds encoder complexity to the original flat bars.
At ~300 proxy FPS, 1080p60 H.264 Balanced with silent system loopback:

| Experiment | A/B/A overhead | Repeated decoded frames | Decision |
| --- | --- | --- | --- |
| Original quality/priorities | 5.57% | 0% | Reference |
| Spatial AQ disabled | 6.00% | 0% | No benefit; preserve quality |
| Lookahead disabled | 6.13% | 33.59% | Reject: audio scheduling regression |
| GPU normal priority | 6.03% | 0% | No demonstrated improvement |

Evidence: `test-output/overhead/alpha8-detail-before/all.json`. These single-run
attribution comparisons included a counter sampler in B only; its overhead is
a limitation. Defaults were not changed based on these results. Initial
`alpha8-audit` runs used the old simple fixture and must not be labeled detailed.

New benchmark changes: A1/B/A2 windows, thermal-drift check, exact fixture-mode
reporting, fresh output directories, optional counter collection, GPU clocks /
power across all phases, and bounded nonblocking D3D11 draw timestamp sampling.
Draw timestamps measure the fixture's GPU work, not capture/encode latency.

One repeat without the counter sampler measured GPU utilization 91% before /
94.8% during capture; graphics clocks 2505 / 2497.5 MHz; power 78.6 / 83.0 W.
This supports shared GPU contention rather than a large clock collapse in that
run. Exact division among duplication copies, RGB conversion, driver scheduling
and NVENC preprocessing remains unmeasured. Nsight was not available. No CPU
readback was observed in the selected direct path.

Matched short-session before/after runs did not establish a FPS improvement:
5.73% versus 7.37% A/B/A overhead, with similar GPU draw-time increases and
near-zero duplicates. Counter sampling was still enabled. Incremental manifest
work is expected to matter with long histories, not explain this short-run GPU
cost. Further repetitions and the stability results must be reported honestly.

## Final validation

- 161 Rust tests passed, plus the explicitly executed opt-in release microbenchmark.
  51 frontend tests, four CSV-analyzer tests, TypeScript, rustfmt and Clippy passed.
- Native isolated updater/mic integration: 42/42 passed for alpha.8. Evidence:
  `test-output/update-e2e-0g5O2o/results.json`. No installer was executed.
- Six hardware codec/ROI cases passed; 12 saved clips fully decoded. H.264,
  HEVC and AV1 actually used their NVENC encoders. 1280x960 and 1440x1080 smooth
  regions had SAR 4:3 / DAR 16:9; exact 1280x960 -> 1920x1080 used CPU fallback.
  See `test-output/alpha8-matrix/results.json` and `aspects.json`.
- Lower GPU priority experiment: ABOVE_NORMAL cost about 8.8/9.3% against HIGH
  6.1/7.6% in the paired runs, and repeated more frames. HIGH remains default.
- 15-minute synthetic replay soak passed, with second NVENC workloads at
  300 s and 880 s. Both overlapping replay saves were ready in 805 ms. Both
  clips fully decoded; no buffer MKVs remained after shutdown. Evidence:
  `test-output/alpha8-soak/summary.json`.
- Soak resident memory 458.62 -> 461.26 MB (first/last steady-minute means),
  peak 528.69 MB including saves. Recorder-tree CPU 6.21% of machine capacity;
  this includes internal CPU synthesis/scaling, not desktop/game capture cost.
- End-of-soak media checks passed: three tracks, synthetic separation
  87.38/106.13 dB, max flash/pulse offset 30 ms over 59 events, monotonic packet
  timestamps. This is not physical microphone/system/editor validation.
- A single driver-stat sample during the soak reported one encoder session,
  83 average FPS and 2792 us average latency. These NVML global statistics are
  not unique captured FPS, per-frame latency or end-to-end capture delay.
- Normal-profile recording fields were read without changes: 1080p60, H.264
  NVENC, Balanced, stretch, Smooth. The newest cached probe was 1920x1080 D3D11;
  historical entries also included Intel hybrid/zero-copy and 1440x1080 D3D11.
  Cache entries do not prove which path the Fortnite screenshots used.

## Manual acceptance

Use [FORTNITE_ABA.md](FORTNITE_ABA.md) for sustained average FPS, 1% lows,
frame-time CSVs, cap handling, NVIDIA and Discord comparisons. The analyzer is
read-only and refuses short/unusable inputs. Actual fullscreen stretched mode
changes, physical microphone, editor multitrack playback and Discord sharing
remain manual tests. Live source FPS, texture-copy/conversion time, submission
and encoder-output latency are still unavailable and never reported as zero.

Public references: [NVENC guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvenc-video-encoder-api-prog-guide/index.html),
[FFmpeg NVENC implementation](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/nvenc.c),
[FFmpeg D3D11 scaler](https://github.com/FFmpeg/FFmpeg/blob/master/libavfilter/vf_scale_d3d11.c),
[documented Windows GPU scheduling classes](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d3dkmthk/ne-d3dkmthk-_d3dkmt_schedulingpriorityclass).
