# Third-party components

REPLAYX application code is MIT licensed. This does not change the licenses of its dependencies or grant codec patent rights.

| Component | License / source |
| --- | --- |
| Tauri and official plugins | MIT / Apache-2.0, https://github.com/tauri-apps/tauri and https://github.com/tauri-apps/plugins-workspace |
| React | MIT, https://github.com/facebook/react |
| Vite | MIT, https://github.com/vitejs/vite |
| Lucide icons | ISC, https://github.com/lucide-icons/lucide |
| wasapi-rs | MIT, https://github.com/HEnquist/wasapi-rs |
| Microsoft windows-rs | MIT / Apache-2.0, https://github.com/microsoft/windows-rs |
| FFmpeg | LGPL configuration selected; https://ffmpeg.org/legal.html |
| OpenH264 | BSD-2-Clause; https://github.com/cisco/openh264 |

FFmpeg is executed as a separate local process. The development setup selects BtbN's **win64 LGPL shared** build, with the archive digest recorded in `tools/ffmpeg-lock.json`. Never substitute a GPL or nonfree build without updating the licensing and distribution decision. The shared libraries remain replaceable; the application does not prevent replacing them with compatible versions. FFmpeg build details and license output are included by `prepare-bundle.ps1`.

Build recipes and dependency versions: https://github.com/BtbN/FFmpeg-Builds . FFmpeg corresponding source: https://git.ffmpeg.org/ffmpeg.git . The exact FFmpeg revision is in `media/BUILD-CONFIGURATION.txt` after preparation. Cargo.lock and package-lock.json record application dependencies.

**Public release gate:** before redistributing the local test installer, archive the exact corresponding FFmpeg source and all included LGPL dependency source/build recipes, include the relevant upstream notices/license texts, and make the corresponding source available with the release as required by the selected licenses. A link to a moving upstream branch is not a substitute for that obligation. The local installer produced during development is not a declaration that this redistribution audit is complete. Review every DLL included in `src-tauri/media` against the upstream build recipes. Use an immutable download/source mirror for durable public releases.

NVENC, AMF, and Quick Sync are interfaces to vendor drivers; those drivers and hardware are not described as open source. Hardware availability and codec patent licensing are separate issues. H.264/AAC encoding may raise patent obligations depending on jurisdiction and distribution. A third-party build of OpenH264 does not automatically carry Cisco's binary-distribution patent coverage. HEVC/AV1 are not offered by this release.

Development metadata inventories for this hardening pass are recorded in `test-output/validation-0.1.1-alpha.1/npm-licenses.json` and `rust-licenses.json` (133 npm entries; 307 resolved Windows Rust packages). These are license metadata, not a complete legal audit or the corresponding-source package. They are local validation artifacts and are not automatically published.
