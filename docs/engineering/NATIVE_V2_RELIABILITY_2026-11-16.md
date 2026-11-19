# Native V2 frame-pacing reliability continuation ? 2026-11-16

**Verdict: measurable improvement and safer failure handling; V2 smoothness is not fully qualified. No default promotion or gameplay FPS claim.**

Work stayed in `replayx-perf`, branch `performance/capture-replay-stutter-optimization`, HEAD `0f1f003921c37480a6f182e9a6e61ec92c8c7d96`. The pre-existing work remains uncommitted. An initial source/binary snapshot is retained under `test-output/native-reliability-v2-20261116/before`; `initial-state.json` records its hashes. No main checkout edits, merges, commits, pushes, installs, releases, Windows/GPU setting changes or game launches were performed.

This continues [the previous validation](NATIVE_V2_VALIDATION_2026-11-16.md). Primary evidence lives in `test-output/native-reliability-v2-20261116/`; real captures and independent MP4 audits live in the named `test-output/native-smoothness/` cases. Failed cases were retained, not replaced by successful trials.

## Proven causes

### Original three busy-desktop losses

The historical `resume16-phase5994-long` trace was replayed with its recorded hints, calibration and reconstructed idle-watermark calls. All **1,337 selected slots** matched, including lost source pictures **483, 713 and 1333**. Changing only the post-start hint source to the diagnostic source-ID changes restored all three, with two remaining repeats. This is an offline counterfactual, not a production identity detector: the old trace did not record dirty rectangles.

A new controlled source at 30/59.94/60 FPS plus an unrelated 240 FPS window proved the hint contamination directly. Global dirty activity repeatedly declared that the main source changed when only the other window changed. The old global policy had **3,970 false positives** in the background comparison and **3,432** in the foreground comparison. The spatial policy had **zero false positives and zero false negatives** in those measured main-source regions.

Matched replay comparisons (each 1,200 output frames, 59.94 FPS main source):

| Controlled case | Global hint | Spatial hint |
| --- | ---: | ---: |
| Background source: available IDs lost | 10 | 2 |
| Background source: repeated source IDs | 11 | 5 |
| Foreground source: available IDs lost | 26 | 0 |
| Foreground source: repeated source IDs | 28 | 1 |

The foreground spatial replay's one repeat was required by 59.94 FPS input on a 60 FPS timeline. The background case remained a raw motion failure: IDs 508 and 828 lived only about 4.17 ms after irregular delivery. These sequential hardware comparisons establish the targeted improvement, not a controlled gameplay FPS benefit.

### Compressed capture/source bursts

The old isolated gap trace also replayed exactly (**517 slots**). Even perfect post-start source hints still lost ID 484. Its captured timeline was:

`482 --41.664 ms--> 483 --4.168 ms--> 484 --4.166 ms--> 485`

CFR slots selected `482, 482, 483, 485`. This is distinct from dirty-hint contamination. A later foreground 60 FPS test lost ID 467 after a **30.7036 ms source submission interval**; that picture was captured for **4.1661 ms**. Another run missed source ID 1282 upstream; DXGI presentation statistics confirmed it was displayed, but no WGC content probe contained it.

A 60 FPS timestamp-preserving sampler cannot guarantee displaying every picture of arbitrary compressed sub-frame bursts while also retaining their original times, bounded phase lead and fixed output cadence. Increasing a fixed offset, suppressing duplicates or relabeling these pictures as never captured would conceal the limitation. This trace is a counterexample to the implemented sampler, not a proof that every possible bounded adaptive CFR scheduler must fail. A history/retiming policy might recover some bursts, but has not been implemented or qualified; its temporal and latency tradeoffs would have to be explicit. The explicit capture-VFR experiment remains an alternative for preserving delivered updates and their timestamps; it was not made a default and does not promise 60 unique pictures per second.

### Severe encoder starvation under contention

A quiet instrumented test passed **840 consecutive recording pictures / 720 consecutive replay pictures**, zero repeats or lost source updates. A predeclared repeat with safe Rust test contention reproduced long freezes. Stage wall times proved:

| Stage | Quiet maximum | Contended maximum |
| --- | ---: | ---: |
| Wait for encoder input credit | 0.5762 ms | **2,212.8402 ms** |
| MFT ProcessInput call | 0.4516 ms | 2.8901 ms |
| MFT ProcessOutput call | 0.0257 ms | 0.0626 ms |
| Encoded transport write | 0.5529 ms | 1.1723 ms |
| Buffered flush | 0.1979 ms | 0.4756 ms |

