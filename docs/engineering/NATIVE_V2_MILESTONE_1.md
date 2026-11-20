> **Final 2026-11-15 status: smoothness gate remains FAIL.** Earlier 720-frame unique-content success did not survive all start phases: the final app recording has 65 repeats in 480 frames, with every source ID captured and correct encoder output. A full-core native transport spin was separately fixed and measured. See [complete evidence and remaining blocker](NATIVE_SMOOTHNESS_2026-11-15.md). Do not treat compile/decode/oracle agreement as milestone completion.

# Native V2 - NVIDIA compatibility milestone (2026-11-15)

> Historical before-fix milestone. The subsequent [NVIDIA smoothness correction](NATIVE_SMOOTHNESS_2026-11-15.md) includes frame-stage instrumentation, a verified clocked-sampling fix and actual MP4/desktop results. This file preserves the earlier failures; it does not describe the newest candidate.

**NVIDIA desktop recording/replay now works through the shared native app pipeline on this RTX 4070 laptop. V2 is NOT complete. Smooth gameplay, AMD/Arc hardware, multi-adapter monitor selection and full-pipeline performance remain unverified or blocked.** No game/fullscreen benchmark, sound playback, physical audio capture, Windows/GPU/security change, install, merge, push or release was performed in this milestone. All work and evidence remain in `replayx-perf`; FFmpeg stays the production/default backend.

## Milestone 1: same-adapter native hardware correctness

Functional acceptance: initialize the actual display GPU's hardware encoder, preserve existing H.264 High / 1080p60 / 15 Mb/s target / 23 Mb/s cap / 120-frame GOP / no B-frames settings, encode/decode real desktop video, save recording and replay MP4s, drain with no live input leases, and keep Intel and fallback regressions passing.

**Functional result: PASS on the NVIDIA laptop/display tested here. Performance/smoothness acceptance: pending.**

### What was fixed

- V1 rejected NVIDIA because it assumed every hardware MFT implements the B-frame-count setter. This driver returns `E_INVALIDARG` for that setter and `E_NOTIMPL` for its capability/readback APIs. The other five required controls were accepted. Moving controls before media-type negotiation, as required for the documented B-frame control, did not by itself fix NVIDIA.
- The encoder now validates slice headers in **every encoded access unit before writing**. Unsupported B-frame control is allowed only with this output enforcement; B pictures, invalid/truncated/unsupported slice formats or unverified data fail closed. No bitrate, profile, latency or GOP requirement was dropped. `settings_accepted` honestly records the unsupported setter as false; the report separately identifies bitstream enforcement.
- NVIDIA retained input sample references after successful output drain, including after END_STREAMING/FLUSH. Explicitly shutting down the drained MFT released the tracked leases. All texture ownership and bounded waits remain in place; a surface is never marked free just because its encoded output arrived.
- The app's paced PCM fixtures used an earlier QPC origin and trimmed a startup-dependent prefix. They now use the same shared frame-zero clock as real native audio. A new native-start check surfaces audio producer errors directly instead of waiting for a generic segment timeout. Real WASAPI DSP, clock correction and the default FFmpeg path were retained.

### What was implemented

- Intel retains the verified V1 media-type/control initialization sequence; NVIDIA/AMD initialization differences are startup-only and covered by policy tests. No per-frame selector behavior was changed.
- Startup-only shared Intel/NVIDIA/AMD PCI policy; no vendor-specific copy of the recorder and no per-frame vendor enumeration. Explicit incompatible encoder overrides fail to FFmpeg with a reason.
- Hardware MFT enumeration filtered by the **capture adapter LUID** using MFTEnum2. Capture, video processing and encoding share the same D3D11 device. The native path adds no GPU-to-CPU-to-GPU transfer. Exact encoder name/vendor/LUID and transfer path are reported.
- Runtime output-clock and required GOP-boundary checks before muxing, in addition to B-frame validation. Output that violates them cannot be reported as healthy native recording.
- SDR display validation: HDR/unknown/non-BT.709 display color paths are explicitly rejected instead of silently tagging 8-bit HDR capture as SDR. HDR conversion remains unimplemented.
- Passive `replayx-cli native-topology` inventory of all adapters/outputs, without changing legacy FFmpeg output numbering. This is diagnosis, not a claim that every listed output is selectable or supported.
- Vendor-selectable generated encode checks, NVIDIA ownership/stall/cancellation/watchdog/start-stop regressions, an opt-in Radeon suite, and an offline windowed-fixture analyzer that rejects a static desktop as smoothness evidence.

