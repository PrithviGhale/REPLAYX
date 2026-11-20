> **2026-11-16 follow-up:** [Measured motion improvement, physical audio, 90-second retention and remaining counterexamples](NATIVE_V2_VALIDATION_2026-11-16.md). The opt-in candidate has clean 600/480-frame recording/replay evidence and 0.370% recorder CPU. Mixed-desktop and capture-burst cases remain limitations; defaults are unchanged. Results below are historical.

> Continuation: [frame-pacing reliability and performance preparation](NATIVE_V2_RELIABILITY_2026-11-16.md) records newer spatial-cadence, starvation/recovery and benchmark-preparation results. V2 is still experimental.


> **Latest follow-up:** [Live phase-lock validation and bounded-recovery candidate](NATIVE_V2_PACING_LIVE_2026-11-15.md). Earlier NVIDIA 60/59.94 app/replay tests passed, a 30 FPS boundary defect and redundant-capture sealing were fixed, and the final candidate is rebuilt. Final GPU/app/audio acceptance is pending while the user is gaming. Defaults remain unchanged. The results below are historical.

> **Final 2026-11-15 status: smoothness gate remains FAIL.** Earlier 720-frame unique-content success did not survive all start phases: the final app recording has 65 repeats in 480 frames, with every source ID captured and correct encoder output. A full-core native transport spin was separately fixed and measured. See [complete evidence and remaining blocker](NATIVE_SMOOTHNESS_2026-11-15.md). Do not treat compile/decode/oracle agreement as milestone completion.

# REPLAYX Native Engine V1 Alpha

Experimental desktop integration in `replayx-perf`. The default remains the existing FFmpeg Engine. This is a development application, not an installer or release, and the existing app version remains `0.3.1-alpha.9`.

## Launch and select

Run in a terminal when you want to use the Alpha:

```powershell
cd "C:\Users\Prithvi\Desktop\VS CODE\replayx-perf"
npm run desktop:native
```

The launcher builds the optimized native sidecar offline and starts the existing Tauri/React interface. Dependencies must already be available locally. Keep its terminal open. In **Video > Recording engine**, choose **REPLAYX Native Engine (Experimental)** and click **Save changes**. You can change the engine only while capture is stopped. A new Alpha profile starts on **Existing FFmpeg Engine (default)** with microphone capture disabled; enable and select the microphone in Audio before a speech test.

Use the usual Record button or Ctrl+Shift+F9. Turn on Instant Replay before the moment you want to keep; wait for the history counter to grow, then click Save replay or press Ctrl+Shift+F10. Replay saves can run while recording continues. Stopping recording leaves an already-enabled replay buffer running. Ctrl+Shift+F8 toggles replay. Clips shows saving/ready state and generates thumbnails asynchronously. Closing a window may leave the app in its tray; use Quit to shut down capture and workers.

Check the actual backend in Home/Performance after capture starts. Selecting Native is a preference, not proof that the current session is native. Incompatible settings, hardware or initialization failures display the reason and start the existing FFmpeg backend for that session without changing the saved preference. FFmpeg fallback retains its existing MKV output. Successful native sessions save MP4.

## Current hardware status (V2 milestone, 2026-11-15)

Native Alpha now tries a hardware H.264 MFT on the selected display's exact GPU adapter. The common path supports 1920 x 1080 at 60 FPS, whole-display Stretch to fill, GPU scaling, H.264 and Balanced/Custom settings. Existing rate/quality settings remain required. Intel and NVIDIA laptop encoders passed generated encode/decode checks. The RTX 4070 external display produced native recording and replay MP4s with no fallback. AMD's shared MFT branch is implemented but hardware-unverified; a direct AMF backend is not yet implemented.

**NVIDIA smoothness remains a blocker.** One run delivered 720 consecutive source pictures, but a later real app run produced 65 repeats in 480 frames despite zero captured-grid/conversion/decode errors. Stable phase-independent source delivery is not verified. Static and low-rate repetitions remain correct. The [complete investigation](NATIVE_SMOOTHNESS_2026-11-15.md) records both passes and failures, the verified transport CPU fix, and the remaining sampling-phase issue. Gameplay FPS, other physical refresh modes and untested vendors remain unverified. The [earlier V2 milestone](NATIVE_V2_MILESTONE_1.md) preserves the original failure evidence.

Capture, scaling and encoding use the same D3D11 device; hardware MFT selection is filtered by adapter LUID. Incompatible/unknown vendors or controls, uncertain geometry, rotation, source rectangles, unsupported canvas/resolution/codec settings and unsupported HDR/color paths fail explicitly or use the existing FFmpeg fallback. A display-size change stops and preserves the current native clip; seamless native display-mode recovery remains unimplemented. The UI still uses legacy default-adapter display indices; passive full topology does not imply arbitrary multi-adapter monitor selection is ready.

## Isolation and safety

- Settings, library metadata and buffer: `test-output/native-desktop/data`.
- Default clips: `test-output/native-desktop/Clips` (a user-selected output folder takes precedence).
- Separate app identifier: `io.replayx.nativealpha`; development port 1421.
- Alpha skips updater polling and Windows startup-entry synchronization. Bundling is disabled in its Tauri override. It does not install or update the installed application.
- The normal launcher clears generated/physical-test environment flags before launch and never starts capture automatically.
- `npm run desktop:native -- --generated` is a developer-only generated-media mode with separate `native-desktop-generated` data/clips, clearly labelled window title, and no desktop/audio-device capture. It still uses the Intel GPU encoder; do not run it during gaming without approval.
- The native sidecar is `src-tauri/target/release/replayx-native-proto.exe`; the Tauri development executable is `src-tauri/target/debug/replayx.exe`. Launch through the command above so the Alpha configuration and isolated data directory apply.

Native initialization validates a closed playable segment before claiming success. Worker lifetime/texture ownership remains tracked and bounded. Stop drains the encoder, waits a bounded time for shared-clock PCM, closes audio, and finalizes segments. Runtime failure reports Error and attempts to save completed footage rather than silently continuing across a gap. Source segments remain recoverable on failure; the unfinished segment can be lost. Automatic mid-recording continuation on a different backend is not claimed.

## Audio and compatibility

The app reuses its WASAPI sources, device selection, mute/gain/Voice Enhancer controls, playback mix and independent tracks. Native audio/video share the first video frame's QPC clock, with startup silence and the existing drift correction. Named audio tracks and recording/game metadata are retained in new MP4 exports. Application loopback continues through the existing source implementation where Windows supports it, but physical application-audio routing, editor import, device switching and native acoustic synchronization have not been validated here. Unsupported/open-failed sources must surface an error or initialization fallback, not fabricated audio success.

The shared recording ring, replay retention, pinned concurrent saves, library, hotkeys and game-attribution machinery are reused. Completed native videos are remuxed without video re-encoding. A race-safe Windows publication step refuses to overwrite an existing MP4.

See [validation evidence and real-session checklist](NATIVE_ENGINE_ALPHA_VALIDATION_2026-11-15.md) before treating this as a verified gameplay recorder. The earlier [terminal harness](NATIVE_V1.md) remains available; its historical limitations describe that harness, not the newer desktop integration.
