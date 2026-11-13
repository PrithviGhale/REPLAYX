# Troubleshooting

| Problem | What to try |
| --- | --- |
| My microphone is silent | Check **Windows Settings → Privacy & security → Microphone**, pick the right device in **Audio**, and use **Test mic**. If you use Voicemod or another virtual microphone, choose your real headset instead; otherwise REPLAYX processes already-processed audio. |
| My voice sounds too processed | Choose **Natural**, or lower noise suppression to **Light**; compare with **Test mic → Original**. |
| Games lose FPS while recording | Use 60 fps and the **Performance** or **Balanced** preset. The **Performance** page shows whether the encoder falls behind. |
| A recording looks choppy | Check **Performance → Dropped frames**. On laptops where the NVIDIA GPU drives the screen, keep the default **Smooth** scaling. |
| A shortcut does nothing | Another program may own it (NVIDIA's overlay uses Alt+F10). Pick another in **Settings → Hotkeys**; REPLAYX shows which shortcuts it couldn't register. |
| I can't find REPLAYX after closing it | It's in the tray: click the **^** next to the clock, or press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F11</kbd>. |
| The overlay doesn't show in a game | Windows can't draw desktop windows over exclusive full-screen games. Use borderless or windowed mode. |
| A game isn't recognised | Add a correction in **Settings → Game detection → Your corrections**. |
| A save failed | The footage stays on disk. Open **Instant Replay** to recover interrupted sessions, and check free disk space. |
| SmartScreen blocks the installer | Builds aren't Windows code-signed yet. Compare the SHA-256 with the release's `SHA256SUMS.txt`, then review the warning with security protections enabled. |
| Something else | Check the [known limitations](KNOWN_LIMITATIONS.md), or [open an issue](https://github.com/PrithviGhale/REPLAYX/issues/new/choose) with your Windows version, GPU and what you did. |
