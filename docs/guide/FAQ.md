# FAQ

**Is REPLAYX free?**
Yes. It's open source under the MIT licence, with no account, ads or subscriptions.

**Is it safe to use with anti-cheat games?**
REPLAYX records the screen through Windows (DXGI Desktop Duplication) and audio through WASAPI, the same system interfaces other recorders use. It never injects into games, reads their memory or hooks their graphics. Game detection uses only the name of the program in front.

**Does it slow down my games?**
The newer detailed synthetic workload generally measured 5–8% at 1080p60 on this RTX 4070 laptop. Fortnite screenshot averages suggest roughly 13%, but sustained game measurements are still needed. See [Performance](../development/PERFORMANCE.md) and [the A/B/A procedure](../engineering/FORTNITE_ABA.md).

**Where are my clips?**
In `Videos\REPLAYX\<Game>\` by default; change it in **Settings**. Clips are `.mkv` files, which survive crashes and keep every audio track. Export any clip to MP4 from the library.

**Does anything leave my PC?**
No. There's no telemetry, analytics or upload. Playtime, settings and clips stay local. See [Privacy](PRIVACY.md).

**Why is it called alpha?**
REPLAYX works well on the machines it has been tested on, but it hasn't been used on many PCs yet. Read the [known limitations](KNOWN_LIMITATIONS.md) before relying on it for something you can't replay.

**Why does Windows warn about the installer?**
The builds aren't Windows code-signed yet. Check the file against the release's `SHA256SUMS.txt`. If Windows blocks it, stop and review the warning with security protections enabled.

**What is the large `ffmpeg-corresponding-source` file on the release page?**
The source code of the FFmpeg build REPLAYX ships, with the libraries built into it and their licence texts. Its licences require it to be published with each release. You don't need it to install or use REPLAYX.

**Which GPUs are supported?**
NVIDIA (NVENC), Intel (Quick Sync) and AMD (AMF) hardware encoders, with a CPU fallback. Only codecs your PC can actually encode are offered.

**How do I update REPLAYX?**
From the first signed release on, REPLAYX finds new versions itself (a dot on Settings and a pop-up) and installs them when you click **Update Now**; **Settings → Updates** has *Check Now* and the automatic options. Version 0.3.1-alpha.6 and older have to be updated once by running the newer installer. Either way your clips and settings are kept.

**Can I turn my microphone off quickly?**
Yes: the switch next to *Microphone* on the home page, the microphone line in the tray menu, or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd>. It works during a recording and doesn't affect game or chat audio.

**How do I quit REPLAYX completely?**
Right-click the tray icon → **Quit**.

**How do I stop it from starting with Windows?**
Turn off **Settings → Start with Windows**. REPLAYX removes its startup entry right away.
