# Architecture

React/TypeScript renders a Tauri 2 desktop window. Rust owns settings, process lifecycle, capture state, disk retention, clip operations, hotkeys, tray actions and update policy. No mock recording backend is used. A browser-only preview disables recording.

```
Desktop Duplication (D3D11) / GDI → FFmpeg H.264 encoder → 2s MKV segments
WASAPI render loopback / mic / PID tree → bounded PCM queue → local FFmpeg inputs
                                    → AAC playback mix + independent tracks
Finalized CSV entries → retention ring → pinned hard-link snapshot → concat/remux
                                                               → ffprobe + full decode → published clip
```

## Capture and encoding

FFmpeg's `ddagrab` implements Desktop Duplication. Its frames are D3D11 textures on **the GPU that drives the display**. On hybrid laptops that is usually the integrated GPU, while NVENC sits on the discrete GPU, so those textures cannot be handed to NVENC directly.

`geometry::plan` turns the validated source rectangle, output height and aspect mode into a `VideoPlan`: the capture rectangle (Crop to fill crops here, at capture time), the encoded frame size, and the scaled image size (smaller than the frame only for letterboxing). `media::scaling_candidates` lists chains in order of cost, and `Engine::select_video_path` runs each with a real 5-frame capture→filter→encode probe that keeps FFmpeg's error text:

| Scaler | Chain | Copies |
| --- | --- | --- |
| `D3d11` | `scale_d3d11` → NVENC/AMF | none (display GPU = encoder GPU) |
| `QsvZeroCopy` | `hwmap` to QSV → `vpp_qsv` → `h264_qsv` | none (Intel display GPU) |
| `QsvHybrid` | `vpp_qsv` → download scaled NV12 → NVENC/AMF | one scaled frame |
| `Cpu` | download BGRA → `swscale` → encoder | full-resolution frame |

Letterboxed plans skip the zero-copy scalers, because padding happens in system memory. The selection is cached per adapter, display, plan and encoder settings. A GPU path that passes its probe but fails before the first validated segment is denied for that configuration, and capture restarts on the next path. The chosen path, encoded size and rejection reasons are part of `Status`. Hardware encoders force IDR frames at the 2 s keyframes, so every segment decodes on its own.

FFmpeg's `ddagrab` paces acquisition with a monotonic clock and stamps each frame at acquisition time (not the DWM present time, as 0.2.0 documentation wrongly said). `media::regrid_filter` rounds each timestamp to its nearest 1/fps slot before constant-frame-rate output. 0.2.0 instead forced every frame to at least one slot after the previous one; on real motion that built up a backlog and caused 7% repeated pictures with freezes of up to 53 frames, the stutter users saw. With rounding, a saved 1080p60 replay of a 60 Hz motion pattern showed 0.36% single-frame slips in steady state (see VALIDATION.md). Windows Graphics Capture (`gfxcapture`) was measured as an alternative and repeated 15.7% of pictures, so it is not used.

The first finalized segment's dimensions must equal the plan's frame, which catches stale geometry or desktop leakage. GDI all-desktop and window-title modes are explicit user choices, scaled on the CPU. A failure never silently switches to capturing a wider desktop region.

Windows Graphics Capture is not implemented. Individual outputs on the default DXGI adapter are selected by index. GDI captures the virtual desktop across all displays. Arbitrary monitor subsets and cross-adapter DXGI enumeration are not implemented.

## Audio

Microphones open at their native channel count; `audio::to_stereo` duplicates mono, passes stereo through, and averages arrays to centred mono. System and per-app loopback request stereo and keep Windows' mask-aware surround downmix.

Dedicated COM MTA threads use wasapi-rs to initialize device and process-loopback clients in shared polling mode with a 100ms device buffer. Each requests 48kHz stereo 32-bit float with `AUTOCONVERTPCM | SRC_DEFAULT_QUALITY`, so Windows converts any endpoint rate, channel count or sample type; packets are decoded explicitly as little-endian f32. Capture reads `IAudioCaptureClient` directly to correctly handle silent packets that may have null buffers.

`audio::Timeline` (pure, unit-tested) turns packets into a continuous stream paced by QPC, the clock of WASAPI packet timestamps:

