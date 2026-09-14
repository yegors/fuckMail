"""Validate or freeze the functional D036 checkpoint; D031 performance is deferred.

Defaults are the explicitly planned final reports, never an automatic search for
an older pass. Override a selected attempt with --report gate=filename. --check
is read-only; --archive is the sole mode that creates the immutable destination.
Earlier malformed/failed/interrupted attempts remain byte-for-byte evidence.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

import run_native as native

ROOT, PACKET = native.ROOT.resolve(), native.OUT.resolve()
DEST = ROOT / "docs/research/assets/phase-2/p2-01b-interim"
BASE_COMMIT = "71291298f5ea00cf56dedf502eb917c76fcdcb20"
sys.path.insert(0, str(ROOT / "scripts"))
from verify_google_navigation import source_manifest as runtime_manifest
from verify_google_calendar import BASE_UNIX, Ledger, expected_accounts, same, verify_publications, validate_status
from verify_google_calendar_gui import native_expectation, validate_native
from verify_google_account_sources import source_inputs as account_source_inputs
from verify_google_accounts_gui import native_expectation as account_native_expectation
from verify_google_navigation import expected_message, LIMITS

REPORTS = {
    "workspace": "workspace-result-03.json",
    "production": "rust-production-build-02.result.json",
    "feature": "feature-tests-02.result.json",
    "feature_cli": "release-cli-feature-tests-02.result.json",
    "cli_accounts": "cli-accounts-01.json",
    "native_accounts": "native-accounts-01.json",
    "cli_calendar": "cli-provider-01.json",
    "native_calendar": "native-provider-01.json",
    "cli_actions": "cli-draft-actions-regression-01.json",
    "remote_drafts": "remote-drafts-regression-01.json",
    "draft_send": "draft-send-regression-01.json",
    "upgrade": "upgrade-02.json",
    "native_ctest": "gui-dev-ctest-01.result.json",
}
PREFIXES = {
    "production": "rust-production-build", "feature": "feature-tests",
    "feature_cli": "release-cli-feature-tests",
    "cli_accounts": "cli-accounts", "native_accounts": "native-accounts",
    "cli_calendar": "cli-provider", "native_calendar": "native-provider",
    "cli_actions": "cli-draft-actions-regression",
    "remote_drafts": "remote-drafts-regression", "draft_send": "draft-send-regression",
    "upgrade": "upgrade", "native_ctest": "gui-dev-ctest",
}
OUTPUT_SUFFIXES = {
    "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log",
    "workspace": ".workspace.log", "fixture": ".fixture.json",
    "source": ".source.json", "sources": ".source.json", "gui": ".gui.log",
    "gui_report": ".gui.json", "failure_png": ".gui.failure.png", "expectation": ".expectation.json",
    "ledger": ".ledger.json", "refusal": ".refusal.log",
    "ordinary_refusal": ".ordinary-refusal.log", "feature_refusal": ".feature-refusal.log",
}
BOUNDARY = {"Core": 24, "Gmail": 12, "Mail_UI": 22, "CLI": 17, "Quality": 20,
            "Calendar": 17, "Platform": 13, "Search": 5, "schema": 18,
            "fixtures": [0, 3], "protocol": 1}

# Only generated evidence families from this packet. Never traverse a profile,
# cache, blob tree or baseline executable directory to discover more evidence.
ARTIFACT_PREFIXES = {
    *PREFIXES.values(), "workspace-result", "workspace-source", "workspace-tests",
    "workspace-clippy", "workspace-format", "calendar-baseline-seed", "core-calendar",
    "core-clippy", "core-engine-clippy", "engine-calendar", "engine-default-process", "engine-feature-process",
    "engine-provider-clippy", "provider-calendar", "cli-tests", "cli-clippy",
    "gui-build", "gui-controls", "gui-controls-build", "gui-driver-preflight", "gui-send-controls",
    "gui-release-build", "gui-dev-build", "gui-dev-source", "gui-dev-result", "gui-dev-ctest",
    "native-diagnostics", "release-sequence", "cli-feature-tests", "core-check", "core-lifecycle",
    "core-calendar-read", "core-workspace-fixes", "google-auth", "engine-auth", "engine-account-fixture",
    "engine-account-process", "engine-default-clippy", "engine-feature-clippy",
}
ARTIFACT_SUFFIXES = {
    ".json", ".log", ".xml", ".png", ".result.json", ".source.json", ".sources.json", ".note.json",
    *OUTPUT_SUFFIXES.values(), ".LastTest.log", ".junit.xml", ".meta.json", ".diagnostic.json",
    ".binaries-before.json", ".binaries-after.json", ".baseline-engine.log", ".baseline-owner.log",
    ".before-engine.log", ".before-owner.log", ".after-engine.log", ".after-owner.log",
    ".old-engine-refusal.log", ".reopened-engine.log", ".reopened-owner.log",
}
HELPERS = {"archive_interim.py", "bind_git_source.py", "run_gui_release.py", "run_native.py",
           "run_release.py", "run_scoped.py", "run_workspace.py"}


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
        suffix = r"\.result\.json" if gate in ("production", "feature", "feature_cli", "native_ctest") else r"\.json"
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


def check_account_sources(value):
    expected = []
    for account in expected_accounts():
        for remote in ("vr0000", "vr0001"):
            message = expected_message(account, remote, BASE_UNIX)
            expected.append({"account": account["id"], "remote_id": remote,
                             "source_sha256": message["source_hash"], "source_size": len(message["source"]),
                             "conversation_id": message["conversation_id"]})
    expected.sort(key=lambda row: (row["account"], row["remote_id"]))
    require(value == {"messages": expected, "calendar_sources": 2, "threads": 2},
            "Account source receipts differ from independently generated MIME identities")


def check_accounts(gate, name, report, fixture):
    """Validate retained JSON only: never open the temporary profile or vault."""
    is_native = gate == "native_accounts"
    stem = name.removesuffix(".json")
    require(report["version"] == 1 and report["base_unix_secs"] == BASE_UNIX and report["limits"] == LIMITS
            and report["scenario"] == ("isolated-google-account-lifecycle-native" if is_native else "isolated-google-accounts-cli"),
            "Wrong account lifecycle scenario/date/limits")
    account_fixture = fixture["account_fixture"]
    require(account_fixture == report["account_fixture"] and account_fixture["enabled"] is True
            and account_fixture["mode"] == "accounts-v1" and account_fixture["initial_bound_accounts"] == 0
            and account_fixture["credential_accounts"] == [] and account_fixture["errors"] == []
            and account_fixture["active_callbacks"] == account_fixture["active_vault_calls"] == 0,
            "Account fixture or final native credential/owner evidence is inconsistent")
    counts = ({"auth_attempts": 3, "callbacks_sent": 3, "callbacks_denied": 0, "token_exchanges": 3,
               "jwks_reads": 3, "vault_store_attempts": 3, "vault_store_completed": 3,
               "vault_remove_attempts": 4, "vault_remove_completed": 3, "vault_remove_failed": 1}
              if is_native else
              {"auth_attempts": 8, "callbacks_sent": 8, "callbacks_denied": 1, "token_exchanges": 7,
               "jwks_reads": 7, "vault_store_attempts": 5, "vault_store_completed": 5,
               "vault_remove_attempts": 5, "vault_remove_completed": 4, "vault_remove_failed": 1})
    require(all(account_fixture[key] == value for key, value in counts.items())
            and account_fixture["consumed_steps"] == list(range(1, (3 if is_native else 8) + 1)),
            "Account fixture omitted an explicit OAuth/cleanup boundary")
    for key in ("modify_completed", "draft_created", "draft_updated", "draft_deleted", "send_completed",
                "calendar_mutations", "calendar_mutation_attempts"):
        require(fixture["counters"][key] == 0, "Unsolicited provider write during account lifecycle: " + key)
    check_account_sources(report["sources_checked" if is_native else "initial_sources"])
    retained = report["retained_content"]
    require(set(retained) == {"tables", "blobs"} and retained["blobs"]
            and set(retained["tables"]) == {"messages", "conversations", "attachments", "drafts", "capture_operations",
                                          "calendar_sources", "calendar_visible_sources", "google_draft_sources",
                                          "google_message_state", "google_mail_coverage", "calendar_write_operations",
                                          "non_removal_operations"}
            and retained["tables"]["messages"]["rows"] == 4
            and retained["tables"]["conversations"]["rows"] == 2
            and retained["tables"]["drafts"]["rows"] == 2
            and retained["tables"]["calendar_sources"]["rows"] == 2
            and retained["tables"]["calendar_visible_sources"]["rows"] == 2,
            "Incomplete account retained-content inventory")
    for group in retained.values():
        for value in group.values():
            require(re.fullmatch(r"[0-9a-f]{64}", value["sha256"]), "Malformed retained content digest")
    if is_native:
        require(set(report["executables"]) == {"cli", "engine", "gui"} and report["gui_exit_code"] == 0
                and report["offline_queries_preserved_content_and_provider"] is True
                and report["final_owner_drained_engine"] is True,
                "Native account owner/offline verification is incomplete")
        require(report["display_zone_setup"]["preferences"]["calendar_display_time_zone"] == "UTC",
                "Native Calendar date window requires the recorded explicit UTC display preference")
        observed, expectation = read(stem + ".gui.json"), read(stem + ".expectation.json")
        # Reconstruct expected human input without reading any profile path.
        require(expectation == account_native_expectation(Path(expectation["client_config_path"]).parent,
                                                           Path(report["executables"]["cli"]["path"])),
                "Native account independent expectation changed")
        require(observed["version"] == 1 and observed["result"] == "passed" and not observed.get("error")
                and isinstance(observed["gui_pid"], int) and observed["gui_pid"] > 0
                and observed["browser_handoffs"] == observed["fixture_step_count"] == 3
                and observed["active_account_count"] == 0 and observed["archive_account_count"] == 2
                and observed["unsaved_buffer"] is False,
                "Native account report is incomplete")
        provider_before = report.get("provider_before_restore")
        provider_after = report.get("provider_after_restore")
        require(isinstance(provider_before, dict) and isinstance(provider_after, dict)
                and set(provider_before) == {"updated_unix_ms", "captured_after_unix_ms", "counters"}
                and set(provider_after) == {"updated_unix_ms", "counters"}
                and provider_before == observed.get("provider_before_restore")
                and provider_after == {"updated_unix_ms": fixture["updated_unix_ms"], "counters": fixture["counters"]}
                and report.get("provider_counter_exclusions") == ["vault_loads", "vault_stores"],
                "Native Restore requires the actual pre-Restore sample and final authoritative fixture counters")
        before_counters, after_counters = provider_before["counters"], provider_after["counters"]
        require(isinstance(before_counters, dict) and isinstance(after_counters, dict)
                and set(before_counters) == set(after_counters)
                and {"vault_loads", "vault_stores", "http_started", "http_completed", "http_cancelled", "http_rejected",
                     "raw_active", "raw_held_active", "calendar_reads", "draft_read"} <= set(before_counters)
                and all(type(value) is int and value >= 0 for values in (before_counters, after_counters)
                        for value in values.values()), "Native Restore counter maps are incomplete or malformed")
        timestamps = (report.get("gui_started_unix_ms"), provider_before["captured_after_unix_ms"],
                      provider_before["updated_unix_ms"], report.get("gui_finished_unix_ms"),
                      provider_after["updated_unix_ms"])
        require(all(type(value) is int and value > 0 for value in timestamps)
                and timestamps[0] <= timestamps[1] <= timestamps[2] <= timestamps[3]
                and timestamps[4] >= timestamps[2], "Native Restore fixture sample is stale or outside the actual GUI run")
        require(before_counters["http_started"] > 0
                and before_counters["http_started"] == before_counters["http_completed"] + before_counters["http_cancelled"] + before_counters["http_rejected"]
                and before_counters["raw_active"] == before_counters["raw_held_active"] == 0,
                "Native Restore baseline does not prove the explicit acquisition had drained")
        require(all(before_counters[key] == after_counters[key] for key in before_counters
                    if key not in {"vault_loads", "vault_stores"}),
                "Restore/removal/archive triggered additional provider activity")
        expected = expectation["accounts"]
        require([row["account"] for row in observed["accounts"]] == [row["account"] for row in expected],
                "Native account identities differ")
        operations = {}
        for row, wanted in zip(observed["accounts"], expected, strict=True):
            require(row["add_lifecycle"] == {"state": "active", "revision": 1, "cleanup": None}
                    and row["acquired"] == {"message_count": 2, "event_ids": ["ci0000"]},
                    "Native Add/acquisition did not use both accounts")
            before = row["saved_before_remove"] if wanted["identity"] == "personal" else row["saved_draft"]
            archived = row["archive"]["draft"]
            body = wanted["draft"]["dirty_body" if wanted["identity"] == "personal" else "body"]
            require(before["account"] == archived["account"] == wanted["account"]
                    and before["draft_id"] == archived["draft_id"] == row["saved_draft"]["draft_id"]
                    and before["body"] == archived["body"] == body and archived["from"] == wanted["email"]
                    and archived["subject"] == wanted["draft"]["subject"]
                    and archived["account_archived"] is True and archived["read_only"] is True
                    and row["archive"]["unread_before"] == row["archive"]["unread_after"],
                    "Native archive altered source identity, text or read state")
            for field in ("from", "from_name", "identity_revision", "signature_text", "body", "body_html", "subject",
                          "to_recipients", "cc_recipients", "bcc_recipients", "references", "source_message_id",
                          "source_hash", "conversation_id", "mode", "revision", "created_at", "updated_at",
                          "legacy_reply", "attachments", "attachment_ids", "discarded", "origin"):
                require(before[field] == archived[field], "Native archive changed authored field: " + field)
            require([item["email"] for item in archived["to_recipients"]] == [wanted["draft"]["recipient"]]
                    and archived["cc_recipients"] == archived["bcc_recipients"] == archived["attachments"] == [],
                    "Native archive changed recipients or saved an unfinished address")
            for receipt in row["removals"]:
                operation = receipt["operation"]
                identity = (wanted["account"], receipt["request_id"])
                require(identity not in operations and receipt["account"] == identity[0]
                        and operation["account"] == identity[0] and operation["request_id"] == identity[1]
                        and operation["kind"] == "account_remove" and operation["retention"] == "archive",
                        "Native removal receipt identity differs")
                operations[identity] = operation
        personal = observed["accounts"][0]
        require(personal["archive"]["combined_calendar_excluded"] is True
                and personal["restored_email"] == expected[0]["renamed_email"]
                and personal["restore_lifecycle"]["state"] == "active"
                and personal["restore_lifecycle"]["cleanup"] is None
                and [row["lifecycle"]["cleanup"]["state"] for row in personal["removals"]]
                    == ["failed", "complete", "complete"],
                "Native failed-cleanup/fresh-retry/same-subject restore evidence differs")
        external = observed["external_removal"]
        identity = (external["account"], external["request_id"])
        require(identity[0] == expected[1]["account"]
                and external["operation"]["account"] == identity[0]
                and external["operation"]["request_id"] == identity[1]
                and external["operation"]["kind"] == "account_remove"
                and external["operation"]["retention"] == "archive"
                and (identity not in operations or operations[identity] == external["operation"]),
                "Native/external CLI operation identity differs")
        operations[identity] = external["operation"]
        require(len(operations) == 4
                and report["native"] == {"removal_operations": 4, "retained_drafts": 2, "accounts": 2},
                "Native/external CLI operation agreement is missing")
        buffer = observed["retained_buffer"]
        require(buffer["account"] == expected[1]["account"]
                and buffer["draft"]["body"] == expected[1]["draft"]["dirty_body"]
                and buffer["recipient_inputs"]["bcc_recipients"] == expected[1]["draft"]["pending_recipient"]
                and all(observed["disposition"][key] is True for key in
                        ("keep_here_preserved", "navigation_guarded", "close_guarded", "explicit_discard_completed")),
                "Native unsaved buffer/close/navigation evidence is incomplete")
    else:
        require(set(report["executables"]) == {"cli", "auth_cli", "engine", "ordinary_engine"}
                and report["archive_count"] == report["draft_count"] == 2,
                "CLI account executable/archive evidence is incomplete")
        saved = read(stem + ".ledger.json")
        expected_steps = [
            ("personal", "denied", "full", False), ("personal", "success", "full", False),
            ("workspace", "success", "mail_readonly", False), ("workspace", "success", "full", False),
            ("personal", "success", "full", False), ("workspace", "success", "full", False),
            ("personal", "success", "full", False), ("personal", "success", "full", True)]
        require(saved["auth_steps"] == [{"step": i, "identity": row[0], "outcome": row[1],
                                         "grant": row[2], "renamed": row[3]}
                                        for i, row in enumerate(expected_steps, 1)],
                "CLI cancellation/scope/identity/restore scenarios changed")
        labels = {"removed cached reads and refused mutations", "same-subject restore and historical replay",
                  "both removed and archive exports", "refused ordinary and feature restart"}
        preserved_digest = hashlib.sha256(json.dumps(retained, sort_keys=True).encode()).hexdigest()
        require(len(saved["preservation_checks"]) == len(labels)
                and {row["label"] for row in saved["preservation_checks"]} == labels
                and all(row["passed"] is True and row["sha256"] == preserved_digest for row in saved["preservation_checks"]),
                "CLI archive/source/export preservation proofs differ")
        pending = [row for row in saved["lifecycle"] if row["label"] == "cancelled-after-store"]
        require(len(pending) == 1 and pending[0]["value"]["state"] == "removed"
                and pending[0]["value"]["cleanup"]["state"] == "pending"
                and pending[0]["value"]["cleanup"]["replay_cleanup"] is False
                and pending[0]["owned_cli_exit_code"] != 0,
                "Cancelled native persistence must retain truthful pending cleanup")
        replays = [row for row in saved["lifecycle"] if row.get("operation_is_current") is False]
        require(replays and any(row["value"]["state"] == "active" for row in replays),
                "No historical removal replay against a restored account was retained")
        for suffix in (".ordinary-refusal.log", ".feature-refusal.log"):
            require(evidence_path(stem + suffix).read_text(encoding="utf-8").strip()
                    == "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine",
                    "Missing marked-profile startup refusal")
    return {"accounts": 2, "independent_mime_sources": 4, "independent_calendar_sources": 2,
            "retained_drafts": 2, "empty_active_inventory": True, "credential_accounts_after_cleanup": [],
            "auth_and_cleanup_counts": counts, "cancelled_after_store": not is_native,
            "native_dirty_buffer_disposition": is_native, "performance_acceptance": False}


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
            require(not directory.is_symlink() and directory.resolve() == directory.absolute(),
                    "Linked or redirected visual directory is not evidence")
            files.extend(p for p in directory.iterdir() if p.is_file() and p.suffix in {".png", ".json", ".log"})
    baseline_manifest = PACKET / "baseline/manifest.json"
    if baseline_manifest.is_file():
        files.append(baseline_manifest)
    for path in files:
        within(path, PACKET)
        require(not path.is_symlink() and path.stat().st_size < 32 * 1024 * 1024,
                "Unexpected linked/oversized artifact: " + str(path))
        if re.fullmatch(r"native-accounts-[0-9]{2}\.gui\.failure\.png", path.name):
            with path.open("rb") as image:
                require(image.read(8) == b"\x89PNG\r\n\x1a\n", "Failure image is not a PNG: " + path.name)
            report_path = evidence_path(path.name.removesuffix(".gui.failure.png") + ".json")
            if report_path.is_file():
                # Failure-only images are historical evidence, not acceptance.
                # An interrupted/malformed earlier report remains retainable.
                try:
                    earlier = read(report_path.name)
                except (json.JSONDecodeError, UnicodeDecodeError):
                    earlier = None
                if isinstance(earlier, dict) and isinstance(earlier.get("artifacts"), dict):
                    value = earlier["artifacts"].get("failure_png")
                    if value is not None:
                        require(isinstance(value, dict) and value.get("name") == path.name,
                                "Failure image names a different attempt")
                        check_hash(path.name, value.get("sha256"))
    return sorted(set(files))


def validate(reports):
    require(not DEST.exists(), "Never overwrite or complete a partially created immutable checkpoint")
    require(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() == BASE_COMMIT,
            "The checkpoint must be validated against its declared implementation base")
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
        field = "status" if gate in ("workspace", "production", "feature", "feature_cli", "native_ctest") else "result"
        require(report.get(field) == "passed" and not report.get("error"), "Unpassed gate: " + name)
        selected[gate] = {"filename": name, "sha256": native.digest(evidence_path(name)), "status": "passed"}
        if gate == "workspace":
            attempt = name.removeprefix("workspace-result-").removesuffix(".json")
            manifest = "workspace-source-" + attempt + ".json"
            check_hash(manifest, report["source_check"]["manifest_sha256"])
            same_inputs(read(manifest), release_sources, name)
            scope_checks[gate] = {"rust_and_fixture_inputs_rechecked": len(release_sources),
                                  "scope": "Cargo checks Rust and recorded fixtures; current GUI/verifier inputs require separate final gates."}
            require(report["exit_code"] == 0 and report["source_check"]["changed_rust_sources"] == []
                    and report["source_check"]["changed_sources"] == []
                    and report["source_check"]["rust_sources_checked"] == len(release_sources)
                    and report["summary"]["failed"] == 0 and report["summary"]["passed"] > 0
                    and {c["name"] for c in report["checks"]} == {"tests", "clippy", "format"}
                    and len(report["checks"]) == 3 and all(c["exit_code"] == 0 for c in report["checks"]),
                    "Incomplete ordinary workspace verification")
            require(set(report["logs"]) == {"tests", "clippy", "format"}, "Missing workspace logs")
            for key, value in report["logs"].items():
                require(value["path"] == f"workspace-{key}-{attempt}.log", "Wrong workspace log identity")
                check_hash(value["path"], value["sha256"])
        elif gate in ("production", "feature", "feature_cli"):
            stem = name.removesuffix(".result.json")
            check_hash(stem + ".source.json", report["source_manifest_sha256"])
            check_hash(stem + ".log", report["log_sha256"])
            same_inputs(read(stem + ".source.json"), release_sources, name)
            require(report["exit_code"] == 0 and report["changed_sources"] == [], "Release sources changed during verification")
            command = report["command"]
            require("--release" in command and "--locked" in command, "Expected locked Release verification")
            if gate in ("feature", "feature_cli"):
                crate, binary, test = (("fm-engine", "fm-engine", "verification_profile") if gate == "feature"
                                       else ("fm-cli", "fm", "arguments"))
                require(command == ["test", "--release", "--locked", "-p", crate, "--features", "verification-google",
                                    "--target-dir", "target/verification-google", "--bin", binary, "--test", test]
                        and len(report["suites"]) == 2 and all(row["status"] == "ok" and row["failed"] == 0
                                                               and row["passed"] > 0 for row in report["suites"]),
                        "Missing isolated unit/process tests")
                require(set(report["executables"]) == ({"engine"} if gate == "feature" else {"cli"}),
                        "Isolated executable identity is missing")
            else:
                require(command[0] == "build" and {"fm-engine", "fm-cli"}.issubset(command)
                        and "verification-google" not in command, "Expected ordinary engine/CLI Release build")
                require(set(report["executables"]) == {"engine", "cli"}, "Ordinary Release identities are missing")
            check_binaries(report["executables"], binaries, name)
            scope_checks[gate] = {"rust_and_fixture_inputs_rechecked": len(release_sources),
                                  "suites": report["suites"], "command": command}
        elif gate == "native_ctest":
            check_native(name, report, sources, binaries)
            scope_checks[gate] = {"tests": report["tests"], "native_tests": report["native_tests"],
                                  "build_fixtures": report["build_fixtures"], "failures": 0,
                                  "reruns_rust_workspace_tests": False, "aggregate_watchdog_seconds": 180,
                                  "primary_watchdog_seconds": 120, "child_watchdog_seconds": 45,
                                  "diagnostic_count": report["diagnostic_count"]}
        elif gate == "upgrade":
            same_inputs(report["sources"], upgrade_sources, name)
            require(report["scenario"] == "schema-17-to-18-account-lifecycle" and report["version"] == 4
                    and report["before"]["schema"] == 17 and report["after"]["schema"] == 18
                    and report["refused"]["logical_database_sha256"] == report["after"]["logical_database_sha256"]
                    and report["refused"]["retained_blobs"] == report["after"]["retained_blobs"]
                    and report["before"]["retained_blobs"] == report["after"]["retained_blobs"]
                    and report["before"]["table_digests"] == report["after"]["table_digests"],
                    "Wrong upgrade/refusal boundary")
            require(len(report["calendar"]["requests"]) == len(report["calendar"]["receipts"]) == 4
                    and [row["kind"] for row in report["calendar"]["requests"]] == ["create", "edit", "rsvp", "create"]
                    and [row["state"] for row in report["calendar"]["receipts"]] == ["fake_acknowledged"] * 3 + ["pending"]
                    and report["capture_receipt"] and report["draft_id"], "Legacy Calendar/capture preservation evidence is missing")
            before, after = report["accounts"]["before"], report["accounts"]["after"]
            require(before["fixture_revision"] == after["fixture_revision"] == 3
                    and len(before["accounts"]) == len(after["accounts"]) == 3,
                    "Missing old synthetic account upgrade evidence")
            for old, new in zip(before["accounts"], after["accounts"], strict=True):
                require(new == {**old, "lifecycle": {"state": "active", "revision": 1, "cleanup": None},
                                "read_only": False}, "Schema18 changed an old account or omitted active defaults")
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
            baseline = json.loads((PACKET / "baseline/manifest.json").read_text(encoding="utf-8"))
            require(baseline["source_commit"] == BASE_COMMIT and baseline["schema"] == 17
                    and baseline["synthetic_fixture"] == 3
                    and baseline["source_report"] == "docs/research/assets/phase-2/p2-06b-interim/rust-production-build-04.result.json",
                    "Upgrade baseline does not identify the immutable schema17 checkpoint")
            for old_key, baseline_key in (("fm", "cli"), ("fm-engine", "engine")):
                require(report["executables"]["old"][old_key]["sha256"] == baseline["executables"][baseline_key]["sha256"],
                        "Old executable differs from the preserved baseline identity")
            scope_checks[gate] = {"schema_before": 17, "schema_after": 18, "old_account_defaults": 3,
                                  "legacy_calendar_receipts": 4, "exact_old_columns_and_blobs_preserved": True,
                                  "old_engine_refused": True, "fresh_reopen": True}
        elif gate in ("cli_accounts", "native_accounts"):
            stem = name.removesuffix(".json")
            source_count = same_inputs(read(stem + ".source.json"), account_source_inputs(gate == "native_accounts"), name)
            if gate == "cli_accounts":
                keys = {"engine", "watch", "personal", "workspace", "fixture", "source", "ledger",
                        "ordinary_refusal", "feature_refusal"}
                require(set(report["outputs_sha256"]) == keys, "CLI account artifact set differs")
                check_outputs(name, report, keys)
                check_hash(stem + ".source.json", report["source_sha256"])
            else:
                keys = {"engine", "watch", "gui", "fixture", "gui_report", "expectation", "sources"}
                require(keys <= set(report["artifacts"]) <= keys | {"failure_png"},
                        "Native account artifact set differs")
                for key, value in report["artifacts"].items():
                    require(value["name"] == stem + OUTPUT_SUFFIXES[key], "Cross-attempt native account artifact")
                    check_hash(value["name"], value["sha256"])
            check_binaries(report["executables"], binaries, name)
            fixture = read(stem + ".fixture.json")
            require(fixture["status"] == "complete" and fixture["errors"] == []
                    and fixture["transport"] == "in_memory_only" and fixture["native_vault"] is False
                    and fixture["counters"] == report["provider_counters"], "Account fixture final isolation/counters differ")
            scope_checks[gate] = {**check_accounts(gate, name, report, fixture), "source_inputs_rechecked": source_count}
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
    return sources, binaries, selected, artifacts, len(release_sources), scope_checks


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
    require(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() == BASE_COMMIT,
            "Git implementation base changed during archival")
    checkpoint = {
        "status": "functional_checkpoint_performance_deferred", "packet": "P2-01B", "decision": "D036",
        "performance_deferral": "D031", "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "base_commit": BASE_COMMIT,
        "boundary": BOUNDARY, "source_hashes": sources, "runtime_source_count": len(sources),
        "workspace_rust_inputs_rechecked": rust_count, "executables": binaries, "functional_reports": selected,
        "source_scope_checks": scope_checks,
        "historical_evidence_policy": "All retained earlier attempts, including failures, malformed reports and interrupted logs, are historical observations; only selected final reports satisfy gates.",
        "excluded_data": ["Executable bytes (including baseline)", "Disposable profiles, database/blob/vault trees", "Python caches"],
        "limitations": ["D031 retains the B2 navigation failure; no new performance run or performance acceptance.",
                        "D036 covers generated Add, local account removal, retained read-only archive and same-subject Restore; recurring Calendar workflows and running-app reminders remain separate work.",
                        "Retaining a read-only local archive is the conservative implementation default, not a founder answer about real-data retention; removal does not erase local content or revoke the remote Google grant.",
                        "Generated in-memory providers/vaults and development binaries only; no real accounts, delivery, package or CI acceptance.",
                        "Account OAuth uses the explicitly enabled isolated loopback fixture and in-memory credential store; native OS vault behavior and live sign-in are not verified by these reports.",
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
