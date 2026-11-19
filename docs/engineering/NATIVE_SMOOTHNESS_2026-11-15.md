# Native NVIDIA smoothness correction - 2026-11-15

**PARTIAL FIX; FAIL for the final moving-source smoothness acceptance gate.** Scheduling, safe conversion/encoding and a full-core transport spin have verified corrections. A later repeated app test still exposed phase-sensitive repeat/skip pairs: **65 repeats / 480 MP4 frames**, despite every source ID reaching capture and zero captured-grid/conversion/decode errors. The earlier 720-unique-frame success is real, but is not enough to mark this blocker resolved. No gameplay-FPS improvement or production readiness is claimed.

Work remains entirely in `replayx-perf`, based on `performance/capture-replay-stutter-optimization` / `0f1f003921c37480a6f182e9a6e61ec92c8c7d96`, with existing uncommitted changes preserved. No merge, push, install, release, Windows graphics/security setting change, sound playback, physical microphone/system-device capture, game launch or fullscreen benchmark occurred. Test sources were windowed.

## Final repeat run: unresolved phase sensitivity

`app-mp4-transport-final/` is the final optimized candidate's real desktop/app test. The windowed source submitted 60 FPS content (15.8876-17.5142 ms submit intervals; median 16.6525 ms). All source IDs reached WGC, including all independently confirmed displayed IDs. There were 1,064 captures and 519 selected/encoded pictures across the complete trace. Capture intervals were quantized around 0/4/8/12/16/20 ms; newly observed source-content intervals were mostly 16 ms but also 20/24/28 ms.

The final MP4 recording contains **480 frames, 65 repeats**; replay contains **360 frames, 57 repeats**. Both decode completely, have strictly increasing 60 FPS timestamps, required 120-frame keyframes, no B-frames, zero source-to-conversion changes and zero selected-to-decoded changes. Full selected content has 71 repeat transitions and 72 two-ID advances across 519 pictures. There are no missing raw source IDs. The duplicates are not encoder corruption, surface reuse or unavailable source updates in this run.

The exact phase mechanism is visible: at output slots 3, 9, 15, 21, etc., the next source picture's WGC rendering timestamp is about **2 ms after** the output-grid time. The strictly causal sampler repeats the previous picture, then skips ahead. A separate offline phase sweep of these **same captured textures** produces **0-86 repeats in 480 frames** solely by changing the grid origin within one frame. This explains why earlier fixed-rate/one-start-phase experiments were inconsistent; it does not justify selecting a convenient phase after seeing test IDs.

Evidence: `app-mp4-transport-final/source-delivery-audit.json`, `phase-sweep.json`, `trace.jsonl`, `mp4-audit.json`; `app-motion-acceptance-final.log` records the expected **FAIL**. The independent auditors now have an additional known-moving-60-FPS identity gate, so matching the captured-timestamp oracle alone cannot yield a smoothness PASS. Static and 30 FPS sources are exempt from this one-to-one requirement and retain their correct repetitions.

No arbitrary phase offset, duplicate suppression, early drop, source-content readback in production or silent timestamp shift was added to hide this failure. Choosing a robust source-presentation-aware phase/resampling policy without source IDs, while preserving arbitrary-rate input, gaps and A/V timing, remains an unresolved engineering gate. These results support further development, **not completion of the NVIDIA smoothness milestone**.

Final saved files:

```text
test-output/native-alpha-desktop-silent-20261115-155316/clips/Recording_2026-11-15_15-53-25.mp4
test-output/native-alpha-desktop-silent-20261115-155316/clips/Replay_2026-11-15_15-53-23.mp4
```

Final replay was Ready in **672 ms**, recording in **3214 ms**, while saving concurrently. Clip lengths are six/eight seconds rather than the earlier four/six seconds; these are not paired saving-speed results.

## Root cause and evidence

The original aggregate (62 repeated pictures / 360 frames) did not include per-capture identifiers, so its 62 instances cannot honestly be partitioned retrospectively. The same failure was reproduced with full stage instrumentation, and the faulty mechanisms are now demonstrated rather than inferred from FPS metadata.