- Device samples are appended **contiguously**. Packet timestamps never re-place individual packets; 0.1.2 and earlier did, and timestamp jitter inserted/dropped samples at almost every 10ms boundary (the crackling defect, see VALIDATION.md).
- A low-pass-filtered timing error (2s) drives a proportional drift controller (at most +/-0.5%) that steers a 32-tap Kaiser-windowed sinc variable-ratio resampler. At ratio 1 it is bit-exact; correction keeps the source on the capture clock for A/V sync without discontinuities.
- After starvation or idle loopback (no packets while nothing plays) the next packet re-anchors exactly; silence is inserted only where output was already silent, and the interpolator tail is flushed so nothing plays late. Data that arrives after its slot was already output is discarded and counted. Disagreements over 50ms realign once and are counted; flagged, zero or implausible timestamps are ignored and counted.
- Delivery holdback starts at 50ms and grows (to 400ms) when a device delivers packets later, e.g. high-latency Bluetooth/USB microphones. This delays delivery to FFmpeg only; sample-to-clock alignment is unchanged.

Per-source counters (discontinuities, Windows-flagged glitches after the first packet, timestamp errors, applied and independently measured drift ppm) appear in Performance and CLI reports. A separate writer sends PCM over local loopback TCP. The backlog beyond the holdback is bounded to 250ms; a full queue becomes a visible error instead of silently dropping audio. Device errors and process exit/PID reuse stop the pipeline.

FFmpeg creates a normalized playback mix (track 1), then separate AAC source tracks (tracks 2–6). System and app inclusion sources cannot be combined, preventing accidental duplication outside the intended preview mix. App capture uses PID plus validated process start time and includes children; exclusion uses Microsoft's exclude-tree mode. Reattachment after restart is explicit in this alpha.

Each source is now drift-corrected to QPC, but long physical sessions and absolute A/V offset (video capture latency versus audio) have not been measured. Synthetic multitrack tests verify muxing/timestamps; they are not proof of physical-device or multi-device A/V sync. See hardware validation requirements.

## Buffer and recording

Only files listed in FFmpeg's finalized segment CSV enter the live ring. Segments have keyframes at approximately two-second boundaries. Retention is bounded by requested duration and encoded-byte budget. Save takes hard links while holding the ring mutex; copy is a fallback for filesystems without hard links. Saved snapshots survive unlinking of ring entries. Independent saves run outside the capture mutex.

Concat-list durations account for each segment's container start time (`media::concat_durations`). A session's first segment starts at about 21ms because of AAC priming; before 0.1.3 this left a 21ms audio/video timestamp hole at the first seam.

Replay selects complete segments covering the requested interval. It can include up to one extra segment and end up to one segment before the keypress. It does not claim frame-exact boundaries. Full recording pins each finalized segment. Stopping recording finalizes the shared encoder, exports it, and restarts replay if requested; a short capture gap occurs.

Output is first written as `.partial.mkv`. Only after stream inspection, duration checks, and a complete FFmpeg decode does it receive its final name. MP4 export remuxes every stream and verifies the result. Recovery scans retained segments, probes them, and rebuilds verified MKV files. Source data remains if verification fails.

## Watchdog

A background thread checks process exit, encoded-frame progress, finalized segments, audio-source errors, and storage every 200ms. A six-second frame stall, eight-second finalized-segment stall, three-second PCM write stall after startup, missing microphone packets, audio queue overflow, or storage below a 128MiB reserve stops capture and exposes Error. Startup requires 256MiB free on both volumes. Healthy capture requires encoded frames and a probed finalized segment with the expected tracks and canvas dimensions. These are liveness checks, not proof of perceptual audio quality or synchronization.

## Threads and the engine lock

The engine state lives behind one mutex. Rule since 0.2.1-alpha.2: **the window's UI thread never waits for it.** Tauri runs synchronous commands on the UI thread, so `status` and `settings` read published snapshots (the tick republishes every 200 ms; actions publish when they change state). Every other command that touches the engine is async and runs on a worker. Nothing that can send a window message runs while the lock is held: the foreground-window sample is taken between ticks without the lock, and REPLAYX's own windows are skipped, because `GetWindowTextW` on a window of the calling process sends `WM_GETTEXT` to that window's thread. 0.2.1-alpha.1 broke both rules and deadlocked when Record was pressed.

## Starting capture

`Engine::start` takes the lock only to check state and record the intent (`Starting { recording, replay }`), then releases it while `prepare_pipeline` checks the media runtime (once per launch), measures the display, selects the capture path, opens audio devices and starts FFmpeg. It then re-takes the lock to install the encoder, or to discard it if Stop/Cancel cleared the intent meanwhile. A Record while capture already runs attaches to that encoder. The working GPU path is stored in `video-path.json` (keyed by display device, mode, plan and encoder settings, not by the per-boot adapter LUID) and reused without probing; a path that fails before its first valid segment is denied, forgotten and replaced by probing. A watchdog abandons starts that exceed 60 s. User stops delete the replay buffer they leave behind (marker file plus background retries for briefly locked segments); buffers left by crashes stay for recovery.

