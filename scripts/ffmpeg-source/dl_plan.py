"""Read FFmpeg-Builds recipes as text (never executed) and list every source
they download: git repositories at pinned commits and tarballs with checksums.
usage: python -I dl_plan.py <FFmpeg-Builds dir> <out.json>"""
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
scripts = sorted(list((root / "scripts.d").glob("*.sh")) + list((root / "scripts.d").glob("*/*.sh")))
plan = []
for s in scripts:
    text = s.read_text(encoding="utf-8", errors="replace")
    vars_ = dict(re.findall(r'^(SCRIPT_[A-Z0-9_]+)="([^"]*)"', text, re.M))
    name = s.relative_to(root / "scripts.d").as_posix()
    items = []
    m = re.search(r"ffbuild_dockerdl\(\)\s*\{(.*?)\n\}", text, re.S)
    body = m.group(1) if m else 'default_dl .'
    sub = lambda t: re.sub(r'\$\{?(SCRIPT_[A-Z0-9_]+)\}?', lambda x: vars_.get(x.group(1), x.group(0)), t)
    if "svn checkout" in body and "SCRIPT_REV" in vars_:
        items.append({"svn": vars_["SCRIPT_REPO"], "rev": vars_["SCRIPT_REV"]})
        body = ""
    for line in body.splitlines():
        line = sub(line)
        if "default_dl" in line and "SCRIPT_REPO" in vars_:
            items.append({"git": vars_["SCRIPT_REPO"], "commit": vars_.get("SCRIPT_COMMIT", "")})
        for repo, ref in re.findall(r'git-mini-clone \\?"([^"\\]+)\\?" \\?"([^"\\]+)\\?"', line):
            items.append({"git": repo, "commit": ref})
        for url in re.findall(r'(https?://[^\s"\\]+\.(?:tar\.(?:gz|xz|bz2)|tgz|zip|tar))', line):
            items.append({"url": url})
    if not items and "SCRIPT_REPO" in vars_ and "svn" not in vars_["SCRIPT_REPO"]:
        items.append({"git": vars_["SCRIPT_REPO"], "commit": vars_.get("SCRIPT_COMMIT", "")})
    seen = set()
    uniq = []
    for i in items:
        k = json.dumps(i, sort_keys=True)
        if k not in seen:
            seen.add(k)
            uniq.append(i)
    lgpl_excluded = bool(re.search(r'\$VARIANT == lgpl\*\s*\]\]\s*&&\s*return', text))
    linux_only = bool(re.search(r'\$TARGET == linux\*\s*\]\]\s*\|\|\s*return', text))
    plan.append({"script": name, "sources": uniq, "gpl_or_nonfree_only": lgpl_excluded, "linux_only": linux_only})
Path(sys.argv[2]).write_text(json.dumps(plan, indent=1), encoding="utf-8")
total = sum(len(p["sources"]) for p in plan)
print(f"{len(plan)} recipes, {total} sources; {sum(p['gpl_or_nonfree_only'] for p in plan)} recipes are GPL/non-free-only")
for p in plan:
    if not p["sources"]:
        print("no source:", p["script"])
bad = [p["script"] for p in plan for i in p["sources"] if "git" in i and not re.fullmatch(r"[0-9a-f]{7,40}", i["commit"] or "")]
print("non-hash refs:", bad)
