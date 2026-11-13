# Fortnite A/B/A test

This is manual. REPLAYX does not launch Fortnite, change its settings, inject
code, modify anti-cheat or join matches. Screenshot averages are preliminary;
the reported 346 / 300.3 / 349.3 / 274 FPS samples do not establish sustained
overhead or an improvement over the earlier build.

1. Open the local build. Set H.264 NVENC, 1920x1080, 60 FPS, Balanced, Voice
   Enhancer off. Preserve your normal audio layout. Save diagnostics from
   Performance, and record actual encoder, source geometry, scaler and GPU.
2. Use the same Creative scene, graphics settings, camera/route and power mode.
   Warm up for two minutes. Keep NVIDIA replay and Discord off initially.
   Log frame times with an existing PresentMon/FrameView installation, targeting
   only Fortnite's process. Use the same logging method in all three phases.
3. A1: REPLAYX replay off, log 60 seconds. B: enable REPLAYX replay, wait five
   seconds, log the same 60-second scene. A2: stop replay, wait five seconds,
   log the same scene again. Repeat the full sequence three times. Save a clip
   during a separate run and inspect motion and audio; save activity should not
   contaminate the basic overhead comparison.
4. Export each phase to its own CSV. Compare using:

   ```powershell
   py -3 scripts/bench/compare-frametimes.py A1.csv B.csv A2.csv --process FortniteClient-Win64-Shipping.exe --cap 360
   ```

   The script reads files only. It chooses the dominant swapchain for that
   process, drops initial warmup, and reports sustained present FPS, 1% lows,
   p99 frame time, control drift and a conservative frame-cap warning. It does
   not measure displayed FPS or attribute GPU kernel costs. Frame generation
   should be disabled manually for this comparison.
5. Results within 5% of the 360 FPS cap are flagged: a ceiling can conceal
   overhead, especially when comparing NVIDIA's replay with capture off.
   If you want uncapped comparison data, change the cap yourself and keep it
   identical for every phase. Do not compare capped and uncapped runs. Reject
   >5% A1/A2 drift and repeat after temperatures/clocks settle.
6. Repeat with NVIDIA replay alone at comparable resolution/FPS/codec/quality,
   then both recorders. Separately repeat with Discord streaming alone and with
   REPLAYX. Confirm uninterrupted sharing, saved clip motion and sync. Do not
   conclude compatibility from synthetic second-encoder tests alone.

For 1280x960 and 1440x1080 fullscreen, test Windows resolution changes and
NVIDIA Control Panel scaling separately. Record GPU/display scaling selection,
refresh rate, game mode, capture geometry and actual output/sample aspect.
Inspect all edges, stretch, offsets, transitions and editor playback. The
smooth path can encode native pixels with sample-aspect metadata; exact output
may use a slower transfer path. Physical fullscreen transitions remain unverified.

Share the Performance > Capture diagnostics report and the CSV summary.
The report omits paths, device IDs, game titles and raw error text. Source FPS,
video queue depth, per-frame capture/conversion/encode latency and live GPU load
remain unmeasured; null does not mean zero.

CSV definitions: [PresentMon console documentation](https://github.com/GameTechDev/PresentMon/blob/main/README-ConsoleApplication.md).