AMD's branch currently uses the same hardware-MFT/D3D11 pipeline. **Direct AMD AMF API integration is not implemented.** The policy/transport infrastructure is shared, but real Radeon driver controls, formats, generation-specific limits and performance require AMD hardware. Do not describe this as verified Radeon support.

### Actual hardware and files

Passive inventory: `test-output/native-v2/topology.json`.

- NVIDIA GeForce RTX 4070 Laptop GPU, adapter LUID `0:78795`, DXGI adapter 0, attached `DISPLAY1`, 1920 x 1080 desktop.
- Intel Iris Xe, adapter LUID `0:90248`, DXGI adapter 1, no attached output reported during this session.
- No Radeon or Arc device is available here. No display or GPU settings were changed to expose another output.

The owner approved short generated NVIDIA/Intel tests and a short silent desktop test after finishing gaming. Tests used a **windowed** motion fixture, never fullscreen. No physical audio devices were opened.

The integrated Engine (the same implementation used by Tauri) saved:

- `test-output/native-alpha-desktop-silent-20261115-131826/clips/Recording_2026-11-15_13-18-34.mp4`
- `test-output/native-alpha-desktop-silent-20261115-131826/clips/Replay_2026-11-15_13-18-34.mp4`

Both are 1920 x 1080 H.264 MP4, 360 decoded frames, 5.999 seconds, zero audio tracks, correct game/type metadata, strictly increasing PTS, zero B-frames, and keyframes at 0/120/240. Actual backend was `native`, encoder `h264_nvenc`, fallback `null`. Recording Ready took **1752 ms**; replay Ready **1299 ms**, concurrently saving. These are single-run end-to-end timings, not a claim of improvement over FFmpeg. Evidence: `native-v2/desktop-app-retry.json` and `desktop-motion-audit.json`.

The first desktop test hit its diagnostic 12-second sidecar limit before the controller stopped, and its windowed fixture geometry was sampled before mapping (1 x 1). This was a failed integration run, not success evidence. Native output itself completed 722 frames with no live leases; its segments/replay remain in `native-alpha-desktop-silent-20261115-131559`. The test controller now stops earlier and waits for fixture mapping. Normal recording limits/defaults were not changed.

The final five-second desktop capture with the timestamp/GOP/SDR guards produced **300 decoded frames**, no B-frames, keyframes 0/120/240, success true and zero live leases. Files: `native-v2/final-desktop.ts`, `final-desktop.json`, `final-desktop-audit.json`. No motion-smoothness claim is made from this idle desktop file.

## Milestone 2: capture/motion diagnosis

Hypothesis: requesting WGC frames more often would reduce repeated pictures on the high-refresh external display while leaving selected-frame conversion/output at 60 FPS. Acceptance requires controlled content analysis and no quality/resource/CPU regression, followed by an approved gameplay comparison before any FPS claim.

**Result: hypothesis is not reliably confirmed; no cadence/default change was adopted.** Existing frame-selection behavior and production defaults remain unchanged.

| Short windowed trial | Capture request | Actual captures/s | Output pictures repeated | Skipped fixture ticks | Mean / P99 encode latency |
|---|---:|---:|---:|---:|---:|
| Same standalone recorder, default request | 12.5 ms | 63.97 | 26/480 transitions (5.42%) | 25 | 16.62 / 31.57 ms |
| Same standalone recorder | 8.333 ms | 79.94 | 19/480 (3.96%) | 18 | 16.63 / 30.20 ms |
| Same standalone recorder | 4.166 ms | 99.33 | 209/480 (43.54%) | 209 | 16.62 / 27.46 ms |

