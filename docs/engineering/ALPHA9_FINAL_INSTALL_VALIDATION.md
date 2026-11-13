# Alpha.9 final installation validation

November 13, 2026. Application fix commit `cf92d5a` is present. HEAD is
`219efba`, which adds the prior installation checklist. The working tree was
clean before this inspection.

**Current verdict — BLOCKED by account-management preflight and the absence of
a verified isolated login.** The owner subsequently approved a temporary standard
account and the installation/upgrade/uninstall/capture tests. Execution stopped
at the account-management restriction documented below. Application source was
not changed during this inspection.

## Candidate identity

- Path `src-tauri/target-alpha9/release/bundle/nsis/REPLAYX_0.3.1-alpha.9_x64-setup.exe`
- Size 61,393,529 bytes.
- Independently computed SHA-256
  `f937078924c85ec0a876cb3d0e6f23916431702dd1bacc273ae7f7809749e173`.
- Detached updater signature verifies against REPLAYX's public key for exactly
  `0.3.1-alpha.9`. This is separate from Windows Authenticode signing.
- Alpha.8 installer is available locally under
  `src-tauri/target/release/bundle/nsis/REPLAYX_0.3.1-alpha.8_x64-setup.exe`.
  Its hash remains
  `1f420d0729454393cf17d7229d642eccd2894029bf513d91cec2e3aa5c7f8186`.
- The existing owner executable reports Alpha.4 in both FileVersion and
  ProductVersion. No copy was launched or changed. The original Alpha.4 installer
  was subsequently downloaded from the repository's public release into
  `test-output/alpha9-final-install-validation/REPLAYX_0.3.1-alpha.4_x64-setup.exe`.
  Its 61,134,250-byte size and SHA-256
  `f1e218d14eb3eb0e4b9f1d4445dda3462a1d74506a7324f443a61c5e0b7da206`
  match GitHub's release metadata. The downloaded file was not executed.

## Installer and data isolation audit

The actual generated `src-tauri/target-alpha9/release/nsis/x64/installer.nsi`
defines `INSTALLMODE=currentUser` and `RequestExecutionLevel user`. Its
`utils.nsh` sets current-user shell context and 64-bit registry view.

