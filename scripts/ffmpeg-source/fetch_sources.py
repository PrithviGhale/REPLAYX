"""Fetch every source in plan.json at its pinned ref as a .tar.gz made by
`git archive` (the same tree FFmpeg-Builds' git-mini-clone checks out; no
code from the repositories is run). Tarball sources are downloaded as is.
usage: python -I fetch_sources.py plan.json out_dir work_dir"""
import json
import re
import shutil
import subprocess
import sys
import time
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
        jobs.append((stem + (f"-{i}" if i else ""), s, p.get("linux_only", False)))


def run(cmd, cwd=None):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=1800)
    if r.returncode:
        raise RuntimeError((r.stderr or r.stdout).strip().splitlines()[-1] if (r.stderr or r.stdout).strip() else f"exit {r.returncode}")


def fetch(job):
    stem, s, _ = job
    last = None
    for attempt in range(3):
        try:
            return fetch_once(stem, s)
        except Exception as e:  # noqa: BLE001 - retried, then reported
            last = e
            shutil.rmtree(work / stem, ignore_errors=True)
            time.sleep(10 * (attempt + 1))
    return stem, f"FAILED: {last}", s.get("git") or s.get("url") or s.get("svn")


def fetch_once(stem, s):
    if "url" in s:
        name = s["url"].rsplit("/", 1)[-1]
        dest = out / f"{stem}--{name}"
        if not dest.exists():
            urllib.request.urlretrieve(s["url"], str(dest) + ".part")
            Path(str(dest) + ".part").rename(dest)
        return stem, "ok", dest.name
    if "svn" in s:
        dest = out / f"{stem}--svn-r{s['rev']}.tar.gz"
        if dest.exists():
            return stem, "ok", dest.name
        repo = work / stem
        shutil.rmtree(repo, ignore_errors=True)
        run(["git", "svn", "clone", "-q", f"-r{s['rev']}", s["svn"], str(repo)])
        tmp = str(dest) + ".part"
        run(["git", "-C", str(repo), "archive", "--format=tar.gz", f"--prefix={stem}/", "-o", tmp, "HEAD"])
        Path(tmp).rename(dest)
        shutil.rmtree(repo, ignore_errors=True)
        return stem, "ok", dest.name
    if not s.get("commit"):
        raise RuntimeError("no pinned ref")
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


with ThreadPoolExecutor(6) as pool:
    results = list(pool.map(fetch, jobs))
linux = {j[0] for j in jobs if j[2]}
required_missing = []
for stem, status, what in results:
    if status != "ok":
        print(f"{stem}: {status} ({what}){' [Linux-only, not in the Windows build]' if stem in linux else ''}")
        if stem not in linux:
            required_missing.append(stem)
ok = sum(1 for r in results if r[1] == "ok")
print(f"{ok}/{len(results)} fetched")
(out.parent / "fetch-report.json").write_text(json.dumps([{"component": r[0], "status": r[1], "source": r[2], "linux_only": r[0] in linux} for r in results], indent=1), encoding="utf-8")
if required_missing:
    sys.exit(f"Missing sources the Windows build may use: {', '.join(required_missing)}")
