"""Validate or freeze the functional D035 checkpoint; D031 performance is deferred.

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
DEST = ROOT / "docs/research/assets/phase-2/p2-06b-interim"
sys.path.insert(0, str(ROOT / "scripts"))
from verify_google_navigation import source_manifest as runtime_manifest
from verify_google_calendar import BASE_UNIX, Ledger, expected_accounts, same, verify_publications, validate_status
from verify_google_calendar_gui import native_expectation, validate_native

REPORTS = {
    "workspace": "workspace-result-02.json",
    "production": "rust-production-build-02.result.json",
    "feature": "feature-tests-02.result.json",
    "cli_calendar": "cli-provider-01.json",
    "native_calendar": "native-provider-01.json",
    "cli_actions": "cli-draft-actions-regression-01.json",
    "remote_drafts": "remote-drafts-regression-01.json",
    "draft_send": "draft-send-regression-01.json",
    "upgrade": "upgrade-01.json",
    "native_ctest": "gui-dev-ctest-01.result.json",
}
PREFIXES = {
    "production": "rust-production-build", "feature": "feature-tests",
    "cli_calendar": "cli-provider", "native_calendar": "native-provider",
    "cli_actions": "cli-draft-actions-regression",
    "remote_drafts": "remote-drafts-regression", "draft_send": "draft-send-regression",
    "upgrade": "upgrade", "native_ctest": "gui-dev-ctest",
}
OUTPUT_SUFFIXES = {
    "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log",
    "workspace": ".workspace.log", "fixture": ".fixture.json",
    "source": ".source.json", "sources": ".source.json", "gui": ".gui.log",
    "gui_report": ".gui.json", "expectation": ".expectation.json",
    "ledger": ".ledger.json", "refusal": ".refusal.log",
}
BOUNDARY = {"Core": 23, "Gmail": 11, "Mail_UI": 21, "CLI": 16, "Quality": 19,
            "Calendar": 16, "Platform": 12, "Search": 5, "schema": 17,
            "fixtures": [0, 3], "protocol": 1}

# Only generated evidence families from this packet. Never traverse a profile,
# cache, blob tree or baseline executable directory to discover more evidence.
ARTIFACT_PREFIXES = {
    *PREFIXES.values(), "workspace-result", "workspace-source", "workspace-tests",
    "workspace-clippy", "workspace-format", "calendar-baseline-seed", "core-calendar",
    "core-clippy", "core-engine-clippy", "engine-calendar", "engine-default-process", "engine-feature-process",
    "engine-provider-clippy", "provider-calendar", "cli-tests", "cli-clippy",
    "gui-build", "gui-controls", "gui-controls-build", "gui-driver-preflight",
    "gui-release-build", "gui-dev-build", "gui-dev-source", "gui-dev-result", "gui-dev-ctest",
    "native-diagnostics",
}
ARTIFACT_SUFFIXES = {
    ".json", ".log", ".xml", ".png", ".result.json", ".source.json", ".sources.json",
    *OUTPUT_SUFFIXES.values(), ".LastTest.log", ".junit.xml", ".meta.json", ".diagnostic.json",
    ".binaries-before.json", ".binaries-after.json", ".baseline-engine.log", ".baseline-owner.log",
    ".before-engine.log", ".before-owner.log", ".after-engine.log", ".after-owner.log",
    ".old-engine-refusal.log", ".reopened-engine.log", ".reopened-owner.log",
}
HELPERS = {"archive_interim.py", "bind_git_source.py", "run_gui_release.py", "run_native.py",
           "run_release.py", "run_scoped.py", "run_workspace.py", "verify_calendar_seed.py"}


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


def check_calendar(gate, name, report, fixture):
    """Recheck recorded generated data only; no Store or provider is opened."""
    is_native = gate == "native_calendar"
    stem = name.removesuffix(".json")
    accounts = expected_accounts()
    expected_counts = {"publications": 8 if is_native else 24, "mutation_attempts": 8 if is_native else 24,
                       "lost_responses": 0 if is_native else 4, "acknowledged_responses": 8 if is_native else 20}
    require(report["scenario"] == "isolated-google-calendar-" + ("native" if is_native else "cli")
            and report["version"] == 1 and report["base_unix_secs"] == BASE_UNIX,
            "Wrong generated Calendar scenario/version/date")
    require(report["journal_rows"] == (8 if is_native else 36)
            and re.fullmatch(r"[0-9a-f]{64}", report["journal_sha256"])
            and report["offline_queries_preserved_journal_and_http"] is True
            and report["final_owner_drained_engine"] is True,
            "Calendar journal or owner/offline proof is incomplete")
    saved = read(stem + ".ledger.json")
    ledger = Ledger(accounts)
    if is_native:
        require(report["initial_sources"] == {"visible_sources": 14, "hidden_retained_sources": 0}
                and report["final_sources"] == {"visible_sources": 14, "hidden_retained_sources": 2}
                and set(report["executables"]) == {"cli", "engine", "gui"}, "Native source/executable scope changed")
        observed = read(stem + ".gui.json")
        expectation = native_expectation(accounts)
        require(read(stem + ".expectation.json") == expectation, "Independent native expectation differs")
        require(report["gui_exit_code"] == 0 and observed["result"] == "passed"
                and isinstance(observed["gui_pid"], int) and observed["gui_pid"] > 0
                and report["native_accounts"] == observed["accounts"], "Actual native report is missing or differs")
        validate_native(observed, expectation, accounts, ledger)
    else:
        require(report["independently_verified_initial_sources"] == 14
                and report["window"] == {"from": "2026-09-13", "to": "2026-09-15"}
                and set(report["executables"]) == {"cli", "engine", "ordinary_engine"},
                "CLI Calendar initial sources/ordinary refusal identity is missing")
        require(report["expected_provider_publications"] == saved["publications"], "Expected publication evidence changed")
        by_id = {account["id"]: account for account in accounts}
        for publication in saved["publications"]:
            require(publication["account"] in by_id and publication["response"] in {"lost_response", "acknowledged"},
                    "Unexpected Calendar publication account/response")
            ledger.mutation(by_id[publication["account"]], publication["event"], publication["method"],
                            publication["body"], publication["response"] == "lost_response")
        requests, statuses = saved["requests"], saved["statuses"]
        require(len(requests) == len(statuses) == 36 and set(requests) == set(statuses), "Missing exact CLI request/status evidence")
        states = {}
        for identity, request in requests.items():
            account_id, separator, request_id = identity.partition("/")
            require(separator and account_id in by_id and request["request_id"] == request_id,
                    "Calendar request/account identity differs")
            status = statuses[identity]
            validate_status(status, by_id[account_id], request, status["state"], status["target"]["event"], ledger)
            states[status["state"]] = states.get(status["state"], 0) + 1
        require(states == {"acknowledged": 20, "reconciled": 4, "prepared": 12}, "CLI certainty/refusal coverage changed")
        refusals = report["restart_refusals"]
        expected_refusals = {
            "feature_unflagged": ("engine", [], "isolated_build_guard",
                                  "fm-engine: This isolated build requires --verification-google and a new disposable live profile"),
            "feature_flagged": ("engine", ["--verification-google"], "isolation_marker",
                                "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine"),
            "ordinary_unflagged": ("ordinary_engine", [], "isolation_marker",
                                   "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine"),
        }
        require(len(refusals) == 3 and {value["case"] for value in refusals} == set(expected_refusals),
                "All three isolated-build/profile-marker refusal cases are required")
        logged_refusals = [json.loads(line) for line in evidence_path(stem + ".refusal.log").read_text(encoding="utf-8").splitlines()]
        require(logged_refusals == refusals, "Actual refusal log differs from the reported per-case evidence")
        for value in refusals:
            executable, flags, kind, diagnostic = expected_refusals[value["case"]]
            require(value["executable"] == executable and value["flags"] == flags and value["diagnostic_kind"] == kind
                    and isinstance(value["exit_code"], int) and value["exit_code"] != 0
                    and value["output"].splitlines() == [diagnostic] and value["journal_unchanged"] is True
                    and value["journal_before_sha256"] == value["journal_after_sha256"] == report["journal_sha256"],
                    "Wrong exact refusal diagnostic, arguments or unchanged-journal proof: " + value["case"])
    same(saved["sources"], ledger.sources, "Independent final Calendar source ledger")
    same(saved["publications"], ledger.publications, "Independent Calendar before/after ledger")
    verified = verify_publications(fixture, ledger)
    require(verified == expected_counts and report["provider_verification" if is_native else "verified_provider"] == verified,
            "Calendar attempts, publications or lost-response counts differ")
    return {**verified, "journal_rows": report["journal_rows"], "independent_initial_sources": 14,
            "intent_version": 2, "notification_delivery": "unverified",
            "actual_native_report": is_native, "offline_queries_preserved_journal_and_http": True,
            "final_owner_drained_engine": True}


def evidence_files():
    # Root result/log/source/reproduction files plus explicitly generated image
    # trees. Do not descend into profiles, blobs, arbitrary scratch or binaries.
    pattern = re.compile("(?:" + "|".join(re.escape(prefix) for prefix in ARTIFACT_PREFIXES) + r")-[0-9]{2}(.+)$")
    files = []
    for path in PACKET.iterdir():
        match = pattern.fullmatch(path.name)
        if path.is_file() and (path.name in HELPERS or (match and match[1] in ARTIFACT_SUFFIXES)):
            files.append(path)
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
    binaries, selected, scope_checks = {}, {}, {}
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
            old_sources = read(manifest)
            non_rust_changes = [{"path": path, "workspace_sha256": old_sources.get(path), "current_sha256": runtime.get(path)}
                                for path in sorted(old_sources.keys() | runtime.keys())
                                if path not in rust and old_sources.get(path) != runtime.get(path)]
            scope_checks[gate] = {"rust_inputs_rechecked": len(rust), "later_non_rust_inputs": non_rust_changes,
                                  "scope": "Cargo checks Rust; current GUI/verifier inputs require their separate final gates."}
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
            scope_checks[gate] = {"tests": report["tests"], "native_tests": report["native_tests"],
                                  "build_fixtures": report["build_fixtures"], "failures": 0,
                                  "reruns_rust_workspace_tests": False, "aggregate_watchdog_seconds": 180,
                                  "primary_watchdog_seconds": 120, "child_watchdog_seconds": 45,
                                  "diagnostic_count": report["diagnostic_count"]}
        elif gate == "upgrade":
            same_inputs(report["sources"], upgrade_sources, name)
            require(report["scenario"] == "schema-16-to-17-calendar-intents" and report["version"] == 3
                    and report["before"]["schema"] == 16 and report["after"]["schema"] == 17
                    and report["refused"]["logical_database_sha256"] == report["after"]["logical_database_sha256"]
                    and report["refused"]["retained_blobs"] == report["after"]["retained_blobs"]
                    and report["before"]["retained_blobs"] == report["after"]["retained_blobs"]
                    and report["before"]["table_digests"] == report["after"]["table_digests"],
                    "Wrong upgrade/refusal boundary")
            require(len(report["calendar"]["requests"]) == len(report["calendar"]["receipts"]) == 4
                    and [row["kind"] for row in report["calendar"]["requests"]] == ["create", "edit", "rsvp", "create"]
                    and [row["state"] for row in report["calendar"]["receipts"]] == ["fake_acknowledged"] * 3 + ["pending"]
                    and report["capture_receipt"] and report["draft_id"], "Legacy Calendar/capture preservation evidence is missing")
            upgrade_logs = {name.removesuffix(".json") + suffix for suffix in (
                ".before-engine.log", ".before-owner.log", ".after-engine.log", ".after-owner.log",
                ".old-engine-refusal.log", ".reopened-engine.log", ".reopened-owner.log")}
            require(len(report["artifacts"]) == 7 and {row["name"] for row in report["artifacts"]} == upgrade_logs,
                    "Upgrade log set is incomplete")
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
                if gate in ("native_calendar", "remote_drafts"):
                    required_outputs.update({"gui", "gui_report", "expectation"})
                if gate in ("cli_calendar", "native_calendar"):
                    required_outputs.add("ledger")
                if gate == "cli_calendar":
                    required_outputs.add("refusal")
            check_outputs(name, report, required_outputs)
            require({"cli", "engine"}.issubset(report["executables"]), "Provider engine/CLI identities are missing")
            check_binaries(report["executables"], binaries, name)
            fixture = read(name.removesuffix(".json") + ".fixture.json")
            require(fixture["status"] == "complete" and fixture["errors"] == []
                    and fixture["transport"] == "in_memory_only" and fixture["native_vault"] is False,
                    "Provider isolation/final shutdown failed")
            require(fixture["counters"] == report["provider_counters"], "Final provider counters changed")
            if gate in ("cli_calendar", "native_calendar"):
                scope_checks[gate] = check_calendar(gate, name, report, fixture)
            elif gate == "cli_actions":
                require(report["scenario"] == "isolated-google-draft-actions" and report["retained_source_count"] == 12
                        and report["retained_changed_source_count"] == 2,
                        "Incomplete action/source scenario")
                require(set(report["verified_provider_publications"]) == {"drafts", "sends", "deletions"},
                        "Missing independent wire/provider publication verification")
                require(fixture["counters"]["draft_delete_attempts"] == 4 and fixture["counters"]["draft_delete_unknown"] == 2
                        and fixture["counters"]["draft_deleted"] == 2 and fixture["counters"]["draft_created"] == 4
                        and fixture["counters"]["draft_updated"] == fixture["counters"]["send_completed"] == 1,
                        "Draft action regression omitted required behavior")
            elif gate == "draft_send":
                require(report["scenario"] == "isolated-google-send-cli" and len(report["submissions"]) == 4
                        and report["independent_send_operations"] == 4 and report["independent_all_google_operations_after_send"] == 9
                        and fixture["counters"]["send_completed"] == 4, "Incomplete draft/Send regression")
            elif gate == "remote_drafts":
                require(report["scenario"] == "isolated-remote-google-drafts" and len(report["accounts"]) == 2
                        and report["independent_retained_sources"] == 104
                        and report["offline_provider_counters"] == fixture["counters"]
                        and set(report["executables"]) == {"cli", "engine", "gui"}, "Incomplete cached remote Drafts regression")
                observed = read(name.removesuffix(".json") + ".gui.json")
                require(report["gui_exit_code"] == 0 and report["gui_result"] == observed
                        and observed["passed"] is True and not observed["error"] and observed["gui_pid"] > 0,
                        "Actual cached remote Drafts GUI did not pass")
    artifacts = {p.relative_to(PACKET).as_posix(): {"sha256": native.digest(p), "size": p.stat().st_size}
                 for p in evidence_files()}
    require(artifacts and all(value["filename"] in artifacts for value in selected.values()), "Missing selected evidence")
    return sources, binaries, selected, artifacts, len(rust), scope_checks


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
    sources, binaries, selected, artifacts, rust_count, scope_checks = validate(reports)
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
        "status": "functional_checkpoint_performance_deferred", "packet": "P2-06B", "decision": "D035",
        "performance_deferral": "D031", "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "boundary": BOUNDARY, "source_hashes": sources, "runtime_source_count": len(sources),
        "workspace_rust_inputs_rechecked": rust_count, "executables": binaries, "functional_reports": selected,
        "source_scope_checks": scope_checks,
        "historical_evidence_policy": "All retained earlier attempts, including failures, malformed reports and interrupted logs, are historical observations; only selected final reports satisfy gates.",
        "excluded_data": ["Executable bytes (including baseline)", "Disposable profiles, database/blob/vault trees", "Python caches"],
        "limitations": ["D031 retains the B2 navigation failure; no new performance run or performance acceptance.",
                        "D034 account addition/removal, recurring Calendar workflows and running-app reminders remain required; this packet covers ordinary organizer invitations and self-RSVP only.",
                        "Generated in-memory providers/vaults and development binaries only; no real accounts, delivery, package or CI acceptance.",
                        "Cached isolated inspection retains a local Watch owner; the separate ordinary-engine upgrade covers only its recorded synthetic restart/refusal.",
                        "Native CTest includes the Rust no-run build fixture and startup-recovery case; it does not rerun the Rust workspace suite.",
                        "Calendar ACKs and observed-state reconciliation do not prove notification delivery; no automatic retry of unknown work is inferred.",
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
