# Security policy

## Reporting a vulnerability

Please report security problems **privately** through GitHub: open the repository's **Security** tab and choose **Report a vulnerability** ([private vulnerability reporting](https://github.com/PrithviGhale/REPLAYX/security/advisories/new)). Do not open a public issue for a vulnerability.

Include the REPLAYX version (Settings → About), your Windows version and the steps to reproduce. Never attach recordings, settings files or FFmpeg logs that contain other people's voices, chats or account details.

REPLAYX is maintained by volunteers, so there is no guaranteed response time. Fixes are released as new alpha builds; only the latest release receives fixes.

## Supported versions

| Version | Supported |
| --- | --- |
| Latest 0.2.x alpha release | Yes |
| Older alpha builds | No: please update |

## Threat model

REPLAYX is an ordinary local desktop application that runs without administrator rights. It has no server, opens no network ports to other machines, and sends nothing anywhere.

- The Tauri window has a restrictive content security policy and no general filesystem or shell permissions. Native commands validate clip names and resolve paths inside the recording folder before any file operation; deletion goes to the Recycle Bin.
- FFmpeg arguments are passed as separate arguments, never through a shell. Concat manifests contain only names REPLAYX generated.
- Audio reaches the FFmpeg child over a loopback (127.0.0.1) TCP socket. Software already running as the same Windows user could interfere with it. That is outside this threat model, like any other same-user process.
- The capture child runs in a kill-on-close Windows job, and each data directory is locked to one REPLAYX instance.
- Game detection reads only the foreground process name and window title. It never injects into, reads the memory of, or modifies games, and it does not interact with anti-cheat software.

## Updates and signing

- Release binaries are **not Authenticode-signed** yet, so Windows SmartScreen may warn.
- In-app updates use the Tauri updater, which verifies signatures against a public key compiled into the app. Update signing keys are never committed to this repository.
- Automatic installation stays disabled until a signing key is configured and a real upgrade has been tested. Until then, update by downloading the installer from the Releases page.

## Known hardening gaps (alpha)

- Unauthenticated loopback audio transport (see above).
- No sandbox around FFmpeg's decoders. Do not put untrusted media files into the recording folder.
- No code signing.
