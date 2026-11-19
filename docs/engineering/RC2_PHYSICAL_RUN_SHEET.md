# REPLAYX rc.2 physical test run sheet

The Windows installer scenarios remain deferred. The benchmark writes to fresh folders under `replayx-rc/test-output` and does not use the installed app profile. FFmpeg remains the shipping default. The runner selects the same-adapter H.264 encoder for the single active display. The internal 2560 × 1600, 240 Hz panel was Intel-driven and selects Quick Sync. The external 1920 × 1080, 250 Hz display was RTX 4070-driven and selects NVENC. In the external preflight, both FFmpeg NVENC and the Native NVIDIA Media Foundation encoder produced eight decodable 1080p H.264 frames on the RTX 4070. This qualifies encoder startup for that topology. Native NVIDIA previously failed activation when the Intel panel was active and the RTX had no active display, with FFmpeg NVENC available as fallback. Keep external and internal results in separate ledgers and calculate overhead against each setup's own recording-OFF phases.

## 15-second physical audio window

Approve desktop capture, opening the default microphone, and playing the quiet marker. Keep games closed, use headphones, and confirm that the intended devices are already the Windows defaults. Close other capture sessions. The 15-second measurement starts after the Native recorder reports it is ready, following the initial preroll. A fallback interrupts this qualification and is reported.

Show the plan without starting capture:

```powershell
.\src-tauri\target\release\replayx-cli.exe native-app-test --physical --seconds 15 --plan-only
```

After approval, run the same command without `--plan-only`. When it prints **Physical capture is active**, start the prepared fixture from a separate terminal or have it queued. Say a short phrase after the first few pulses. Leave the player visible and the Windows volume unchanged.

```powershell
.\src-tauri\media\ffplay.exe -autoexit -alwaysontop -x 640 -y 480 -volume 30 test-output/rc2-physical-qualification/flash-beep-12s.mkv
```

The CLI saves both a recording and a replay. Independently decode both and verify the mix, system and microphone tracks. Match early and late flashes to the system track with `py scripts/bench/av-sync.py src-tauri/media/ffmpeg.exe PATH_TO_CLIP 1` after confirming stream order. The fixture calibration detected 11 usable markers and measured a constant -2 ms detector bias. Compare the offsets, marker count and final A/V timestamps, and listen for the spoken phrase and track separation. To measure absolute microphone-to-picture timing, include an audible reference on the microphone track. For example, hold one earcup near the mic for the final few pulses while keeping the earlier pulses isolated for the separation check.

## CS2 engine comparison

After explicit approval, use an open local practice match on the single active test display. Use the same view or repeatable movement in every phase. Use the actual FPS cap, with 0 only for uncapped. Keep resolution and display mode steady. Quit REPLAYX from its tray if running so the test has exclusive use of the recorder. The signed PresentMon executable is at `..\REPLAYX\test-output\fortnite-live-alpha8\PresentMon-2.6.0-x64.exe`. Its ETW recording requires an administrator terminal on this host. Each phase uses a unique trace session scoped to the current CS2 PID.

```powershell
py scripts/bench/cs2_ab.py --seconds 20 --warmup 30 --pairs 2 --units-per-run 1
```

That command only prints the plan. For the approved run, append `--execute --approved-gameplay --cap <actual-cap> --out test-output/cs2-rc2-external --presentmon <path-to-PresentMon>`. Repeat with the same arguments and `--resume` to reverse the engine order. Each unit measures 20 seconds per phase and retains 15 seconds of PresentMon data. Allow 4 minutes 3 seconds per unit, including settling and warm-up, plus the offline audit. The ceiling is 8 minutes 33 seconds per unit including focus wait and audit. The two recorded profiles are FFmpeg WGC at steady 2x and Native phase lock with the existing 6666 µs test interval. Both target 1080p60 H.264 at 15 Mbps, use system audio only, and keep Crosshair off. These are explicit benchmark profiles.

Inspect average FPS, 1% lows, P99, recorder CPU and working set, per-process GPU engines, temperatures, clocks and present modes. Both neighboring recorder-OFF phases must be valid. Reject focus loss, thermal or baseline drift, a limiting cap, changed adapter, fallback, or incomplete save. The offline audit decodes and hashes the clips, checks cadence, keyframes and audio timestamps, and records save latency. Review actual motion and audio before accepting a comparison. A static scene can produce genuine repeated pictures. If there is no useful motion in either recording, defer a smoothness verdict to a repeatable moving-scene capture.

### External-monitor continuation at 1,000 FPS

The first unit completed all five live phases. Its original post-run audit rejected FFmpeg's 21 ms interval at the first keyframe join. The offline recheck decoded all four clips and confirmed that the interval did not remove a 60 FPS slot. The original ledger and clips remain in `test-output/cs2-rc2-external-cap1000-20261118-204632`. The isolated continuation has its own ledger and references the original clips by SHA-256. It admits just one live attempt in the reverse order.

Return to the same local practice map and stationary camera position. Keep the RTX 4070 external monitor at 1920 × 1080, 250 Hz and preserve the 1,000 FPS in-game cap and the same graphics settings. Wait for explicit approval before running. Use the approved elevated PresentMon runner from the repo root.

