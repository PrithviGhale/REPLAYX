# Privacy

REPLAYX has no accounts, telemetry, advertisements, analytics, remote control, or clip uploads. Recording begins only through an explicit button, tray action, or configured shortcut. The optional *Start with Windows* setting opens REPLAYX in the tray at sign-in (a per-user `Run` registry value); it never starts capture. Closing the window stops capture unless you explicitly enable close-to-tray.

Settings are local in `%LOCALAPPDATA%/io.replayx.desktop/settings.json`. The same folder holds cached clip thumbnails (`thumbnails/`, at most 400 files) and the latest device test recording (`tests/audio-test.wav`, overwritten by the next test). Application icons are read from executables on this PC to show in the mixer and are not stored or sent anywhere. Deleted clips go to the Windows Recycle Bin. The same directory contains encoded replay segments and pinned recordings/saves. Default saved clips are in `%USERPROFILE%/Videos/REPLAYX`. `REPLAYX_DATA_DIR` overrides the application directory for isolated testing.

Audio crosses a random ephemeral **127.0.0.1-only** TCP connection between the WASAPI producer and FFmpeg. No external network interface is bound. This is local IPC, not an upload. The alpha does not defend against other malicious software already running as your Windows user racing a local IPC connection. A named pipe with explicit ACL and peer authentication is a future hardening task.

The app holds a bounded FFmpeg diagnostic tail in memory. It can include device labels, selected process labels, and the local segment directory. It is not uploaded or automatically written to a log. Optional persistent diagnostic logging is not implemented. `session.json` stores the local settings needed to identify interrupted sessions. Replay data and interrupted recordings persist until retention or explicit recovery processes them; users can remove their own application-data directory after exiting REPLAYX.

Update checks are disabled by default. A published build can enable user-initiated or opted-in startup checks against its compile-time HTTPS GitHub Releases endpoint. Those requests expose ordinary connection information to GitHub. No screen or audio data is sent. The development build has no configured update endpoint.

Protected content, secure desktop capture, anti-cheat bypass, and DRM bypass are not supported. App-audio exclusion only excludes audio from the specified process tree; it does not hide that app's pixels from monitor capture.
