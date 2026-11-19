# Native Engine V1 Alpha validation

> Historical V1 Alpha results. The subsequent [V2 NVIDIA compatibility milestone](NATIVE_V2_MILESTONE_1.md) supersedes the Intel-only restriction and adds actual NVIDIA desktop evidence, content-based smoothness failures and a hardware support matrix. Do not combine the candidates or interpret this earlier verdict as V2 completion.

Report prepared 2026-11-15. Application/device evidence was collected during the 2026-11-14 session in `replayx-perf`; final offline decode/format checks completed on 2026-11-15. No new game, fullscreen benchmark or capture was started after the owner said they were playing.

## Verdict

**PASS for a usable, isolated experimental desktop application; NOT fully validated for native real-world gameplay recording.** Selecting Native on the currently connected NVIDIA-driven display correctly falls back to FFmpeg. Native Intel encoding with the app lifecycle works, but native physical desktop capture on this current display is unsupported. Audible system audio, microphone speech, acoustic synchronization, editor behavior, long-duration drift/memory and all-features performance remain release gates.

No production default, version or installed profile was changed. Work is uncommitted on `performance/capture-replay-stutter-optimization`, based on `0f1f003921c37480a6f182e9a6e61ec92c8c7d96`. Existing uncommitted work and test recordings were retained. No merge, push, install or release was performed.

## Implemented and checked

- Existing Tauri/React Video page has FFmpeg/default and Native/Experimental selection; preferences survive a session-specific fallback.
- Native normal recording, stopping, playable MP4, disk-backed replay, concurrent saves, pending/ready events, thumbnails and library use the app's existing Engine/ring/save paths.
- A timestamped MPEG-TS bridge avoids raw-H.264 input scheduling stalls. Video is copied into segments and MP4; no CPU pixel readback or video re-encode was added to the native path. Transport/audio/mux overhead has not yet been benchmarked.
- Real PCM producers share the native frame-zero QPC clock. Removing `-shortest` for owned PCM resolved premature mux completion; generated lavfi input still uses it. The standalone harness now tolerates a brief clean mux EOF before its native supervisor writes completion.
- MP4 exports now carry handler names for audio tracks plus game/type tags. Windows publication refuses to replace an existing clip.
- Native startup waits for a validated closed segment. Unsupported initialization uses explicit FFmpeg fallback. Injected runtime worker failure reports Error and automatically preserves a playable recording from completed segments. Unfinished footage can be lost; no seamless runtime handoff is claimed.
- Updater polling and startup registry synchronization are disabled in the isolated Alpha launch.

## Automated checks

These completed before gaming resumed unless explicitly labelled offline final checks. Opt-in hardware tests used generated video/PCM without desktop capture, device capture or playback.

| Check | Actual result | Evidence under `test-output/` |
|---|---|---|
| Rust library/desktop suite | 199 passed, 0 failed, 4 ignored | `native-alpha-rust-final.log` |
| Native binary unit tests | 10 passed | same log |
| Ignored native app lifecycle cases, run explicitly and serially | 3 passed: concurrent replay/MP4/library, runtime failure recovery, incompatibility fallback | `native-alpha-app-tests.log` |
| Shared-clock delayed-device startup regression | passed | `native-alpha-clock-tests.log` |
| Python suite, including generated native ownership/stall/cancellation/start-stop/recovery cases | 29 passed | `native-alpha-python-final.log` |
| Frontend suite | 71 passed | `native-alpha-frontend-tests.log` |
| TypeScript and Vite production build | passed | `native-alpha-frontend-build.log` |
| Clippy with warnings denied | passed | `native-alpha-clippy-final.log` |
| Rust formatting and `git diff --check` | passed; rechecked offline 2026-11-15 | terminal output |
| Optimized native sidecar and CLI build | passed; fresh build completed in 13.24 seconds | `native-alpha-release-final.log` |

The 4 default ignored Rust cases are the 3 app lifecycle cases above and an explicit manifest microbenchmark. Older standalone selector/encoded-pattern checks passed earlier in the work; the final extra rerun was not executed because automatic approval review hit a usage limit. They were not rerun while the owner was gaming. Earlier failed integration attempts were followed by the fixes and passing serial suites above; they are not counted as successful runs.

Fresh optimized sidecar: `src-tauri/target/release/replayx-native-proto.exe`, 702464 bytes. SHA-256:

```text
523cf35cd0ecd92ec31226e747091eb0b02f539b048119a68f78e6176b517b5b
```

Its filesystem write time is 2026-11-15 04:29:10 UTC. This identifies the uncommitted Alpha candidate, not a committed or released artifact.

## Physical validation and exclusions

The owner approved an unattended short desktop/system/microphone capture, without participation, test sounds or games. No sound was played and no game was launched. These idle/silent device checks cannot prove audible quality, separation or acoustic synchronization.

1. `native-alpha-physical-20261114-223907/report.json`: real NVIDIA-driven desktop with FFmpeg fallback; 8.025-second recording and 6.025-second replay, 1080p, 3 audio tracks. Microphone packets were received; system audio was idle with 0 packets. No discontinuities were reported. This verifies fallback output and device opening, not native desktop capture.
2. `native-alpha-devices-20261114-225820/report.json`: **native Intel encoded generated video plus actual WASAPI system/microphone devices**. Recording 10.026167 seconds/600 video frames; replay 6.036833 seconds/360 frames; 3 AAC tracks. Microphone received 784 packets in a pre-stop sample; both sources reported 0 discontinuities. System source received 0 packets while idle. Microphone levels were effectively silent. Short-run drift correction was approximately -0.16 ppm; this is not a long-session drift result.
3. Earlier `native-alpha-devices-20261114-224443` and `...225143` attempts failed during initialization/transport. The first predates the physical-test fallback guard and is not valid native physical-audio success evidence. The later guarded attempt refused synthetic fallback. Historical outputs are retained.
4. `native-alpha-nvidia-feasibility.json` contains the failed NVIDIA required codec-control acceptance attempt. The error is not evidence that D3D11 is absent. Intel-only compatibility remains enforced; no quality or GPU settings were weakened.