| Resource | Current-user behavior |
| --- | --- |
| Default installation | `%LOCALAPPDATA%\REPLAYX` |
| Previous installation discovery | Restores the path from `HKCU\Software\replayx\REPLAYX` |
| Uninstall registration | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\REPLAYX` |
| Application settings, library/playtime, replay/recovery/update state | `%LOCALAPPDATA%\io.replayx.desktop` |
| Default recordings | `%USERPROFILE%\Videos\REPLAYX`, or the configured output directory |
| Shortcuts | Current user's Desktop and Start Menu |
| Autostart | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `REPLAYX` |
| Optional uninstall data removal | Recursively removes both `%APPDATA%\io.replayx.desktop` and `%LOCALAPPDATA%\io.replayx.desktop` when selected and outside update mode |

Data-location conclusions come from `desktop.rs`'s app-local-data lookup and
`model.rs` defaults. WebView profile data can also occupy the per-user app-data
locations. `REPLAYX_DATA_DIR` changes engine storage but does not independently
isolate the installer registration, autostart, single-instance identity or all
WebView storage. A separate installation directory in the owner's account is
therefore insufficient for these tests.

A separately signed-in standard Windows account provides a distinct profile,
HKCU hive, app-data paths and default recordings folder. The single-instance
plugin creates an unprefixed, identifier-based mutex. Its default scope is the
Windows login session. Use a separate login session, rather than running a second
identity inside the owner's desktop session. Microsoft documents that default
kernel-object namespace behavior.
[Kernel object namespaces](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces)

This is profile isolation on shared hardware. WebView2 can be shared
machine-wide. The installer checks HKLM then HKCU for WebView2 and can invoke its
bootstrapper if missing. Microsoft notes that even a non-elevated per-user
runtime install may become per-machine when the machine Edge updater exists.
Require an already-present runtime for second-account tests, or move missing-
runtime qualification to a disposable VM with separately approved provisioning.
[WebView2 deployment](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution#installing-the-runtime-as-per-machine-or-per-user)

The NSIS script also searches HKLM for matching older WiX installations and can
offer migration/uninstallation. Any such machine-wide detection is a stop
condition in a second-account test. Restart Manager targets the exact installed
executable path. Silent/passive operation can force-close processes using that
file, so initial validation should use the interactive installer and explicitly
quit only the disposable profile's test app first.

Uninstall deletes the bundled program/runtime files and this user's registration.
Ordinary uninstall removes the HKCU autostart value. With application-data
removal unchecked, the generated script does not recursively remove app data
or the configured external recordings folder. If recordings were deliberately
placed beneath a selected application-data deletion target, selecting that
checkbox would delete them too. Test preservation using disposable recordings
and leave data removal unchecked. Actual retention still requires execution.

## Update behavior

Alpha.9 defaults to automatic checking on, automatic download off and automatic
installation off. The update loop first checks after about 20 seconds. Existing
saved update preferences are retained, so an upgraded test profile with prior
auto-install opt-in needs separate treatment. Initial installer/data migration
tests should run with networking disconnected inside the disposable environment,
without changing the owner's network or security settings. Enable networking
later for a separately approved updater test.

Alpha.8/Alpha.9 discovery selects the highest compatible published alpha release
from GitHub's release listing, requires coherent prerelease labels and a
`latest.json` asset, then verifies the signed version and payload. A draft is
excluded. Publishing Alpha.9 as a prerelease exposes it to eligible alpha clients
that check this channel. It is not a private canary test.

Alpha.4 uses an older compile-time `REPLAYX_UPDATE_URL` and
`REPLAYX_UPDATE_PUBKEY` mechanism. Its tag has empty updater configuration by
default. Whether the distributed Alpha.4 binary had those build-time values
requires verification in a disposable Alpha.4 installation. Do not promise that
Alpha.4 will automatically discover the new prerelease.

Release builds ignore the debug localhost endpoint override. The current NSIS
update mode proceeds without ordinary uninstall and suppresses optional app-data
removal. These source checks do not establish a working installed update.

## Prepared execution matrix

Each row starts from a pristine VM snapshot or independent disposable profile.
Only test-owned fixtures and settings may be changed. Do not copy the owner's
actual settings, playtime or recordings into an older version.

| Scenario | Preparation and acceptance |
| --- | --- |
| Clean Alpha.9 | Verify digest and empty profile, record SID/session and resolved paths, run approved interactive installer, inspect actual HKCU changes, verify About and deployed runtime. |
| Alpha.4 → Alpha.9 | Verify original Alpha.4 installer, install in disposable profile, set custom FPS/hotkeys/output folder and create test clips/playtime. Quit it, snapshot/hash fixture settings and clips, install Alpha.9 into that same test installation. Verify migration and preserved values/content. |
| Alpha.8 → Alpha.9 | Repeat the upgrade sequence from the verified local Alpha.8 installer in another pristine test state. |
| Runtime checks for each row | Startup, tour/restore, one manual recording and one Instant Replay save, concurrent saves where supported, system audio and explicitly approved microphone source, clip listing/playback, correct encoder/geometry, no healthy state after a failed capture, and one notice per genuine test-game session through focus changes and immediate relaunch. |
| Uninstall | Quit only the disposable test app, hash test recordings, uninstall with data removal unchecked, confirm binaries/registration/autostart removed and recordings retained byte-for-byte. |
| Owner isolation | Compare the owner's Alpha.4 executable and small-state baseline after testing. Account for any independent writes made by the owner's running app. Check test paths never resolve into the owner profile or shared recording folders. |
| No unexpected updates | Keep initial trials offline within the guest, inspect inherited update preferences and post-install process versions. Run a separate approved online updater case later. |

Stop on SmartScreen/security blocks, an elevation request, machine-wide WiX
migration, unexpected install/profile paths, another account's process selection,
or a missing runtime requiring new installation. No bypass or security-setting
change is part of the test procedure.

Before launch, verify the disposable process has no inherited `REPLAYX_DATA_DIR`
override pointing outside its profile and no shared recordings path. Inspect
the older installer's mode before using it too. Do not assume the Alpha.9 NSIS
audit proves the Alpha.4 or Alpha.8 installer's behavior.

A read-only owner baseline was saved locally under
`test-output/alpha9-final-install-validation/owner-baseline.json`. Four existing
executable/small-state files were hashed successfully. User recording contents
were not read or modified for that baseline.

No suitable disposable VM command was found in the initial tool-path check.
That check does not prove that no VM exists elsewhere. No VM, user account or
Windows feature was created or enabled.

## Current checklist

| Item | Result |
| --- | --- |
| Candidate commit and digest | PASS |
| Exact-version updater signature | PASS |
| Current-user install/data/registry source audit | PASS, with shared-runtime and WiX caveats above |
| Second-account runtime isolation demonstrated | NOT RUN, requires an approved account and separate sign-in |
| Clean Alpha.9 installation | NOT RUN, approval required |
| Alpha.4 → Alpha.9 installer upgrade | NOT RUN, verified original installer prepared, approval required |
| Alpha.8 → Alpha.9 installer upgrade | NOT RUN, approval required |
| Installed startup, recording, replay, audio, clips, notices | NOT RUN, installation/capture approval required |
| Uninstall retention | NOT RUN, approval required |
| Installed updater replacement/restart | NOT RUN, separate approval and suitable release endpoint required |

No runtime test is marked FAIL without execution, and no pending test is counted
as PASS. The pending installation, upgrade, recording and uninstall cases block
a final release-validation PASS.

After publication, the updater can be exercised in a disposable Alpha.8 profile
with approval, verified release metadata and the same signed installer. That
test cannot guarantee zero exposure to other eligible alpha clients because
publication makes the release discoverable. Complete local installer upgrades
first. A later clean online update/restart and retention result is required for
an updater PASS.

## Approved execution attempt — November 13, 2026

The owner authorized creation of a dedicated temporary standard account, clean
installation, Alpha.4 and Alpha.8 installer upgrades, short recording/audio tests
and uninstall tests. Cleanup of the account/results was explicitly withheld.

Candidate identity was rechecked. The Alpha.9 installer still has SHA-256
`f937078924c85ec0a876cb3d0e6f23916431702dd1bacc273ae7f7809749e173`
and its updater signature verifies for the exact version. `cf92d5a` remains in
the current history.

The supported Microsoft.PowerShell.LocalAccounts commands are available.
The executing identity is the owner's standard Windows account with an unelevated token. The first
account-management operation was a read-only collision check using
`Get-LocalUser -ErrorAction Stop` for the proposed `REPLAYX-Validation` name.
It failed with `Object reference not set to an instance of an object`.
`New-LocalUser` was never reached. This is an observed account-management API
failure in this execution context, rather than evidence that the installer
failed. Its underlying cause was not established. No elevation request or
alternate account-creation method was attempted after that failure.

As required by the owner's stop condition, account and installation operations
stopped. No account, credentials file or installer process was created. No
Windows settings or account memberships were changed. No temporary account or
test results were deleted.

The baseline was rechecked after stopping. All five baseline entries match,
including the Alpha.4 executable and the three existing small state files.
The fifth entry remains absent as expected. Evidence is
`test-output/alpha9-final-install-validation/owner-baseline-check-after-preflight.json`.
Recording contents were never opened or modified during this attempt. Other
installation files and recording contents were not comprehensively hashed, so
the byte-comparison claim is limited to the saved baseline entries.

### Execution results

| Requested validation | Result | Evidence or reason |
| --- | --- | --- |
| Latest candidate checksum and exact-version updater signature | PASS | Reverified before account operations |
| Supported temporary account setup | FAIL / environment prerequisite | Account enumeration failed before creation, no administrator token |
| Separate login/session and shared-dependency isolation | BLOCKED | No test login available, shared-runtime preconditions remain unverified there |
| Clean Alpha.9 installation | BLOCKED | Installer not launched |
| Alpha.4 → Alpha.9 upgrade | BLOCKED | Installer not launched |
| Alpha.8 → Alpha.9 upgrade | BLOCKED | Installer not launched |
| Independent clean baseline between scenarios | BLOCKED | No installation scenario started |
| Startup/settings preservation/shutdown in installed candidate | BLOCKED | Requires installed test copy |
| Recording/Instant Replay/audio/clip saving | BLOCKED | No capture performed |
| Error notifications/game detection | BLOCKED | No installed runtime test performed |
| Uninstall and test-recording retention | BLOCKED | No uninstall performed |
| Owner baseline preservation | PASS for the recorded baseline | All five entries match, no owner install/uninstall or recording operation executed |
| No unexpected update or unrelated overwrite during execution | PASS for this attempt only | No test app or installer was started |

The final installation-validation verdict remains **BLOCKED**. This run does
not establish that the candidate is safe to publish as a prerelease. No new
application defect was identified by the failed environment prerequisite.

### Required handoff to continue

A Windows administrator needs to create a clearly labelled standard account
through the supported account-management interface. The owner must handle
credentials and interactive sign-in, then establish the separate test session.
Verify the actual account/SID, profile paths, standard-user token, environment
overrides, existing WebView2 availability and absence of a matching machine-wide
installation before running the first installer. Transfer only the three verified
installer packages and test instructions into that profile. Preserve the owner's
profile and the temporary account/results.

The existing approval still covers the requested test operations. No renewed
permission is needed for that same scope after the environment is available.
Any security block, elevation request or unexpected machine-wide change remains
a stop condition.