1. **Capture delivery was not the advertised 80 FPS.** A 12.5 ms WGC minimum interval produced irregular 12/16/20/24/28 ms delivery in the traced Tk reproduction. Raw source textures already omitted 31 generated counter updates. With the presentation-aware D3D source, the old path omitted 69 IDs; 48 of those were independently confirmed as displayed by DXGI statistics. This distinguishes real capture omissions from merely unpresented application updates.
2. **The legacy selector used a nominal input duration and FFmpeg rounding/tolerance behavior instead of sealing actual output-grid times.** Requested capture rate was treated as an input duration even when actual delivery differed. The traced old run selected content inconsistent with the newest captured picture valid at the output-grid time in **231 of 481 slots**. Its decoded output contained 81 repeated pictures, while a causal oracle expected 56 repeats from those same available captures. A separate D3D reproduction had 105 wrong grid choices in 480 pictures. Its raw repeat count (75) was actually lower than the oracle's 106, demonstrating why duplicate suppression/count alone is an invalid acceptance test.
3. **A startup tie mattered:** WGC can return updated content with the same compositor timestamp. The first full-app trace found one slot sealed before the later equal-time update arrived. Grid times are now sealed only after a later timestamp (or a bounded watermark), so the latest equal-time capture wins. A targeted regression reproduces this edge.
4. **Conversion, ownership and encoding were ruled out as the repetition source in these traces.** Source-stripe IDs matched GPU-converted IDs, selected IDs matched every decoded picture, no surface was converted with a live encoder lease, and all tracked input leases were released at shutdown. Regular encoder timestamps alone were not used as proof.

WGC's `SystemRelativeTime` is the compositor's QPC rendering timestamp, not the arrival callback time ([Microsoft API contract](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframe.systemrelativetime)). The trace records both, separately.

## Exact correction

- `src-tauri/src/bin/native/grid.rs`: a pure, bounded causal CFR scheduler. It chooses the most recent **actual capture** valid at each rational 60 Hz grid point. Every capture is processed before retirement; a capture that filled no earlier slot remains available for later gap recovery. This does not restore the rejected early-drop behavior.
- `native_proto.rs`: retain one pending WGC frame/texture lease, seal output choices after a bracketing capture, and use a condition-variable deadline with at most one output-frame watermark delay when idle. Static/low-rate content repeats correctly. Reverse clocks and gaps exceeding five seconds fail explicitly with completed footage retained; they do not allocate an unbounded catch-up list or claim healthy fresh capture indefinitely.
- NVIDIA native WGC uses no minimum capture throttle (`MinUpdateInterval=0`), based on the measured omissions above. This is coupled to correct grid scheduling, not another guessed capture rate. Windows still decides actual delivery; measured rates were about 99-107 captures/s on the tested 250 Hz desktop, not a promised 250 FPS capture rate.
- Three WGC pool buffers allow one retained pending capture, one current capture and producer headroom. The eight tracked NV12 encoder surfaces, fixed-function conversion, bitrate/profile/GOP controls, incremental transport and bounded encoder waits are preserved. Only selected pictures are converted. No production CPU pixel readback or extra copy of every captured frame was added.
- Intel keeps its verified legacy native default. The shared clocked algorithm also passed actual Intel GPU encode/decode tests via explicit selection. AMD can use the shared scheduler, but Radeon/Arc physical support remains unverified. FFmpeg remains the application default and initialization fallback.
- `native/trace.rs`, `lifetime.rs`: explicitly opt-in JSONL timing/lease diagnostics and bounded asynchronous copies of a small frame-ID stripe. Production runs construct neither probes nor a trace writer. Normal Alpha launch clears the test trace environment. Detailed generated schedules are opt-in and bounded.
- `motion_source.rs`: a low-load, windowed D3D fixture with unique 24-bit IDs, planned/submit/return timestamps and DXGI presentation statistics. It has no fullscreen/audio mode. Static and low-rate modes are intentional sources, not errors.
- Independent Python audits decode actual output and use a separate `bisect` oracle over the complete captured timeline. They share no Rust scheduling implementation.
- CLI/benchmark helpers expose an explicit test-only engine choice, fingerprint the selected sidecar and reject Native phases that fell back to FFmpeg. No production backend default or user profile was changed.

## Before/after frame checks

Evidence paths below are relative to `test-output/native-smoothness/`. All corrected rows have zero source-to-conversion and selected-to-decoded mismatches, zero wrong grid choices, zero B-frames, required keyframes and no live leases at completion.

