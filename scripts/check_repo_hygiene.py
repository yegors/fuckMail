#!/usr/bin/env python3
"""Check the prospective source tree without reading ignored local artifacts."""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
MAX_FILE_BYTES = 2 * 1024 * 1024
MAX_DOC_ASSET_BYTES = 10 * 1024 * 1024
GENERATED_PREFIXES = (
    ".artifacts/", ".tools/", "build/", "dist/", "target/",
    ".profiles/", "profiles/", "docs/research/assets/phase-", "docs/search/assets/",
)
GENERATED_DOC_SUFFIXES = (".log", ".zip", ".tar", ".gz", ".7z", ".sqlite", ".sqlite3")


def check(root):
    # Deleted files are excluded so this also works before staging the cleanup.
    # Forced additions of ignored files remain visible through --cached.
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
    ).decode("utf-8").split("\0")
    problems = []
    asset_bytes = 0
    checked = 0
    for name in sorted(set(filter(None, names))):
        path = root / name
        if not path.exists() and not path.is_symlink():
            continue
        checked += 1
        if name.startswith(GENERATED_PREFIXES):
            problems.append(f"{name}: generated output belongs in ignored artifact storage")
            continue
        if name.startswith("docs/") and name.lower().endswith(GENERATED_DOC_SUFFIXES):
            problems.append(f"{name}: raw evidence belongs in ignored artifact storage")
        # Do not follow tracked links into local SDKs, profiles or other trees.
        if path.is_symlink():
            continue
        size = path.stat().st_size
        if size > MAX_FILE_BYTES:
            problems.append(f"{name}: {size:,} bytes exceeds the 2 MiB source-file limit")
        if name.startswith("docs/research/assets/"):
            asset_bytes += size
    if asset_bytes > MAX_DOC_ASSET_BYTES:
        problems.append(f"documentation assets: {asset_bytes:,} bytes exceeds the 10 MiB curated-asset limit")
    return checked, asset_bytes, problems


def main():
    checked, asset_bytes, problems = check(ROOT)
    if problems:
        print("Repository hygiene failed:", file=sys.stderr)
        for problem in problems[:20]:
            print(f"  {problem}", file=sys.stderr)
        if len(problems) > 20:
            print(f"  ... and {len(problems) - 20} more", file=sys.stderr)
        print("See docs/research/evidence-archive.md.", file=sys.stderr)
        return 1
    print(f"Repository hygiene passed: {checked} source files; {asset_bytes:,} bytes of curated documentation assets.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