Capture and encoder submission currently share one thread. Waiting for credits therefore stops draining WGC; that contended run had a **3,812.5262 ms capture timestamp gap**, and its 720-frame recording contained **486 repeated pictures**. This was not a transport-write bottleneck or a pixel-order/texture conversion failure. It remains a performance limitation under severe contention; no gameplay test was used to infer its frequency in games.

## Implemented corrections

- `native/cadence.rs`: bounded metadata-only spatial cadence tracking (24 regions, 32 observations each). It recognizes split dirty rectangles using union coverage, avoids fast unrelated/merged regions, favors a larger qualified 30/60 Hz region, and reseeds only the phase observation history. Captures still reach selection; there is no early-drop filter or production pixel readback.
- Irregular burst observations remain valid captured content but are quarantined from phase retraining until regular timing resumes. No picture is fabricated, reordered or assigned a made-up source timestamp.
- `native/grid.rs`: idle watermark calls respect the same existing 1.25-period late-recovery window as arriving unchanged frames. Only the next wake changes, so there is no zero-time spin. Forced stop still seals promptly at the nominal deadline. Output PTS and the shared audio origin do not move.
- `native/phase.rs`: bounded reseeding/saved-state support lets diagnostics replay the exact startup state and source switches. Audio/video origin is rebased only before recording begins.
- `native_proto.rs`: opt-in traces now include raw dirty rectangles, actual phase hints, startup state, reseeds, watermark calls, encoder input-credit waits, driver input/output boundaries, transport writes and flushes. Diagnostics remain opt-in; normal capture constructs no stripe readback probes.
- **Fail explicitly on serious input starvation:** only the opt-in live phase-locked path now uses a **500 ms input-credit wait budget**. That already spans 30 output slots. Offline/generated encoding retains its five-second allowance, and normal Intel legacy/clocked and FFmpeg defaults are unchanged. This is failure containment, not an FPS optimization or a promise that all driver calls are preemptible.
- The actual app contention retest reached `Error`, `recording=false`, `replay=false`, `saving=0`, and recovered a **1,722,764-byte MP4** that fully decoded. It did not continue reporting healthy frozen recording. Existing completed-segment recovery and surface ownership were retained.
- `engine_native.rs` gives starvation a plain-language message and retains the complete diagnostic log. The short-lived CLI test saves failure status/samples and waits for the existing recovery save before exiting.
- Motion fixtures now support a deliberately foreground ordinary window, a separately updating noise window, fractional source rates, phase offsets and an explicit bounded source pause. They never launch a game or enter exclusive fullscreen.

No quality, bitrate, GOP, production backend preference, audio DSP/separation or hotkey defaults changed. Spatial selection is an experimental heuristic, not knowledge of which region the user cares about. Moving/resizing windows, stale/ambiguous dirty metadata, older Windows versions without the API and several competing motion regions remain qualification risks.

## Gap and motion outcomes

A deliberate **50 ms source pause** produced 600 recording / 480 replay frames with three repeated slots, no available IDs lost, no backwards frames, correct MP4 cadence and GOP. Its generic strict zero-repeat gate correctly remained false; the explicit pause explains the repetitions. It was not rewritten as a smooth constant-motion pass.

Additional retained hardware outcomes:

| Case | Recording / replay frames | Available IDs lost | Notes |
| --- | --- | --- | --- |
| reliability-busy-foreground | 1320 / 1200 | 0 / 0 | 59.94 FPS, one expected repeat each |
| reliability-busy-foreground60 | 1320 / 1200 | 0 / 0 | Recording missed upstream ID1282; replay had 1200 consecutive IDs |
| reliability-busy-foreground30 | 720 / 600 | 0 / 0 | Every interior source picture held twice |
| reliability-foreground60-repeat | 1320 / 1200 | 1 / 1 | Compressed source picture467 retained as a failure |
| reliability-final60 | 480 / 360 | 1 / 1 | Concurrent heavy tests; 77 / 5 repeats, 2.2s input/capture interruption |
| reliability-stage-quiet | 840 / 720 | 0 / 0 | All pictures consecutive from frame zero |
| reliability-stage-contention | 720 / 600 | 0 / 0 | 486 / 367 repeats and many upstream omissions; **FAIL** |
| reliability-guard-contention | Interrupted | Error/recovery | Explicit 500ms input-starvation failure; recovered clip decoded |

