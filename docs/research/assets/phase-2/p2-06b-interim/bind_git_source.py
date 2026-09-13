"""Check or bind D035 frozen observed sources to staged Git bytes.

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
CHECKPOINT = ASSETS / "p2-06b-interim/source-checkpoint.json"
OUTPUT = ASSETS / "p2-06b-git-source.json"


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
    checkpoint_bytes = CHECKPOINT.read_bytes()
    checkpoint = json.loads(checkpoint_bytes)
    manifest = json.loads((CHECKPOINT.parent / "logs-manifest.json").read_text(encoding="utf-8"))
    if (checkpoint["status"] != "functional_checkpoint_performance_deferred"
            or checkpoint["packet"] != "P2-06B" or checkpoint["decision"] != "D035"
            or checkpoint["boundary"]["schema"] != 17
            or manifest["files"]["source-checkpoint.json"]["sha256"] != digest(checkpoint_bytes)):
        raise RuntimeError("Wrong or changed D035 source checkpoint")
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
    if CHECKPOINT.read_bytes() != checkpoint_bytes or any(digest((ROOT / name).read_bytes()) != value for name, value in sources.items()):
        raise RuntimeError("Frozen sources changed during source binding")
    report = {"scope": "Verified worktree source to staged Git source binding, before the checkpoint commit",
              "archive": "p2-06b-interim/source-checkpoint.json", "source_count": len(sources),
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
