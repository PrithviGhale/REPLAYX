# Native V2: measured motion improvement, remaining limits ? 2026-11-16

> Continuation: [frame-pacing reliability and performance preparation](NATIVE_V2_RELIABILITY_2026-11-16.md) records newer spatial-cadence, starvation/recovery and benchmark-preparation results. V2 is still experimental.


**Verdict: useful opt-in experimental candidate, not a completed universal
smoothness milestone and not ready for default promotion.** Real NVIDIA
recording and replay improved substantially. Mixed-desktop source selection and
large capture gaps still have retained counterexamples. Defaults remain unchanged.

Worktree: `C:\Users\Prithvi\Desktop\VS CODE\replayx-perf`, branch
`performance/capture-replay-stutter-optimization`, base HEAD
`0f1f003921c37480a6f182e9a6e61ec92c8c7d96`. Existing uncommitted work, settings
and recordings were preserved. All new artifacts are inside this worktree.
No game/fullscreen benchmark, merge, commit, push, installation, release,
Windows graphics-mode change or volume change was performed. One four-second
quiet tone was played after the user's specific approval.

## What actually improved

The old native clocked baseline lost **19 available pictures in 480 recording
frames**, and **14 in 360 replay frames**. Fresh phase-locked runs produced:

- **600 consecutive source IDs (47?646)** in the final recording; no repeats,
  skips, conversion errors, backwards frames or selection/decode mismatches.
- **480 consecutive source IDs (47?526)** in that replay; the same checks passed.
- Another start produced **360 / 240** unique recording/replay pictures with no
  selection losses. Start delays of 7, 11 and 13 ms were used in different cases;
  this is not a physical sweep of every possible phase or refresh mode.
- The longer isolated 59.94 FPS test produced **1320 / 1200** frames with one
  expected hold and no available source updates lost.
- Isolated 30 FPS recording/replay both contained 360 frames, 180 unique source
  pictures, each held for exactly two output slots.

These are controlled source-ID measurements of desktop motion, not gameplay FPS.
The baseline/candidate clips are neighboring runs with their full frame counts
shown; they are not equal-duration statistical gameplay comparisons.

## Root cause and implementation

The retained candidate from the preceding session corrected the first-capture
phase anchor, which could place output deadlines inside the compositor's arrival
cluster, and premature closure of repeated output slots by redundant captures.
Encoding was already faithfully encoding the wrong selections. The candidate:

- learns eight change observations before video frame zero, bounded by 500 ms;
- rebases the learned phase and publishes the shared audio/video clock afterward;
- leaves an unsealed repeated slot available for a late picture within the
  existing 1.25-period bound (20.833 ms at 60 FPS);
- uses picture age instead of the age of its newest redundant capture;
- requires one normal slot per near-60 Hz picture, two per near-30 Hz picture;
- keeps equal-time latest-update behavior, rational n/60 PTS, bounded state and
  existing texture leases. No rejected early-drop path was restored.

The common `grid.rs` / `phase.rs` policy was preserved during this continuation.
The following additional issues were fixed and independently exercised:

1. **Diagnostic ROI geometry.** The source window landed at odd pixel coordinates,
   and the physical capture was larger than 1080p. The old probe rejected that ROI,
   causing the first resumed test to fall back to FFmpeg and hit its memory guard.
   `native/trace.rs` now uses separate input/output layouts, rational per-bit
   scaling, aligned NV12 copy rectangles and bounded per-bit sampling. Two new
   tests cover odd origins, noninteger scaling and invalid bounds. The probe still
   exists only under explicit diagnostic flags; normal capture has no readback.
2. **DPI-virtualized inventory.** `native-topology` originally reported logical
   2048x1280 while the actual WGC surface was 2560x1600. It now uses the existing
   per-thread physical-pixel RAII guard and labels its coordinate space. The
   guard restores the thread afterward; Windows display settings are unchanged.
3. **Test validity.** MP4 ID decoding now maps scaled output back to the capture
   coordinates. Actual pacing mode is checked. PID/RSS guard failures retain
   their details. The VFR auditor distinguishes never-captured IDs from recorder
   losses and preserves both counters. No failed test has been erased.
