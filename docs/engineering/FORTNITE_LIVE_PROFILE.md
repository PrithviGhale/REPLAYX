# Live Fortnite investigation — November 11, 2026

The FPS problem is **unresolved**. No valid rendered/presented FPS comparison
was obtained, and no production capture/encoder change or new app release is
qualified by this investigation. Alpha.8 remains the rollback build. The game
was already running; no gameplay input, setting change, injection, game-memory
access, anti-cheat interaction or game restart occurred.

## Collection and validity

Fortnite was initially behind VS Code. Its existing window was activated once
without gameplay inputs. It was in an unpaused Creative scene, with a stationary
camera facing water and a platform. Foreground/minimized state was checked each
second during the resource runs; it stayed in front. This is a real game render
workload, **not a moving gameplay/competitive-match acceptance test**. No frame
generation or graphics settings were inspected or changed. The user's 360 FPS
cap was preserved; a cap can conceal overhead.

The existing NVIDIA PresentMon 1.9.12728 console exited 1 without producing a
CSV. Official Intel PresentMon 2.6.0 was downloaded as a standalone tool under
`test-output/fortnite-live-alpha8/`, without installation. Its SHA-256 matched
the release metadata:
`b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af`.
It exited 6: ETW trace-session access denied; an administrator token or Performance
Log Users membership is required. Sandbox escalation does not grant that token.
No UAC action, group/service change, security change or alternative injection was
attempted. See the [official console documentation](https://github.com/GameTechDev/PresentMon/blob/v2.6.0/README-ConsoleApplication.md).

Consequently game FPS, 1% lows, p95/p99 frame times, GPU Busy/Wait per game frame,
frame-time variance/spikes and frame-level capture/game correlation remain
**unmeasured**. Screenshots are not substituted for CSV frame-time measurements.

The resource collector runs the existing alpha.8 CLI with 1920x1080, 60 FPS,
H.264 NVENC, Balanced, Smooth/stretch, a 60-second replay window and a 256 MB
buffer. It opens system loopback only, never the microphone. Startup, two
concurrent saves, shutdown and settling are outside the 60-second B window.
NVIDIA and PDH observers run in all A1/B/A2 windows. No compilation or heavy
test ran during these windows.

Initial run 1 GPU averages are invalid: Windows buffered NVIDIA stdout and the
first collector attributed samples by receipt time. Raw evidence is retained
and annotated. The corrected collector uses the driver's timestamp, records
UTC phase boundaries and rejects inadequate sample coverage. PDH wildcard
queries are recreated regularly so contexts created after A1 can be seen.

## Resource results

Timestamped run 2 (`test-output/fortnite-live-alpha8/aba-resources-2/`):

| Device-wide NVIDIA mean | A1: replay off | B: replay on | A2: replay off |
| --- | ---: | ---: | ---: |
| GPU utilization | 38.22% | 47.56% | 39.15% |
| Encoder utilization | 0% | 20.31% | 0% |
| Graphics clock | 2449.5 MHz | 2505 MHz | 2467.8 MHz |
| Memory clock | 8001 MHz | 8001 MHz | 8001 MHz |
| Board power | 45.59 W | 51.81 W | 46.83 W |
| GPU temperature | 57.64 C | 59.69 C | 58.85 C |
| Driver samples | 60 | 59 | 60 |

PDH's game 3D-engine counter averaged 79.45 / 66.88 / 79.31%. It is a different
metric from NVML device utilization and must not be equated with FPS. Run 2's
perpetual wildcard query did not reliably enumerate new encoder contexts;
capture-process 3D/Copy/VideoEncode attribution from that run is unavailable.

Measured B CPU means, normalized across all logical CPUs: CLI 0.284%, FFmpeg
0.284%, existing REPLAYX main process 0.267%. Peak resident sizes were 16.50,
243.95 and 38.48 MB respectively. These are named processes, not a complete
WebView/process-tree measurement. Fortnite CPU averaged 17.62 / 17.09 / 17.22%.
Observer overhead is included; no game-FPS effect is derived from these numbers.

The driver reported one encode session in B and none in either control. Its
average encode latency was 1038 microseconds. This driver statistic is **not**
acquisition-to-file latency; its reported ~85 FPS is not the recorder's actual
frame rate. No NVIDIA replay setting was toggled or assumed.

## Recording and replay results

Run 2 wrote 1920x1080 H.264 with a playback mix and independent system-audio
track. Two concurrent 60.021-second saves became validated/playable in
**1842 / 1740 ms**. The first save's stages were snapshot 81 ms, remux complete
991 ms and validation ready 1836 ms. Saves use the existing packet-copy path;
there was no replay re-encode. Thumbnail generation remains asynchronous.

Across the capture, FFmpeg reported 5131 output frames, 59.74 FPS, 7 output-grid
drops and 32 output-grid duplicates; the last measured video lag was 242 ms.
These counters do not count repeated source pictures. Audio discontinuities
were zero and final drift correction was -0.009 ppm. No physical audio listening,
microphone, Discord separation or editor acceptance is inferred.

Run 1 also completed two saves in 1818 / 1929 ms, with zero audio
discontinuities, 0 grid drops and 5 grid duplicates. Its resource-telemetry
phase error does not invalidate its separately recorded media/save timings.

## Public architecture research and pinned-source audit

NVIDIA describes ShadowPlay's replay and recording features, but publishes no
complete internal capture/scheduling architecture on the
[NVIDIA app page](https://www.nvidia.com/en-us/software/nvidia-app/). Claims about
private hooks, internal zero-copy mechanisms or exact ShadowPlay threading would
be speculation. The [Video Codec SDK](https://developer.nvidia.com/video-codec-sdk)
provides supported hardware interfaces and integration examples.

The [NVENC guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvenc-video-encoder-api-prog-guide/index.html)
recommends asynchronous output on WDDM Windows, reusable registered resources,
and separating blocking output handling from submission. It also warns about
cross-thread DXGI acquisition and DirectX bitstream access, and specifies async
encoding, blocking bitstream locking and system-memory bitstream output for
that situation. RGB encoding, lookahead, AQ and multipass can use CUDA resources
in addition to the dedicated encoder. This explains why hardware encoding need
not have zero rendering cost; it does not quantify REPLAYX's cost.

The exact pinned FFmpeg revision is
[`b204e94e90`](https://github.com/FFmpeg/FFmpeg/tree/b204e94e90). Source was fetched
read-only; runtime binaries were not replaced or patched:

- `libavfilter/vsrc_ddagrab.c`: acquires the display surface, copies it to an
  internal fallback texture, copies the requested region to the output texture,
  releases the duplication frame, then waits on the next request's pacing slot.
  CPU readback is absent on this direct GPU BGRA path. GPU residency does not
  imply no copies. Surfaces are reused through FFmpeg's hardware-frame pool.
- `libavcodec/nvenc.c`: registers/caches GPU resources, maps them for encoding,
  uses `enableEncodeAsync=0` and `doNotWait=0` for bitstream retrieval. Output
  surfaces and pending/ready queues are bounded. The CLI's `-delay`/surface
  buffering is not an event-driven NVENC async-mode switch.
- `fftools/ffmpeg_enc.c`: encoder processing has its own scheduler thread.
  REPLAYX's separate audio/video graphs avoid the known shared-filter-thread
  stall, but do not turn synchronous NVENC retrieval into NVIDIA's recommended
  event model. No trace established that an internal lock is the dominant stall.

Source SHA-256 (raw bytes):

| File | SHA-256 |
| --- | --- |
| vsrc_ddagrab.c | a5786f903fb38f641c5c88dbff0f70fd7b5cb8f445d4df9622a70ceb3f80eb8a |
| nvenc.c | 72bb256ab1665d407b468d6eab7756672fe257bef87a2dcae79b0ec1b731f39c |
| ffmpeg_enc.c | b1352c75f485330d4d5a3453e526b8ae80e91f08fa250bfa2cd64b4f35c76ca8 |

Microsoft recommends minimizing the interval between
[ReleaseFrame and the next acquisition](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutputduplication-releaseframe),
because updates while the frame is unowned can cause unnecessary compositor
copies. This makes ownership timing worth testing; it does not prove that
changing FFmpeg's release point is safe or sufficient.

## Isolated operation probe

`replayx-dda-profile` is a diagnostic executable, not a new capture backend. It
requires an existing foreground Fortnite PID, uses only public DXGI/D3D APIs,
reuses one or two GPU textures, samples timestamp queries every 16 acquisitions,
and exits on focus loss or geometry change. No frame pixels reach CPU memory
or files. CPU API wall times and GPU query spans are separately labeled.
Both policies explicitly Flush submission without waiting because the probe
has no Present/NVENC call; it does not reproduce all FFmpeg scheduling.

Eight 20-second runs (two repetitions per combination), NVIDIA-driven
1920x1080, 250 Hz, 60 acquisitions/s:

| Policy | Copies | Mean GPU copy span, repetition 1 / 2 | Mean CPU acquisition wall time, repetition 1 / 2 |
| --- | ---: | ---: | ---: |
| Release before pacing wait | 2 | 0.0491 / 0.0458 ms | 0.0952 / 0.1045 ms |
| Hold until next acquisition | 2 | 0.0411 / 0.0410 ms | 2.0939 / 2.1197 ms |
| Release before pacing wait | 1 | 0.0245 / 0.0254 ms | 0.1059 / 0.1056 ms |
| Hold until next acquisition | 1 | 0.0212 / 0.0214 ms | 2.0705 / 2.1071 ms |

Each run completed 1201 acquisitions and 75 GPU samples, with no acquisition or
query timeouts. The two-millisecond acquisition increase is blocking wall time
waiting for a display update; it is not two milliseconds of GPU computation.
GPU spans include scheduling/preemption, not just hardware busy time. About
5000 accumulated desktop updates per run reflect the display rate, not rendered
game FPS or dropped recording frames.

Removing a copy approximately halves this tiny isolated copy span. That saves
only ~0.02 ms per acquired frame in this probe and does not establish a large
game-FPS gain. The existing internal copy handles pointer-only updates and
fallback surfaces; it cannot simply be removed safely. Holding the frame is a
candidate upstream experiment, **not a retained production optimization**.

## Decisions and remaining work

- Preserve quality, lookahead, AQ, GPU HIGH priority, WASAPI pacing and bounded
  queues. Alpha.8's lookahead-off repetition regression came from video/audio
  scheduling, not evidence that the encoder requires lookahead for smoothness
  in every architecture. AQ/priority experiments did not show convincing gains.
- Do not rewrite NVENC or replace FFmpeg without controlled real-game evidence.
  Event-driven output and less duplication ownership idle time are plausible
  experiments requiring a compatible runtime build, thread/device lifetime
  checks, motion/audio regression tests and valid game frame-time measurements.
- GPU NV12 conversion could avoid some RGB preprocessing, but alpha.8's pinned
  D3D11 NV12 scaler could not allocate its pool on this hardware. Retain fallback.
- [Windows Graphics Capture](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture)
  provides display/window frame pools; desktop
  [monitor interop](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createformonitor)
  is available from Windows 10 1903. A pool still needs format conversion,
  scheduling, resize/device recovery and safe ownership. No WGC performance
  comparison was obtained, so Desktop Duplication was retained.
- In-app source FPS, source drops, queue depth, individual D3D/NVENC operation
  latency and GPU activity remain null where unmeasured. External one-second
  telemetry must not be presented as per-frame kernel attribution.
- A person must collect permitted PresentMon traces while maintaining the same
  scene and foreground state. Follow [FORTNITE_ABA.md](FORTNITE_ABA.md). Repeat
  A/B/A three times; preserve the cap and flag capped phases. For NVIDIA-only
  and both-recorder tests, change NVIDIA replay manually using its supported UI,
  then repeat the identical sequence. No NVIDIA-only comparison was performed.
- Physical stretched fullscreen transitions, moving Fortnite gameplay,
  microphone listening/device changes, Discord capture and editor playback
  still require manual acceptance. Do not install the preserved alpha.8
  installer as part of this investigation.
