# Manual acceptance for 0.3.1-alpha.7

These checks are pending. Synthetic measurements cannot qualify real gameplay,
Discord, physical audio or installed update replacement.

1. Use the same repeatable game scene with capture off/on/off, three runs each,
   60 seconds after warmup. Record average FPS and 1% lows with PresentMon or
   the game's benchmark. Record display GPU, resolution, refresh rate, codec,
   actual encoder/scaler from Performance, and Discord hardware acceleration.
   Target <10% initial overhead; <5% is a goal, not a guarantee.
2. Fullscreen Valorant: test 1280x960 and 1440x1080 via Windows Settings and
   NVIDIA Control Panel. Test GPU/display full-screen scaling separately.
   Change resolution during replay; save before/after. Inspect all edges,
   stretch, offsets, transitions and saved playback. Repeat borderless and
   multi-monitor. Do not qualify from resolution metadata alone.
3. Start replay, start Discord screen sharing, save twice concurrently, stop
   sharing, save again. Confirm Discord remains live and all clips decode with
   continuous motion. Repeat window and desktop capture, fullscreen/borderless.
   Compare FPS with Discord alone and with both applications active.
4. Physical microphone: toggle on Home, Audio and tray while idle, recording
   and buffering; disconnect/reconnect the headset. Listen for leaks, clicks,
   missed speech and sync at beginning/middle/end. Muting has transport/encoder
   latency and a 10 ms fade. Open MKV tracks in the actual target video editor;
   verify isolated mic/system/app audio and preserve the WASAPI crackle fix.
5. In a disposable Windows VM, install an older signed build with fixture data.
   Offer a newer signed build from a controlled HTTPS manifest. Verify install,
   restart, version and preservation of settings/playtime/clips. Repeat with
   replay, recording, export and saves active: automatic installation must wait.
   Test installer cancellation and recovery. Never use the working installation
   as the test target. No installed replacement was performed in this session.

For a shareable report use Performance > Capture diagnostics. Raw private paths,
device IDs, game names and stderr are excluded. Unique source FPS, GPU load and
per-frame encoder latency remain unmeasured live and are reported as null.