```powershell
py scripts/bench/cs2_ab.py --execute --approved-gameplay --resume --cap 1000 --seconds 20 --warmup 30 --pairs 2 --units-per-run 1 --out test-output/cs2-rc2-external-cap1000-reverse --presentmon '../REPLAYX/test-output/fortnite-live-alpha8/PresentMon-2.6.0-x64.exe'
```

Only **OFF → Native V2 → OFF → FFmpeg → OFF** will run. The scheduled time is 4 minutes 3 seconds plus offline clip audit. The ceiling is 8 minutes 33 seconds, including a possible focus wait. Recheck capture and telemetry adapter LUIDs, reported encoder and fallback status. Then compare FPS, 1% lows and P99 relative to each phase's two neighboring OFF baselines. Pair recorder CPU, process GPU activity, NVENC and clip audit results with the same phase window. Report both order-specific overhead estimates with OFF spread, clocks and temperatures. Keep conclusions provisional with two units if the paired effect is inconsistent or its uncertainty spans zero.

## CS2 rapid-camera-motion comparison

Keep this data in a new folder such as `test-output/cs2-rc2-motion-<date>`. Use the RTX-driven external 1920 × 1080, 250 Hz monitor, the 1,000 FPS cap, and the original graphics settings. Use the same practice map, position and mouse sensitivity for all five phases. Pick two clearly different landmarks roughly 180 degrees apart. Scene edges and textured surfaces help reveal a held picture.

1. Use the normal gated engine comparison above. One matched unit measures five 20-second phases and takes 4 minutes 3 seconds before audit. Hold off on a second unit if the first exposes a recording defect. Keep stationary and moving results in distinct ledgers.
2. During each measured phase, including neighboring OFF phases, repeat the same movement. Hold at landmark A for the first five seconds. Turn quickly to B, then back to A, with one turn per second for the next ten seconds. Finish with a consistent five-second sweeping pan. Return to A during settling between phases. A second person can read phase starts from the runner without pulling focus from the game. Use matched turns across engines. If the movement or timing differs substantially, retain the footage and reject the comparison.
3. For each saved replay, mark the turns from playback using times relative to the clip's first decoded picture. The replay includes startup and may trim the beginning, so the PresentMon phase timestamp is not the clip timestamp. Enter each comparable range in the offline analyzer. The numbers below are example clip times.

```powershell
py scripts/bench/frame-pacing.py <clip> --motion-window 8 18 --motion-window 18 23 --out <new-output.json>
```

The motion-window report compares adjacent decoded pictures at their presentation timestamps and shows the longest held run, the difference distribution and interval percentiles. Review individual turns at normal speed and frame by frame. Check for visible holds and jumps, changing edges, fine detail, first and last audio timing, and complete playback across segment joins. The normal audit independently checks full-resolution exact repeats, timestamp gaps, keyframes, all audio tracks and playable save times. Tie the manual review to both clip hashes. PresentMon establishes game presentation cadence and helps check spikes. It does not identify which game frame the recorder received, so report repeated decoded pictures separately from proven capture or encoder drops. Use the deterministic motion fixture when source IDs are needed.

Compare game FPS, 1% lows, P99, temperatures, clocks, recorder CPU and RSS, per-process GPU work, and save latency against matched phase baselines. Preserve the Native worker's submit and output latency, repeat/drop counts, and surface waits. Compare those with FFmpeg progress speed, output lag and duplicate/drop counters. FFmpeg's regular benchmark progress provides aggregate lag, not per-frame submit-to-output latency. Keep any heavier FFmpeg timestamp tracing out of the scored gameplay window. A recognisable in-game sound and visual event can support the playback review. The separate flash-and-beep procedure qualifies physical A/V more precisely.

## CS2 Crosshair ON/OFF

Run a separate gated session with `--crosshair` and a new folder such as `test-output/cs2-rc2-crosshair-<date>`. Each unit has two FFmpeg WGC 2× recording phases with identical encoder and audio settings, bracketed by recorder-OFF baselines. One unit takes 4 minutes 3 seconds before offline audit. Resume from its own ledger for the reversed order once the first unit qualifies.

Follow the game's overlay rules. During ON, observe the REPLAYX crosshair over active CS2 and confirm the game retains input focus. During OFF, confirm the same overlay is absent. The isolated benchmark uses the actual overlay window and checks Windows capture exclusion. Check the reticle location frame by frame in the ON replays to confirm the overlay is absent there. Hash-bound review requires both `crosshair_visible_in_game_review_passed` and `crosshair_exclusion_review_passed` for each ON clip. If the display mode hides the overlay, the ON phase remains unqualified.

Compare average FPS, 1% lows, P99, CPU/RSS, GPU engines and PresentMon presentation mode under ON and OFF. A presentation-mode change belongs in the report even if average FPS is similar.

For either comparison, save manual observations in a separate JSON array. Each entry contains the clip's SHA-256 from the ledger, a reviewer name and the review flags. Run the offline summary with `py scripts/bench/cs2_ab.py --analyze-only --out <session-folder> --reviews <review.json>`. Leave a flag absent when evidence is pending. Use false to reject a failed clip and keep the original measurement.
