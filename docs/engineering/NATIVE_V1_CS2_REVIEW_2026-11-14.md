# Independent CS2 review and Native Engine V1 ? 2026-11-14

## Verdict

**PASS to continue experimental Native Engine V1 development. Not approved as a production replacement.** The completed measurements consistently favor the native video path. This is a small, same-order, mostly stationary-scene comparison, not proof of performance or motion quality across games, PCs or long sessions.

Work remained in `C:\Users\Prithvi\Desktop\VS CODE\replayx-perf`. Earlier uncommitted changes and all source recordings were preserved. No further CS2/fullscreen benchmark, physical audio capture, microphone use, settings change, installation, commit, merge, push or release was performed.

## Independent measurement check

Input: `test-output/cs2-session-20261114-211929`.

The analyzer independently re-read the completed phases' PresentMon CSVs, applying the existing five-second trim to each 45-second measurement. Approximately 40 seconds per completed phase contribute to each metric. These are means of phase metrics, not a pooled 1% low. All analyzed phases used the same process/swap chain and 100% Hardware Composed: Independent Flip.

| Mean of completed phases | Recorder OFF | Existing WGC recorder | Native video prototype |
| --- | ---: | ---: | ---: |
| Completed phases | 6 | 3 | 3 |
| Game FPS | 485.85 | 380.37 | 410.18 |
| Phase 1% low FPS | 224.11 | 169.11 | 186.58 |
| Phase P99 present interval | 4.2385 ms | 5.6415 ms | 5.1010 ms |
| Recorder CPU, whole machine | ? | 1.5867% | 0.6500% |
| DWM CPU, whole machine | 0.18?0.23% per phase | 1.5667% | 1.5567% |

Native's descriptive gain is 29.81 FPS (+7.84% versus the current recorder), 17.47 FPS in phase 1% lows, and 0.5406 ms lower phase P99. Recorder-process CPU is about 59% lower; compositor CPU remains similar. This is a real present-interval measurement, not recording-file FPS metadata. PDH engine names are driver labels; a label containing `videodecode` is not proof that video decoding, rather than the driver's encoder engine mapping, caused the workload.

| Neighbor-normalized phase | FPS cost | Neighbor spread |
| --- | ---: | ---: |
| Current B1 | 22.3861% | 0.7951% |
| Native B2 | 15.4996% | 0.3495% |
| Current B3 | 22.0535% | 0.0431% |
| Native B4 | 14.8070% | 4.2406% |
| Current B5 | 20.6733% | 5.8000% |
| Native B6 | **Unavailable** | A7 incomplete |

Only **two valid pairs** exist: 6.8865 and 7.2465 percentage points less FPS overhead. Their mean is 7.0665 percentage points. There is no third paired result or three-pair confidence interval. B6 remains valid for descriptive recorder metrics and file integrity; it is excluded from the complete-neighbor cost comparison.

A7 has a partial CSV but no completed summary phase. It was not used as a baseline or silently substituted. The summary reports 12/13 completed phases, a focus/exit stop after 957.177 seconds (about 15:58), and a planned 988 seconds. The stop occurred with no recorder active. It is not evidence of encoder failure. All six recording phases exited successfully.

The actual order was current/native repeated three times. Native always followed current, so order bias remains. The A5 dip and later recovery also show why neighboring normalization matters. GPU temperatures averaged 66.77?C in both recording groups, but temperature similarity alone does not exclude scene, scheduling, power or thermal-state bias. Two pairs support further engineering, not a statistical significance claim.

## All three native recordings

Every frame was decoded twice for integrity/hash and motion analysis. Four chronological frames per clip (0, 18, 36, 54 seconds) were also visually inspected. Each contact sheet shows full gameplay with the same mostly stationary view; no desktop strip, black border or obvious corrupted region is visible in those samples. This is not a stretched-resolution validation or a substitute for watching a motion-rich camera sweep.

