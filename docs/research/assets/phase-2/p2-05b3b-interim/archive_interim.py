"""Validate or freeze the functional D033 checkpoint; D031 performance is deferred.

Defaults are the explicitly planned final reports, never an automatic search for
an older pass. Override a selected attempt with --report gate=filename. --check
is read-only; --archive is the sole mode that creates the immutable destination.
Earlier malformed/failed/interrupted attempts remain byte-for-byte evidence.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

import run_native as native

ROOT, PACKET = native.ROOT.resolve(), native.OUT.resolve()
DEST = ROOT / "docs/research/assets/phase-2/p2-05b3b-interim"
sys.path.insert(0, str(ROOT / "scripts"))
from verify_google_navigation import source_manifest as runtime_manifest

REPORTS = {
    "workspace": "workspace-result-02.json",
    "production": "rust-production-build-02.result.json",
    "feature": "feature-tests-02.result.json",
    "cli_actions": "cli-actions-02.json",
    "native_actions": "native-actions-01.json",
    "remote_drafts": "remote-drafts-regression-01.json",
    "draft_send": "draft-send-regression-01.json",
    "upgrade": "upgrade-02.json",
    "native_ctest": "gui-dev-ctest-01.result.json",
}
PREFIXES = {
    "production": "rust-production-build", "feature": "feature-tests",
    "cli_actions": "cli-actions", "native_actions": "native-actions",
    "remote_drafts": "remote-drafts-regression", "draft_send": "draft-send-regression",
    "upgrade": "upgrade", "native_ctest": "gui-dev-ctest",
}
OUTPUT_SUFFIXES = {
    "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log",
    "workspace": ".workspace.log", "fixture": ".fixture.json",
    "source": ".source.json", "sources": ".source.json", "gui": ".gui.log",
    "gui_report": ".gui.json", "expectation": ".expectation.json",
}
BOUNDARY = {"Core": 22, "Gmail": 11, "Mail_UI": 20, "CLI": 15, "Quality": 18,
            "Calendar": 15, "Platform": 12, "Search": 5, "schema": 16,
            "fixtures": [0, 3], "protocol": 1}


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def within(path, directory):
    require(path.resolve().is_relative_to(directory.resolve()), "Path outside its owned directory: " + str(path))
    return path


def evidence_path(name):
    require(isinstance(name, str) and Path(name).name == name and name not in (".", ".."),
            "Expected a packet-local artifact filename")
    return within(PACKET / name, PACKET)


def read(name):
    path = evidence_path(name)
    require(path.is_file() and path.stat().st_size < 32 * 1024 * 1024, "Missing/oversized artifact: " + name)
    return json.loads(path.read_text(encoding="utf-8"))


def check_hash(name, expected):
    path = evidence_path(name)
    require(re.fullmatch(r"[0-9a-f]{64}", expected or "") is not None
            and path.is_file() and native.digest(path) == expected, "Artifact hash changed: " + name)


def same_inputs(values, expected, label):
    require(isinstance(values, dict) and values and values == expected,
            "Final source scope differs from " + label + "; select a fresh passing attempt")
    return len(values)


def latest_attempt(gate, name):
    if gate == "workspace":
        chosen = re.fullmatch(r"workspace-result-([0-9]{2})\.json", name)
        pattern = r"workspace-(?:result|source|tests|clippy|format)-([0-9]{2})(?:\.|$)"
    else:
        prefix = re.escape(PREFIXES[gate])
        suffix = r"\.result\.json" if gate in ("production", "feature", "native_ctest") else r"\.json"
        chosen = re.fullmatch(prefix + r"-([0-9]{2})" + suffix, name)
        pattern = prefix + r"-([0-9]{2})(?:\.|$)"
    require(chosen is not None, "Wrong report family for " + gate + ": " + name)
    attempt = int(chosen[1])
    require(attempt > 0, "Attempts must be numbered from 01")
    newer = [p.name for p in PACKET.iterdir()
             if (match := re.match(pattern, p.name)) and int(match[1]) > attempt]
    require(not newer, "A later complete or interrupted " + gate + " attempt exists: " + ", ".join(sorted(newer)))


def check_binaries(values, binaries, label, baseline=False):
    require(isinstance(values, dict) and values, "Missing executable identities: " + label)
    for value in values.values():
        require(isinstance(value, dict) and "path" in value, "Malformed executable identity: " + label)
        path = Path(value["path"])
        path = path if path.is_absolute() else ROOT / path
        within(path, PACKET / "baseline" if baseline else ROOT)
        require(path.is_file() and native.digest(path) == value["sha256"], "Executable changed: " + str(path))
        require("size" not in value or value["size"] == path.stat().st_size, "Executable size changed: " + str(path))
        relative = path.resolve().relative_to(ROOT).as_posix()
        current = {"sha256": value["sha256"], "size": path.stat().st_size}
        require(relative not in binaries or binaries[relative] == current, "Selected reports disagree on executable: " + relative)
        binaries[relative] = current


def check_outputs(report_name, report, required_keys):
    values = report.get("outputs_sha256")
    require(isinstance(values, dict) and set(required_keys).issubset(values), "Missing output hashes: " + report_name)
    stem = report_name.removesuffix(".json")
    for key, value in values.items():
        if key in OUTPUT_SUFFIXES:
            name = stem + OUTPUT_SUFFIXES[key]
        else:
            require(key.startswith(stem + ".") and Path(key).name == key, "Unexpected output identity: " + key)
            name = key
        check_hash(name, value)


def check_native(name, report, sources, binaries):
    stem = name.removesuffix(".result.json")
    suffixes = (".source.json", ".binaries-before.json", ".binaries-after.json", ".log",
                ".meta.json", ".LastTest.log", ".junit.xml", ".diagnostic.json")
    hashes = report["artifacts_sha256"]
    require(set(hashes) == {stem + suffix for suffix in suffixes}, "Native companion inventory is incomplete")
    for artifact, digest in hashes.items():
        check_hash(artifact, digest)
    require(report["exit_code"] == 0 and report["source_unchanged"] is True
            and report["changed_source_paths"] == [] and report["failures"] == 0, "Native execution was not frozen/passing")
    require(not any(path.startswith("build/dev/") for path in report["changed_binary_paths"]),
            "Native application changed during the selected test")
    same_inputs(read(stem + ".source.json"), sources, name)
    before, after = read(stem + ".binaries-before.json"), read(stem + ".binaries-after.json")
    require(report["binaries"] == after, "Native result and after inventory disagree")
    tested = {path: value for path, value in after.items() if path.startswith("build/dev/")}
    require(tested and all(before.get(path) == value for path, value in tested.items()), "Native binaries were not stable")
    check_binaries({path: {**value, "path": path} for path, value in tested.items()}, binaries, name)
    meta, diagnostic = read(stem + ".meta.json"), read(stem + ".diagnostic.json")
    require(meta["native_selection"] == "^gui-" and meta["includes_rust_build_fixture"] is True
            and meta["reruns_rust_workspace_tests"] is False, "Native test scope changed")
    for value in (meta, diagnostic):
        require(value["aggregate_watchdog_adjusted_under"] == "D030"
                and value["aggregate_watchdog_seconds"] == 180 and value["primary_watchdog_seconds"] == 120
                and value["child_watchdog_seconds"] == 45 and value["primary_and_child_watchdogs_unchanged"] is True
                and value["all_assertions_preserved"] is True, "Native D030 allowance changed")
    require(meta["watchdogs_unchanged"] is False and diagnostic["source_changes"] is False
            and diagnostic["watchdog_seconds"] == 180 and report["integration_watchdog_seconds"] == 180,
            "Native watchdog/source metadata is inconsistent")
    xml = ET.parse(evidence_path(stem + ".junit.xml")).getroot()
    cases = list(xml.iter("testcase"))
    expected_names = {"rust-test-build"}
    for cmake in (ROOT / "CMakeLists.txt", ROOT / "gui/CMakeLists.txt"):
        expected_names.update(re.findall(r"add_test\(NAME (gui-[\w-]+)", cmake.read_text(encoding="utf-8")))
    require(len(cases) == len(expected_names) and {c.attrib["name"] for c in cases} == expected_names
            and all(c.find("failure") is None and c.find("error") is None and c.find("skipped") is None
                    and c.attrib.get("status") == "run" for c in cases), "Native JUnit omitted or failed a required case")
    actual = [{"name": c.attrib["name"], "status": c.attrib.get("status"), "seconds": float(c.attrib["time"])} for c in cases]
    require(report["cases"] == actual and report["tests"] == len(cases)
            and report["native_tests"] == len(cases) - 1 and report["build_fixtures"] == 1,
            "Native result does not match JUnit")


def evidence_files():
    # Root result/log/source/reproduction files plus explicitly generated image
    # trees. Do not descend into profiles, blobs, arbitrary scratch or binaries.
    files = [p for p in PACKET.iterdir() if p.is_file() and p.suffix in {".json", ".log", ".xml", ".py", ".png"}]
    for directory in PACKET.iterdir():
        if directory.is_dir() and re.fullmatch(r"(?:visual|captures?)-[0-9]{2}", directory.name):
            files.extend(p for p in directory.rglob("*") if p.is_file() and p.suffix in {".png", ".json", ".log"})
    baseline_manifest = PACKET / "baseline/manifest.json"
    if baseline_manifest.is_file():
        files.append(baseline_manifest)
    for path in files:
        within(path, PACKET)
        require(not path.is_symlink() and path.stat().st_size < 32 * 1024 * 1024,
                "Unexpected linked/oversized artifact: " + str(path))
    return sorted(set(files))


def validate(reports):
    require(not DEST.exists(), "Never overwrite or complete a partially created immutable checkpoint")
    sources, runtime = native.source_manifest(), runtime_manifest()
    rust = {name: value for name, value in runtime.items()
            if name.startswith("crates/") or name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}}
    release_sources = {**rust, **{p.relative_to(ROOT).as_posix(): native.digest(p)
                                 for p in (ROOT / "fixtures").rglob("*") if p.is_file()}}
    upgrade_helpers = {"scripts/verify_draft_snapshot_upgrade.py", "scripts/verify_upgrade.py",
                       "scripts/verify_google_navigation.py", "scripts/verify_search_ingestion.py", "scripts/dev.py",
                       "scripts/toolchain.json", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
    upgrade_sources = {**release_sources, **{name: value for name, value in runtime.items() if name in upgrade_helpers}}
    binaries, selected = {}, {}
    for gate, name in reports.items():
        latest_attempt(gate, name)
        report = read(name)
        field = "status" if gate in ("workspace", "production", "feature", "native_ctest") else "result"
        require(report.get(field) == "passed" and not report.get("error"), "Unpassed gate: " + name)
        selected[gate] = {"filename": name, "sha256": native.digest(evidence_path(name)), "status": "passed"}
        if gate == "workspace":
            attempt = name.removeprefix("workspace-result-").removesuffix(".json")
            manifest = "workspace-source-" + attempt + ".json"
            check_hash(manifest, report["source_check"]["manifest_sha256"])
            # Cargo workspace checks compile Rust inputs. The broad manifest
            # also records concurrent GUI/Python observations, whose final
            # sources are independently required by the native/provider gates.
            same_inputs({path: digest for path, digest in read(manifest).items()
                         if path.startswith("crates/") or path in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}}, rust, name)
            require(report["exit_code"] == 0 and report["source_check"]["changed_rust_sources"] == []
                    and report["summary"]["failed"] == 0 and report["summary"]["passed"] > 0
                    and {c["name"] for c in report["checks"]} == {"tests", "clippy", "format"}
                    and len(report["checks"]) == 3 and all(c["exit_code"] == 0 for c in report["checks"]),
                    "Incomplete ordinary workspace verification")
            require(set(report["logs"]) == {"tests", "clippy", "format"}, "Missing workspace logs")
            for key, value in report["logs"].items():
                require(value["path"] == f"workspace-{key}-{attempt}.log", "Wrong workspace log identity")
                check_hash(value["path"], value["sha256"])
        elif gate in ("production", "feature"):
            stem = name.removesuffix(".result.json")
            check_hash(stem + ".source.json", report["source_manifest_sha256"])
            check_hash(stem + ".log", report["log_sha256"])
            same_inputs(read(stem + ".source.json"), release_sources, name)
            require(report["exit_code"] == 0 and report["changed_sources"] == [], "Release sources changed during verification")
            command = report["command"]
            require("--release" in command and "--locked" in command, "Expected locked Release verification")
            if gate == "feature":
                require(command[0] == "test" and "verification-google" in command and "verification_profile" in command
                        and len(report["suites"]) == 2 and all(row["status"] == "ok" and row["failed"] == 0
                                                               and row["passed"] > 0 for row in report["suites"]),
                        "Missing isolated unit/process tests")
                require(set(report["executables"]) == {"engine"}, "Isolated engine identity is missing")
            else:
                require(command[0] == "build" and {"fm-engine", "fm-cli"}.issubset(command)
                        and "verification-google" not in command, "Expected ordinary engine/CLI Release build")
                require(set(report["executables"]) == {"engine", "cli"}, "Ordinary Release identities are missing")
            check_binaries(report["executables"], binaries, name)
        elif gate == "native_ctest":
            check_native(name, report, sources, binaries)
        elif gate == "upgrade":
            same_inputs(report["sources"], upgrade_sources, name)
            require(report["scenario"] == "schema-15-to-16-draft-provenance"
                    and report["before"]["schema"] == 15 and report["after"]["schema"] == 16
                    and report["refused"]["logical_database_sha256"] == report["after"]["logical_database_sha256"]
                    and report["refused"]["retained_blobs"] == report["after"]["retained_blobs"]
                    and report["before"]["retained_blobs"] == report["after"]["retained_blobs"]
                    and report["before"]["table_digests"] == report["after"]["table_digests"],
                    "Wrong upgrade/refusal boundary")
            require(len(report["artifacts"]) == 7, "Upgrade log set is incomplete")
            for value in report["artifacts"]:
                require(value["name"].startswith(name.removesuffix(".json") + "."), "Cross-attempt upgrade artifact")
                check_hash(value["name"], value["sha256"])
            refusal = evidence_path(name.removesuffix(".json") + ".old-engine-refusal.log")
            require("schema_mismatch" in refusal.read_text(encoding="utf-8"), "Upgrade did not prove schema refusal")
            check_binaries(report["executables"]["new"], binaries, name)
            check_binaries(report["executables"]["old"], binaries, name, baseline=True)
        else:
            if gate == "draft_send":
                same_inputs(report["sources"], runtime, name)
                required_outputs = {name.removesuffix(".json") + suffix
                                    for suffix in (".engine.log", ".watch.log", ".personal.log", ".workspace.log", ".fixture.json")}
            else:
                same_inputs(read(name.removesuffix(".json") + ".source.json"), runtime, name)
                required_outputs = {"engine", "watch", "personal", "workspace", "fixture",
                                    "source" if gate == "remote_drafts" else "sources"}
                if gate == "native_actions":
                    required_outputs.update({"gui", "gui_report", "expectation"})
            check_outputs(name, report, required_outputs)
            require({"cli", "engine"}.issubset(report["executables"]), "Provider engine/CLI identities are missing")
            check_binaries(report["executables"], binaries, name)
            fixture = read(name.removesuffix(".json") + ".fixture.json")
            require(fixture["status"] == "complete" and fixture["errors"] == []
                    and fixture["transport"] == "in_memory_only" and fixture["native_vault"] is False,
                    "Provider isolation/final shutdown failed")
            require(fixture["counters"] == report["provider_counters"], "Final provider counters changed")
            if gate in ("cli_actions", "native_actions"):
                expected_scenario = "isolated-google-draft-actions" + ("-native" if gate == "native_actions" else "")
                require(report["scenario"] == expected_scenario and report["retained_source_count"] == 12
                        and report["retained_changed_source_count"] == (2 if gate == "cli_actions" else 0),
                        "Incomplete action/source scenario")
                require(set(report["verified_provider_publications"]) == {"drafts", "sends", "deletions"},
                        "Missing independent wire/provider publication verification")
                if gate == "native_actions":
                    require(set(report["executables"]) == {"cli", "engine", "gui"}, "Native executable identity missing")
                    require(read(name.removesuffix(".json") + ".gui.json")["result"] == "passed", "Native action report failed")
    artifacts = {p.relative_to(PACKET).as_posix(): {"sha256": native.digest(p), "size": p.stat().st_size}
                 for p in evidence_files()}
    require(artifacts and all(value["filename"] in artifacts for value in selected.values()), "Missing selected evidence")
    return sources, binaries, selected, artifacts, len(rust)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="Validate only; creates no files (default)")
    mode.add_argument("--archive", action="store_true", help="Create the immutable checkpoint after validation")
    parser.add_argument("--report", action="append", default=[], metavar="GATE=FILENAME")
    args = parser.parse_args()
    reports = REPORTS.copy()
    for override in args.report:
        gate, separator, name = override.partition("=")
        require(separator and gate in reports, "Unknown explicit gate: " + override)
        reports[gate] = name
    sources, binaries, selected, artifacts, rust_count = validate(reports)
    summary = {"status": "checked", "artifacts": len(artifacts), "runtime_sources": len(sources),
               "rust_inputs": rust_count, "executables": len(binaries), "reports": reports}
    if not args.archive:
        print(json.dumps(summary, indent=2))
        return 0
    within(DEST, ROOT / "docs/research/assets/phase-2")
    DEST.mkdir(parents=True, exist_ok=False)
    for relative, value in artifacts.items():
        source, target = PACKET / relative, DEST / relative
        within(source, PACKET)
        within(target, DEST)
        require(native.digest(source) == value["sha256"], "Evidence changed during archival: " + relative)
        target.parent.mkdir(parents=True, exist_ok=True)
        with source.open("rb") as input_file, target.open("xb") as output_file:
            shutil.copyfileobj(input_file, output_file)
        require(native.digest(target) == value["sha256"], "Copied artifact mismatch: " + relative)
    require(native.source_manifest() == sources, "Runtime source changed during archival")
    require({p.relative_to(PACKET).as_posix() for p in evidence_files()} == set(artifacts), "Evidence set changed during archival")
    require(all(native.digest(PACKET / name) == value["sha256"] for name, value in artifacts.items()),
            "Source evidence changed during archival")
    require(all(native.digest(ROOT / path) == value["sha256"] for path, value in binaries.items()),
            "Executable changed during archival")
    checkpoint = {
        "status": "functional_checkpoint_performance_deferred", "packet": "P2-05B3B", "decision": "D033",
        "performance_deferral": "D031", "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "boundary": BOUNDARY, "source_hashes": sources, "runtime_source_count": len(sources),
        "workspace_rust_inputs_rechecked": rust_count, "executables": binaries, "functional_reports": selected,
        "historical_evidence_policy": "All retained earlier attempts, including failures, malformed reports and interrupted logs, are historical observations; only selected final reports satisfy gates.",
        "excluded_data": ["Executable bytes (including baseline)", "Disposable profiles, database/blob/vault trees", "Python caches"],
        "limitations": ["D031 retains the B2 navigation failure; no new performance run or performance acceptance.",
                        "D034 requires complete GUI/CLI account addition and removal; disconnect alone is insufficient. Remaining Calendar invitations, recurrence and running-app reminders also remain required.",
                        "Generated in-memory providers/vaults and development binaries only; no real accounts, delivery, package or CI acceptance.",
                        "Cached isolated inspection retains a local Watch owner; the separate ordinary-engine upgrade covers only its recorded synthetic restart/refusal.",
                        "Native CTest includes the Rust no-run build fixture and startup-recovery case; it does not rerun the Rust workspace suite.",
                        "Native diagnostics and earlier failures are preserved; no warning-free or complete Google readiness claim.",
                        "Native target/debug/deps inventories are retained observations; current execution identities rechecked here are build/dev and selected Release/baseline binaries."]}
    native.write(DEST / "source-checkpoint.json", checkpoint)
    artifacts["source-checkpoint.json"] = {"sha256": native.digest(DEST / "source-checkpoint.json"),
                                           "size": (DEST / "source-checkpoint.json").stat().st_size}
    manifest = {"status": checkpoint["status"], "files": artifacts, "count": len(artifacts),
                "total_bytes": sum(value["size"] for value in artifacts.values())}
    native.write(DEST / "logs-manifest.json", manifest)
    require(all(native.digest(DEST / name) == value["sha256"] for name, value in artifacts.items()), "Archive readback mismatch")
    print(json.dumps({**summary, "status": checkpoint["status"], "files": len(artifacts), "bytes": manifest["total_bytes"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
