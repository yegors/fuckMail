"""Run and record ordinary Rust workspace checks without overwriting attempts."""
from datetime import datetime, timezone
from pathlib import Path
import argparse
import json
import os
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts"))
from verify_google_navigation import source_manifest as all_sources, require
from verify_search_ingestion import sha256

def source_manifest():
    inputs = {name: value for name, value in all_sources().items()
              if name.startswith("crates/") or name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}}
    inputs.update({path.relative_to(ROOT).as_posix(): sha256(path)
                   for path in (ROOT / "fixtures").rglob("*") if path.is_file()})
    return inputs

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--attempt", type=int, default=1)
args = parser.parse_args()
require(1 <= args.attempt <= 99, "Use a bounded numbered attempt")
packet = Path(__file__).resolve().parent
suffix = f"{args.attempt:02}"
paths = {name: packet / f"workspace-{name}-{suffix}.log" for name in ("tests", "clippy", "format")}
manifest = packet / f"workspace-source-{suffix}.json"
report = packet / f"workspace-result-{suffix}.json"
require(all(not path.exists() for path in [*paths.values(), manifest, report]), "Never overwrite an attempt")
before = source_manifest()
manifest.write_text(json.dumps(before, indent=2) + "\n", encoding="utf-8")
env = os.environ.copy()
env["CARGO_HOME"] = str(ROOT / ".tools/cargo")
env["RUSTUP_HOME"] = str(ROOT / ".tools/rustup")
cargo = ROOT / ".tools/cargo/bin/cargo.exe"
commands = {"tests": ["test", "--workspace", "--locked"],
            "clippy": ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
            "format": ["fmt", "--all", "--", "--check"]}
result = {"status": "failed", "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
          "scope": "Ordinary Rust workspace; feature-only, native, provider and navigation checks are separate.",
          "checks": [], "logs": {}}
for key, command in commands.items():
    print("Running workspace " + key, flush=True)
    started = time.monotonic()
    with paths[key].open("wb") as output:
        completed = subprocess.run([str(cargo), *command], cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                                   stdout=output, stderr=subprocess.STDOUT)
    check = {"name": key, "exit_code": completed.returncode, "wall_clock_seconds": time.monotonic() - started}
    result["checks"].append(check)
    result["logs"][key] = {"path": paths[key].name, "sha256": sha256(paths[key])}
    print(json.dumps(check), flush=True)
    if completed.returncode:
        break
text = paths["tests"].read_text(encoding="utf-8", errors="replace")
summary_pattern = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; .*finished in ([\d.]+)s", re.MULTILINE)
matches = summary_pattern.findall(text)
result["summary"] = {"passed": sum(int(row[1]) for row in matches), "failed": sum(int(row[2]) for row in matches),
                     "ignored": sum(int(row[3]) for row in matches), "summed_test_suite_seconds": sum(float(row[4]) for row in matches),
                     "test_targets": len(re.findall(r"^\s+Running ", text, re.MULTILINE)),
                     "doc_test_targets": len(re.findall(r"^\s+Doc-tests ", text, re.MULTILINE)),
                     "wall_clock_seconds": result["checks"][0]["wall_clock_seconds"]}
after = source_manifest()
rust = lambda name: name.startswith(("crates/", "fixtures/")) or name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
changed = sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
result["source_check"] = {"manifest_sha256": sha256(manifest), "changed_sources": changed,
                          "changed_rust_sources": [name for name in changed if rust(name)],
                          "rust_sources_checked": sum(rust(name) for name in before)}
result["exit_code"] = next((row["exit_code"] for row in result["checks"] if row["exit_code"]), 0)
if len(result["checks"]) == 3 and result["exit_code"] == 0 and matches and result["summary"]["failed"] == 0 and not result["source_check"]["changed_rust_sources"]:
    result["status"] = "passed"
report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"status": result["status"], "summary": result["summary"], "changed_sources": changed}), flush=True)
raise SystemExit(0 if result["status"] == "passed" else 1)