4. **Reproducible source.** A new optional ordinary bordered maximized source
   window covers unrelated UI during isolation checks. It never enters exclusive
   fullscreen. Mixed-desktop tests remain in the ledger as failures.
5. **Resource measurement.** The helper retains the worker's final report through
   a local hard link, uses its actual shared QPC origin, excludes startup and save
   work, discovers owned processes once and verifies shutdown/buffer cleanup.
6. **Practical opt-in launch.** `--phase-lock` now works with the existing isolated
   desktop launcher, in a separate profile. Normal launch and FFmpeg defaults
   are unchanged. The GUI launcher was syntax-checked, not interactively tested.

Changed files include `native/trace.rs`, `native_proto.rs`, `geometry.rs`,
`native_gpu.rs`, the motion/resource/FFmpeg audit helpers and the isolated launcher.
The original timing fixes remain in `native/{grid,phase}.rs`. Bitrate, resolution,
quality controls, audio DSP, audio separation, hotkeys and production defaults
were not changed by this continuation.

## Physical hardware actually used

The current active display is the **NVIDIA-driven BOE NE160QDM-NZ5 internal panel**,
2560x1600 at **240 Hz**; Windows reports embedded DisplayPort (technology 11).
The topology and physical geometry now agree. Earlier reports of the external
1920x1080/250 Hz setup must not be applied to this session.

- NVIDIA RTX 4070 Laptop: real monitor capture, GPU scaling to 1920x1080,
  H.264 hardware encoding, MP4 recording/replay and independent decode verified.
- Intel Iris Xe: actual generated-texture H.264 encoding/decode, delayed reuse,
  cancellation and recovery passed. There are **no active Intel outputs**;
  physical Intel-display capture was not tested.
- No active separate external display was available in this session. Multiple
  monitors, mixed physical refresh modes and stretched fullscreen gameplay were
  not newly validated. No display was enabled or reconfigured for a test.
- Radeon hardware is absent. Five AMD tests are explicitly skipped; compilation
  and the shared vendor policy are not an AMD hardware-support claim. Intel Arc
  and other NVIDIA models are also unverified on actual hardware.

## All live motion outcomes, including failures

`repeated` counts identical source IDs, not necessarily identical whole-desktop
pictures. The source occupied a region in the small-window cases. Large dirty
notifications outside it may be genuine desktop changes. The gate reports raw
motion behavior and is not overridden because a failure has an explanation.

| Case | Clip | Frames | Repeated | Available IDs lost | Never in WGC trace | Ready ms | Raw gate |
| --- | --- | ---: | ---: | --- | --- | ---: | --- |
| resume16-clocked60 | recording | 480 | 19 | 19 | [] | 3013 | FAIL |
| resume16-clocked60 | replay | 360 | 14 | 14 | [] | 750 | FAIL |
| resume16-isolated30 | recording | 360 | 180 | 0 | [] | 738 | PASS |
| resume16-isolated30 | replay | 360 | 180 | 0 | [] | 630 | PASS |
| resume16-isolated5994 | recording | 1320 | 1 | 0 | [] | 2990 | PASS |
| resume16-isolated5994 | replay | 1200 | 1 | 0 | [] | 586 | PASS |
| resume16-isolated60 | recording | 480 | 1 | 1 | [] | 3023 | FAIL |
| resume16-isolated60 | replay | 360 | 0 | 0 | [] | 471 | PASS |
| resume16-phase30 | recording | 480 | 240 | 0 | [] | 3159 | FAIL |
| resume16-phase30 | replay | 360 | 180 | 0 | [] | 723 | FAIL |
| resume16-phase5994 | recording | 360 | 0 | 0 | [] | 991 | PASS |
| resume16-phase5994 | replay | 360 | 0 | 0 | [] | 771 | PASS |
| resume16-phase5994-long | recording | 1320 | 5 | 3 | [208] | 3339 | FAIL |
| resume16-phase5994-long | replay | 1200 | 4 | 2 | [208] | 1191 | FAIL |
| resume16-phase60-b | recording | 360 | 1 | 0 | [79] | 2296 | PASS |
| resume16-phase60-b | replay | 360 | 1 | 0 | [79] | 1830 | PASS |
| resume16-phase60-c | recording | 360 | 0 | 0 | [] | 1252 | PASS |
| resume16-phase60-c | replay | 240 | 0 | 0 | [] | 639 | PASS |
| resume16-phase60-final8 | recording | 600 | 0 | 0 | [] | 3068 | PASS |
| resume16-phase60-final8 | replay | 480 | 0 | 0 | [] | 789 | PASS |
| resume16-vfr-evaluation | recording | 761 | 280 | 0 available | [] | 985 | VFR integrity PASS |
| resume16-vfr-evaluation | replay | 761 | 280 | 0 available | [] | 889 | VFR integrity PASS |