| Case | Pictures | Actual repeats | Expected repeats | Wrong grid choices | Evidence |
|---|---:|---:|---:|---:|---|
| Old traced Tk path | 481 | 81 | 56 | 231 | `trace-baseline/audit.json` |
| Old D3D path | 480 | 75 | 106 | 105 | `d3d-legacy-before/audit.json` |
| Corrected Tk path | 480 | 1 | 1 | 0 | `trace-clocked/audit.json` |
| Corrected 60 FPS D3D motion | **720** | **0** | **0** | **0** | `d3d-clocked-final/audit.json` |
| Corrected static D3D image | 480 | 479 | 479 | 0 | `d3d-static/audit.json` |
| Corrected 30 FPS D3D source | 481 | 240 | 240 | 0 | `d3d-30fps/audit.json` |
| Earlier app MP4 recording | **360** | **0** | **0** | **0** | `app-mp4-clocked-ties/mp4-audit.json` |
| Earlier app MP4 replay | **240** | **0** | **0** | **0** | same |
| **Final app MP4 recording: motion FAIL** | **480** | **65** | **65** (captured oracle) | **0** | `app-mp4-transport-final/mp4-audit.json` |
| **Final app MP4 replay: motion FAIL** | **360** | **57** | **57** (captured oracle) | **0** | same |

The 720-frame run decoded source identifiers consecutively in all 719 transitions, with no generated/presented source-ID omissions. Its 1,187 captured textures included legitimate surplus copies; only the 720 valid output pictures were encoded. Source and callback jitter are measured rather than assumed absent. Trials have different durations/start phases; raw repeat percentages are not a statistically paired performance comparison.

App files remain at:

```text
test-output/native-alpha-desktop-silent-20261115-145140/clips/Recording_2026-11-15_14-51-47.mp4
test-output/native-alpha-desktop-silent-20261115-145140/clips/Replay_2026-11-15_14-51-47.mp4
```

Both match a unique contiguous range of the full captured/selected ID trace. Recording covers IDs 42-401; replay covers 42-281. Replay was Ready in **629 ms**, recording in **1214 ms** while saving concurrently. These are individual 4/6-second clips, not a speedup guarantee or a paired comparison with the earlier longer clips. Video remains stream-copied through segmentation/remux; no video re-encoding was introduced.

### Timing, gaps and ownership

- In the 720-frame moving case: capture gap 0-20.0043 ms, maximum compositor-timestamp-to-dequeue delay 8.2615 ms, 1,188 arrival notifications, 1,187 dequeued captures, zero missing diagnostic probes. Coalesced/extra notifications are not treated as extra pictures.
- Output TS PTS are strictly increasing, with 90 kHz quantization (observed 16.655-16.667 ms intervals). Native MFT input/output timestamp guards remain active. Saved MP4s have at most **0.334 ms** error against the ideal per-picture timeline after the existing segment time-base rounding.
- App keyframes are 0/120/240 for the recording and 0/120 for replay; the longer moving test has 0/120/240/360/480/600. Zero decoded B-frames.
- A 5 ms capture which originally filled no output slot is retained and correctly supplies the next slots across a 400 ms gap in the targeted pure regression. Generated hardware gap/jitter/under-rate tests independently check every decoded selected identity.
- Release-delay tests force encoder-surface waits; textures remain immutable until tracked release. Actual lease logs assert zero occupied surfaces at conversion, and shutdown checks zero outstanding leases. This is not proof against every untested driver/device-loss failure.

## Audio and stop regression

Actual NVIDIA encoding was combined with two independent paced PCM fixtures plus playback mix through the app's real audio/mux/save path. Decoded audio markers at 0.5/1.5/2.5 seconds relative to video aligned within **0.396 ms** at all three points, including across a two-second segment boundary. Video frame 30 decoded the correct source marker (luma 76). Evidence is the `test-output/native-v2/app-4AkMWW/av-sync.json`, with the final run logged in `app-tests-final.log`.

AAC priming shifts the container's first video PTS to 0.021 seconds while audio starts at 0. That is a shared container timing shift, not a 21 ms A/V error; the independent check uses actual video PTS, not an assumed zero. Physical WASAPI speech/intelligibility, acoustic sync and long-session drift were not tested here; No physical audio capture streams were started; normal app warm-up may initialize its default loopback client without starting it.

The new clocked generated-session stop test caught an early-stop helper bug: it initially tried to seal the unplayed planned 60-second timeline. It now seals only delivered captures and drains normally. This affected the new generated test path; real desktop stopping already used its actual stop clock.

## Measured overhead and memory

No game FPS was measured. These are trace-disabled, windowed desktop recording profiles, each with five-second OFF neighbors, a fixed 28-second recording reservation containing a 20-second capture, and a total 38-second measured window. GPU data are coarse, device-wide NVML values including the fixture and desktop; they are not per-process GPU cost or FPS loss.

