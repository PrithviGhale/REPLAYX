# REPLAYX 0.4.0 redesign: release qualification (November 19, 2026)

This qualifies the redesigned 0.4.0 candidate built from `3f30a61`. The
qualified fallback (`35b20b6`, installer `0F2E944E…`) and its existing
`v0.4.0` draft remain unchanged until approval.

## Candidate

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `REPLAYX.exe` (NSIS setup) | 60,452,641 | `aaea05cac9169fd025f895ae62a21a5efe652e4562cefa9dc0e2dfc0dbfd3169` |
| `REPLAYX.exe.sig` (Tauri) | 416 | `3e3776d7292c2b737d4249f6ea8b226d747082a11dd445b925897b8871d048bf` |
| `replayx.exe` (app inside setup) | 18,203,648 | `02cfaea3f3a510af25a6dd93f10048369f84b89039f0a36ee88d69d5994655b4` |
| `replayx-native-proto.exe` (worker) | 959,488 | `9636c8991764a1931a02b0753b81b03e2772a1db859482e8cc990cdf547738ec` |

**Provenance.** The candidate's recorded source commit had been added by hand
after the build. So the clean `3f30a61` tree was rebuilt with
`scripts/build-rc.ps1` into a separate target directory.
- **The app and worker are byte-identical** except the MSVC link timestamps and
  PDB GUIDs: 24 and 23 bytes in the PE header and debug directory.
- **So the candidate's code, data and embedded frontend are those of
  `3f30a61`.**
- **Whole-file hashes are not reproducible across rebuilds** for that reason.
  The qualified bytes are the candidate above.

**Updater signature.** It verifies against the embedded key for version 0.4.0,
and is rejected for 0.4.1 and for a single flipped byte.
- **The installed Alpha.9 already trusts this key.** Its `replayx.exe` embeds
  the same public key and the endpoint
  `releases/latest/download/latest.json`.

**Windows Authenticode.** Not signed: the owner allowed unsigned distribution,
with disclosure, pending SignPath.
- **Narrow exception.** `scripts/release-authenticode.ps1` now accepts exactly
  two unsigned 0.4.0 builds, the fallback and this candidate (installer, app
  and worker hashes).
- **Every other version still requires timestamped Authenticode.**

## Local results

| Check | Result |
| --- | --- |
| Rust tests (`--features desktop`) | 291 passed, 8 ignored |
| `cargo fmt --check`, strict Clippy | Passed |
| Frontend tests, `tsc --noEmit` | 81 passed, clean |
| Python offline regressions | 64 passed, 21 opt-in hardware cases skipped |
| Browser regression (headless Chrome, fixture IPC, `dev/redesign-check.mjs`) | 41 passed |
| Generated FFmpeg replay and concurrent save (`self-test`) | **Both clips:** 600 frames, 3 audio tracks, no decode errors, monotonic timestamps.<br>**Track isolation:** 101.0 / 89.3 dB.<br>**A/V sync:** 9 pulse/flash events within 10 ms. |
| Generated recording with concurrent replay (`record-self-test`) | 720 and 960 frames, 3 tracks each, no decode errors, monotonic timestamps; video gaps at most 17 ms, audio at most 22 ms |
| Staged stable assets (`prepare-stable-assets.ps1 -AllowUnsigned040`) | Offline integrity PASS: archive members pinned, installer and version-bound `latest.json` signature verified |

**Release-text fix.** Several characters in the release notes had turned into
`?`:
- "REPLAYX?s";
- "Settings ? Updates ? Check Now".

They flow into `latest.json`, which Alpha.9 shows in its update prompt. They
are corrected, along with the same corruption in the checklist and the
releasing guide.

**Not fixed.** Two Native V2 alpha messages in the app also contain a
corrupted character ("1920 ? 1080"). They appear only with the experimental
engine. Fixing them would change the qualified app bytes, so this is left for
0.4.1.

## Staged release

These are under `release/verified-stable-0.4.0-redesign`, which is ignored by
git:
- `release-assets/REPLAYX.exe`;
- `latest.json`, SHA-256 `ac158ca6…`, which points at
  `releases/download/v0.4.0/REPLAYX.exe`;
- `FFmpeg-source.tar`, SHA-256 `41ec89ff…`;
- `release-body.md`;
- `REPLAYX-installer-qa-0.4.0.zip`, SHA-256
  `8ecf7ac1f51afd167917fa3e0cbcca37f0cbe58061f1555ce78c98ee06860473`.

## Still required before publication (owner approval)

1. Push this branch, then run the hosted installer matrix against the QA ZIP:
   clean, then the four upgrades, then `alpha9-updater`.
2. Retarget the existing `v0.4.0` draft at the release commit, replace its
   assets and body, and keep it a draft.
3. The owner's manual test of the candidate.
4. Publish as Latest, non-prerelease. **This activates in-app updates for
   existing installs.**
5. The live Check Now test from an isolated Alpha.9 installation.