Source-ID probes, selected capture IDs, converted IDs and decoded MP4 IDs agreed in completed motion audits. The last contended run demonstrates why zero available-ID loss alone is insufficient: hundreds of source updates never reached the selector. Both raw motion and upstream omissions remain in the reports.

## Timing, quality and resource evidence

Completed audited CFR MP4s have monotonically increasing timestamps on the 60 FPS grid, at most **0.333334 ms** rounding error, keyframes at frames 0,120,240,..., no B frames, and no source/converted/decoded ID mismatch. Static scenes and 30/59.94 FPS input legitimately repeat. All normal test configurations retain H.264 1080p60, 15 Mbps target / 23 Mbps maximum and GOP120.

A quiet trace-disabled global/spatial/global bracket used 40 / 60 / 40 seconds, with no competing builds or heavy tests:

| Measurement | Global before | Spatial | Global after |
| --- | ---: | ---: | ---: |
| Persistent recorder CPU, whole machine | 0.377066% | 0.384443% | 0.359743% |
| Device-wide GPU activity | 17.959% | 18.381% | 19.151% |
| Device-wide encoder activity | 63.822% | 63.327% | 63.863% |
| Persistent summed RSS peak | 116.996 MiB | 117.906 MiB | 116.273 MiB |

Spatial CPU differed by **+0.016039 percentage points** from the neighboring mean (0.368404%). This short bracket measures the metadata correction before the later failure guard; it is not a statistically proven speedup. GPU counters include desktop/source activity and are not the recorder's isolated GPU cost. The spatial worker's RSS rose from a first-five-sample mean of 69.032 MiB to 69.541 MiB, peak69.566 MiB. Replay retention reached58s /18,483,569 bytes; no persistent capture process or replay segment remained after shutdown. Summed RSS can count shared pages twice; this is not a long-session leak proof.

Physical unattended phase-locked capture opened the actual system/microphone devices, played no sound, saved three-track MP4s and received792 microphone packets without reported discontinuities. The system was silent and returned zero loopback packets: **audible loopback and acoustic/speech synchronization are not newly verified on this pacing path**. The earlier approved quiet-tone test belongs to the prior clocked path. Generated independent PCM markers and segment joins passed separately; they do not replace physical speech/drift/editor tests.

## Automated validation

Final-source checks:

- Rust: **205 library +50 native +6 opt-in app integration =261 passed**. One manifest polling microbenchmark remains intentionally ignored. The six opt-in checks cover app recording/library/concurrent replay, runtime failure recovery, FFmpeg fallback, and NVIDIA clocked/VFR/phase-locked independent PCM/audio markers across segments.
- Python: **67 run,61 passed,6 skipped** (Radeon hardware absent). Real generated Intel/NVIDIA encoder checks include delayed surface release, stalls, live input-starvation cutoff, cancellation, repeated cycles and independent full decode.
- Frontend: **71 passed**; TypeScript and Vite production build clean.
- Strict Clippy across all desktop targets, `cargo fmt --check`, optimized release binaries: passed.
- Real generated GPU schedule checks on NVIDIA and Intel:11 timing cases per vendor (60/80/120/144/240/250, jitter, gap, under-rate,30/static), independently decoded with40ms delayed release, correct GOP/PTS and zero leaked input leases.
- Generated GPU replay of the two retained source traces passed on both encoders with40ms delayed release. The old burst still lost its source picture in CFR; correct encoder output is not labeled a smoothness pass.
- Independent Python/Rust model crosscheck passed on saved and modeled timelines. Exact replay also matched every selected slot in the original busy/burst traces and the instrumented live comparisons.
- Full stable rational60 FPS phase sweep passed **184 phase regimes/equality-boundary cases**, **64,422 output picks**, at modeled120/144/240/250 Hz. Every capture tick and timing-regime boundary is included. The one-second start/end margin is explicit. This is not physical validation of those refresh modes or arbitrary jitter patterns.
- Normal FFmpeg WGC/NVENC recording saved two concurrent replay clips and fully decoded at1080p60. It is a regression smoke check, not a new FFmpeg smoothness/performance certification.

First invocation errors were retained: an app test run omitted the explicit release-sidecar path and correctly fell back; its configured rerun passed. One build attempted to replace the executable while an owned GPU regression still used it; it was allowed to finish normally, then the build passed. A new Clippy argument-count warning and a JSON macro recursion limit were corrected and all final checks rerun. No security restriction was bypassed.

## Available hardware and remaining blockers

