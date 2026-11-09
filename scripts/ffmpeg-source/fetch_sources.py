"""Fetch every source in plan.json at its pinned ref as a .tar.gz made by
`git archive` (the same tree FFmpeg-Builds' git-mini-clone checks out; no
code from the repositories is run). Tarball sources are downloaded as is.
usage: python -I fetch_sources.py plan.json out_dir work_dir"""
import json
import re
import shutil
import subprocess
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
out = Path(sys.argv[2]).resolve()
work = Path(sys.argv[3]).resolve()
out.mkdir(parents=True, exist_ok=True)
work.mkdir(parents=True, exist_ok=True)
jobs = []
for p in plan:
    if p["gpl_or_nonfree_only"]:
        continue
    for i, s in enumerate(p["sources"]):
        stem = re.sub(r"[^A-Za-z0-9._-]+", "_", p["script"].removesuffix(".sh"))
        jobs.append((stem + (f"-{i}" if i else ""), s))


def run(cmd, cwd=None):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=1800)
    if r.returncode:
        raise RuntimeError((r.stderr or r.stdout).strip().splitlines()[-1] if (r.stderr or r.stdout).strip() else f"exit {r.returncode}")


def fetch(job):
    stem, s = job
    try:
        if "url" in s:
            name = s["url"].rsplit("/", 1)[-1]
            dest = out / f"{stem}--{name}"
            if not dest.exists():
                urllib.request.urlretrieve(s["url"], str(dest) + ".part")
                Path(str(dest) + ".part").rename(dest)
            return stem, "ok", dest.name
        if not s.get("commit"):
            return stem, "SKIP (no pinned ref)", s["git"]
        ref = s["commit"]
        dest = out / f"{stem}--{re.sub(r'[^A-Za-z0-9._-]', '_', ref)[:40]}.tar.gz"
        if dest.exists():
            return stem, "ok", dest.name
        repo = work / stem
        if not (repo / ".git" / "FETCH_HEAD").exists():
            shutil.rmtree(repo, ignore_errors=True)
            run(["git", "init", "-q", str(repo)])
            run(["git", "-C", str(repo), "-c", "protocol.version=2", "fetch", "-q", "--depth=1", s["git"], ref])
        tmp = str(dest) + ".part"
        run(["git", "-C", str(repo), "archive", "--format=tar.gz", f"--prefix={stem}/", "-o", tmp, "FETCH_HEAD"])
        Path(tmp).rename(dest)
        shutil.rmtree(repo, ignore_errors=True)
        return stem, "ok", dest.name
    except Exception as e:  # noqa: BLE001 - report every failure, keep going
        return stem, f"FAILED: {e}", s.get("git") or s.get("url")


with ThreadPoolExecutor(6) as pool:
    results = list(pool.map(fetch, jobs))
for stem, status, what in results:
    if status != "ok":
        print(f"{stem}: {status} ({what})")
ok = sum(1 for r in results if r[1] == "ok")
print(f"{ok}/{len(results)} fetched")