## Encoder choice, output size and overload (0.2.1-alpha.3)

* **Encoder ladder.** `media::scaling_candidates` lists GPU paths in order; each is verified by a five-frame probe of the real capture → filter → encoder chain before use, and the winner is remembered per display and settings. On an Intel-driven display the order is D3D11 → NVENC/AMF (works when the display and encoder share a GPU), Quick Sync with the fixed-function **VDEnc** encoder (`h264_qsv_lp`, `-low_power 1`), Quick Sync with the shader-assisted encoder, Quick Sync VPP → NVENC/AMF with a scaled-frame transfer, then the CPU. VDEnc comes first because the alternative runs motion estimation on the Intel shader cores, which on hybrid laptops also composite every game frame.
* **Output size.** Settings hold an explicit `width` × `height`. `geometry::plan` maps the captured region into it with *stretch*, *fit* (bars, CPU pad) or *crop* (centre crop at capture, nothing extra copied); *native* keeps the display's proportions at the chosen height. Quick Sync VPP and the CPU scaler convert with BT.709 limited range and the stream is tagged accordingly.
* **Bitrate.** `media::target_bitrate_mbps` scales a preset's 1080p60 bitrate by pixel count and frame rate; VBR with `maxrate` 1.5× and a 2× buffer.
* **Overload.** FFmpeg's progress reports `speed` and `out_time`. The engine computes how far encoded video trails the wall clock since the first frame. FFmpeg stops reading inputs that run ahead of the slowest output, so a lagging video encoder eventually starves the audio socket. At 8 s of lag capture stops with an explicit overload error (the audio queue holds 10 s). A recording cut short by any error is finalized as a clip and cleared, so the next Record starts normally.

## Game-detected notice

The engine samples the foreground process once a second without its lock (also while idle when the notice is enabled). `Status.foreground_game` feeds `overlay::Announcer`, which reports a game only when it newly comes to the front, at most once per game per 10 minutes. `overlay::Overlay` shows a native layered window (GDI, `WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`, click-through, never focused) in the top-right corner of the work area for about 4 s, on its own thread. Text comes from the engine state (`overlay::capture_line`), so it never claims replay is ready when it is not.

## Saving clips

A save hard-links the selected finalized segments into a snapshot (milliseconds), then a worker joins them
with the concat demuxer and stream copy (no re-encode), validates stream layout, duration and decodes the
first and last second, and only then renames the `.partial.mkv` into place and reports `Ready`.
Each finalized segment's container start time is probed once in the background at below-normal priority
(the session's first segment starts after AAC priming and its last can also start late, so starts are never
assumed); saves only probe segments still unknown. The concat script gives every segment minimal probing,
which removed about 30 ms of per-file stream detection. Stopping a recording records the encoder's output time
and waits, without holding the engine lock, for the running encoder to finalize the segment containing that
moment; capture and replay continue. Save commands return at once; progress and the finished clip are sent
as `clip-save` events.

## Game detection and clip organization

`games.rs` samples the foreground window's process once a second while capturing (documented Win32 calls only:
`GetForegroundWindow`, `GetWindowThreadProcessId`, `QueryFullProcessImageNameW`). The executable is classified
by user mappings, a table of known game executables, an explicit list of launchers/helpers that are never the
game, and store install folders (`steamapps\common\<Game>`, `Epic Games\<Game>`, …). Only changes are stored.
A clip's game is attributed over its own footage window: the top game needs 30% of the time and twice the
runner-up's, otherwise `Mixed` (games dominate) or `Desktop`. Clips are saved as
`<Game>\<Game>_YYYY-MM-DD_HH-MM-SS.mkv` (sanitized, collision-safe) with `REPLAYX_GAME`/`REPLAYX_KIND` tags.

