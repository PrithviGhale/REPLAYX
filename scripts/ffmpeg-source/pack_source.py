"""Pack the corresponding-source folder into one uncompressed .tar (its parts
are already compressed) and write its SHA-256.
usage: python -I pack_source.py <source dir> <output .tar>"""
import hashlib
import os
import sys
import tarfile
from pathlib import Path

root = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2]).resolve()
items = ["SOURCE-README.txt", *sorted(p.name for p in root.glob("*.tar.gz")), "deps", "licenses"]
for i in items:
    if not (root / i).exists():
        sys.exit(f"missing {i} in {root}")
tmp = out.with_suffix(".tar.part")
with tarfile.open(tmp, "w", format=tarfile.PAX_FORMAT) as t:
    for i in items:
        t.add(root / i, arcname=f"replayx-ffmpeg-source/{i}")
os.replace(tmp, out)
h = hashlib.sha256()
with open(out, "rb") as f:
    for chunk in iter(lambda: f.read(1 << 22), b""):
        h.update(chunk)
size = out.stat().st_size
if size >= 2 * 1024**3:
    sys.exit(f"{out.name} is {size} bytes, over GitHub's 2 GiB asset limit; split it before uploading")
print(f"{h.hexdigest()}  {out.name}  ({size} bytes)")
