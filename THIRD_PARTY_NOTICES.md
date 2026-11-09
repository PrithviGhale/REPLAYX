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
| FFmpeg n9.0.2-47-gb204e94e90 | **LGPL-3.0-or-later** (built with `--enable-version3`; no `--enable-gpl`/`--enable-nonfree`), https://ffmpeg.org/legal.html |
| OpenH264 | BSD-2-Clause; https://github.com/cisco/openh264 |
| nnnoiseless 0.5.2 (Rust port of RNNoise, including its trained model weights) | BSD-3-Clause, https://github.com/jneem/nnnoiseless |
| RNNoise (Jean-Marc Valin, Xiph.Org / Mozilla), the algorithm and weights nnnoiseless ports | BSD-3-Clause, https://github.com/xiph/rnnoise |
| rustfft, realfft, num-complex, transpose, strength_reduce, primal-check (nnnoiseless dependencies) | MIT or MIT / Apache-2.0 |

FFmpeg is executed as a separate local process. The setup selects the FFmpeg-Builds (formerly BtbN) **win64 LGPL shared** build from the immutable dated release `autobuild-2026-11-06-12-50`; URL, version, size and SHA-256 are recorded in `tools/ffmpeg-lock.json`. Because the build enables `version3`, the binaries are LGPL **v3** or later, not v2.1. That is compatible with distributing the MIT-licensed REPLAYX as a separate program that runs FFmpeg as a child process. Never substitute a GPL or nonfree build without updating the licensing and distribution decision. The shared libraries remain replaceable; the application does not prevent replacing them with compatible versions. FFmpeg build details and license output are included by `prepare-bundle.ps1`.

Build recipes and dependency versions: https://github.com/FFmpeg-Builds/FFmpeg-Builds (tag `autobuild-2026-11-06-12-50`). FFmpeg corresponding source: commit `b204e94e90` at https://git.ffmpeg.org/ffmpeg.git . The full configure line is in `media/BUILD-CONFIGURATION.txt` after preparation. It enables many LGPL/BSD/Apache-licensed libraries (for example dav1d, libvpx, libaom, SVT-AV1, opus, opencore-amr, OpenH264, libplacebo, zimg); each library's own notice must accompany a public release. Cargo.lock and package-lock.json record application dependencies.

**Public release gate:** before redistributing the local test installer, archive the exact corresponding FFmpeg source and all included LGPL dependency source/build recipes, include the relevant upstream notices/license texts, and make the corresponding source available with the release as required by the selected licenses. A link to a moving upstream branch is not a substitute for that obligation. The local installer produced during development is not a declaration that this redistribution audit is complete. Review every DLL included in `src-tauri/media` against the upstream build recipes. Use an immutable download/source mirror for durable public releases.

NVENC, AMF, and Quick Sync are interfaces to vendor drivers; those drivers and hardware are not described as open source. Hardware availability and codec patent licensing are separate issues. H.264/AAC encoding may raise patent obligations depending on jurisdiction and distribution. A third-party build of OpenH264 does not automatically carry Cisco's binary-distribution patent coverage. HEVC and AV1 can be chosen explicitly and are encoded by the GPU vendor's encoder; HEVC in particular may carry patent obligations depending on jurisdiction. The default codec is H.264.

Development metadata inventories for this hardening pass are recorded in `test-output/validation-0.1.1-alpha.1/npm-licenses.json` and `rust-licenses.json` (133 npm entries; 307 resolved Windows Rust packages). These are license metadata, not a complete legal audit or the corresponding-source package. They are local validation artifacts and are not automatically published.

## BSD-3-Clause licence text (RNNoise, nnnoiseless)

Copyright (c) 2017, Mozilla; Copyright (c) 2007-2017, Jean-Marc Valin; Copyright (c) 2005-2017, Xiph.Org Foundation; Copyright (c) 2003-2004, Mark Borgerding; Copyright (c) 2020, Joe Neeman (nnnoiseless).

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

- Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
- Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
- Neither the name of the Xiph.Org Foundation nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