`library.rs` defines clip ids as paths relative to the clips folder (one category level), validates them
before any filesystem access, and keeps a persistent metadata index (`library-index.json`) so unchanged clips
are never probed again. The UI receives the list immediately and stale metadata later via `library-updated`.
Deletion goes to the Recycle Bin with a shell-safe path (0.2.0 passed a `\\?\` verbatim path, which the shell
rejects with error 124, so Delete never worked).

## Desktop shell and UI

`desktop.rs` exposes Tauri commands: status, settings, capture actions, mix, capabilities, WASAPI audio sessions, executable icons, clip list/thumbnails/actions, device test, updater and folder opening. `winutil.rs` holds the Windows integrations: session enumeration, icon→PNG, Recycle Bin, autostart. A single-instance plugin focuses the running window on relaunch. Process metrics refresh REPLAYX's own process tree each second and rescan the system every 5 s.

The React UI (`src/`) polls `status` every 250 ms while capturing and every second when idle. `store.ts` exposes selector subscriptions with shallow comparison, so only components whose slice changed re-render. Settings are edited as a draft and saved explicitly. Volume and mute apply live through `set_mix`. Pages sit behind an error boundary. `dev/` contains a mocked-IPC preview and DevTools-protocol tooling for screenshots; it is not part of the app build.

## Official references used

- FFmpeg Desktop Duplication implementation: https://ffmpeg.org/doxygen/trunk/vsrc__ddagrab_8c.html
- FFmpeg capture devices: https://ffmpeg.org/ffmpeg-devices.html
- FFmpeg segment muxer: https://ffmpeg.org/ffmpeg-formats.html#segment
- Microsoft process-loopback sample and OS requirements: https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/
- WASAPI wrapper API: https://docs.rs/wasapi/0.26.0/wasapi/struct.AudioClient.html
- Tauri signed updater: https://v2.tauri.app/plugin/updater/

## Geometry and recovery

The selected default-adapter DXGI output is paired with its current Windows display mode. Capture coordinates are output-local physical pixels, independent of negative virtual-desktop positions. The rectangle is bounded by both dimensions. Rotated outputs and out-of-range crops fail explicitly. Existing settings retain source aspect ratio; Stretch uses a fixed 16:9 canvas and square sample aspect ratio. GPU scaling is probed separately before use; CPU fallback remains explicit. The engine cannot infer game boundaries inside a full-resolution desktop surface.

Display geometry is rechecked approximately every 500ms. A change pauses capture, quarantines the last completed and in-flight segments, and retries the same adapter/display identity at most three times with stable-mode checks. Recording parts are exported separately, and replay begins a new session. Quarantined media is retained under the session quarantine directory and excluded from ordinary recovery. This polling approach still needs physical transition testing, including driver-specific changes not reflected in Windows geometry.

An exclusive data-directory file lock prevents competing app instances. The capture child belongs to a kill-on-close Windows Job Object, limiting orphan encoding after a crash. There remains a brief spawn-to-job-assignment interval. Recovery accepts UUID session directories within the expected parent, excludes reparse-point media, and removes source media only after verified export. A failed snapshot rolls back its newly pinned files. Save validation probes every source segment before concat and fully decodes the result; this has measurable cost for long replays and remains unoptimized until profiling is available.

Display geometry queries now temporarily use per-monitor DPI awareness and restore the calling thread context afterwards. DXGI GetDesc can virtualize its desktop rectangle on DPI-unaware threads; interpreting that rectangle as physical pixels would crop the source on scaled desktops. A regression test checks context restoration, and a real 125% scaled desktop was queried and captured after correction.

## Voice Enhancer (0.3.0-alpha.1)

`src-tauri/src/voice.rs`. Microphone feeds run a `voice::Processor` in the WASAPI capture thread on each paced 10 ms chunk (480 frames, mono-summed, written back to both channels), before the volume stage and the 127.0.0.1 socket to FFmpeg. It never touches the video path. Order: high-pass (4th-order Butterworth, 80 Hz) → RNNoise (`nnnoiseless`, strengths mix a delay-aligned dry signal: 480 samples) → expander held open by RNNoise's voice probability → EQ (RBJ biquads) → compressor (soft knee, auto make-up) → split-band de-esser → recording gain → 1 ms look-ahead limiter at −1 dBFS. Non-finite or absurd input is sanitised before RNNoise. FFmpeg now only limits sources above 100% volume (`media::source_chain`).

The live **Test mic** (`audio::monitor`) runs the same feed and processor and renders to an output device with a ≤60 ms queue; settings and the Enhanced/Original switch reach it through the feed's `Meter` (`voice_update`, `bypass`). Original is level-matched with the processor's running speech-energy ratio. **Compare** records the raw microphone into memory (`AppState::voice_sample`) and re-processes it offline with `voice::process_buffer` on each settings change.

## Startup and first run

`index.html` paints the splash before any script (inline styles only, so the CSP stays strict); `src/splash.ts` shows the real steps (interface, settings, engine check) and removes it as soon as settings and the first status answer arrive, or after 8 s of waiting for the engine. A CSS-only 25 s timeout hides it even if scripts fail. Showing the window from the tray does not reload the page, so the splash is not replayed. The tour (`src/Tour.tsx`, `src/tour-steps.ts`) anchors to `data-tour` attributes on the real components, navigates pages but takes all pointer input itself, and records completion with `mark_tour_done` (saved immediately, never undone by a later settings save).
