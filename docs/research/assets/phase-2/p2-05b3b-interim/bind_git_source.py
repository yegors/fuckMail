"""Bind the frozen observed source to staged Git bytes without rewriting evidence."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / "docs/research/assets/phase-2"
CHECKPOINT = ASSETS / "p2-05b3b-interim/source-checkpoint.json"
OUTPUT = ASSETS / "p2-05b3b-git-source.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if OUTPUT.exists():
        raise RuntimeError("Never overwrite an existing source binding")
    sources = json.loads(CHECKPOINT.read_text(encoding="utf-8"))["source_hashes"]
    differences = []
    for name, expected in sources.items():
        path = (ROOT / name).resolve()
        if not path.is_relative_to(ROOT.resolve()):
            raise RuntimeError("Source is outside workspace")
        observed = path.read_bytes()
        if digest(observed) != expected:
            raise RuntimeError("Observed source changed: " + name)
        staged = subprocess.check_output(["git", "show", ":" + name], cwd=ROOT)
        if staged == observed:
            continue
        attributes = subprocess.check_output(
            ["git", "check-attr", "text", "--", name], cwd=ROOT, text=True).strip()
        if (observed.replace(b"\r\n", b"\n") != staged.replace(b"\r\n", b"\n")
                or not attributes.endswith((": text: auto", ": text: set"))):
            raise RuntimeError("Staged runtime content differs: " + name)
        differences.append({"path": name, "observed_sha256": expected,
                            "staged_sha256": digest(staged),
                            "difference": "CRLF/LF only under .gitattributes"})
    report = {"scope": "Verified worktree source to staged Git source binding, before the checkpoint commit",
              "archive": "p2-05b3b-interim/source-checkpoint.json", "source_count": len(sources),
              "result": "passed", "line_ending_only_differences": differences,
              "runtime_content_changes": []}
    with OUTPUT.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"result": "passed", "source_count": len(sources),
                      "line_ending_only_differences": len(differences)}))


if __name__ == "__main__":
    main()
