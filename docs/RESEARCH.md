# Performance research — November 7, 2026

These are design references, not evidence that REPLAYX is smooth. No OBS code
was copied; its GPL implementation was read for architectural principles only.

* [FFmpeg Desktop Duplication source](https://github.com/FFmpeg/FFmpeg/blob/master/libavfilter/vsrc_ddagrab.c): acquisition is paced with a monotonic clock; its output timestamp is sampled around acquisition. The current upstream implementation does **not** simply forward DWM LastPresentTime. It can repeat its last texture on timeout. This repeat is separate from FFmpeg's output CFR duplicate counter. Inspect the pinned runtime experimentally; do not equate zero reported drops with unique motion.
* [FFmpeg output timing](https://github.com/FFmpeg/FFmpeg/blob/master/doc/ffmpeg.texi) and [fps filter](https://github.com/FFmpeg/FFmpeg/blob/master/libavfilter/vf_fps.c): CFR requires selecting or repeating frames against a real timeline. Forcing every frame to `max(previous+slot, timestamp)` cannot recover after a burst; it can move video ahead of its clock. Compare bounded slot selection with the old expression on captured motion. Avoid `N/fps`, which conceals capture stalls and changes elapsed duration.
* [FFmpeg concat demuxer](https://github.com/FFmpeg/FFmpeg/blob/master/doc/demuxers.texi): file durations and inpoints control timestamp offsets; stream layouts must match. Stream copying avoids another encode. Preserve AAC priming and test segment seams rather than assuming nominal segment duration is sufficient.
* [OBS video scheduler](https://github.com/obsproject/obs-studio/blob/master/libobs/obs-video.c): uses deadline-based video ticks and accounts for missed intervals. [OBS replay muxer](https://github.com/obsproject/obs-studio/blob/master/plugins/obs-ffmpeg/obs-ffmpeg-mux.c): pins encoded packets, performs mux work on a worker, then signals completion. Applicable principles: bound work, retain independent snapshots, report completion directly, and avoid encoding again during save. REPLAYX keeps its disk ring instead of adopting OBS's memory packet architecture.
* [Microsoft Desktop Duplication](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api): exposes GPU surfaces and dirty/move regions. Preserve GPU scaling and revalidate dimensions after access loss. [WGC frame pools](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool?view=winrt-26100) support free-threaded arrival and recreation; [SystemRelativeTime](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframe.systemrelativetime?view=winrt-28000) is compositor QPC time. WGC would require a new backend: it is not implemented or claimed as a fix in this patch.
* [NVIDIA SDK 13 encoder guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvenc-video-encoder-api-prog-guide/index.html): Windows WDDM asynchronous output avoids blocking submission; buffer depth and presets trade throughput, delay and quality. FFmpeg owns these interfaces here. Do not blindly force lowest latency or reduce resolution/FPS. [NVIDIA App](https://www.nvidia.com/en-us/geforce/news/nvidia-app-download-and-features/) documents replay/recording controls, not its private capture/queue implementation; no private implementation or superiority claim is inferred.
* [Intel VPL encoding](https://intel.github.io/libvpl/latest/programming_guide/VPL_prg_encoding.html): asynchronous sessions reuse surface pools and enforce ownership until synchronization. Keep native Intel texture processing where the display is Intel-driven; benchmark it against scaled NV12 transfer to NVENC and CPU fallback. No universal best encoder is implied by CPU measurements alone.

Tests must distinguish source animation stalls, capture acquisition repeats,
filter/CFR repeats, encoder throughput and player behavior. GPU engine loads,
hardware queue depths and capture-to-encode latency remain unavailable through
the current process boundary; show “Not measured,” not invented values.

## Game-FPS impact on hybrid laptops — November 8, 2026 (0.2.1-alpha.3)

**Observation (measured, passive):** with REPLAYX 0.2.1-alpha.2 buffering at 120 fps on a near-static desktop, Windows' GPU engine counters showed REPLAYX's FFmpeg using about **34% of the Intel Iris Xe 3D engine** and 19% of its video engine (no game rendering at that moment). The user measured Roblox falling from 150–200 to 80–100 FPS while recording.

**Why that matters on this hardware:** on an Optimus laptop the panel is driven by the Intel GPU. A game rendered on the RTX is copied to the Intel GPU and composited there by DWM before it reaches the screen, so the Intel 3D engine is on every game frame's path. Desktop Duplication must capture on that same adapter. Work REPLAYX puts on the Intel shader cores competes with the game's presentation.

**Sources and what they say:**

* Intel's GPU optimisation guide, [Media Engine Hardware](https://www.intel.com/content/www/us/en/docs/oneapi/optimization-guide-gpu/2023-0/media-engine-hardware.html): H.264 encode has a **low-power VDEnc path that is fully fixed-function**, and a PAK path that uses motion estimation and media kernels on the execution units. Gen 12 (Iris Xe) supports both. [Jellyfin's Intel guide](https://jellyfin.org/docs/general/post-install/transcoding/hardware-acceleration/intel/) gives the same two modes per generation.
* [FFmpeg QSV encoder options](https://ffmpeg.org/ffmpeg-codecs.html#QSV-Encoders): `low_power` "reduce[s] power consumption and GPU usage". Rate control: `global_quality` alone selects ICQ, `maxrate > b` selects VBR, `maxrate = b` CBR. `vpp_qsv` has `scale_mode` (auto, low_power, hq, compute, vd, ve) and `out_color_matrix`/`out_range`.
* [FFmpeg NVENC options](https://ffmpeg.org/ffmpeg-codecs.html): presets p1–p7, `rc vbr` with `cq`, `spatial-aq`. NVENC is a separate engine on the NVIDIA GPU, but on this laptop its frames must first cross from the Intel GPU.
* [OBS encoding-performance guide](https://obsproject.com/kb/encoding-performance-troubleshooting): when a game drives the GPU at full throttle the recorder gets "very little GPU processing power". Recommended fixes are capping the game's frame rate or V-Sync, lowering output resolution or FPS, and letting the recorder reserve GPU priority (running as administrator). REPLAYX does not raise its GPU priority: on this hardware that would take time from the game.
* [Microsoft Desktop Duplication](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api) and [Windows Graphics Capture](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture): both capture on the adapter that composes the desktop. Neither avoids the Intel GPU on a hybrid laptop. ShadowPlay's per-application capture on the NVIDIA GPU relies on driver internals that are not public.

**Decisions in 0.2.1-alpha.3 (pending measurement in real games):**

1. Prefer Quick Sync's fixed-function VDEnc (`-low_power 1`), probed first with automatic fallback. Expected effect: the encoder's motion estimation leaves the Intel shader cores.
2. Keep scaling and colour conversion in Quick Sync VPP on the GPU (zero-copy), with explicit BT.709 conversion. `scale_mode` stays automatic until measured (`REPLAYX_VIDEO_TUNING=vpp_scale_mode=…` allows benchmarking `low_power`/`ve`).
3. Show encoder speed and video lag, and stop with the real cause (encoder or GPU overloaded) instead of an audio symptom.
4. Make 120 fps a visible trade-off in the UI rather than silently reducing it.
5. Build a repeatable workload, `replayx-gameproxy`: a heavy shader on the high-performance GPU, presented through the display GPU like a game, with time-coded frames. `scripts/overhead-bench.py` measures game FPS and 1% lows with each capture path, GPU engine use, and recording smoothness. Results go to PERFORMANCE.md only once they are measured.

## Voice processing (0.3.0-alpha.1)

Goal: an ordinary gaming-headset microphone that sounds clear, full and steady, without robotic, metallic or "underwater" artefacts, built from documented techniques and permissively licensed code only. Voicemod and similar products were studied only through public documentation and general audio-engineering practice; no proprietary code, models, assets or algorithms were used or examined.

**What makes voice sound good (common practice in OBS filter guides and broadcast audio):** remove rumble below ~80 Hz; reduce steady noise *before* any gain or EQ so it is not amplified; a gentle expander rather than a hard gate so word endings and quiet speech survive; cut boxiness around 250–400 Hz and add presence around 3 kHz; compress 2.5–4:1 with a few dB of make-up gain for a steady level; tame sibilance with a split-band de-esser when the top end is boosted; end with a peak limiter so gain never clips.

**Noise suppression options considered:**

| Option | Notes | Decision |
| --- | --- | --- |
| FFmpeg `afftdn` (previous) | Spectral subtraction with a fixed noise-floor estimate. Measured only 2.2 dB less steady fan noise at headset levels with the shipped settings (2.5 dB with stronger ones). | Replaced. |
| RNNoise (Xiph/Mozilla, BSD-3) | Small recurrent network + DSP, 48 kHz, 10 ms frames (exactly REPLAYX's audio chunk), about 10 ms algorithmic delay, also gives a voice-activity probability. | **Chosen**, via `nnnoiseless`, a pure-Rust port (no C toolchain, no DLL). |
| WebRTC audio processing (BSD) | Noise suppression + AGC + echo cancellation; heavier C++ dependency with a build toolchain; AEC is not needed for recording. | Not chosen for now. |
| SpeexDSP preprocessor (BSD) | Older spectral noise suppression and AGC; weaker on non-stationary noise than RNNoise. | Not chosen. |

**Measured** (scripts/voice-eval.ps1: Windows text-to-speech mixed with synthetic fan noise, broadband hiss and keyboard clicks; no microphone was used): Light/Balanced/Strong removed 9.5/17.0/25.0 dB of fan noise and 10.0/18.4/28.0 dB of hiss; speech level changed by at most 0.3 dB; the Noise-Free preset removed 28.6 dB of fan noise. Short keyboard clicks are only partly reduced (RNNoise is weakest on short transients). Processing cost: 0.5–1% of one CPU core; added latency on the microphone track: 11 ms (RNNoise frame + 1 ms limiter look-ahead).

Metrics cannot say whether a voice *sounds* better. Listening tests with real headset recordings, and a fair comparison with Voicemod on the same recording, are still to be done.
