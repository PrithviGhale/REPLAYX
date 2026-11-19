> **2026-11-16 follow-up:** [Measured motion improvement, physical audio, 90-second retention and remaining counterexamples](NATIVE_V2_VALIDATION_2026-11-16.md). The opt-in candidate has clean 600/480-frame recording/replay evidence and 0.370% recorder CPU. Mixed-desktop and capture-burst cases remain limitations; defaults are unchanged. Results below are historical.

# Native V2 phase-lock: live validation and bounded recovery (2026-11-15)

**Status: PARTIAL / NOT accepted as the default scheduler.** The final candidate
is rebuilt and passes CPU regressions, but its final GPU/app/audio regressions
are pending. The user is playing: all GPU and on-screen validation is stopped
until the user says they are finished. Do not equate earlier-candidate passes
with verification of the final source tree.

Worktree: `C:\Users\Prithvi\Desktop\VS CODE\replayx-perf`.
Branch: `performance/capture-replay-stutter-optimization`.
Base HEAD: `0f1f003921c37480a6f182e9a6e61ec92c8c7d96`.
All existing uncommitted work and recordings were preserved. No commits, merges,
pushes, installations, releases, GPU/settings changes or game launches were made.
The production engine remains FFmpeg; native NVIDIA remains clocked and Intel
remains legacy unless explicitly opted in. Phase-lock is NOT the default.

## Proven failure and corrections

The old first-capture-anchored output grid sampled inside the compositor's
new-picture arrival cluster. The phase of the first capture determined whether
the same available picture was repeated and its successor then skipped. Encoder
output, conversion probes and independently decoded IDs agreed with the selected
IDs: the failures were already present before encoding, not invented by NVENC.
Regular container PTS alone never certified smooth motion.

Live validation found three additional faults:

1. **The app test cleared the experimental environment switch.** An apparent
   phase-lock test could exercise the baseline instead. The CLI now has explicit
   `native-app-test --desktop-silent --phase-lock`, rejects incompatible test
   switches, and the Python helper checks the actual native completion mode.
2. **Startup did not have enough phase observations.** A first live trial lost
   available ID 170 during convergence. The experimental WGC path now gathers
   the existing estimator's eight change observations before frame zero, bounded
   by 500 ms. It uses metadata only. The learned phase is rebased to the first
   recorded capture; the shared audio/video origin is published afterward.
   Calibration was 115.7-184.0 ms in measured cases, including unchanged captures.
3. **A deadline could close before a delayed picture.** On the retained live
   calibrated trace, ID 452 arrived at slot 386's grid +14.7387 ms, after its
   +11.7838 ms deadline. Later, redundant ID 530 captures arrived at relative
   7744.0696 and 7776.0659 ms; ID 531 arrived at 7780.0659 ms and ID 533 at
   7788.0666 ms. ID 532 never reached WGC. Sealing a repeated slot at the
   redundant capture lost 531 even though it subsequently became available.

The final correction retains an **unsealed** repeated slot through redundant
captures within the existing 1.25-output-period phase bound (20.833 ms at 60 FPS).
It compares a picture's age, rather than the age of its latest redundant capture.
A late new picture can fill that slot; equal-timestamp updates still choose the
latest capture. Near-60 Hz pictures must already have filled one slot; near-30 Hz
pictures must have filled two. Other rates and timestamp-only hints do not use
this recovery. A real gap still produces truthful holds and ordered recovery.
There is no lookback over sealed slots, frame interpolation, duplicate suppression,
new GPU readback, added texture queue, busy wait or extra copy. The existing
single pending capture and surface ownership contract remain in place.

A separate diagnostic bug reported the final deadline of a recovery batch for
all its slots. `Grid::sealed_deadline` now records each slot's own effective
deadline in a bounded batch. This makes the independent capture oracle meaningful.

Changed implementation: `src-tauri/src/bin/native/{grid,phase}.rs`,
`src-tauri/src/bin/{native_proto,cli}.rs`. Validation helpers now include real
Windows dirty-region counters, bounded resource sampling, exact mode checks,
source-loss/hold metrics, independent decoded MP4 checks, and unique output names.
The CPU Python reference was updated alongside the Rust changes.

## Actual on-screen NVIDIA session (earlier candidate)

RTX 4070 Laptop GPU, external DISPLAY1, 1920x1080, existing 250 Hz display mode,
Windows build 26300. Windowed 960x540 frame-ID source on the desktop; the saved
canvas is 1920x1080/60. No game, sound or microphone was used by these tests.
The Engine/CLI exercises the application's actual capture, segment, recording,
replay, save and library paths. Tauri UI interactions were not revalidated.