| Native file | Decoded frames | Expected duration at 60 FPS | Keyframes | Bytes | Mean video bitrate |
| --- | ---: | ---: | ---: | ---: | ---: |
| B2 | 3,421 | 57.0167 s | 29, every 120 frames | 108,522,455 | 15.2268 Mb/s |
| B4 | 3,422 | 57.0333 s | 29, every 120 frames | 108,555,638 | 15.2270 Mb/s |
| B6 | 3,421 | 57.0167 s | 29, every 120 frames | 108,491,302 | 15.2224 Mb/s |

- All are H.264, 1920?1080, fully decodable with `-xerror`, with decoded count = encoded count = encoder output count and actual bytes = reported bytes.
- Keyframes are exactly at 0, 120, ?, 3,360. Required codec settings were accepted. Encoder mean latency was 26.70?26.81 ms; P99 was 39.53?39.73 ms.
- SPS timing is `num_units_in_tick=1`, `time_scale=120`, progressive, fixed-frame-rate: 60 FPS. The raw demuxer's reported `r_frame_rate=120`/average-rate estimate does not establish 120 FPS output.
- Full-resolution decoded hashes found **zero identical adjacent output pictures** in all three clips. Small-luma mean frame differences were only ~0.119 levels, consistent with the stationary scene and small animations/compression changes. Distinct decoded hashes do not prove every source capture was new. They do not establish visibly smooth active-gameplay motion.
- Encoder telemetry reports ordered timestamps, no duplicates, no repeated dropped sources and no resize skips. Approximately 1,128?1,130 surplus captures per phase were intentionally removed by the 80-to-60 FPS selector; they are not lost encoded output frames.
- **Raw H.264 has no container PTS.** Exact per-frame source QPC timing, capture intervals and jitter cannot be independently reconstructed from these files. SPS, frame counts and the encoder's monotonicity report agree, but do not prove capture-time continuity by themselves. V1 now records its QPC origin and validates every encoder-output step before declaring completion; its generated MP4 packet timeline is independently tested.
- The ~57-second file lengths include startup/settling around the 45-second measurement. They are not supposed to be 45-second clips. Tiny endpoint differences from wall time are compatible with CFR rounding/drain; there is no basis here for a long-session synchronization claim.

Video SHA-256:

```text
B2 c49ffd20b4b56ce612181f5fd6f940c22838d12438fbd5de438726dd2f1bba88
B4 e85d1074568d630a71ff72fa7eecad11689ba5f86c498e6f4a75400664909e12
B6 70190bced7999555b0cc787a3f2107be68b61f26f17ee4d3765eef53ad32be2f
```

The benchmarked native executable hash was `18904e3f0e62235dc8ec779e312218dc5f25156effedcd9f8f89cdb4535ed8e3`, exactly the preceding reliability build. Subsequent V1 work rebuilds that pathname; it must not be confused with the binary that produced these game results. The benchmark's `source` field reports HEAD `0f1f003`; uncommitted implementation changes existed, so the executable hash is essential provenance.

## Surface and memory safety

All three game reports end with **zero live input-sample leases**, no output loss, a successful drain and no surface reuse wait. That is consistent with healthy ownership retirement. It does not stress delayed consumption: the separate tracked-sample/delayed-release pixel-validation tests provide that coverage. Strong references and per-surface lease counts prevent conversion from overwriting a still-owned surface; multiple submissions retain multiple leases. Stall, cancellation, bounded watchdog, repeated-cycle and worker-crash tests passed.

Encoded output is incremental; live per-frame traces are disabled, diagnostic samples stop at 4,096, and pending submission metadata is capped. This is evidence of bounded application storage, not proof that every driver allocation is leak-free. **No RSS/private-memory time series exists in the CS2 run. Memory growth cannot be confirmed or excluded from that run.** A longer instrumented session and real device-loss tests remain pending approval.

Machine-readable audit: `test-output/codex-native-v1-review/audit.json`. Corrected contact sheets are alongside it. The read-only reproduction script is `scripts/bench/audit-native-session.py`.

## V1 development completed after the audit