The first resumed ROI/memory-guard stop made no smoothness claim. A first 60 FPS
candidate pass contained one upstream missing source ID (79), which was not
present in the WGC trace; its full one-to-one field remains false. The later
600-frame / 480-frame pass was clean from frame zero.

## Remaining counterexamples: explained, not hidden

### Busy desktop / regional motion

The small-window 30 FPS case had one three-slot hold despite retaining every
source picture. The larger 59.94 FPS case lost available IDs **483, 713 and 1333**
in 1320 frames; replay lost 483 and 713 in 1200 frames. Source ID 208 never
reached WGC. In the latter trace, **421 dirty positives did not change the source
ID region**. Notifications near the failures arrived roughly every 4 ms and
included substantial regions outside the moving source. Global dirty hints do
not identify which region supplies the intended 30/60 Hz motion clock.

When the same source covered the unrelated UI, the 59.94 FPS and 30 FPS checks
passed with **zero source-ID false-positive hints**. This supports dirty-activity
contamination as the regional cadence failure, not an NVENC pixel-order failure.
It does not establish universal mixed-content smoothness. Production frames do
not contain the diagnostic IDs, so the recorder cannot assume it knows which
region's cadence to preserve. This remains a blocker to default promotion.

### Real input gap and compressed burst

One isolated 60 FPS recording still failed the strict motion gate with one
repeat and available source ID **484** missing. Replay's shorter prefix passed.
The timing evidence is retained in `gap-reconstruction.json`:

```
WGC IDs: 482 --41.664 ms--> 483 --4.168 ms--> 484 --4.166 ms--> 485
CFR slots 435..438 chose: 482, 482, 483, 485
```

There was no FrameArrived notification during the gap. Native encoder submissions
were at most 1.654 ms, capture processing at most 1.978 ms, and surface-reuse waits
were zero. The generator's submission of 484 was itself 12.502 ms late. The exact
Windows/driver reason is not established; do not claim it was conclusively
outside the application. The measured problem is already present in the input
arrival/presentation timeline before output selection.

A 60 FPS timeline cannot retain all three four-millisecond pictures at their
actual capture presentation times. Buffering/retiming could change the tradeoff;
it would not preserve those times exactly. The recording keeps the real hold
and the strict test remains FAIL. No duplicate counter was suppressed and no
frames were invented to conceal this input pattern.

### Explicit VFR evaluation

The existing opt-in timestamp-preserving VFR path was tested independently:
**761 encoded pictures retained all 481 consecutive captured source updates**
in recording and replay. Its 280 source-ID repeats were timestamped captured
updates, not an assertion of 761 unique game pictures. MP4 timing followed the
capture timeline within **0.504 ms**, AVC level 5.2 was signalled, the fixed-rate
flag was clear, PTS were ordered, no B-frames were present and GOP time gaps
stayed within the tested two-second bound.

It was **not 60 FPS CFR** and encoded more samples. Short trace-enabled recorder
CPU was 0.537% of the machine in that case. This is not a controlled CFR/VFR
performance comparison. VFR remains an explicit experiment; it was not silently
selected, made the default or claimed to improve gameplay FPS.

## Capture arrival and timestamp evidence

| Case | WGC captures/s | Capture interval median / P99 / max | Source-ID hint positives without ID change / missed ID changes |
| --- | ---: | --- | --- |
| resume16-phase60-final8 | 94.368 | 12.4993 / 20.8332 / 20.8351 ms | 12 / 0 |
| resume16-isolated5994 | 59.989 | 16.6667 / 20.8406 / 20.8669 ms | 0 / 0 |
| resume16-isolated30 | 30.000 | 33.3335 / 37.5013 / 37.5023 ms | 0 / 0 |
| resume16-isolated60 | 60.000 | 16.6667 / 20.8327 / 41.6639 ms | 0 / 0 |
| resume16-phase5994-long | 133.074 | 4.1674 / 16.7396 / 24.9989 ms | 421 / 0 |