The successful physical-audio files predate the MP4 handler-name/tag fix and still contain generic `SoundHandler` labels. They were preserved unchanged. Subsequent generated app tests verify all 3 source labels; subsequent desktop UI files verify playback mix/system labels and game/type metadata.

CPU/memory values of zero in the CLI capture reports indicate the UI performance sampler was not running. They are **not measured zero overhead or memory**. No new long soak, gameplay comparison or physical frame-content analysis was performed for this app integration.

## Desktop control validation

The real Tauri/React app was exercised in isolated **generated-content mode**, not through mocked UI commands: select Native, Save changes, enable replay, start recording by hotkey, save replay while recording, stop recording while replay remains on, stop replay, inspect Clips. Capture returned to Stopped with zero replay bytes and no surviving capture workers. Library thumbnails and MP4 entries appeared. The normal Alpha launcher was then started with no generated flag; its last observed state was **Stopped**, microphone disabled, new isolated FFmpeg-default profile.

| Saved desktop clip | Duration | Video frames | Time to Ready |
|---|---:|---:|---:|
| `native-desktop-generated/Clips/Desktop/Desktop_2026-11-14_23-28-14.mp4` (Replay) | 32.020833 s | 1920 | 588 ms |
| `native-desktop-generated/Clips/Desktop/Desktop_2026-11-14_23-28-21.mp4` (Recording) | 23.999667 s | 1440 | 1711 ms |

Replay stages were snapshot 8 ms, remux 10 ms, validation 293 ms, Ready 588 ms (cumulative). Recording included 1054 ms to finish/snapshot the current segment. The native generated-video/physical-audio replay reached Ready in 432 ms and its recording in 3456 ms. These are individual local runs, not comparative latency guarantees. Concurrent save correctness was covered by the app integration test; these UI numbers are for one replay and one recording.

## Offline media audit

On 2026-11-15, all 4 MP4s above (2 native-device/generated-video files and 2 generated desktop-UI files) fully decoded with software FFmpeg, all streams mapped, `-xerror`, one decoder thread and idle process priority. No audio playback or hardware capture/encode was used. Results are in `test-output/native-alpha-validation.json`.

All have strictly increasing decoded video PTS and 120-frame keyframe spacing. Per-frame PTS intervals range 16-17 ms because of the millisecond segment timestamps. Container duration includes AAC padding. These checks establish decoding and timestamp/keyframe continuity across joins; they do **not** establish smooth physical motion, lack of repeated source frames, acoustic A/V synchronization or long-run drift. No final content-hash or native surface hardware test was run while gaming. Surface safety is supported by the existing tracked-lease implementation and regressions, not a proof against every driver fault.

## Practical real-session checklist (requires a later approved session)

Do not run while gaming without explicit capture approval. No game/fullscreen benchmark should be launched automatically.

- [ ] Confirm the isolated Alpha title/data paths, FFmpeg default and disabled updater/startup integration. Choose Native, save, then confirm the **actual** backend after starting. A fallback session does not count as a native test.
- [ ] Use a supported Intel-driven display and 1080p60 H.264 Stretch/Balanced or Custom. Inspect capture geometry; do not change Windows/NVIDIA settings without approval. Native on the current NVIDIA display is blocked.
- [ ] Select the intended output and microphone endpoints, enable microphone, and check gain/mute/Voice Enhancer. With participation approved, use headphones and known system audio plus speech/claps and a visible marker at beginning/end. Idle zero-packet loopback is not an audible test.
- [ ] Enable replay and allow at least 6 seconds of history. Record motion-rich content; inspect pans and moving detail for repeats/skips. Source/output content analysis must accompany timestamp and metadata checks.
- [ ] Save replay twice while recording continues. Confirm saving state appears promptly, both clips become playable, thumbnails arrive, and stopping recording leaves replay running. Stop replay and verify buffer/process cleanup.
- [ ] Open MP4s in a player and an editor. Scrub around 2-second segment joins. Verify Playback mix/System audio/Microphone names, independent muting, actual source isolation and clean sound. Test application audio/Discord separately where Windows supports it.
- [ ] Compare visible/acoustic markers at start and end over an approved 1-5 minute test; target within one output frame. Measure drift across joins, not just initial alignment.
- [ ] With separate approval, measure combined app/native/mux CPU, GPU, memory and disk growth for 10-15 minutes, including repeated saves and at least 3 start/stop cycles. Preserve the previous video-only CS2 result as historical; remeasure the full audio-enabled app before claiming the same gains.
- [ ] With device-test approval, test endpoint change/disconnect, display-size change and recovery. Errors must be visible, completed files preserved and initialization fallback explicit. Do not inject driver faults, alter security/anti-cheat, or change GPU settings.

Remaining blockers: current-display native support; audible system/microphone and real A/V sync; editor/application-audio validation; native long-session/device-change/display-recovery behavior; and measured full-application performance. No installer, publication or production-readiness verdict is part of this milestone.
