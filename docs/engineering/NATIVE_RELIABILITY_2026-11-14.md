# Native encoder reliability validation ? 2026-11-14

Scope: `replayx-perf`, branch `performance/capture-replay-stutter-optimization`, based on `0f1f003`. Existing uncommitted audit/benchmark changes were retained. This report supersedes the unresolved native ownership/wait/output findings in the earlier `CODEX_NATIVE_AUDIT_2026-11-14.md`; that report remains historical evidence. No commits, main-worktree edits, installation, publication, Windows/GPU configuration changes or interactive capture were performed.

## Correctness changes

- Each submitted NV12 surface is owned by an MF tracked sample and a callback lease holding a strong texture reference. Per-surface lease counts include repeated submissions. Conversion cannot overwrite a ring entry until all consumers release it. The unsafe sample-cache experiment was removed. Submission failure releases its lease through RAII.
- Input credits, reuse, outstanding-output pressure and drain use absolute five-second deadlines, nonblocking event polling and cancellation checks. Deadlines are checked even when events keep arriving. Failed Media Foundation events propagate as errors. Pending submission metadata is capped at 64.
- A parent process owns the encoder worker through a kill-on-close Windows job. It polls rather than waiting indefinitely; a synchronous stuck driver call is bounded by an operation deadline. Stop-file cancellation or lost game focus grants at most five seconds before forced termination (plus a bounded five-second termination observation). Job attachment failure also has a bounded termination wait. OS-level unkillable driver failures cannot be proven impossible by these tests.
- Capture-frame close, MFT shutdown, WinRT uninitialization and Media Foundation shutdown have RAII guards. Partial output is flushed on recoverable errors. Cancellation deliberately reports failure and retains the already-written decodable prefix; it does not claim every in-flight frame was saved.
- Encoded access units go directly to a 64 KiB buffered file writer, flushed at least every 250 ms while output arrives and at normal completion. Normal completion synchronizes file data. `create_new` refuses to overwrite an existing recording. There is no whole-recording compressed-video vector. Diagnostic vectors stop at 4,096 entries; full-run counters and timestamp monotonicity remain scalar. Live recording does not retain per-frame source traces.

Frame-selection code and the common GPU conversion implementation are unchanged. Production engine/defaults are unchanged. Prototype settings remain 1920?1080, 60 FPS, mean 15 Mb/s, max 23 Mb/s, peak-constrained VBR, GOP 120, no B-frames and low-latency mode off. All six codec settings must be accepted or startup fails. The synthetic delay and stall switches default off.

## Actual terminal validation

| Check | Result |
| --- | --- |
| Core Rust regression tests | 191 passed, 0 failed, 1 existing ignored |
| Native Rust tests | 7 passed, 0 failed |
| Python discovery with `REPLAYX_NATIVE_TEST=1` | 23 passed, including 5 opt-in hardware/worker tests |
| Selector versus FFmpeg | 7 patterns passed, identical slots and counters |
| Native synthetic encode/decode patterns | 5 passed, zero wrong source slots, expected keyframes |
| Final native Clippy, warnings denied | PASS |
| Rust formatting and diff whitespace | PASS |
| Release native build | PASS, optimized executable freshly rebuilt |
| Full benchmark workflow, virtual clock/processes | PASS: 13 phases, 3 pairs, all processes exited, DONE at scheduled 11:43 |
| CS2 / fullscreen / desktop recording | NOT RUN in this task |

The hardware tests use generated GPU textures only, never desktop pixels or audio. Coverage includes a real tracked sample retained by a delayed consumer; injected 50 ms allocator-release delays that force reuse waits; pixel validation of every decoded selected frame; encoder-event withholding and timeout; a deliberately blocked worker terminated by the watchdog; cancellation after incremental bytes arrive, followed by full decoding of the partial file; and three complete real Quick Sync encoder start/stop cycles with no surviving worker processes.

A paced 240-frame generated workload used Intel Iris Xe / Intel Quick Sync H.264:

- 240 input and output frames; 60.0029 frames/s over 3.99981 seconds, increasing output timestamps.
- Keyframes at 0 and 120. All six required settings accepted. Separate native pattern tests fully decoded H.264 and checked source identity.
- Process CPU: 3.05990 ms/frame; submission wall time mean 2.75164 ms, P99 5.36790 ms; encode latency mean 31.73173 ms, P99 37.20350 ms.
- 785,280 bytes / 1.57056 Mb/s for this simple generated content. VBR target settings are unchanged; low-complexity content does not demonstrate gameplay bitrate or quality.
- This is an encoder/converter feasibility measurement, not a gameplay CPU/FPS comparison. The last rebuild only additionally bounded the job-attachment failure path; the five hardware/worker regression cases were rerun against that final executable.

Claude's process-snapshot fix is preserved: game identity is inspected once at startup; the hot path uses foreground checks at most four times per second. A 32-call diagnostic measured snapshot checks at 8.30078 ms CPU / 7.90092 ms wall per call, versus foreground checks at 0.000331 ms wall (CPU below timer resolution). This supports retaining the fix, but does not substitute for an updated gameplay measurement.

Evidence lives under `test-output/codex-native-review/`: `reliability-lib-final.log`, `reliability-rust.log`, `reliability-python-final.log`, `reliability-selector-final.log`, `reliability-proto-final.log`, `reliability-clippy-final.log`, `reliability-release.log`, `reliability-paced-final.json`, `reliability-paced-final.h264`, `reliability-guard-final.json`, and `reliability-plan.txt`.

## Fresh executable

- Path: `C:\Users\Prithvi\Desktop\VS CODE\replayx-perf\src-tauri\target\release\replayx-native-proto.exe`
- Size: 658,432 bytes
- Build time: 2026-11-15 01:40:47 UTC (2026-11-14 evening local)
- SHA-256: `18904e3f0e62235dc8ec779e312218dc5f25156effedcd9f8f89cdb4535ed8e3`

## Next CS2 comparison ? prepared, not launched

Shortest useful exploratory plan: three adjacent current/native pairs, reversing the middle pair's order, with recorder-OFF phases on both sides of every recording phase. Each phase has 10 seconds to settle and 30 seconds measured. Order: OFF/current/OFF/native/OFF/native/OFF/current/OFF/current/OFF/native/OFF.

**Scheduled total: 703 seconds = 11 minutes 43 seconds.** This includes countdown 15 s, telemetry setup 5 s, warm-up 60 s, settling 130 s, measurements 390 s, cleanup reservations 93 s and analysis reservation 10 s. Use the 60-second warm-up only once CS2 is already in sustained warmed operation. A 150-second warm-up makes the schedule 13:13. Startup, cleanup or analysis overruns fail the run and preserve partial results; Windows scheduling cannot promise an exact wall-clock finish under failures.

Capture-free plan command:

```powershell
python scripts/bench/fortnite-session.py --game cs2 --seconds 30 --warmup 60 --cap 0 --configs wgc-auto,native-record,native-record,wgc-auto,wgc-auto,native-record --plan-only
```

A live run requires explicit owner approval, the game already running in a repeatable scene, a verified existing current-recorder build, and an existing PresentMon executable supplied with `--presentmon`. Omit `--plan-only` only after approval. The script never launches/focuses the game or changes its settings. Both paths use the same 1080p60 silent recording setup. Quit the normal recorder first. No microphone or test sounds are used.

The runner prints the expected schedule and finish time, phase N/13 and remaining phases, countdowns, DONE or STOPPED, and average game FPS, phase 1% lows/P99, per-process CPU, GPU engines and recorder counters. Artifacts include executable hashes, telemetry, per-phase recording reports, neighboring-baseline FPS costs and a three-pair exploratory 95% t interval. Missing telemetry stays unavailable. Three pairs provide a small exploratory estimate, not a definitive significance claim; order effects, scene variation and thermal drift can still bias it. Independently decode the resulting videos and inspect their actual motion before claiming smoother gameplay capture.

## Readiness and limits

Ready for the next explicitly approved CS2 comparison as an experimental video prototype. The ownership/deadline/output correctness prerequisites now pass terminal tests. Gameplay impact, motion quality after these fixes, actual GPU/device-loss recovery and real driver stalls remain unverified. The stall tests simulate withheld events and a blocked process; they do not force a GPU fault. No long-duration memory soak was performed. Bounded application storage is established by implementation and regression tests, not an hours-long hardware run. Native audio, replay/mux integration and production replacement remain outside this prototype milestone.