The clean final 60 FPS case read **995 dirty reports, zero read errors**; its
11 pre-origin observations are excluded from the 984 recorded-capture count.
There were no conversion/content errors, bad surface-lease events or leaked
input samples in the audited clips. Its encoder output latency was mean
16.638 ms / P99 24.393 ms; conversion and submission remain GPU-resident.
Probe completion timestamps are asynchronous readback completion, not exact
GPU execution timestamps. FrameArrived is a wake notification, not a frame ID.

All CFR app clips had monotonically increasing rational 60 FPS PTS, at most
0.333334 ms container quantization error, no B-frames and keyframes every 120
pictures. The complete 90-second recording (**5400 frames**) and retained replay
(**3600 frames**) were independently fully decoded and checked: 45 / 30 keyframes,
GOP 120, correct PTS, no decode errors. The trace-disabled long run establishes
integrity and bounded resources, not 90 seconds of unique-picture proof.

## Clean CPU, GPU, memory and retention measurements

No build, other test, audio playback or game ran concurrently with these three
measurements. The source window remained the ordinary small window. Sampling
used the worker's shared QPC origin, excluded the first two seconds and used a
conservative pre-save cutoff. These are owned persistent recorder processes;
Tauri UI, the test source and transient save/decode jobs are not part of this CPU
figure. Summed RSS can count shared DLL pages more than once.

| Mode / duration | Machine CPU | One-core CPU | Device GPU / encoder | Worker RSS first / last | Persistent RSS first / last |
| --- | ---: | ---: | --- | --- | --- |
| resume16-perf-before | 0.336% | 6.718% | 18.18% / 63.81% | 68.86 / 69.41 MiB | 115.44 / 115.90 MiB |
| resume16-perf-phase | 0.370% | 7.402% | 18.10% / 63.67% | 68.80 / 69.54 MiB | 114.84 / 116.66 MiB |
| resume16-perf-after | 0.319% | 6.370% | 18.27% / 63.71% | 68.60 / 69.11 MiB | 115.27 / 115.70 MiB |

The neighbor-baseline CPU mean was **0.327%**; phase measured **0.370%**, a cost
of **0.043 percentage points of machine CPU** (20 logical CPUs). The low recorder
CPU objective is preserved; this does not demonstrate a gaming FPS improvement.
The different durations and device-wide counters do not support a statistical
claim about recorder-only GPU cost.

The 90-second phase run lasted 90.573 seconds in the native worker and encoded
5426 pictures, including its tail beyond the final saved cut. Worker RSS peaked
at **69.56 MiB**, persistent summed RSS at **117.02 MiB**. Replay retained at most
**60 seconds / 19,748,467 bytes** and cleaned all owned buffer segments/workers on
shutdown. A 90-second check is not a multi-hour leak or long-session drift proof.
The saved 60-second replay became playable in **1724 ms**, and its 90-second
recording in **2323 ms**. The clean shorter replay took **789 ms**. Saves use the
existing video-copy/remux path; no video re-encoding was added. Concurrent replay
saves and library visibility passed the generated app integration checks.

## Actual audio and regression evidence

- The approved four-second tone was recovered from the independent **System
  audio** track at peak amplitude **0.0050068**, matching its 0.005 source.
- Real WASAPI system/microphone capture delivered **442 / 805 packets**, with
  **zero discontinuities** on both. Playback mix, System audio and Microphone
  were separate MP4 tracks. No volume or audio-device setting was changed.
- The physical signal test used the native backend's existing clocked mode.
  Phase-specific generated A/V markers passed separately across segments, within
  **0.396 ms**, including independent tracks. Physical speech, acoustic A/V
  alignment and long-session physical drift are not established by these tests.
- NVIDIA and Intel retained-trace hardware encodes independently decoded to the
  selected IDs, with correct GOP/PTS and zero leaked surfaces. A 40 ms delayed
  release test actually exercised surface waits and passed.
