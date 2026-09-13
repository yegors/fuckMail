"""Freeze the functional B3A checkpoint; D031 performance remains deferred."""
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import subprocess

import run_native as native

ROOT, PACKET = native.ROOT, native.OUT
DEST = ROOT / "docs/research/assets/phase-2/p2-05b3a-interim"


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def read(name):
    return json.loads((PACKET / name).read_text(encoding="utf-8"))


def same_inputs(values, predicate=lambda name: True):
    selected = {name: value for name, value in values.items() if predicate(name)}
    require(all((ROOT / name).is_file() and native.digest(ROOT / name) == value
                for name, value in selected.items()), "Recorded input changed")
    return len(selected)


require(not DEST.exists(), "Never overwrite an immutable checkpoint")
required = {
    "workspace-result-01.json": "status",
    "rust-production-build-01.result.json": "status",
    "feature-tests-01.result.json": "status",
    "paired-remote-drafts-02.json": "result",
    "draft-send-regression-01.json": "result",
    "upgrade-01.json": "result",
    "gui-dev-ctest-01.result.json": "status",
}
for name, field in required.items():
    result = read(name)
    require(result[field] == "passed" and not result.get("error"), "Unpassed gate: " + name)

native_result = read("gui-dev-ctest-01.result.json")
require(native_result["source_unchanged"] and native_result["failures"] == 0,
        "Native source or outcome is not frozen")
require(not any(name.startswith("build/dev/") for name in native_result["changed_binary_paths"]),
        "Native application changed during execution")
sources = native.source_manifest()
same_inputs(read("gui-dev-ctest-01.source.json"))
same_inputs(read("paired-remote-drafts-02.source.json"))
rust_count = same_inputs(read("workspace-source-01.json"),
                         lambda name: name.startswith("crates/") or name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"})
same_inputs(read("rust-production-build-01.source.json"))
same_inputs(read("feature-tests-01.source.json"))
same_inputs(read("core-migration-tests-01.source.json"))
same_inputs(read("upgrade-01.json")["sources"])

binaries = {}
for name in ("rust-production-build-01.result.json", "feature-tests-01.result.json"):
    for value in read(name)["executables"].values():
        path = Path(value["path"])
        require(native.digest(path) == value["sha256"], "Release executable changed")
        binaries[path.relative_to(ROOT).as_posix()] = {"sha256": value["sha256"], "size": path.stat().st_size}
for value in read("paired-remote-drafts-02.json")["executables"].values():
    path = Path(value["path"])
    require(native.digest(path) == value["sha256"], "Paired executable changed")
    binaries[path.relative_to(ROOT).as_posix()] = {"sha256": value["sha256"], "size": path.stat().st_size}
for name, value in native_result["binaries"].items():
    path = ROOT / name
    require(native.digest(path) == value["sha256"], "Native executable changed")
    binaries[name] = value

# Only generated evidence/reproduction scripts, never executable/profile data.
files = sorted(path for path in PACKET.rglob("*") if path.is_file()
               and not {"baseline", "__pycache__"}.intersection(path.relative_to(PACKET).parts)
               and path.suffix in {".json", ".log", ".xml", ".py", ".png"})
require(all(path.stat().st_size < 32 * 1024 * 1024 for path in files), "Unexpected oversized artifact")
DEST.mkdir(parents=True)
artifacts = {}
for path in files:
    relative = path.relative_to(PACKET)
    target = DEST / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(path, target)
    digest = native.digest(path)
    require(native.digest(target) == digest, "Copied artifact mismatch")
    artifacts[relative.as_posix()] = {"sha256": digest, "size": target.stat().st_size}

checkpoint = {
    "status": "functional_checkpoint_performance_deferred",
    "packet": "P2-05B3A", "decision": "D032", "performance_deferral": "D031",
    "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
    "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "boundary": {"Core": 21, "Gmail": 10, "Mail_UI": 19, "CLI": 14, "Quality": 17,
                 "Calendar": 15, "Platform": 12, "Search": 5, "schema": 15, "fixtures": [0, 3], "protocol": 1},
    "source_hashes": sources, "runtime_source_count": len(sources),
    "workspace_rust_inputs_rechecked": rust_count, "executables": binaries,
    "functional_reports": required,
    "limitations": ["B2 navigation failure remains open; no new performance measurement.",
                    "Generated providers and standalone/development binaries only; no live account or delivery.",
                    "Paired cached reopening retains an explicit local Watch owner; separate ordinary-engine migration proves its scoped restart.",
                    "B3B authoring/adoption/deletion, remaining Calendar work, packages/CI and real-pilot admission remain required.",
                    "Earlier failures and native diagnostics remain in the copied evidence; no warning-free or whole-readiness claim."],
}
native.write(DEST / "source-checkpoint.json", checkpoint)
artifacts["source-checkpoint.json"] = {"sha256": native.digest(DEST / "source-checkpoint.json"),
                                      "size": (DEST / "source-checkpoint.json").stat().st_size}
manifest = {"status": checkpoint["status"], "files": artifacts, "count": len(artifacts),
            "total_bytes": sum(value["size"] for value in artifacts.values())}
native.write(DEST / "logs-manifest.json", manifest)
require(all(native.digest(DEST / name) == value["sha256"] for name, value in artifacts.items()), "Archive readback mismatch")
print(json.dumps({"status": checkpoint["status"], "files": manifest["count"], "bytes": manifest["total_bytes"],
                  "runtime_sources": len(sources), "rust_inputs": rust_count, "executables": len(binaries)}))
