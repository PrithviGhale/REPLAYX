"""Copy each source archive's top-level licence files into licenses/<component>/
and write SOURCE-README.txt describing every component (repository, ref,
archive, SHA-256). Reads archives only; writes files under names chosen here.
usage: python -I collect_licenses.py plan.json source_dir"""
import hashlib
import json
import re
import sys
import tarfile
from pathlib import Path

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
root = Path(sys.argv[2]).resolve()
deps = root / "deps"
lic = root / "licenses"
lic.mkdir(exist_ok=True)
pattern = re.compile(r"^(copying|licen[cs]e|notice|copyright|unlicense|patents)([._-].*)?$", re.I)


def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def licences_from(archive, component):
    found = []
    with tarfile.open(archive, "r:gz") as t:
        for m in t:
            parts = m.name.split("/")
            # prefix/FILE or prefix/LICENSES/FILE (REUSE layout)
            top = len(parts) == 2 or (len(parts) == 3 and parts[1].upper() in ("LICENSES", "LICENSE"))
            if not (m.isfile() and top and pattern.match(parts[-1]) and m.size < 2_000_000):
                continue
            data = t.extractfile(m).read()
            safe = re.sub(r"[^A-Za-z0-9._-]", "_", "_".join(parts[1:]))
            dest = lic / component / safe
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
            found.append(safe)
    return found


lines = []
missing = []
by_stem = {p.name.split("--")[0]: p for p in deps.glob("*.tar.gz")}
for p in plan:
    stem0 = re.sub(r"[^A-Za-z0-9._-]+", "_", p["script"].removesuffix(".sh"))
    for i, s in enumerate(p["sources"]):
        stem = stem0 + (f"-{i}" if i else "")
        origin = s.get("git") or s.get("url") or s.get("svn")
        ref = s.get("commit") or (f"r{s['rev']}" if "rev" in s else "")
        if p["gpl_or_nonfree_only"]:
            lines.append(f"{stem:<34} not in this build (GPL/non-free variants only)  {origin}")
            continue
        a = by_stem.get(stem)
        if not a:
            why = "Linux-only, not part of the Windows build" if p.get("linux_only") else "MISSING"
            lines.append(f"{stem:<34} not included ({why})  {origin} @ {ref}")
            missing.append(stem)
            continue
        files = licences_from(a, stem)
        lines.append(f"{stem:<34} {origin} @ {ref}\n{'':34} {a.name}  sha256 {sha256(a)}\n{'':34} licence files: {', '.join(files) or 'none at top level (see headers in the source)'}")

ff = sorted(root.glob("ffmpeg-*.tar.gz"))[0]
fb = sorted(root.glob("FFmpeg-Builds-*.tar.gz"))[0]
licences_from(ff, "ffmpeg")
head = f"""FFmpeg corresponding source for REPLAYX
=======================================

REPLAYX's installer ships an unmodified LGPL build of FFmpeg:
  FFmpeg n9.0.2-47-gb204e94e90, built by FFmpeg-Builds tag autobuild-2026-11-06-12-50
  (win64, LGPL, shared; configured with --enable-version3, without --enable-gpl or --enable-nonfree).
  The exact configure line is in media/BUILD-CONFIGURATION.txt inside the REPLAYX installation.

Contents of this archive:
  {ff.name}  FFmpeg source at commit b204e94e90  sha256 {sha256(ff)}
  {fb.name}  build recipes (scripts.d/ pins every library below)  sha256 {sha256(fb)}
  deps/      source of each library, archived from its repository at the pinned commit/tag
             (the same tree FFmpeg-Builds checks out; LAME from Subversion revision 7113)
  licenses/  the licence files found at the top of each source tree

This is a superset: it includes every library the recipes can build for Windows, whether or not
the LGPL win64 variant enables it, so nothing used by the shipped binaries is missing.
Libraries the recipes reserve for GPL or non-free builds are listed but not included.

To rebuild: use the FFmpeg-Builds recipes (build.sh win64 lgpl-shared 9.0) with the sources here.

Components:
"""
(root / "SOURCE-README.txt").write_text(head + "\n".join(lines) + "\n", encoding="utf-8")
print(f"{len(lines)} components listed; licence folders: {len(list(lic.iterdir()))}; not included: {missing}")
