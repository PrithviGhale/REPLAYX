# Releasing REPLAYX

Releases are cut by hand from `main`. GitHub Actions builds them, but nothing is published automatically: the workflow creates a **draft prerelease** for a maintainer to check and publish.

## Versioning

[Semantic versioning](https://semver.org/) with pre-release labels:

| Stage | Example | Meaning |
| --- | --- | --- |
| Alpha | `0.2.1-alpha.3` | Feature-incomplete or not yet validated on enough hardware |
| Beta | `0.3.0-beta.1` | Feature-complete for the line; bug fixing |
| Stable | `1.0.0` | Supported release; settings and file layout are stable |

The README's download button and install step name the newest release's installer directly (`.../releases/download/vX.Y.Z/REPLAYX_X.Y.Z_x64-setup.exe`). Update them when a release is **published**, not before: GitHub's "Latest" release and `/releases/latest` ignore pre-releases, so while REPLAYX is in alpha a `latest` link would not work.

The version lives in six places that must match: `package.json`, `package-lock.json` (twice), `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`, plus `VERSION` in `src/api.ts`. The release workflow refuses a tag that does not match `package.json`.

## Checklist

1. **Code.** `main` contains the release; `scripts/check.ps1` passes locally and CI is green.
2. **Hardware.** Run the checklist in [VALIDATION.md](VALIDATION.md) on real hardware: record, replay save, audio playback, repeated Record/Stop, Delete, and the game-FPS comparison. Record the results and every untested item in `VALIDATION.md` and `KNOWN_LIMITATIONS.md`.
3. **Notes.** Add a section to `CHANGELOG.md`. Describe only measured improvements.
4. **Tag.** `git tag -a v0.2.1-alpha.3 -m "REPLAYX 0.2.1-alpha.3"` and `git push origin v0.2.1-alpha.3`.
5. **Build.** The *Release (draft)* workflow builds the NSIS installer with the pinned FFmpeg runtime, runs all checks, builds the FFmpeg corresponding-source package (`scripts/ffmpeg-source/build-package.ps1`), writes `SHA256SUMS.txt` for both and creates a **draft** release (marked pre-release for `-alpha`/`-beta` tags). It refuses to run if a release for the tag already exists, so a published release is never rebuilt or overwritten; to rebuild a draft, delete the draft first. Before tagging, you can run the workflow by hand **without a tag** on any branch: that is a test build whose installer and source report are kept as workflow artifacts, and it creates no release.
6. **Verify the artifact.** Download the installer from the draft, compare its SHA-256, install it on a test machine, launch it, record, save a replay, play the clip, uninstall. Scan it with Windows Security.
7. **Publish.** Edit the draft notes (download link, requirements, known limitations), keep **pre-release** ticked for alpha/beta, and publish.

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

## Signed updates (not enabled yet)

The app contains the Tauri updater with signature verification. To enable it, a maintainer creates an update signing key **outside** the repository, stores the private key as the repository secret `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), and the public key as the variable `REPLAYX_UPDATE_PUBKEY`. The release workflow then also publishes `latest.json` and `.sig` files. Test a real upgrade from the previous release before announcing in-app updates. Authenticode code signing is separate and not configured.