| Source / mode | Recording | Replay | Verdict |
| --- | --- | --- | --- |
| 60 FPS, unchanged clocked baseline | 360 frames, 8 repeats / 8 available IDs lost | 240 frames, 6 repeats / 6 lost | FAIL motion |
| 60 FPS, phase + startup/initial late recovery | 360 frames, IDs 45-404, no repeats/loss/reordering | 240 frames, IDs 45-284, no repeats/loss/reordering | PASS this candidate/run |
| 59.94 FPS, clocked baseline | 360 frames, 2 repeats / 2 available IDs lost | 360 frames, same losses | FAIL motion |
| 59.94 FPS, phase | 360 frames, IDs 48-407, no repeats/loss/reordering | 360 frames, same IDs, no losses | PASS this short run |
| 30 FPS, phase before final correction | 480 frames, 240 legitimate repeats, no lost IDs; three 3/1 hold pairs after settling | 360 frames, 180 repeats, one 3/1 hold pair | FAIL cadence |
| 30 FPS, clocked baseline | 360 frames, no lost IDs, three 3/1 hold pairs | 360 frames, same issue | FAIL cadence |

These are separate neighboring runs, not a controlled gameplay FPS benchmark.
59.94-to-60 conversion requires an occasional hold over longer intervals; the
short six-second pass does not establish long-duration drift behavior.

All decoded clips above had zero GPU-conversion/content mismatches, zero
selection-to-decoded ID mismatches, ordered PTS, no B-frames and keyframes every
120 frames. Maximum CFR PTS error was 0.333334 ms (container quantization).
The passed 60 FPS recording was playable in 1140 ms, replay in 605 ms. The 59.94
case took 822 / 768 ms. These are individual measured saves, not speed guarantees.

The 30 FPS source submitted frame 125 only 0.6076 ms after its planned time;
WGC reported it 6.3658 ms after submission and **0.0739 ms after the slot deadline**.
The next source picture arrived earlier within its normal interval. The resulting
3/1 holds were genuine uneven selection, not lost content or an encoder issue.
The final bounded-wait correction addresses this measured boundary; a fresh
on-screen retest is still required.

## Actual dirty-region and arrival evidence

ReportOnly dirty regions were enabled and read successfully on the NVIDIA test
system. The 5%-of-canvas area-sum policy is unchanged. Frame IDs were read back
only by explicit diagnostic probes; normal phase-lock uses metadata.

| Phase case | WGC captures (after calibration) | New-picture interval median / P99 / max | Hint reads / errors | Changed-source false negatives / other-desktop positives |
| --- | --- | --- | --- | --- |
| 60 FPS | 626 | 16.0003 / 20.0018 / 23.9994 ms | 637 / 0 | 0 / 22 |
| 59.94 FPS | 602 | 16.0003 / 23.9998 / 24.0013 ms | 613 / 0 | 0 / 17 |
| 30 FPS | 558 | 32.0012 / 36.0053 / 40.0061 ms | 571 / 0 | 0 / 24 |

Read counts include pre-origin calibration; capture counts exclude it. Other
Windows activity can mark a changed region without changing the controlled
source. These hints are not perfect content IDs. Their false positives and the
older-Windows timestamp-only fallback remain compatibility risks. No claims
are made for Windows versions not exercised here.

## Final candidate: CPU replay of exact retained captures

`test-output/native-phase-live/bounded-retained-regressions.json` records final
binary SHA-256, picks, effective deadlines and complete source timelines.
These are CPU scheduler results with perfect diagnostic change hints, **not**
fresh encoded video or proof of real dirty-region behavior after the last fix.

| Retained trace | Old clocked repeats / available losses | Final phase repeats / available losses |
| --- | --- | --- |
| live-phase60-calibrated | 69 / 67 | 1 / 0 |
| live-clocked60 | 8 / 8 | 0 / 0 |
| app-mp4-transport-final | 71 / 72 | 0 / 0 |
| live-phase30 | 255 / 0, two 3/1 hold pairs | 255 / 0, all 255 pictures held exactly twice |

The remaining single hold on the calibrated trace corresponds to missing WGC
source ID 532. IDs 452 and 531 now reach the output. It would be incorrect to
remove that necessary hold or claim that 532 was captured. The different final
slot counts at the trace tail reflect different selection deadlines; these
comparisons count losses within each output's represented source range.

Before the last correction, generated NVIDIA hardware replay of four retained
traces passed all eight encode/decode, GOP, ordering, rational-timestamp and
surface-release checks. The calibrated case still lost 531 then. Those hardware
results are historical, not final-candidate acceptance.

## Resource measurements and limits

The successful trace-enabled 60 FPS app capture measured 0.584% total-machine CPU
(11.68% of one core on 20 logical CPUs), summed persistent RSS 115.38 -> 116.00 MiB,
and device-wide GPU / encoder means 17.57% / 60.90%. The 59.94 run measured
0.541% CPU and 115.33 -> 115.74 MiB. Read-only GPU counters include the desktop,
source window and unrelated activity; they do not isolate recorder overhead.