- The earlier calibrated retained trace improved in real hardware replay from
  67 available picture losses to zero; its remaining hold corresponds to
  never-captured ID 532. The historical 72-loss trace also reached zero.
- FFmpeg WGC/NVENC, explicit Exact size 1920x1080/60, two concurrent replay files
  and full decode passed. The first FFmpeg smoke gate incorrectly assumed the
  existing Smooth/full-size policy promised exact encoded dimensions; its
  2560x1600 clips decoded correctly and were retained. The test override was
  corrected; the production Smooth policy was not changed.

Final checks:

- Rust: **204 library + 43 native + 6 opt-in app tests = 253 passed**. One explicit
  manifest-cost microbenchmark remains ignored; it is not a functional check.
- Python: **48 passed, 5 Radeon hardware tests skipped**, 53 total.
- Frontend: **71/71**, TypeScript and Vite production build PASS.
- Clippy `-D warnings`, formatting, launcher/Python syntax checks: PASS.
- Offline optimized all-binary release build PASS; the native prototype and
  desktop executable were freshly rebuilt. The subsequently changed motion
  fixture was also freshly rebuilt in release mode.
- Rust/Python retained-trace + adversarial timing crosscheck: **84 matched
  trace/hint combinations**, PASS, stable binary SHA-256 pinned before/after.

The crosscheck includes modeled drift, delays, gaps, ties, low/high source rates,
static content and multiple refresh environments. It is model consistency, not
physical validation of every refresh mode or proof that perfect-ID hints equal
Windows global dirty hints. Hardware and live source tests provide those
separate checks. Raw live motion FAIL cases above remain FAIL despite passing
regression suites.

## Launch the opt-in application

```powershell
cd "C:\Users\Prithvi\Desktop\VS CODE\replayx-perf"
npm run desktop:native -- --phase-lock
```

In **Video > Recording engine**, choose **REPLAYX Native Engine (Experimental)**
and save. The new isolated profile still starts with FFmpeg as the selected
backend; phase-lock is enabled only by this explicit developer command. Check
that Home/Performance reports Native without fallback. The normal launch
without this argument retains the existing native timing/default behavior.

Profile: `test-output/native-desktop-phase-lock/data`.
Clips: `test-output/native-desktop-phase-lock/Clips`.
Two clearly labelled TEST clips were copied into this isolated library without
re-encoding: `Recording_TEST_Before_timing.mp4` (480 frames, 19 repeats/losses)
and `Replay_TEST_After_timing.mp4` (480 frames, zero). Both are eight seconds.
The after replay shares the verified recording's encoded stream; this is a
motion comparison, not an operation-latency comparison. Their originals and
hashes remain in `comparison-clips.json`. No existing file was overwritten.
The installed Alpha.4 and the earlier experimental profile are not used.
The GUI itself was not clicked through in this session; frontend regression and
actual Engine/CLI recording paths were tested. No installer was created.

Native worker: `src-tauri/target/release/replayx-native-proto.exe`.
Desktop executable: `src-tauri/target/release/replayx.exe`.
Use the isolated launcher to supply the test profile and experiment flags.
Full source/binary hashes, branch status and exact clip paths are recorded in
`test-output/native-phase-resume-20261116/candidate.json` and
`measurement-summary.json`. Original startup snapshots and failed runs remain.

## Readiness and remaining work

Measured improvements are real, useful and available explicitly. **Do not mark
V2 complete or promote phase-lock as the default** while mixed-content regional
cadence counterexamples remain. The capture-gap case also needs an explicit
latency/temporal-mapping policy; removing its duplicate would hide the real hold.
VFR is a measured alternative for preserving capture-time updates, not a silent
replacement for the requested CFR behavior.

Gameplay FPS/1% lows, actual fullscreen stretched 4:3 gameplay, an active external
monitor, Intel-driven physical capture, multiple monitors, Radeon hardware,
physical acoustic sync, multi-hour stability, hot-plug recovery and editor import
are not newly verified. No CS2 or other game benchmark was launched. Recording
quality settings and all production defaults are unchanged. Nothing was merged,
pushed, installed or released.
