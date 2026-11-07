# REPLAYX

A local Windows desktop recorder and disk-backed Instant Replay application. This is an alpha undergoing beta validation. Physical gameplay, stretched-resolution capture, usable microphone capture, editor multitrack playback, and installed-app recording are not yet verified for the current changes. See [VALIDATION.md](VALIDATION.md).

## Instant Replay: save what just happened

1. Open **Instant Replay**. Choose a duration, for example **1 min**, and save settings if changed.
2. Click **Enable replay** before playing. The available time starts at zero and grows as you play.
3. After something happens, click **Save replay** or press your configured shortcut (initially **Ctrl+Shift+F10**).
4. Open **Clips** to play the saved MKV or open its folder. Saving keeps the buffer running. Click **Stop replay** when finished.

Replay cannot save anything from before it was enabled. You can save a shorter clip before the buffer fills. Saves use completed two-second segments, so the very latest moment can be missing by about two seconds. Wait a few seconds after the event before saving. Default clips: `%USERPROFILE%\Videos\REPLAYX`.

The old Alt+F10 default conflicts with NVIDIA on some systems. The complete legacy default shortcut set now migrates to Ctrl+Shift combinations. Custom shortcuts are preserved. A conflicting shortcut is reported individually; other available shortcuts and buttons continue working.

## Recording and sound

**Start recording** records forward until **Stop & save recording**. It is separate from saving the preceding replay buffer, but both can share a capture pipeline. Stopping recording briefly restarts an enabled replay buffer.

In **Audio Mixer**, choose system output and/or microphone devices, enable the desired sources, then save settings before starting. Track 1 is the playback mix; with independent tracks enabled, subsequent tracks contain individual sources. A normal player usually plays one selected track. Editor import compatibility requires separate verification; MP4 export preserves tracks but cannot guarantee an editor supports them.

For separate game and Discord audio, use two Application sources and a microphone; disable the full System source to avoid duplicate audio. Select each running process and refresh/reselect after it restarts. Windows process loopback requires build 20348 or newer. Device changes require stopping, refreshing devices, choosing the device, saving settings and restarting; automatic device switching is not implemented.

## Stretched 4:3 capture

In **Recorder**, select Desktop Duplication, the intended display, **1080p**, and **Stretch to 16:9**. The intended output is 1920 x 1080 with square pixels. The engine validates the Windows/DXGI source dimensions and probes GPU scaling. A failed GPU probe uses CPU scaling with the chosen encoder and reports that path in Performance.

If a game occupies only a subregion while Windows still reports the full desktop, an explicit **Manual source region** can select it in local physical pixels. The application does not infer game boundaries or override NVIDIA scaling settings. This is not evidence that all NVIDIA GPU/display scaling combinations work. See the test matrix in [VALIDATION.md](VALIDATION.md).

On a detected display change, capture pauses, potentially affected tail footage is quarantined, and the same display is retried at most three times after checking for a stable mode. Full recordings split into parts; replay starts fresh. Use **Cancel recovery and stop capture** to cancel. Retained data can be recovered from Instant Replay after capture stops.

## Development

See [BUILDING.md](BUILDING.md), [ARCHITECTURE.md](ARCHITECTURE.md), [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md), [PRIVACY.md](PRIVACY.md) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). No account or cloud upload is required. Nothing in this repository automatically publishes a release.