Each trial encoded/decoded 481 frames over about 8 seconds with contiguous timestamps, zero B-frames and zero live leases. The fixture reported zero missed callback ticks, but callbacks do not prove exact DWM presentation timing. Worker CPU was 4.87%, 3.70%, and 4.29% **of one logical CPU**, respectively; these are single-run native-worker observations, not total app/GPU overhead or statistically established savings.

The integrated MP4 trial separately showed **62 repeated pictures / 359 transitions (17.27%) and 61 skipped ticks**. Its source counter was valid in all 360 frames and never went backwards. It must not be represented as visibly smooth 60 FPS. That app run and the standalone trials have different context, duration and phase; their percentages are not a paired performance comparison.

An asynchronous, content-based bottleneck remains between window rendering/DWM capture cadence and output frame selection. The exact cause is **not established** by these trials. Driver/MFT bitstream integrity and timestamp regularity alone do not explain or dismiss repeated source pictures. Source/presentation timing instrumentation is the next focused investigation; blindly raising the capture rate is ruled out by the 4.166 ms result. No new gameplay FPS, 1% low, GPU-utilization or full-app memory-growth claim is made.

## Regression evidence

All paths below are relative to `test-output/native-v2/`.

| Check | Result | Evidence |
|---|---|---|
| Rust library/desktop suite | 202 passed; 4 default ignored | `rust-tests.log` |
| Native binary unit tests | 16 passed | same log |
| Explicit generated app lifecycle, runtime worker failure and FFmpeg fallback | 3 passed serially | `app-tests-final.log` (shared-clock fix) |
| Python incl. actual Intel and NVIDIA ownership/stall/cancellation/repeated-start and media tests | 37 passed; 5 AMD tests skipped (hardware unavailable) | `python-tests.log` (42 total) |
| Frame selector compared with FFmpeg | 7 patterns passed | `selector.log` |
| Actual NVIDIA encode/decode source-frame/keyframe checks | 5 patterns passed | `nvidia-patterns-final.log` |
| Actual Intel encode/decode source-frame/keyframe checks | 5 patterns passed | `intel-patterns-final.log` |
| Frontend | 71 passed | `frontend-tests.log` |
| TypeScript / Vite build | passed | `frontend-build.log` |
| Clippy warnings denied / formatting / diff whitespace | passed | `clippy.log` and terminal |
| Fresh optimized native sidecar / CLI | passed | `release-build.log` |

Default ignored Rust cases are the three app cases above and a manifest microbenchmark. Earlier app failures are retained: the first lacked the explicit sidecar path, and later runs exposed the PCM fixture clock problem. These were not counted as passes; the shared-clock run above passed. Tests do not prove every driver-reset/sleep/device-loss path or extended memory stability.

Machine-readable candidate provenance, including fresh executable SHA-256: `test-output/native-v2/candidate.json`. Work is based on branch `performance/capture-replay-stutter-optimization`, HEAD `0f1f003921c37480a6f182e9a6e61ec92c8c7d96`, plus preserved uncommitted changes. No new commit/release was made.

## Hardware support matrix

`Generated` means real GPU encoding of generated content, not simulated hardware and not physical gameplay capture. Software policy/unit tests do not establish hardware support.