Two trace-disabled 14-second runs completed with no leftover owned workers or
buffer segments: clocked RSS 116.35 -> 116.53 MiB; phase 116.01 -> 116.12 MiB.
Their measured total-machine CPU was 1.175% / 0.977%. They overlapped Rust
compilation/tests and used a short controller-time sample window. They are
**not a clean steady-state CPU comparison**, do not verify the earlier 0.44%
result, and do not establish an improvement or regression. Do not discard these
measurements or cite the lower trace-enabled figures as trace-disabled proof.
Final-candidate overhead and long-session memory behavior remain unverified.
The helper is now prepared to sample using the worker's actual shared QPC origin,
exclude the first two seconds and final 0.2 seconds, and refuse a CPU verdict
with insufficient samples. Its new clock-aligned path is syntax-checked but has
not been run while the user is gaming.

## Tests completed / still pending

- Final Rust: **204 library + 41 native tests passed**, 0 failed; 7 ignored
  (six opt-in app/GPU checks plus a manifest microbenchmark).
- New regressions cover learned startup rebase, per-slot batch deadlines,
  delayed content, equal timestamps, 30 FPS nominal holds, redundant captures
  before late pictures, genuine gaps and bounded retained state. Existing
  cancellation/stall/surface-lifetime/stop and timestamp tests passed.
- Strict Clippy, formatting: PASS. Optimized offline release build of all bins:
  PASS. The rebuilt native prototype is `src-tauri/target/release/replayx-native-proto.exe`.
- Frontend: **71/71**, TypeScript and Vite production build: PASS.
- Safe Python: **35 passed, 18 skipped, 53 total**. GPU opt-ins explicitly disabled.
- Rust/Python retained-trace and synthetic crosscheck: **PASS, 62 matched
  trace/hint combinations**. `crosscheck-postbuild.log` pins the rebuilt binary's
  SHA-256 before and after execution. The first attempted crosscheck overlapped
  the release rebuild and is excluded (`crosscheck-bounded.log`); the post-build
  rerun is the valid result. A matching reference is consistency evidence, not a
  zero-loss assertion for every possible source/delivery pattern.
- Generated NVIDIA/Intel hardware regressions, final phase-lock PCM A/V marker
  and segment tests, fresh final app recording/replay at 60/59.94/30 FPS,
  and the fresh FFmpeg WGC/NVENC smoke check: **PENDING, user gaming**.
- No physical microphone/system-audio test was performed in this session.
  Earlier sub-millisecond generated marker results are historical; the final
  candidate needs its own shared-clock/segment regression.

The attempted final 30 FPS retest was blocked before opening the source by
`A game is foreground; the windowed source was not opened`. No new recording
was made, no guard was bypassed, and the following FFmpeg test was not started.
The user explicitly requested waiting until they finish playing.

## Compatibility and readiness

- NVIDIA: actual earlier-candidate on-screen recording/replay passes plus
  failures retained. Final candidate hardware acceptance remains pending.
- Intel: topology reports no active Intel outputs. Physical built-in-screen
  capture cannot be tested safely without a display/GPU setting change; none
  was made. Generated Intel regression remains pending for this final build.
- AMD Radeon: no hardware available; explicitly UNVERIFIED. Shared selection
  has no NVIDIA-specific timing constants. No universal hardware claim.
- 120/144/240/250 Hz, drift and irregular-delivery models are not physical
  validation of those display modes. A source delivering fewer distinct frames
  cannot be made genuinely 60 FPS by truthful CFR selection.
- Older Windows/no-hint behavior had mixed prior trace results and is not fixed
  by a recovery mechanism that deliberately requires actual change hints.

**Verdict: retain phase-lock as explicit opt-in development work. Do not promote
it to the native default or mark this smoothness milestone complete.** The final
CPU evidence is promising and specific, but fresh final app/replay, hardware,
A/V and uncontaminated resource checks are required after the user finishes.

## Resume after the user explicitly finishes gaming

Run serially, with no concurrent build/tests during resource measurements:

1. Short final 60/59.94/30 FPS source-ID app recording/replay checks, including
   different starts. Keep all counts, even on failure. Use new output names.
2. Generated NVIDIA and Intel retained-trace/surface/backpressure tests and six
   opt-in app lifecycle/fallback/PCM marker checks. No physical sound/devices.
3. Six-second silent FFmpeg smoke check with a fresh output name.
4. A short trace-disabled resource check with capture-origin-aligned sampling;
   do not launch another long soak or a game without separate approval.

Example (do not run while gaming):

```powershell
python scripts/bench/native-app-motion-check.py --name phase-final-resume60 --phase-lock --sample-resources
python scripts/bench/native-app-motion-check.py --name phase-final-resume5994 --fps-num 60000 --fps-den 1001 --phase-lock --sample-resources
python scripts/bench/native-app-motion-check.py --name phase-final-resume30 --fps-num 30 --phase-lock --sample-resources
```

The GUI developer launcher currently clears the timestamp experiment. Merely
setting the environment flag before `npm run desktop:native` does not validate
phase-lock there. The explicit CLI flag above does; no default or installed
profile was changed to enable it.
