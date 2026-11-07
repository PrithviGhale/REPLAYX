# Security

Do not publish sensitive recordings, session settings, or raw FFmpeg diagnostic tails in an issue. Before a public release, enable GitHub private vulnerability reporting under Settings → Code security. Until a repository is created, do not invent a maintainer email or disclosure destination.

Threat model: an ordinary local desktop application running without administrator privileges. No remotely accessible server or arbitrary shell command endpoint. The Tauri window has a restrictive CSP and no general filesystem/shell plugin permissions. Native commands validate clip filenames and canonicalize paths to the recording directory. FFmpeg arguments are passed individually, never interpolated into a command shell. Replay concat manifests contain generated local filenames.

Update URLs must use the configured GitHub HTTPS endpoint. The supported Tauri updater verifies signatures with a compile-time public key. Private keys must never be committed. Installation is blocked while capture or save verification is active. Build/signature integration is present, but real release update verification must be tested with publisher-owned keys before distribution.

Remaining alpha hardening: authenticated named-pipe audio transport, sandboxing untrusted media decoders, exhaustive junction/reparse-point races, exhaustive crash/instance-lock validation, and signed Windows binaries. Do not import untrusted files into the recording directory. Windows code signing and Tauri update signing are different mechanisms.

The app now holds an exclusive per-data-directory lock and assigns the capture child to a kill-on-close Windows job. Geometry-change media is quarantined outside ordinary recovery. These mitigations do not establish protection against malicious software already running as the same user. Media-runtime security alerts must be reviewed without disabling Windows protection.