Only the NVIDIA-driven **2560x1600 /240Hz built-in panel** was active. The user approved continuing with available hardware, not changing topology. Intel Iris Xe passed generated encoding on actual silicon but had no active display. No external monitor or simultaneous two-display setup was available; no Radeon hardware was available. Physical Intel/Arc/Radeon capture, external/mixed-refresh and both-adapter routing remain unverified.

Release/default-promotion blockers remain: burst-limited CFR motion, encoder-credit starvation under severe contention (now explicit error), source-region ambiguity, physical audio/speech/long drift and editor qualification, current dual-display/Intel-display/Radeon validation, broader hotplug/sleep/storage-full/device-loss tests and long RAM/VRAM measurement. Driver calls, surface waits and transport have their existing separate watchdog/bounds; the new500ms rule specifically covers live input-credit polling, not every possible OS/driver hang.

## CS2 comparison preparation ? not executed

`python scripts/bench/cs2_ab.py` prints a plan only. It never launches CS2. Execution requires a new explicit gameplay approval flag, an already-running foreground game, existing PresentMon, and a hash-current readiness artifact whose `pacing_milestone_passed` is true. The current artifact is deliberately **false**.

- Three target pairs, each `OFF ? FFmpeg ? OFF ? Native ? OFF`; engine order alternates between pairs. Each measured phase is30s with10s settling. Matching configs explicitly use1080p60 H.264 NVENC,15Mbps,exact scaling and system audio; microphone is not opened. Native worker command lines must prove phase-lock/spatial mode; fallback/recovery invalidates the pair.
- Expected capture schedule **14:09**, including60s initial warm-up. Three bounded offline decode/audit reservations add6:00: **20:09 scheduled total**, excluding manual quality review. Six-attempt hard bound including failure cleanup is **42:48**. The console prints phases, remaining work, deadlines and DONE/INCOMPLETE/FAILED.
- An atomic hash-pinned ledger resumes completed eligible pairs without restarting the game or selecting the best FPS samples. Failed/interrupted attempts remain visible. Missing A7 or any other OFF phase is never invented.
- OFF FPS spread>5%, OFF GPU temperature spread>5C, CPU/GPU clock drift>10%, focus/process failure, cap-limited frames, PresentMon application/PID/swapchain/interval mismatch, missing telemetry, backend mismatch, fallback, memory growth or decode/timestamp/GOP/audio-duration failure prevents eligibility.
- Metrics include average FPS,1% lows,P99, recorder/game CPU, per-physical-engine GPU activity, device clocks/temperature and RSS. Process discovery is cached per phase rather than enumerating every measured second; NVIDIA samples use driver timestamps rather than buffered stdout arrival time.
- Full video/audio decode and frame hashes are automated. Natural-gameplay frame hashes alone cannot prove no missed motion, and track metadata cannot prove acoustic A/V sync. Exact clip hashes must receive explicit motion/picture-quality/audio/sync review before the pair supports a performance claim. Three accepted pairs produce only an exploratory interval.
- Windows kill-on-close jobs and a start gate contain the harness and descendants on timeout/cancel. Eleven pure benchmark tests plus the existing virtual full-session schedule tests passed; no real game comparison was run.

For opt-in application use, launch from this worktree:

```powershell
npm run desktop:native -- --phase-lock
```

It uses the isolated Native Alpha profile. In Video, select **REPLAYX Native Engine (Experimental)** and Save. FFmpeg remains the saved default in a fresh profile. The existing GUI was not interactively retested during this continuation; backend integration used the same Engine as Tauri.


## Final rebuilt-candidate check

After the input guard and plain-language failure message were finalized, all builds/heavy tests completed before these fresh captures:

- **12-second 60 FPS moving source with unrelated240 FPS window:** recording720 consecutive pictures (IDs67?786), replay600 (IDs67?666), **zero repeats, available losses, upstream missing IDs, backwards pictures or conversion/decoder mismatches**. Playable recording ready in1548ms; replay737ms.
- **60-second trace-disabled resource/cleanup check:** persistent recorder CPU **0.416527% of the machine**; worker peak **69.508 MiB**, summed persistent peak **117.148 MiB**. Replay buffer max58s /18,772,824bytes. No capture process or replay segment remained after shutdown. This is bounded short-run evidence, not a long-session leak qualification.
- Final device-wide GPU activity **18.558%**, encoder activity **63.965%**. Device-wide VRAM ranged2893.105?2991.809MiB; this includes desktop/source/other allocations and is not isolated recorder VRAM.
- Native encoder submit mean **0.2025ms**, P99 **0.6918ms**, max **1.6211ms**. Submit-to-output latency mean **16.6489ms**, P99 **24.6552ms**, max **29.8274ms**, over3630 outputs. Active live input-wait budget reported500ms.
- Fresh unattended physical capture: native, no fallback, three tracks in both clips, **780 microphone packets**, zero reported system/mic discontinuities. Silent loopback returned0 packets; physical system sound/speech quality/acoustic sync remain unverified for phase lock.
- All **six final MP4 files fully decoded**: motion720/600, resource recording3600/replay3480, device recording600/replay480. Independent frame timestamps, GOP120 and no-B checks passed. The normal FFmpeg WGC/NVENC check again saved and fully decoded two concurrent clips.

