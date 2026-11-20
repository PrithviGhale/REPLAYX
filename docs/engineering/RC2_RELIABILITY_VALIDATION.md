# REPLAYX 0.4.0-rc.2 reliability validation

## Earlier reliability results on the Intel-driven display

| Area | Result | Evidence |
| --- | --- | --- |
| Native MP4 finalization | PASS | The retained segment contained 346.5 ms of audio beyond video. The finalizer now derives the endpoint from actual final-segment video packet timestamps, translates it to the concat clock and discards AAC packets beyond that endpoint. The reproduction ended 22.167 ms before video. All 329 video packet hashes and presentation timestamps matched the old MP4. |
| Normal save, concurrent replay, failure recovery | PASS | Independently decoded the latest Intel-generated recording and two concurrent replay saves (360 frames each). Across the mix and both separate tracks, audio ended 20.833 ms early in the recording and 25.833 ms early in both replays. The injected-worker-failure save and explicit Recover each decoded 327 frames and all tracks, with audio ending 30.5 ms early. This measures end alignment, not acoustic microphone/system sync. |
| Intel Native and fallback | PASS | The same-adapter Intel Quick Sync worker encoded 120/120 generated 1080p60 frames. Its activated H.264 transform reported asynchronous operation and D3D11 awareness. FFmpeg NVENC passed a separate 1080p60 encode on the RTX 4070. The app reported and used FFmpeg on unsupported Native settings and on failed NVIDIA initialization. |
| NVIDIA Native on current laptop topology | FAIL | The exact-adapter and global probes enumerated NVIDIA H.264 Encoder MFT CLSID 60F44560-5A20-4857-BFEF-D29773CB8040 and found the hardware URL attribute. Both failed at ActivateObject with 0x8000FFFF. The documented LUID blob, explicit COM MTA initialization and the archived worker gave the same failure. The NVIDIA D3D11 device could be created, and no D3D11 manager or media type had been sent to the transform when activation failed. Distinguishing a topology-dependent failure from other platform or vendor initialization behavior requires an NVIDIA-driven display session. |
| CPU and memory | PASS in short generated check | About 3.8 steady-state seconds per build on one machine. Original versus fixed capture-process CPU was 1.37% versus 0.64% of the machine and summed peak working set was 300.6 versus 299.3 MiB. Both fixed clips decoded. This single sequential comparison includes test variability and cannot establish a sustained CPU improvement, prove a leak is absent, or establish game FPS. |
| Automated checks | PASS with three opt-in NVIDIA failures | Standard Rust 285 passed, 8 ignored. Opt-in app integration 3 passed and the three NVIDIA modes failed at activation. Frontend 76 passed. Python with generated Intel hardware tests enabled 65 passed and 12 skipped. Strict Clippy, rustfmt, TypeScript, Vite and optimized diagnostic binaries passed. |
| Physical, installation and updater | BLOCKED | No physical audio, CS2, VM startup, installation, uninstall or signed updater test ran. No Radeon hardware was available. |

Native remains experimental and FFmpeg remains the shipping default. This work does not measure immersive motion or the in-game Crosshair cost. The previous controlled desktop Crosshair tests are documented in [rc.2 validation](RC2_VALIDATION.md).

