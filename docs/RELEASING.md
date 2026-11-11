# Releasing REPLAYX

Releases are cut by hand from `main`. GitHub Actions builds them, but nothing is published automatically: the workflow creates a **draft prerelease** for a maintainer to check and publish.

## Versioning

[Semantic versioning](https://semver.org/) with pre-release labels:

| Stage | Example | Meaning |
| --- | --- | --- |
| Alpha | `0.2.1-alpha.3` | Feature-incomplete or not yet validated on enough hardware |
| Beta | `0.3.0-beta.1` | Feature-complete for the line; bug fixing |
| Stable | `1.0.0` | Supported release; settings and file layout are stable |

The README's download button and install step name the newest release's installer directly (`.../releases/download/vX.Y.Z/REPLAYX_X.Y.Z_x64-setup.exe`). Update them when a release is **published**, not before. (GitHub's "Latest" release and `/releases/latest` ignore releases marked *pre-release*; REPLAYX releases are published as regular releases titled "— Alpha", so they do appear as Latest.)

The version lives in six places that must match: `package.json`, `package-lock.json` (twice), `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`, plus `VERSION` in `src/api.ts`. The release workflow refuses a tag that does not match `package.json`.

## Checklist

1. **Code.** `main` contains the release; `scripts/check.ps1` passes locally and CI is green.
2. **Hardware.** Run the checklist in [VALIDATION.md](VALIDATION.md) on real hardware: record, replay save, audio playback, repeated Record/Stop, Delete, and the game-FPS comparison. Record the results and every untested item in `VALIDATION.md` and `KNOWN_LIMITATIONS.md`.
3. **Notes.** Add a section to `CHANGELOG.md`. Describe only measured improvements.
4. **Tag.** `git tag -a v0.2.1-alpha.3 -m "REPLAYX 0.2.1-alpha.3"` and `git push origin v0.2.1-alpha.3`.
5. **Build.** The *Release (draft)* workflow builds the NSIS installer with the pinned FFmpeg runtime, runs all checks, builds the FFmpeg corresponding-source package (`scripts/ffmpeg-source/build-package.ps1`), writes `SHA256SUMS.txt` for both and creates a **draft** release titled `REPLAYX vX.Y.Z — Alpha` (or `— Beta`). It is not marked *pre-release*, so once published GitHub shows it as the **Latest** release in the repository sidebar; the title and version keep its alpha/beta status clear. It refuses to run if a release for the tag already exists, so a published release is never rebuilt or overwritten; to rebuild a draft, delete the draft first. Before tagging, you can run the workflow by hand **without a tag** on any branch: that is a test build whose installer and source report are kept as workflow artifacts, and it creates no release.
6. **Verify the artifact.** Download the installer from the draft, compare its SHA-256, install it on a test machine, launch it, record, save a replay, play the clip, uninstall. Scan it with Windows Security.
7. **Publish.** Check the draft notes (download link, requirements, known limitations) and publish. Leave *Set as a pre-release* unticked so the release shows as Latest; the title already says Alpha or Beta.

## FFmpeg and licences

The installer ships an LGPL FFmpeg build pinned in `tools/ffmpeg-lock.json` (immutable release tag and SHA-256). Its licences (and those of the LGPL/MPL libraries built into it) require the corresponding source with every public release. Build the source package and attach it to the release:

```powershell
powershell -File scripts/ffmpeg-source.ps1                    # FFmpeg source + FFmpeg-Builds recipes -> release/ffmpeg-source/
tar -xzf release/ffmpeg-source/FFmpeg-Builds-*.tar.gz -C $env:TEMPfb --strip-components=1
python -I scripts/ffmpeg-source/dl_plan.py $env:TEMPfb plan.json        # reads the recipes as text (never runs them)
python -I scripts/ffmpeg-source/fetch_sources.py plan.json release/ffmpeg-source/deps $env:TEMPfb-work
git svn clone -r<REV> <lame svn url> lame; git -C lame archive --format=tar.gz --prefix=50-libmp3lame/ -o release/ffmpeg-source/deps/50-libmp3lame--svn-r<REV>.tar.gz HEAD
python -I scripts/ffmpeg-source/collect_licenses.py plan.json release/ffmpeg-source   # licences/ + SOURCE-README.txt
```

Then pack `SOURCE-README.txt`, the two archives, `deps/` and `licenses/` into one uncompressed `ffmpeg-corresponding-source-<ffmpeg version>.tar` (under GitHub's 2 GiB asset limit), add its SHA-256 to `SHA256SUMS.txt`, and attach both to the release. Fetch failures must be resolved or explained in `SOURCE-README.txt` (for v0.3.1-alpha.4 only PulseAudio, which is Linux-only, is absent).

## Signed updates

Installed copies of REPLAYX update themselves from GitHub Releases (Settings → Updates). Three things make that work, and all three are checked by the release workflow:

1. **The signing key.** Updates are signed with a dedicated Tauri updater key (minisign, Ed25519). The **public** half is in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) and is compiled into the app. The **private** half and its password never enter the repository, a log or a release:
   - On the maintainer's PC they live in `%USERPROFILE%\.replayx-signing\` (`replayx-updater.key`, `password.txt`), readable only by that Windows account.
   - **Back both up somewhere safe and offline** (a password manager, an encrypted USB stick). If the key is lost, installed copies can no longer be updated in place: users would have to install a build with a new key by hand. If it leaks, anyone could sign an "update".
   - GitHub needs them as repository secrets for release builds. Set them once, from the maintainer's PC (values are read from the files, never typed or shown):
     ```powershell
     gh secret set TAURI_SIGNING_PRIVATE_KEY -R PrithviGhale/REPLAYX < "$env:USERPROFILE\.replayx-signing\replayx-updater.key"
     gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R PrithviGhale/REPLAYX < "$env:USERPROFILE\.replayx-signing\password.txt"
     ```
   - This is separate from Windows code signing (Authenticode), which REPLAYX does not have yet: SmartScreen warnings are unaffected.
2. **The signed installer.** A release build runs `tauri build --config src-tauri/tauri.updater.conf.json`, which also writes `REPLAYX_<version>_x64-setup.exe.sig`. The workflow then verifies that signature against the built-in public key and for exactly the release's version (`scripts/verify-update-signature.mjs`), and fails if it doesn't verify. A release build without the secret fails before building.
3. **The manifest.** `scripts/updater-manifest.ps1` writes `latest.json` (version, highlights from the release notes, download address, signature). The draft step checks that it points at exactly this release's installer and carries the same signature, then attaches `latest.json` and the `.sig` to the draft.

The app reads `https://github.com/PrithviGhale/REPLAYX/releases/latest/download/latest.json`. GitHub resolves `latest` to the newest **published** release that is not a draft and not marked *pre-release*, so:

- a draft, a failed build or a commit is never offered: only publishing the release makes the update visible;
- releases must stay unmarked as *pre-release* (they are titled "— Alpha" instead), or installed copies won't see them;
- a release without `latest.json` (v0.3.1-alpha.6 and older) answers 404 and the app reports that no update file was found.

The app refuses anything that isn't newer than itself (semantic versions: `alpha.7` > `alpha.6`, `beta` > `alpha`, a release > its pre-releases), any package not signed by the key, any signature made for a different version, and any download address outside this repository's release assets. Updating keeps settings, clips and playtime: the installer only replaces the program files.

**Before the first signed release:** set the two secrets, tag, let the workflow draft the release, and test a real update from the previous version (install the previous installer, publish the draft, press *Check Now*). `node dev/update-e2e.mjs` tests the whole flow against a local test server without installing anything.
