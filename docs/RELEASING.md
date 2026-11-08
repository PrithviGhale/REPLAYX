# Releasing REPLAYX

Releases are cut by hand from `main`. GitHub Actions builds them, but nothing is published automatically: the workflow creates a **draft prerelease** for a maintainer to check and publish.

## Versioning

[Semantic versioning](https://semver.org/) with pre-release labels:

| Stage | Example | Meaning |
| --- | --- | --- |
| Alpha | `0.2.1-alpha.3` | Feature-incomplete or not yet validated on enough hardware |
| Beta | `0.3.0-beta.1` | Feature-complete for the line; bug fixing |
| Stable | `1.0.0` | Supported release; settings and file layout are stable |

The version lives in six places that must match: `package.json`, `package-lock.json` (twice), `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`, plus `VERSION` in `src/api.ts`. The release workflow refuses a tag that does not match `package.json`.

## Checklist

1. **Code.** `main` contains the release; `scripts/check.ps1` passes locally and CI is green.
2. **Hardware.** Run the checklist in [VALIDATION.md](VALIDATION.md) on real hardware: record, replay save, audio playback, repeated Record/Stop, Delete, and the game-FPS comparison. Record the results and every untested item in `VALIDATION.md` and `KNOWN_LIMITATIONS.md`.
3. **Notes.** Add a section to `CHANGELOG.md`. Describe only measured improvements.
4. **Tag.** `git tag -a v0.2.1-alpha.3 -m "REPLAYX 0.2.1-alpha.3"` and `git push origin v0.2.1-alpha.3`.
5. **Build.** The *Release (draft)* workflow builds the NSIS installer with the pinned FFmpeg runtime, runs all checks, writes `SHA256SUMS.txt` and creates a draft prerelease.
6. **Verify the artifact.** Download the installer from the draft, compare its SHA-256, install it on a test machine, launch it, record, save a replay, play the clip, uninstall. Scan it with Windows Security.
7. **Publish.** Edit the draft notes (download link, requirements, known limitations), keep **pre-release** ticked for alpha/beta, and publish.

## FFmpeg and licences

The installer ships an LGPL FFmpeg build pinned in `tools/ffmpeg-lock.json` (immutable release tag and SHA-256). Before each public release, confirm [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) and the release notes still point to the FFmpeg corresponding source for that exact build.

## Signed updates (not enabled yet)

The app contains the Tauri updater with signature verification. To enable it, a maintainer creates an update signing key **outside** the repository, stores the private key as the repository secret `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), and the public key as the variable `REPLAYX_UPDATE_PUBKEY`. The release workflow then also publishes `latest.json` and `.sig` files. Test a real upgrade from the previous release before announcing in-app updates. Authenticode code signing is separate and not configured.