These positive checks do not erase the retained compressed-burst and severe-contention failures. The current readiness artifact still has `pacing_milestone_passed:false`.

Final optimized native executable:

`C:\Users\Prithvi\Desktop\VS CODE\replayx-perf\src-tauri\target\release\replayx-native-proto.exe`

SHA-256: `3493df8ea764c40700a08c79a9730b5ae6e8706a74698bb4d2616598d5eac1d3`.

The GUI and CLI were also rebuilt in the same release directory. `candidate.json` pins exact executable paths, timestamps and hashes plus source hashes; `verification-summary.json`, `candidate-clip-audit.json`, `candidate-device-audit.json`, `all-motion-results.json`, `resource-comparison.json` and `live-guard-audit.json` retain machine-readable evidence.

For future offline quality review, supply a JSON array containing one hash-bound entry per clip, with reviewer and all four explicit booleans:

```json
[{"clip_sha256":"HASH_FROM_CLIP_AUDIT","reviewer":"NAME","motion_review_passed":true,"picture_quality_review_passed":true,"av_sync_review_passed":true,"audio_clean_passed":true}]
```

Use `python scripts/bench/cs2_ab.py --analyze-only --out test-output/SESSION --reviews test-output/SESSION/reviews.json` without running a game. Missing reviews remain pending; an explicit false rejects the pair so `--resume` can obtain a replacement after approval. The last review for a clip hash wins, and earlier entries should remain in that review artifact. Review artifact SHA-256 is included with reviewed comparisons. Baselines or quality failures are never discarded to keep a favorable FPS result.


## Milestone expectations and measured limits

| Milestone | Expected result | Actually measured | Unverified/risk |
| --- | --- | --- | --- |
| Spatial phase hints | Stop unrelated fast UI from clocking the main30/60Hz source | Zero hint false positives/negatives in controlled regions; equal-length foreground replay loss26?0 | Ambiguous/partial dirty regions and gameplay sources still require qualification |
| Idle/gap handling | Keep the promised late window open; regular CFR output after a real gap | Idle/forced-stop regressions pass;50ms source pause retains every available ID with3 explained repeats | Compressed sub-frame bursts still counterexample; no universal CFR claim |
| Encoder starvation | Stop serious live input-credit starvation explicitly instead of spending the5s offline budget frozen |500ms budget exercised; actual appError/stopped flags and recovered full-decode MP4 | No FPS gain; driver calls and other waits have separate existing bounds; gameplay false-positive rate unverified |
| Overhead | Retain approximately0.44% persistent CPU without new GPU copies/readbacks | Final candidate0.4165%; spatial bracket+0.0160pp versus neighboring global mean | Short recorder-only data, not game FPS or long-session RAM/VRAM certification |
| CS2 comparison system | Matched configs, bounded/resumable pairs, honest quality/thermal/focus gates |11 new pure tests plus existing9 virtual schedule/timing tests pass; default plan20:09 | No game launched; current readinessfalse; manual hash-bound motion/picture/audio/sync review remains required |

Final clean motion trace: **3030 dequeued captures / 3065 FrameArrived notifications**, capture timestamp interval median **4.1665 ms**, P99 **4.1680 ms**, maximum **12.4988 ms**. The complete worker trace replayed all **759 selected slots** exactly with zero repeated/lost source IDs; saved clips are the720/600-frame subsets listed above. Encoded order matched selected order and the trace reported **0 surface lease violations**. Capture notifications include redundant desktop refreshes and startup calibration; unique60 FPS source pictures are verified separately.

The final generated clocked, capture-VFR and phase-locked app A/V tests each measured **0.395833 ms** error at all three independent PCM markers across segment joins. This is synthetic shared-clock/track evidence, not physical acoustic synchronization.
