"""Check or bind D037 frozen observed sources to staged Git bytes.

Default/--check is read-only. --bind creates one new source-binding report;
neither mode stages files, commits, changes Git state or rewrites the archive.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / "docs/research/assets/phase-2"
CHECKPOINT = ASSETS / "p2-06c-interim/source-checkpoint.json"
OUTPUT = ASSETS / "p2-06c-git-source.json"
BASE_COMMIT = "324284952876cc3c9feba7509f816601d6aa022e"
BOUNDARY = {"Core": 25, "Gmail": 12, "Mail_UI": 23, "CLI": 18, "Quality": 21,
            "Calendar": 18, "Platform": 13, "Search": 5, "schema": 19,
            "fixtures": [0, 3], "protocol": 1}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="Validate staged source only (default)")
    mode.add_argument("--bind", action="store_true", help="Create the immutable binding report after validation")
    args = parser.parse_args()
    if OUTPUT.exists():
        raise RuntimeError("Never overwrite an existing source binding")
    if not CHECKPOINT.is_file() or not (CHECKPOINT.parent / "logs-manifest.json").is_file():
        raise RuntimeError("The immutable P2-06C archive checkpoint and manifest must exist before source binding")
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() != BASE_COMMIT:
        raise RuntimeError("Source binding must precede the checkpoint commit on the declared implementation base")
    checkpoint_bytes = CHECKPOINT.read_bytes()
    checkpoint = json.loads(checkpoint_bytes)
    manifest_path = CHECKPOINT.parent / "logs-manifest.json"
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    if (checkpoint["status"] != "functional_checkpoint_performance_deferred"
            or checkpoint["packet"] != "P2-06C" or checkpoint["decision"] != "D037"
            or checkpoint["boundary"] != BOUNDARY
            or checkpoint["performance_deferral"] != "D031" or checkpoint["readiness"] != "0/9"
            or checkpoint["base_commit"] != BASE_COMMIT
            or manifest["files"]["source-checkpoint.json"]["sha256"] != digest(checkpoint_bytes)):
        raise RuntimeError("Wrong or changed D037 source checkpoint")
    files = manifest["files"]
    if (not isinstance(files, dict) or manifest["count"] != len(files)
            or manifest["total_bytes"] != sum(value["size"] for value in files.values())
            or manifest["status"] != checkpoint["status"]):
        raise RuntimeError("Archive manifest totals/status differ")
    for name, value in files.items():
        path = CHECKPOINT.parent / name
        if (Path(name).is_absolute() or not path.resolve().is_relative_to(CHECKPOINT.parent.resolve())
                or path.is_symlink() or not path.is_file()
                or path.stat().st_size != value["size"] or digest(path.read_bytes()) != value["sha256"]):
            raise RuntimeError("Archive artifact changed or escaped its root: " + name)
    observed_files = {p.relative_to(CHECKPOINT.parent).as_posix()
                      for p in CHECKPOINT.parent.rglob("*") if p.is_file()}
    if observed_files != set(files) | {"logs-manifest.json"}:
        raise RuntimeError("Archive has missing or unmanifested artifacts")
    sources = checkpoint["source_hashes"]
    if not sources or checkpoint["runtime_source_count"] != len(sources):
        raise RuntimeError("Empty or inconsistent source inventory")
    index_before = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=ROOT)
    differences = []
    staged_hashes = {}
    for name, expected in sources.items():
        path = (ROOT / name).resolve()
        if not path.is_relative_to(ROOT.resolve()):
            raise RuntimeError("Source is outside workspace")
        observed = path.read_bytes()
        if digest(observed) != expected:
            raise RuntimeError("Observed source changed: " + name)
        staged = subprocess.check_output(["git", "show", ":" + name], cwd=ROOT)
        staged_hashes[name] = digest(staged)
        if staged == observed:
            continue
        attributes = subprocess.check_output(
            ["git", "check-attr", "--cached", "text", "--", name], cwd=ROOT, text=True).strip()
        if (observed.replace(b"\r\n", b"\n") != staged.replace(b"\r\n", b"\n")
                or not attributes.endswith((": text: auto", ": text: set"))):
            raise RuntimeError("Staged runtime content differs: " + name)
        differences.append({"path": name, "observed_sha256": expected,
                            "staged_sha256": digest(staged),
                            "difference": "CRLF/LF only under .gitattributes"})
    if subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=ROOT) != index_before:
        raise RuntimeError("Git index changed during source binding")
    if (CHECKPOINT.read_bytes() != checkpoint_bytes or manifest_path.read_bytes() != manifest_bytes
            or any(digest((ROOT / name).read_bytes()) != value for name, value in sources.items())
            or any(digest((CHECKPOINT.parent / name).read_bytes()) != value["sha256"] for name, value in files.items())):
        raise RuntimeError("Frozen sources changed during source binding")
    report = {"scope": "Verified worktree source to staged Git source binding, before the checkpoint commit",
              "archive": "p2-06c-interim/source-checkpoint.json", "source_count": len(sources),
              "packet": "P2-06C", "decision": "D037", "base_commit": BASE_COMMIT,
              "performance_deferral": "D031", "readiness": "0/9",
              "archive_manifest_sha256": digest(manifest_bytes),
              "archive_checkpoint_sha256": digest(checkpoint_bytes), "staged_source_hashes": staged_hashes,
              "result": "passed", "line_ending_only_differences": differences,
              "runtime_content_changes": []}
    if args.bind:
        with OUTPUT.open("x", encoding="utf-8") as stream:
            json.dump(report, stream, indent=2)
            stream.write("\n")
    print(json.dumps({"result": "bound" if args.bind else "checked", "source_count": len(sources),
                      "line_ending_only_differences": len(differences)}))


if __name__ == "__main__":
    main()
