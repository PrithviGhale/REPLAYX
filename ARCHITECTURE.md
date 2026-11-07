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

FFmpeg's `ddagrab` implements Desktop Duplication. NVENC, AMF, QSV, OpenH264 and MPEG-4 fallback are tried with actual three-frame encode probes, in that order. The complete D3D11 scaling-to-NVENC path is separately probed; unsupported adapters use `hwdownload` and CPU scaling. That fallback has real CPU/memory bandwidth cost. GDI all-desktop and window-title modes are explicit user choices. A failure never silently switches to capturing a wider desktop region.

Windows Graphics Capture is not implemented. Individual outputs on the default DXGI adapter are selected by index. GDI captures the virtual desktop across all displays. Arbitrary monitor subsets and cross-adapter DXGI enumeration are not implemented.

## Audio

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