| Metric, one verified before/after pair | V2 before | Corrected |
|---|---:|---:|
| Native worker CPU, percent of one logical CPU | 7.80% | 4.92% |
| CPU normalized to this 20-logical-CPU machine | 0.390% | 0.246% |
| Encoder worker RSS, start/end | 68.16 / 68.40 MiB | 68.26 / 68.66 MiB |
| Mean / P99 encode latency | 16.65 / 34.38 ms | 16.65 / 25.57 ms |
| Device GPU active mean | 23.17% | 26.87% |
| Device GPU OFF-neighbor mean | 11.26% | 12.02% |
| Encoder-engine utilization mean | 20.01% | 19.88% |
| Device VRAM active minus OFF-neighbor mean | about 49.9 MiB | about 59.7 MiB |

Evidence: `profile-before-verified/profile.json`, `profile-after-verified/profile.json`. Earlier CPU samples overlapped (before 4.76%, after 5.31% of one CPU), so **no statistically established encoder-worker CPU gain is claimed from this pair**. Correctness was the optimization hypothesis and acceptance gate. Higher GPU activity is a measured tradeoff of recovering capture updates; a gameplay comparison is needed to determine its FPS effect. An extra retained WGC buffer is roughly 8 MiB at 1080p; coarse device counters are consistent with a small additional allocation, but are not exclusive allocation attribution.

The first profiling attempt mistakenly sampled the supervisor's RAM. Its files are retained as `profile-before/` and `profile-after/`, explicitly marked `rss_sampling_verified=false`; those RAM numbers are excluded. Verified profiles identify the actual `--native-worker` PID.

### Additional verified fix: host transport spinning

The whole-pipeline soak revealed CPU cost hidden by encoder-worker-only measurements. The accepted video socket inherited the listener's nonblocking mode on this Windows machine; the relay retried `WouldBlock` immediately. A direct regression reproduced an idle read returning `WouldBlock` in **17.9 microseconds**, while the host's single relay thread consumed almost one core.

`engine_native.rs::configure_video_transport` now explicitly restores blocking mode before applying the existing 500 ms read timeout. Idle reads sleep in the OS, live data still forwards immediately, and shutdown still closes the socket. A real-socket regression checks bounded idle blocking, data delivery and EOF. There is no extra per-frame sleep, GPU copy or changed scheduling. Final Clippy also corrected three test-only constant-chunk lint findings.

Same windowed-source, 90-second full-engine tests, using raw process CPU counter deltas over the steady interval:

| Metric | Before blocking-relay fix | After |
|---|---:|---:|
| Host CPU, percent of one logical CPU | 99.06% | 2.46% |
| Identified persistent capture processes together | 103.02% | 8.71% |
| Normalized to 20 logical CPUs | 5.151% | 0.435% |

Evidence: original `app-soak90/soak.json`, final `app-soak90-transport-final/soak.json`, `host-cpu-comparison.json`, `host-cpu-diagnostic/threads.json`, `transport-regression.log`. This demonstrates removal of an idle CPU spin, **not a measured game-FPS gain**. GPU figures above are separate native-worker profiles; the final full-engine soak did not collect GPU counters.

The final 90-second check saved valid 91.999-second recording/59.999-second replay, retained at most 60 seconds / 11,375,269 bytes of replay segments, and cleaned up every owned capture process and buffer segment. Encoder RSS **68.69 -> 69.37 MiB**; persistent-process RSS **114.36 -> 115.99 MiB**, peak 116.41 MiB. Recording/replay decoding and audio-free cleanup passed; smoothness was not inferred from this soak.

### Full-engine 90-second check

`app-soak90/soak.json`: 90-second capture target, about 98.65 seconds including startup/save/verification; saved recording 91.999 seconds and replay 59.999 seconds. Encoder RSS rose **68.90 -> 69.70 MiB**. Identified persistent capture processes together rose **114.46 -> 116.47 MiB**, peak 116.56 MiB. Shared DLL pages can be double-counted. Replay retention reached exactly 60 seconds and 11,600,800 bytes for this simple source. All capture workers exited; no buffer segment files remained. Saved clips were fully decoded by the test command.

This covers the headless app Engine, native worker, muxer, recording pins, retention and saves. It excludes the Tauri frontend and transient verifier memory. It is a short stability check, **not a multi-hour leak/drift or low-memory/exhaustion certification**.

## Regression ledger

Final logs are in `test-output/native-smoothness/`.

