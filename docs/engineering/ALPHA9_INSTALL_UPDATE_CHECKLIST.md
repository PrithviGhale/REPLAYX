# Alpha.9 install and update test checklist

For the owner, before approving publication.

**Candidate** (built from commit `cf92d5a`):

- Installer:
  `src-tauri\target-alpha9\release\bundle\nsis\REPLAYX_0.3.1-alpha.9_x64-setup.exe`
- Size: 61,393,529 bytes
- SHA-256: `f937078924c85ec0a876cb3d0e6f23916431702dd1bacc273ae7f7809749e173`

Older alpha.9 builds are in `target-alpha9\superseded-051f935` and
`target-alpha9\superseded-2e52a53`. Don't test or publish them.

**Important:**

- The REPLAYX installed on this PC is **0.3.1-alpha.4**
  (`%LOCALAPPDATA%\REPLAYX`), not alpha.8. Alpha.8 was never installed, so
  "update from alpha.8" means installing alpha.8 first.
- The installer is per-user. Your settings, clips and playtime are in
  `%LOCALAPPDATA%\io.replayx.desktop`, and installing affects only the Windows
  account that runs it.
- To keep your own install and data untouched, do both tests in a **second
  Windows user account** (Settings → Accounts → Other users → Add account →
  "I don't have this person's sign-in information" → local account). Windows
  11 Home has no Windows Sandbox.

## A. Clean installation (second account, nothing installed)

1. Copy `REPLAYX_0.3.1-alpha.9_x64-setup.exe` there. Check its SHA-256
   with `Get-FileHash`; it must equal the hash above.
2. Run it. SmartScreen will warn, because the installer isn't
   Authenticode-signed (expected). Choose *More info → Run anyway*.
3. REPLAYX opens. *Settings → About* shows **0.3.1-alpha.9**.
4. First start: there is no settings notice, and the tour appears once.
5. Turn on Instant Replay, wait 30 s, then press *Save replay*. A clip
   appears in *Clips* and plays with sound.
6. Record 30 s, stop it, and check that the recording plays.
7. Start a game (or any app listed on *Games*). One "Game detected" notice
   appears. Alt+Tab a few times: no repeat. Close the game and relaunch it
   at once: one new notice.
8. Close to the tray, reopen from the tray, then quit from the tray.
9. Uninstall from *Settings → Apps*. If it offers to delete application
   data, leave that unchecked the first time. The REPLAYX program folder is
   removed, and clips in `Videos\REPLAYX` must remain.

## B. Update from alpha.8 (second account)

1. Install `src-tauri\target\release\bundle\nsis\REPLAYX_0.3.1-alpha.8_x64-setup.exe`.
2. In alpha.8, change some settings (FPS, a shortcut, the clips folder),
   save a replay, and note your playtime.
3. Quit REPLAYX from the tray.
4. Run the alpha.9 installer over it (the same path the updater uses).
5. Check:
   - *About* shows 0.3.1-alpha.9.
   - The settings from step 2 are kept, with no settings notice.
   - The clip and the playtime are kept.
   - Shortcuts still work.
6. Repeat steps A.5–A.7.
7. **In-app updater:** release builds only update from GitHub, so this can
   only be tested after alpha.9 is published, ideally as a **pre-release**
   first. Then:
   1. In the alpha.8 account, run *Settings → Updates → Check now*. It
      offers 0.3.1-alpha.9.
   2. *Update Now* downloads it with progress, verifies it and installs.
   3. REPLAYX restarts on 0.3.1-alpha.9 with the same settings and clips.
   4. *Not Now* hides the offer, and it isn't offered again for three days.

## C. Optional: settings recovery (second account only)

1. Quit REPLAYX.
2. Edit `%LOCALAPPDATA%\io.replayx.desktop\settings.json` and delete its last
   `}`.
3. Start REPLAYX.
4. Check:
   - The notice reads "Your settings were reset" and names a
     `settings.unreadable-….json` file.
   - That file holds the broken text.
   - Changing a setting works.

Never do this test in your own account.

## Not covered here

- Physical Bluetooth headset switching.
- Multi-monitor notice placement.
- Games with strong anti-cheat.
- Long recording or soak runs.

Each needs your go-ahead, because they use the screen.