| Target | Architecture | Encoder initialized / video decoded | Audio | Replay | V2 gameplay perf | Fallback | Remaining gate |
|---|---|---|---|---|---|---|---|
| Intel Iris Xe integrated | Shared same-adapter MFT | Yes, generated | Generated PCM; prior V1 idle-device evidence only | Generated app pass | Not measured | Generated incompatibility pass | Physical laptop panel/audio/session, full-app cost |
| Intel Arc discrete | Same Intel branch | Unverified: no hardware | Unverified | Unverified | Unmeasured | Policy only | Arc generation/driver device test |
| NVIDIA laptop (this RTX 4070) | Shared exact-adapter MFT | Yes, generated + real desktop | No physical audio in V2 | Real silent MP4 pass | Not measured | Existing path preserved; physical V1 fallback historical | Smooth motion, real audio, long sessions, gameplay A/B |
| NVIDIA desktop / older GeForce | Same NVIDIA branch | Unverified: no corresponding hardware | Unverified | Unverified | Unmeasured | Policy only | Per-generation encoder/format/control tests |
| Radeon RX discrete | AMD hardware-MFT branch; direct AMF pending | Unverified: no hardware | Unverified | Unverified | Unmeasured | Policy only | Actual Radeon; AMF backend evaluation/implementation |
| Ryzen Radeon integrated | Same AMD branch | Unverified: no hardware | Unverified | Unverified | Unmeasured | Policy only | APU shared memory/format/control tests |
| Intel + NVIDIA hybrid | Both encoders exercised; selected NVIDIA output | Yes on both GPUs; physical output NVIDIA only | V2 generated | Yes as above | No new result | Explicit source-vendor mismatch covered by policy | Active Intel panel / transfer-cost alternatives |
| AMD + NVIDIA / AMD + Intel | Same policy and LUID matching | Unverified | Unverified | Unverified | Unmeasured | Policy only | Corresponding hybrid hardware |
| Multiple independent displays / eGPU | Passive complete topology | Only currently selected default-adapter output tested | Unverified | Single selected output tested | Unmeasured | No wrong-output aliasing added | Native UI source identity beyond legacy indices, hotplug/spanning/migration |
| Unsupported/old/basic adapter | Explicit native rejection | No native support claim | Existing FFmpeg rules | Existing FFmpeg rules | Unmeasured | Unknown-vendor policy test | Physical fallback and no silent CPU downgrade checks |

## AMD/Arc and recovery test gate

On an approved Radeon machine, build the same sidecar and set `REPLAYX_NATIVE_TEST=1` and `REPLAYX_NATIVE_AMD_TEST=1`; run `python -W error -m unittest discover -s tests -v`. For NVIDIA use `REPLAYX_NATIVE_NVIDIA_TEST=1`. These flags opt into actual generated GPU encoding and must not be used while gaming without approval. The current AMD suite validates the MFT backend only, not a direct AMF implementation. Record exact GPU generation, driver, adapter LUID and actual encoder report. Do not mark AMD pass when those five tests are skipped.

For Radeon RX, Radeon APUs, Arc and older NVIDIA separately: initialize required H.264 High/1080p60/rate controls on the selected adapter; decode all generated source-frame patterns; inspect actual GOP/no-B/color output; run delayed-release/stall/cancel/repeated-start suites; then approve short physical motion/system/mic capture, independent tracks/editor playback, segment-join sync, concurrent replay saves, device changes and a bounded soak. Test exact source identity with two adapters/monitors before migration/hotplug/spanning claims. Direct AMF must preserve D3D11 surface ownership and bounded backpressure, using real per-device capabilities rather than assumed vendor-wide features.

Remaining release gates include physical audio/sync/editor validation on NVIDIA, motion-repeat root cause and smoothness, Radeon/Arc hardware, direct AMF, multi-adapter source/fallback identity, HDR conversion, recovery/device/sleep/disk-full tests, long RAM/VRAM/disk measurement and a controlled game A/B after explicit approval. Existing historical CS2 gains belong to the earlier video-only Intel prototype and cannot be assigned to V2.

Primary references used: [Microsoft H.264 encoder property requirements](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder), [adapter-filtered MFT enumeration](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mftenum2), [B-frame property type](https://learn.microsoft.com/en-us/windows/win32/codecapi/avencmpvdefaultbpicturecount-property), [AMD AMF H.264 API](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/blob/master/amf/doc/AMF_Video_Encode_API.md). They define API behavior, not proof of local hardware success.