The archived observations are in `test-output` under `rc2-mft-10de-adapter.json`, `rc2-mft-10de-global.json`, `rc2-av-tail-fixture`, `rc2-followup-native-integration.log` and `rc2-offline-resource-comparison.json`. The adapter attribute follows the [Microsoft MFTEnum2 example](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mftenum2). The audio packet filter uses the documented [FFmpeg noise bitstream filter](https://ffmpeg.org/ffmpeg-bitstream-filters.html#noise) with zero noise and a timestamp-based drop expression.

## Isolated installer

Use the `rc2-reliability-*` kits in `test-output/installer-qa` for the refreshed unsigned installer. The earlier rc.2 kits and binary hashes remain available as the baseline. The four new kits passed version, SHA-256 and read-only payload checks for clean installation and upgrades from Alpha.4, Alpha.9 and rc.1. Windows Sandbox is unavailable on this host. Use an existing disposable VM with WebView2 and no shared host profile or recordings. The optimized app and installer are in `test-output/rc2-reliability-package`, with hashes in `checksums.json`.

## Shortest useful physical qualification

Each session below needs separate approval. Keep the installed app, settings and clips on the main Windows profile out of the test.

1. **Physical A/V, about 15 seconds.** Use 1080p60 on the built-in panel with Native selected and confirm the Intel encoder in status. Use isolated clip storage, one system device, one microphone and separate tracks. Use headphones to keep playback out of the mic. In a small test window, produce simultaneous visible flash and system-audio markers near the start and end, with a spoken cue and a visibly timed clap between them. Save a replay before stopping the recording. Decode the video and all three tracks from both MP4s. Check the markers, drift, speech intelligibility, track separation and the final packet timestamps.
2. **CS2, two matched units.** Use the existing local practice match from the same camera position on the built-in screen. Keep the overlay off for both recording engines and preserve the actual game settings. The dry run schedules 4 minutes 3 seconds per unit, including warm-up and transitions. It measures 20 seconds per phase and keeps 15 seconds of PresentMon data. Run OFF, FFmpeg, OFF, Native, OFF. Resume with the engines reversed. The per-unit safety ceiling is 8 minutes 33 seconds including offline audit and focus wait. Verify active adapter, actual encoder, phase lock, clip decoding and save completion. Reject focus loss, thermal drift, a limiting FPS cap, or a fallback. Record average FPS, 1% lows, P99 and process/GPU counters. PresentMon may require one UAC approval for the benchmark process.
3. **Crosshair cost and capture exclusion.** With local practice explicitly approved and overlay use permitted, run two 20-second off/on pairs in opposite order using the same recorder and scene. Confirm that the reticle is visible to the player and absent from both recording and replay. Check FPS, P99, CPU and memory using the same sampling. Record any game that hides or blocks the overlay. Use the existing click-through desktop overlay without game injection.
4. **NVIDIA topology, if available.** With a display actually driven by NVIDIA, run the short same-adapter encoder probe and an isolated recording/replay. Compare the activation HRESULT, verified adapter, motion and saved audio with the Intel-panel result. This check can be run separately from CS2.

Start with the [installer qualification guide](INSTALLER_QUALIFICATION.md) once an existing disposable Windows 11 VM with WebView2 is available and installation is approved. Snapshot the guest and start fresh for each case. Keep networking and host data sharing disabled while testing clean install, prior versions and uninstall. Use a separately approved signed feed for the updater check.


## External RTX follow-up

The two historical CS2 units used the RTX 4070-driven 1920 × 1080, 250 Hz display, the 1,000 FPS cap, and stationary practice footage. Both orders led in favor of Native V2, with matched overhead advantages of 4.63 and 2.49 percentage points. The mean advantage was 3.56 points with an exploratory 95% interval of −10.06 to +17.18 points. There were two matched observations and a roughly 50 FPS difference between the units' OFF averages. The clip audit decoded all eight clips. The human review of motion, picture quality and acoustic timing remains pending.

| Metric | OFF | FFmpeg WGC 2× | Native V2 |
| --- | ---: | ---: | ---: |
| Average FPS | 430.59 | 416.41 | 429.07 |
| 1% lows, FPS | 201.29 | 191.57 | 205.32 |
| Mean phase P99, ms | 4.68 | 4.82 | 4.66 |
| Recorder CPU | — | 1.21% | 0.91% |
| Recorder mean working set, MiB | — | 271.5 | 122.5 |
| Recorder NVENC engine activity | — | 21.85% | 9.54% |
| Save to playable, seconds | — | 2.40–2.45 | 2.07–2.18 |

The Native worker reported NVIDIA H.264 Encoder MFT on the same RTX capture and encoder adapter, LUID 0000000000013833, in both units. Each worker submitted and encoded 2,101 frames, reported one repeated scheduler slot and zero surface reuse waits, and released all input surfaces. P95 encoder latency was 23.46–23.89 ms. The selected clips contained no exact full-resolution adjacent repeats or missing timestamp slots. FFmpeg's first keyframe join had a 21 ms interval. The full FFmpeg decode passed and that join spent roughly 4.33 ms of extra sub-frame time. FFmpeg's final reported video lag was 204 ms in the forward phase and zero in the reverse phase. This is aggregate output lag.

The benchmark FFmpeg graph forces WGC 2×, selects on the 60 FPS grid and hands D3D11 textures to same-adapter NVENC. There is no CPU readback in that recorded path. Fresh settings use Desktop Duplication, Smooth canvas policy and FFmpeg. When WGC is selected, its shipping tuning starts at 1× with an adaptive 2× recovery path. Capture cadence, surface traffic, pixel conversion and encoder rate control all differ from Native or from normal FFmpeg defaults. The traces do not isolate which accounts for how much of the measured advantage. A tooling fix retries discovery once in the pre-measurement settle window so a slow worker completion report can still be retained. Crosshair qualification now requires in-game visibility and clip exclusion as separate manual checks.

### Offline NVENC lookahead check

Using one saved CS2 clip and the RTX D3D11VA-to-NVENC path, two ABBA blocks varied only lookahead depth. Both used 1080p60, p5, HQ tuning, spatial AQ, 15 Mb/s target VBR and zero B frames. Both timed runs used the same 22.5 Mb/s experimental peak value and source. The saved clip input was already compressed. The first run's decoder surface pool was too small for retained lookahead frames. The timed runs used two decoder threads and 64 extra decoder surfaces for both depths.

| Lookahead | Mean wall time, four runs | Effective throughput | Reference SSIM | File size | Offline traced P95 encode stage |
| --- | ---: | ---: | ---: | ---: | ---: |
| 20 | 4.383 s | 218.8 FPS | 0.992319 | 32.45 MB | 111.9 ms |
| 12 | 4.329 s | 221.5 FPS | 0.992178 | 32.56 MB | 71.2 ms |

The audited file from each depth had 959 video packets at 16–17 ms intervals, 120-frame keyframe spacing, two AAC tracks with identical compressed packet payloads, and a clean full decode. Stage latency came from separate, instrumented encodes that include software queueing. This measures offline throughput and pipeline latency. It does not measure gameplay overhead, live WASAPI sync, or WGC freshness effects. The roughly 1.2% throughput gain did not justify changing the 20-frame shipping setting. Live FFmpeg history has shown audio pacing and repeat regressions when lookahead is removed.

### Release status

- **PASS, offline:** Native generated-media Intel and NVIDIA suites with the RTX monitor active. All six Rust app integrations passed, including the three NVIDIA A/V modes that previously failed at activation with the Intel-driven display. The tests cover normal and concurrent replay saves, independent generated audio tracks, forced worker failure and preserved playable output, unsupported-settings FFmpeg fallback and retained preferences.
- **PASS, regressions:** The full desktop-feature Rust retest passed 288 tests with eight opt-ins outside that run. Six of those opt-ins passed separately in the generated-media app suite. Python passed 78 tests with six Radeon tests skipped, and the frontend passed 76. Formatting, strict Clippy, TypeScript, Vite and the optimized diagnostic tool build passed. A missing runtime fixture initially failed the Python virtual-clock test. The first parallel desktop run also failed a replay test that included FFmpeg drain time in its cancellation assertion. Both tests were corrected and rerun.
- **PASS, startup handling:** A pre-handshake Native worker is cancelled without waiting for the normal six-second drain. There is no muxer or replay output to drain at this point. Runtime recovery retains its normal save and finalization path. Runtime NVIDIA activation diagnostics identify the Media Foundation error and adapter when available. The failure with an Intel-driven panel and inactive NVIDIA output remains open.
- **PASS, motion tools:** The offline analyzer measured 59/59 near-identical pairs in the known static window and 0/119 in the motion window. On one historical reverse-order clip per engine it decoded 1,920 Native frames and 1,800 FFmpeg frames. Stationary footage contains visually similar pictures at a 0.15-luma threshold, which does not establish missing source frames.
- **PENDING, physical:** Rapid turns, overlay ON/OFF, physical A/V under motion and additional Intel-panel gameplay qualification. Earlier physical Intel testing used the Voicemod route and observed 17 ms system audio offset on ten markers and 34 ms on one, with no measurable progressive drift. The microphone's absolute offset was not measured.
- **PENDING, hardware and release:** Actual Radeon validation, multiple active displays, installer qualification and updater. Keep FFmpeg as default. A future automatic choice should qualify the actual display and adapter configuration, require a playable native segment, and retain FFmpeg fallback.

Evidence is under test-output in cs2-rc2-external-cap1000-reverse/two-order-offline-comparison.json, rc2-offline-nvenc-lookahead-20261118-212443, and rc2-performance-followup-20261118-213038. The next approved session is documented in the [physical run sheet](RC2_PHYSICAL_RUN_SHEET.md).

## External RTX rapid-motion unit

One complete OFF → FFmpeg WGC 2× → OFF → Native V2 → OFF unit ran on the 1920 × 1080, 250 Hz RTX 4070 display with the 1,000 FPS cap. The original results are in test-output/cs2-rc2-motion-20261118-214654. The runner rejected the FPS unit because the OFF averages were 323.44, 339.97 and 303.47 FPS. The second OFF bracket spread was 11.34%, beyond the 5% limit. The two engines captured different views and actions during the practice session. Neither the FPS difference nor the difference in possible held pictures qualifies as a matched effect.

| Phase | Average FPS | 1% low FPS | P99 ms | Recorder CPU | NVENC activity |
| --- | ---: | ---: | ---: | ---: | ---: |
| OFF, mean of three phases | 322.30 | 154.42 | 5.95 | — | — |
| FFmpeg WGC 2× | 322.22 | 150.77 | 6.22 | 1.24% | 21.31% |
| Native V2 | 323.03 | 151.20 | 6.18 | 1.13% | 9.17% |

All four replay files independently decoded with 1,920 video frames, two AAC tracks, 120-frame keyframe spacing and zero exact adjacent picture repeats. The FFmpeg MKV had one 21 ms interval at its first keyframe join, 4.33 ms longer than an output slot. The rest were 16 or 17 ms. Native MP4 video intervals followed the 60 Hz grid with no frame-sized timestamp gap. The Native worker received 4,345 WGC frames at 123.10/s, encoded 2,087 output slots, reported one repeat and zero late captures or surface-reuse waits. It selected 60 output pictures each second from its faster capture stream. Encoder P95 was 23.86 ms and max was 43.56 ms. FFmpeg reported zero drops/repeats at the output, 0.992× encoding speed and 224 ms final output lag. That lag is aggregate and cannot identify an encoder stall.

The offscreen luma check found two FFmpeg and seven Native episodes of very similar adjacent pictures surrounded by stronger changes. Longer Native episodes occurred around 8.8–10.2 s. Review images show short holds near aim and movement transitions. Neither arbitrary gameplay input nor PresentMon supplies the source picture IDs needed to attribute those holds to the recorder. No change to frame selection was justified by this unit. Concurrent saves became playable in 2.096–2.097 s on FFmpeg and 2.011–2.033 s on Native.

Both engines recorded real CS2 system sound. Native AAC cadence was regular. Its audio timestamps began and ended about 21 ms earlier than the video. FFmpeg audio ended 11.33 ms before video but showed periodic timestamp gaps and overlaps throughout the tracks, up to 28.67 ms of gap. Matching start/end timestamps alone missed this issue. The video is not a calibrated A/V source, so acoustic offset was not established.

The FFmpeg command applied a 1/60 encoder time base to both video and AAC. Generated 48 kHz audio reproduced the 16/17/33/34 ms AAC timestamp intervals. Scoping that option to video restored 21/22 ms AAC intervals in the generated test and left the 16/17 ms video cadence unchanged. The full Rust regression exercises both audio tracks through the app's encoder arguments. Clean settings and the FFmpeg backend, video tuning and capture defaults are unchanged. The benchmark also now audits complete clips when matching rejects the FPS baselines. The original ledger remains as recorded and the companion audit holds the independent results.

The rebuilt CLI's generated-media replay test saved two 1080p60 clips concurrently. Each decoded all 480 pictures and the mix and two independent AAC tracks. Video stayed at 16/17 ms between packets throughout. Every audio track stayed at 21/22 ms across all joins. The final desktop-feature Rust suite passed 289 tests with eight opt-ins outside this run. Offline Python passed 64 tests with 21 opt-ins skipped. Formatting, strict Clippy and the optimized CLI build also passed. The physical timestamp fix still needs validation on a new gameplay clip.

The motion unit remains useful clip-quality evidence. Its FPS comparison failed the baseline check and stays excluded. No additional performance benchmark is required for rc.3 release preparation. Package checks and isolated installation, upgrade, uninstall and updater qualification are the remaining release gates.