An opt-in terminal harness now joins the existing native path to a separate stream-copy muxer. It is not wired into the Tauri UI or selected by production defaults.

- Bounded loopback video transport, with timeout and local-address restriction. File mode remains the default and refuses existing output files.
- Exact first-capture QPC origin and full-run encoder timestamp-step validation. Frame selection/conversion/quality settings and Claude's process-snapshot fix remain intact.
- A separately supervised audio bridge reuses the current WASAPI producer, crackling fix, drift correction and device selection. It exposes each PCM stream's actual clock origin. Independent sources are trimmed/delayed in integer samples against the video origin, then encoded once to independent AAC tracks. Omitted sources mean silence/no device capture; microphone capture requires an explicit source configuration.
- One persistent FFmpeg muxer copies H.264 into two-second closed MKV segments. It assigns explicit CFR packet timestamps. Existing production capture/encoding is untouched.
- A bounded replay ring pins completed files before concurrent saves. Up to two save workers/four outstanding requests stream-copy into MP4, with `saving`/`ready`/`failed` states and refusal to overwrite. The live segment is never selected. Recording pins survive ring pruning. Normal completion also creates a full MP4 recording.
- Generated tests found and fixed a real join bug: audio-inclusive Matroska durations caused gaps between concatenated video segments. Save lists now use the segment-list video interval minus the initial video/priming offset. Tests check every joined video interval and every decoded selected frame.
- Closed-segment recovery indices survive stop/crash. Windows kill-on-close jobs, stop deadlines, audio health, disk-space checks and segment-progress checks stop the session on failure. Offline recovery rejects a live controller and copies preserved completed segments to a new MP4 after stop/crash.
- Failure is explicit `fallback_required`, with `healthy=false` and whether all workers stopped. **Automatic handoff to the existing recorder is not implemented.** The existing recorder remains the manual fallback. Completed files are preserved; a restart must not be represented as gap-free continuity.

## Actual tests and remaining gates

- Core Rust: **191 passed, 1 existing ignored**.
- Native Rust: **8 passed**; audio bridge compiles (real device capture not invoked).
- Full Python suite with synthetic native tests: **29 passed**, including six new V1 tests.
- V1 generated integration: independent AAC tracks, video stream-copy/source-identity preservation, MP4 decode, continuous 60 FPS packet spacing across joins, concurrent replay saves, retention, stop, forced controller crash, no surviving descendants, live-recovery refusal and post-crash recovery.
- Generated audio markers at 1, 3 and 5 seconds: each decoded onset within **5 ms** of the expected video clock, and independent tracks within 5 ms of each other, across segment joins. This is a synthetic mux/timeline check, not physical microphone/system sync verification.
- Seven selector patterns match FFmpeg; five native encode/decode patterns retain correct source slots and GOP.
- Formatting, native/core Clippy with warnings denied and optimized native/audio builds pass.

Remaining gates before this becomes a fully functional application recorder: physical system/microphone capture and startup/end synchronization; device changes/unplug and recovery; real motion-rich gameplay capture; editor playback of independent tracks; long-session memory/drift/storage tests; automatic existing-engine fallback and UI integration; performance measurement with audio, muxing and replay saving enabled. The measured CS2 gains belong to the video-only candidate and **are not yet verified for this V1 harness**. Only generated terminal tests were run after the audit.

Fresh V1 build fingerprints are recorded in `test-output/codex-native-v1-build-manifest.json`:

- `C:\Users\Prithvi\Desktop\VS CODE\replayx-perf\src-tauri\target\release\replayx-native-proto.exe` ? SHA-256 `de371f9ec12b5ec2d41f5054ea10b8fa29adde2119b5f6a5c12c86546c7bb559`
- `C:\Users\Prithvi\Desktop\VS CODE\replayx-perf\src-tauri\target\release\replayx-native-audio.exe` ? SHA-256 `8bf33cc17ab318fe317b6910bda4824e0f5fe3b5828fb756dd6e4b807a3d73a8`