| Check | Result/evidence |
|---|---|
| Rust library/desktop | 203 passed; 5 default ignored, `rust-tests-final.log` |
| Native unit tests | 22 passed, including six independent grid/edge regressions |
| Explicit app/PCM/runtime-failure/fallback suite | Four passed serially on the final candidate; `app-tests-final.log` |
| Python | 43 passed; five AMD hardware cases explicitly skipped (48 total), `python-tests-final.log` |
| Legacy selector vs FFmpeg | Seven patterns, `legacy-selector.log` |
| Intel/NVIDIA unchanged legacy encode/decode | Five patterns per GPU, `legacy-intel.log` / `legacy-nvidia.log` |
| Clocked source schedule + full decode on actual Intel and NVIDIA | Eleven patterns per GPU, `grid-intel.log` / `grid-nvidia.log` |
| Clocked NVIDIA injected 50 ms release delay | Regular/gap cases, `clocked-backpressure.log` |
| Frontend | 71 passed, `frontend-tests.log` |
| TypeScript / Vite | clean, `frontend-build.log` |
| Clippy / formatting / optimized builds | strict all-target desktop Clippy: `clippy-final.log`; `release-build-final.log`; formatting checks |
| Final moving desktop/MP4 source-identity acceptance | **FAIL**, 65/480 recording repeats and 57/360 replay repeats; `app-motion-acceptance-final.log` |
| Normal FFmpeg WGC/NVENC real desktop smoke | `ffmpeg-regression/smoke.json`; a decode/backend regression check, not new smoothness certification |

Earlier failed runs remain evidence, not passes: startup timestamp-tie exposure, generated early-stop planned-timeline error, the initial A/V test's incorrect zero-start assumption, and a regression harness that waited on undrained diagnostic stdout pipes. The harness now uses file-backed output so a larger JSON report cannot masquerade as an encoder stall. Encoder deadlines were not increased to make tests pass.

Fresh executable/source fingerprints and exact test totals are recorded in `candidate.json`. No installer was made.

## Hardware scope and remaining gates

- **Physically tested:** RTX 4070 Laptop GPU, NVIDIA-driven 1920 x 1080 external SDR desktop, 250 Hz Windows mode. Some windowed moving-content runs passed; the final 60 FPS identity run **failed phase-independent smoothness**, as detailed above. Static/30 FPS input, MP4/replay decode, lease safety and silent full-engine soak passed.
- **Actual Intel hardware, generated textures:** legacy preservation and all clocked encode/decode cases passed on Iris Xe. No active Intel-driven panel was available; its default stays legacy. Intel Arc remains physically unverified.
- **120/144/240/250/500 Hz:** deterministic timestamp/delivery models were tested without changing monitor settings. They are not claims of physical testing at all those display modes.
- **AMD Radeon desktop/APU and mixed AMD hybrids:** shared scheduling/ownership design is compatible; hardware encoding remains explicitly unverified. Five AMD hardware tests stay skipped. Direct AMF work is still a separate milestone.
- Multiple physical displays, rotation/HDR conversion, GPU switching, sleep/resume and device-loss recovery are not newly certified. Existing explicit geometry/color rejection and FFmpeg fallback remain.
- Physical game capture, actual microphone/system sound quality, acoustic A/V sync, long sessions and thermally steady gameplay FPS still need approved sessions. No old video-only CS2 gains are assigned to this candidate.

## Prepared gameplay comparison - NOT RUN

The existing controller now has `app-native-nvenc`, which exercises the full shared Engine pipeline rather than comparing FFmpeg with a bare native encoder. It validates the actual backend and rejects fallback phases. The selected sidecar is pinned to the same release directory and fingerprinted. Both arms are silent 1920 x 1080 / 60 FPS H.264 Balanced / stretch / 60-second replay, with FFmpeg NVENC explicitly selected.

Safe plan-only command (already executed; no process capture or game launch):

```powershell
python scripts/bench/fortnite-session.py --game cs2 --configs wgc-nvenc-auto,app-native-nvenc,app-native-nvenc,wgc-nvenc-auto,wgc-nvenc-auto,app-native-nvenc --seconds 30 --warmup 60 --plan-only
```

Expected **11:43**, 13 phases including 60-second warm-up, countdown, settling, cleanup and analysis reservations. Each of the six recording phases has adjacent OFF phases; three ordered pairs are available only if their required phases complete. Missing A7 cannot become a completed baseline or a third paired result. The controller reports phase/deadline progress, failures/partial results, FPS/1% lows/P99 when an approved PresentMon collector is supplied, CPU/GPU and recording statistics.

Do not start this performance comparison until the outstanding phase-sensitive smoothness gate is resolved. Actual execution also requires the owner's approval and a compatible PresentMon executable supplied through `--presentmon PATH`; none was installed or launched here. A valid, repeatable in-game motion scene must already be running and foreground. Keep graphics, frame cap, power state and audio equal without changing settings automatically. Check saved gameplay picture identities/motion alongside FPS; metadata is insufficient. This planned comparison cannot establish a gain until measured.
