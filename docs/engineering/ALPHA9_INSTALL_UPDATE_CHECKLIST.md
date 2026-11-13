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
- The installer uses current-user mode. Application data defaults to
  `%LOCALAPPDATA%\io.replayx.desktop`. Recordings default to
  `%USERPROFILE%\Videos\REPLAYX`, with a configurable output folder.
- Use an approved disposable VM or a **separately signed-in standard Windows
  account** with its own profile and recordings folder. Account creation and
  installation require the owner's approval. A second account shares drivers,
  hardware and potentially the WebView2 runtime. A different installation folder
  in the owner's account still shares application data and registry registration.
- A VM snapshot or independent pristine profile is needed for each scenario
  below. Include Alpha.4 → Alpha.9 as well as Alpha.8 → Alpha.9. Obtain and verify
  the original Alpha.4 installer before claiming that installer upgrade passed.

## A. Clean installation (second account, nothing installed)

1. Copy `REPLAYX_0.3.1-alpha.9_x64-setup.exe` there. Check its SHA-256
   with `Get-FileHash`; it must equal the hash above.
2. After approval, run it as the disposable standard user. If SmartScreen,
   a security block or an elevation request appears, stop and record the result.
   Do not bypass the block or change Windows security settings.
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
   be qualified against the published artifact after alpha.9 is published as a
   correctly labelled **pre-release**. Publication exposes it to eligible alpha
   clients. Keep the owner's Alpha.4 installation out of this test. Alpha.4's
   older updater depends on build-time endpoint/key configuration and must be
   assessed independently. Then, in the disposable Alpha.8 profile only:
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

## Current validation status

See [ALPHA9_FINAL_INSTALL_VALIDATION.md](ALPHA9_FINAL_INSTALL_VALIDATION.md)
for the installer isolation audit, approvals, exact candidate identity and
the pending clean/Alpha.4/Alpha.8 installation and uninstall results.
