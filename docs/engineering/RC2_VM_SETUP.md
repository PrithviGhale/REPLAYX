# Disposable Windows VM for REPLAYX rc.2 qualification

| Host check | Observed |
| --- | --- |
| Laptop | ASUS ROG Zephyrus M16, i9-13900H, 20 logical processors, 31.6 GiB RAM |
| Windows | Windows 11 Home x64, edition ID Core, build 26300.9974 |
| Hyper-V | Hypervisor reported active. The Hyper-V VM role is unavailable on this edition, and its management provider, tools and service were absent. |
| VMware and VirtualBox | No local installation or running guest found. |
| Free space | C: 133.3 GiB, D: 402.7 GiB at inspection time |
| Installer qualification | BLOCKED pending a guest. All four installer versions and payload hashes passed static verification. |

Use Oracle VirtualBox after approval. Oracle documents possible performance degradation when the Windows hypervisor is active. Keep current host security and virtualization settings in force, and stop if the guest cannot boot with those settings. Use VM results for installer and data preservation qualification. Use a physical session for encoder and game performance. [Microsoft Hyper-V requirements](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/get-started/Install-Hyper-V) · [Oracle VirtualBox compatibility](https://docs.oracle.com/en/virtualization/virtualbox/7.2/user/Troubleshooting.html)

## Provision the guest after approval

1. Install Oracle VirtualBox for Windows hosts from Oracle. This installs host virtualization components and may request administrator approval or a restart. Use the official Windows 11 x64 ISO and an edition licensed for this test VM. Download the Microsoft WebView2 Evergreen Standalone Installer for x64 if the guest lacks WebView2. [Oracle installation guide](https://docs.oracle.com/en/virtualization/virtualbox/7.2/user/installation.html) · [Windows 11 x64 ISO](https://www.microsoft.com/en-us/software-download/windows11) · [Microsoft WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
2. Create **REPLAYX-QA-Win11** in VirtualBox Manager under `D:\REPLAYX-QA-VM`. Select Windows 11 x64, 4 virtual CPUs, 8 GiB RAM and a dynamically allocated 100 GiB VDI. Enable EFI, Secure Boot and TPM 2.0. Attach the Windows ISO as a virtual DVD and use normal Windows setup on the new virtual disk. [Windows 11 VM requirements](https://learn.microsoft.com/en-us/windows/whats-new/windows-11-requirements) · [Oracle VM settings](https://docs.oracle.com/en/virtualization/virtualbox/7.2/user/working-with-vms.html)
3. Keep host folders and disks, clipboard sharing, drag-and-drop, USB passthrough and microphone redirection disabled. Use NAT only for initial guest setup or WebView2 provisioning if required, with no port forwarding. Verify **Microsoft Edge WebView2 Runtime** in the guest and use a dedicated standard guest account for the tests. Before any REPLAYX setup, power off the guest and set its adapter to **Not Attached**. Verify that no host storage is mapped.
4. In **Settings → Storage**, attach `test-output/rc2-vm-qualification/REPLAYX-0.4.0-rc.2-VM-QA.iso` as a virtual DVD. The image contains only four checked installers and their manifest. Verify the manifest from inside the guest. Image SHA-256 is `FD8E095116E1B709FBFEA3D4F1951133EAE93125F82D2D7735DC5A57C15BC305`.
5. Take a powered-off snapshot called **Clean Windows 11 + WebView2**. Restore it before each scenario, and save a separate result snapshot before the restore so test clips and evidence remain available. Keep the VM and its results until cleanup is approved. [Oracle snapshot instructions](https://docs.oracle.com/en/virtualization/virtualbox/7.2/user/working-with-vms.html#working-with-vms-snapshots)

## Acceptance cases

| Case | Start from the clean snapshot |
| --- | --- |
| Clean install | Run `Candidate-setup.exe` |
| Alpha.4 upgrade | Run `Alpha4-setup.exe`, create guest fixtures, then run `Candidate-setup.exe` |
| Alpha.9 upgrade | Run `Alpha9-setup.exe`, create guest fixtures, then run `Candidate-setup.exe` |
| rc.1 upgrade | Run `RC1-setup.exe`, create guest fixtures including a Crosshair preset, then run `Candidate-setup.exe` |

Verify startup and version, one per-user entry in Installed Apps, the bundled worker and FFmpeg runtime, and clean tray exit. Before each upgrade, record replay length, a custom hotkey, the guest-local recording path, and the SHA-256 of a playable test clip. After upgrading, recheck the setting values, clip hashes and playback. Keep the remove-app-data option clear throughout. After uninstall, verify that registration and shortcuts were removed and that clip hashes still match. If guest capture is unsupported, report it as blocked and use a playable fixture to test file preservation. Stop and record any SmartScreen prompt or unexpected elevation without a bypass. [Installer qualification details](INSTALLER_QUALIFICATION.md)

A signed updater round trip remains a separate qualification.
