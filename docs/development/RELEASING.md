# Releasing REPLAYX

Releases are cut by hand. GitHub Actions can prepare a draft prerelease. Stable 0.4.0 uses the signing and qualification steps below.

## Versioning

[Semantic versioning](https://semver.org/) with pre-release labels:

| Stage | Example | Meaning |
| --- | --- | --- |
| Alpha | `0.2.1-alpha.3` | Feature-incomplete or not yet validated on enough hardware |
| Beta | `0.3.0-beta.1` | Feature-complete for the line; bug fixing |
| Stable | `1.0.0` | Supported release; settings and file layout are stable |

The README's download button and install step link directly to `.../releases/download/vX.Y.Z/REPLAYX.exe`. Public assets are `REPLAYX.exe`, `latest.json` and `FFmpeg-source.tar`. Checksums appear in the release notes. GitHub also provides its standard source-code archives. Update them when a release is **published**, not before. Alpha/beta tags must be marked prerelease. Update discovery is independent of GitHub's Latest badge.


Keep the package and lockfile versions, Cargo package and lockfile versions, and Tauri config version aligned. The web UI reads the package version directly. The release workflow checks that the tag matches the package version.

## Checklist

1. **Code.** `main` contains the release; `scripts/check.ps1` passes locally and CI is green.
2. **Recording.** Review the existing regression evidence and run the short physical audio check. Follow the [0.4.0 release checklist](../engineering/FINAL_0.4.0_RELEASE_CHECKLIST.md) for remaining deployment checks.
3. **Notes.** Add a section to `CHANGELOG.md`. Describe only measured improvements.
4. **Prerelease tag.** `git tag -a v0.2.1-alpha.3 -m "REPLAYX 0.2.1-alpha.3"` and `git push origin v0.2.1-alpha.3`.
5. **Build.** The *Release (draft)* workflow builds the NSIS installer with the pinned FFmpeg runtime, runs all checks, builds the FFmpeg corresponding-source package (`scripts/ffmpeg-source/build-package.ps1`), writes `SHA256SUMS.txt` for both and creates a **draft** release titled `REPLAYX vX.Y.Z — Alpha` (or `— Beta`). Alpha and beta versions are marked **pre-release**. It refuses to run if a release for the tag already exists, so a published release is never rebuilt or overwritten; to rebuild a draft, delete the draft first. Before tagging, you can run the workflow by hand **without a tag** on any branch: that is a test build whose installer and source report are kept as workflow artifacts, and it creates no release.
6. **Verify the artifact.** Download the installer from the draft, compare its SHA-256, install it on a test machine, launch it, record, save a replay, play the clip, uninstall. Scan it with Windows Security.
7. **Publish.** Check the draft notes (download link, requirements, known limitations) and publish. Keep **Set as a pre-release** enabled for alpha and beta versions. Record incomplete validation explicitly. Update discovery follows the application's release channel independently of GitHub's Latest badge.

## Stable release 0.4.0

Use the signed build and stage those exact files. The tag-driven workflow builds prerelease drafts. The stable package uses the locally staged and qualified bytes so pushing its tag cannot rebuild a different installer. Keep the publicly available README download link on the previous release until publication is approved.

### Windows signing

Use a trusted code signing identity on an approved signing runner. Microsoft Artifact Signing Basic Public Trust is an option for eligible US and Canadian individual developers. In the Azure portal create an Artifact Signing account under a paid subscription, complete Public identity validation, then create a Public Trust certificate profile. Assign the runner identity the Artifact Signing Certificate Profile Signer role on that profile. Authorize that identity through the provider. For CI, use a narrowly scoped federated identity where available. If you already have a trusted code-signing certificate, the certificate-store path in the build script also works. Keep keys, PINs, identity documents and passwords with their provider. [Identity setup](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart) · [Signer configuration](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations)

For the Microsoft signing adapter, configure the runner's `REPLAYX_SIGNTOOL_PATH`, `REPLAYX_ARTIFACT_SIGNING_DLIB` and `REPLAYX_ARTIFACT_SIGNING_METADATA` paths. The metadata JSON contains `Endpoint`, `CodeSigningAccountName` and `CertificateProfileName`. Give the authorized runner the existing Tauri updater key and password privately through its signing environment. Use `TAURI_SIGNING_PRIVATE_KEY_PATH` for a protected file, or `TAURI_SIGNING_PRIVATE_KEY` for a protected secret, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep the current embedded public key.

```powershell
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target/stable-signed-0.4.0'
./scripts/build-rc.ps1 -SigningScriptPath ./scripts/sign-windows-artifact.ps1 -UpdaterArtifacts
./scripts/prepare-stable-assets.ps1 `
  -BundleDir (Join-Path $env:CARGO_TARGET_DIR 'release/bundle/nsis') `
  -BaselineDir '../replayx-rc/test-output/rc3-final-preflight/payload' `
  -FfmpegSource './test-output/stable-qualification/corresponding-source/FFmpeg-source.tar' `
  -Output './release/verified-stable-0.4.0'
```

With an existing certificate, replace `-SigningScriptPath` with `-CertificateThumbprint`, `-TimestampUrl` and `-SignToolPath`. The build checks Windows signatures and timestamps on the app, worker and setup. It then verifies the exact version-bound Tauri signature on the final installer. The staging command writes the three public release files under `release-assets` and keeps `SHA256SUMS.txt` outside that directory for the release notes. Tauri's manifest contains the version, signature and canonical download URL. The SHA-256 belongs in the release notes and records.

After approval, upload the separate QA ZIP to an unpublished QA draft and dispatch [the installer qualification](../engineering/INSTALLER_QUALIFICATION.md) with `candidate_version=0.4.0`. Run clean first, then all four upgrades and the optional `alpha9-updater` scope. That optional case exercises Tauri's passive NSIS updater arguments and checks that setup starts the new application on its own. Production discovery, downloading and restart as one flow still require an approved staging or published-release check. Windows Server also leaves interactive Windows 11 Home setup, WebView2 controls and graceful quit outside the verified scope.

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
   - If you authorize the prerelease CI signer, supply the existing key and password through protected GitHub Actions secrets. The command reads the file without displaying its contents.
     ```powershell
     Get-Content -Raw -LiteralPath "$env:USERPROFILE\.replayx-signing\replayx-updater.key" | gh secret set TAURI_SIGNING_PRIVATE_KEY -R PrithviGhale/REPLAYX
     Get-Content -Raw -LiteralPath "$env:USERPROFILE\.replayx-signing\password.txt" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R PrithviGhale/REPLAYX
     ```
   - Authenticode uses a separate signing identity. Tauri signatures verify the update inside REPLAYX.
2. **The signed installer.** A release build runs `scripts/build-rc.ps1 -UpdaterArtifacts` to stage the Native worker and build the installer, which also writes `REPLAYX_<version>_x64-setup.exe.sig`. The workflow then verifies that signature against the built-in public key and for exactly the release's version (`scripts/verify-update-signature.mjs`), and fails if it doesn't verify. A release build without the secret fails before building.
3. **The manifest.** `scripts/updater-manifest.ps1` writes `latest.json` (version, highlights from the release notes, download address, signature). The draft step checks that it points at exactly this release's installer and carries the same signature, then attaches `latest.json` to the draft. Its embedded signature verifies the installer; the standalone `.sig` stays in build artifacts.

The app queries the repository Releases API (up to the 100 most recent published releases), selects the highest compatible semantic version with a trusted `latest.json` asset, then checks that tag's manifest. Drafts and incorrectly labeled prereleases are rejected. Stable builds follow stable releases; alpha/beta builds follow the same named channel or stable. GitHub errors are failed checks with backoff, never proof that the app is up to date.

The app refuses anything that isn't newer than itself (semantic versions: `alpha.7` > `alpha.6`, `beta` > `alpha`, a release > its pre-releases), any package not signed by the key, any signature made for a different version, and any download address outside this repository's release assets. Updating keeps settings, clips and playtime: the installer only replaces the program files.

**Before publication:** verify the exact signed installer and its manifest, and qualify its manual installation and upgrades in a disposable guest. The production updater discovers published REPLAYX GitHub releases. It ignores drafts and the loopback test path is development-only. Verify the actual GitHub discovery and updater restart in a disposable older installation as soon as the maintainer authorizes publication. See the [0.4.0 release checklist](../engineering/FINAL_0.4.0_RELEASE_CHECKLIST.md).
