# Experimental Native Engine V1 terminal harness

> Historical terminal-harness milestone. The newer desktop Alpha, its partial physical-device evidence and remaining limits are documented in [Native Engine V1 Alpha](NATIVE_ENGINE_ALPHA.md) and its [validation report](NATIVE_ENGINE_ALPHA_VALIDATION_2026-11-15.md). Statements below about missing UI/physical validation refer to this earlier harness milestone.

Development only, in `replayx-perf`. This is not the installed application, is not a production default and has not been physically validated with microphone/system audio. Do not run a new CS2/fullscreen comparison without the owner's explicit approval.

## Safe generated-media validation

Nothing is captured from the desktop or sound devices, and no audio is played:

```powershell
$env:REPLAYX_NATIVE_TEST='1'
python -W error -m unittest discover -s tests -v
```

The test suite builds on a freshly built `src-tauri/target/release/replayx-native-proto.exe` and `replayx-native-audio.exe`. Build with:

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --offline --release --bin replayx-native-proto --bin replayx-native-audio
```

## Explicit future live session

Do not launch until capture is approved and the supported game is already in front. The native guard accepts the selected supported game process, verifies its identity once, and checks foreground ownership thereafter. The harness never starts/focuses a game or changes its settings.

A fresh session path must be inside this worktree's `test-output`. Nothing loads or saves the installed app's settings. Omit `--sources` for silent recording. To request system/microphone recording, create an explicit source file such as:

```json
[
  {"kind":"system","name":"System audio","enabled":true,"device":"","volume":1.0},
  {"kind":"microphone","name":"Microphone","enabled":true,"device":"","volume":1.0}
]
```

An empty device ID means that source's Windows default; explicit endpoint IDs are also supported. The producer retains the existing DSP/clock behavior. Opening physical devices requires capture authorization; no such test was performed for this milestone.

Future approved invocation (replace PID and choose a new session name):

```powershell
python scripts/bench/native-v1.py --pid 12345 --seconds 30 --session test-output/native-v1-approved-session --sources test-output/native-v1-sources.json
```

This is monitor capture at the unchanged 1080p60/15 Mb/s target/23 Mb/s maximum/GOP 120 settings. The experiment is bounded to 2?600 seconds. The only source kinds currently exposed are system and microphone; no application-audio routing or editor support is claimed. Source ordering defines track ordering; there is no separate playback mix yet.

## Replay, stop and recovery

Write a complete request JSON atomically into `SESSION/requests/name.json`:

```json
{"seconds": 10, "filename": "my-moment.mp4"}
```

Only closed segments are included, so the latest ~two seconds can remain unavailable until the next segment closes. The selected GOP-aligned clip can exceed the requested length by a segment. `clips/my-moment.mp4.json` changes from `saving` to `ready` or `failed`; `ready` means the MP4 copy job finished. Existing files are never overwritten. Concurrent requests pin their sources before retention can unlink them. Full normal recordings appear as `clips/recording.mp4`.

`--replay-only` disables permanent full-recording pins; only the bounded ring and saved clips remain. `--replay-seconds` controls retained history and `--buffer-mib` caps ring bytes. Full recordings/saved clips intentionally consume additional disk outside the ring budget. Less than 512 MiB free stops the session. Long-duration exhaustion behavior is still unverified.

Creating `SESSION/stop` requests cancellation. It is reported explicitly as an interrupted session; already completed segments survive. Jobs/deadlines bound capture, audio and muxer shutdown. `status.json` reports `fallback_required`, `healthy=false` and whether old workers stopped. There is no automatic existing-engine restart or continuity claim. A live failure may lose the unfinished segment.

After stop or a controller crash, recover completed recording pins (or retained replay segments) without opening capture devices:

```powershell
python scripts/bench/native-v1-recover.py --session test-output/native-v1-approved-session --filename recovered.mp4
```

Recovery refuses a still-running controller and preserves source files. It refuses existing output names. A crashed session's stale status can be superseded by a successful recovery result, but is not rewritten as a successful recording.

## Limits and next gates

This harness proves the generated-media path and recovery mechanics. It does not yet integrate with the main UI, continuous multi-hour recording, automatic fallback or the app's save notifications/library. Physical audio sync and recovery, moving-gameplay quality, video-editor behavior and performance with all features active remain unverified. Per-save probes can add latency to longer replays; further optimization needs measured evidence. Keep the existing recorder as the manually selected fallback until the automatic handoff is implemented and tested.

See `NATIVE_V1_CS2_REVIEW_2026-11-14.md` for the independent completed-run analysis, exact exclusions and test evidence.
