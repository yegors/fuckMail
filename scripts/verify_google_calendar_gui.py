#!/usr/bin/env python3
"""Functional native Calendar actions with the shared independent provider oracle.

A new generated profile only. The GUI starts and pauses its own Settings sync
owners; no real provider, notification delivery or performance claim is made.
"""
import argparse
from contextlib import ExitStack, closing
from copy import deepcopy
from datetime import datetime, timedelta
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_calendar import (BASE, BASE_UNIX, WINDOW, Ledger, calendar, expected_source,
                                    journal_snapshot, same, validate_status, verify_publications, verify_retained_sources)
from verify_google_navigation import LIMITS, expected_accounts, read_database, require, source_manifest
from verify_google_drafts import read_json
from verify_search_ingestion import sha256, stop_children


def native_expectation(accounts):
    expected = {"version": 1, "limits": LIMITS, "calendar_date": BASE.date().isoformat(), "accounts": []}
    for account in accounts:
        source = expected_source(account, 0)
        start = BASE + timedelta(hours=12)
        fresh = {"title": "Native invitation " + account["key"],
                 "start": start.isoformat(timespec="seconds"),
                 "end": (start + timedelta(minutes=45)).isoformat(timespec="seconds"),
                 "time_zone": "UTC", "location": "Native fixture room",
                 "description": "Native Calendar guest verification.\nUnicode café 日本語.",
                 "attendees": ["native-created-" + account["key"] + "@example.test"]}
        # These values come from independent fixture constants, not GUI/CLI
        # projections. Time-zone-specific display fields are checked separately.
        event = {"calendar_id": calendar(account), "event_id": source["id"], "etag": source["etag"],
                 "title": source["summary"], "description": source["description"], "location": source["location"],
                 "conference_url": source["conferenceData"]["entryPoints"][0]["uri"],
                 "guest_addresses": ["alpha@example.test", "beta@example.test"],
                 "protected_attendees": [person for person in source["attendees"] if person.get("self") or person.get("organizer")]}
        expected["accounts"].append({"account": account["id"], "event": event,
                                      "add_guest": "native-added-" + account["key"] + "@example.test", "new_event": fresh})
    return expected


def checked_time(value, expected):
    require(isinstance(value, str) and re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:Z|\+00:00)", value),
            "Native UTC timestamp has an unexpected representation")
    require(datetime.fromisoformat(value.replace("Z", "+00:00")) == datetime.fromisoformat(expected),
            "Native event time differs from the independently authored instant")
    return value


def validate_native(observed, expectation, accounts, ledger):
    require(observed.get("version") == 1 and observed.get("result") == "passed" and not observed.get("error"),
            "Native Calendar walkthrough did not pass")
    native_rows = observed.get("accounts", [])
    require(len(native_rows) == len(accounts) and [row.get("account") for row in native_rows] == [a["id"] for a in accounts],
            "Native Calendar omitted, duplicated or crossed an account")
    requests = {}
    for account, expected, native in zip(accounts, expectation["accounts"], native_rows):
        same(native["limits"], LIMITS, "Native owned quota limits")
        for key, value in expected["event"].items():
            same(native["initial_event"].get(key), value, "Independent initial event " + key)
        require(native["initial_event"]["date"] == BASE.date().isoformat(), "Native initial day was not the expected UTC date")
        created = None
        for action, kind in (("add", "edit"), ("remove", "edit"), ("create", "create"), ("delete", "delete")):
            record = native[action]
            request = deepcopy(record["request"])
            request_id = request.get("request_id")
            require(isinstance(request_id, str) and re.fullmatch(r"gui-(?:event|delete)-[0-9]+-[0-9]+", request_id)
                    and len(request_id) <= 128, "Unexpected actual native request identity")
            identity = account["id"] + "/" + request_id
            require(identity not in requests, "Native Calendar reused an action request identity")
            fields = None
            if action in ("add", "remove"):
                event_id = "ci0000"
                guests = expected["event"]["guest_addresses"] + ([expected["add_guest"]] if action == "add" else [])
                fields = {"attendees": guests}
                body = ledger.expected_patch(account, event_id, guests=guests)
                method = "PATCH"
            elif action == "create":
                event_id = "f" + hashlib.sha256(json.dumps([account["id"], calendar(account), request_id],
                                                           separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
                created = event_id
                fresh = expected["new_event"]
                fields = {**deepcopy(fresh), "all_day": False}
                for name in ("start", "end"):
                    fields[name] = checked_time(request.get("fields", {}).get(name), fresh[name])
                body = {"id": event_id, "summary": fresh["title"], "description": fresh["description"], "location": fresh["location"],
                        "start": {"dateTime": fields["start"], "timeZone": "UTC"},
                        "end": {"dateTime": fields["end"], "timeZone": "UTC"},
                        "attendees": [{"email": email, "responseStatus": "needsAction"} for email in fresh["attendees"]]}
                method = "POST"
            else:
                require(created is not None, "Native deletion did not follow its own creation")
                event_id, method, body = created, "DELETE", None
            wanted = {"account": account["id"], "calendar_id": calendar(account), "kind": kind, "request_id": request_id}
            if kind != "create":
                wanted.update({"event_id": event_id, "expected_etag": ledger.sources[account["id"]][event_id]["etag"]})
            if fields is not None:
                wanted["fields"] = fields
            # IPC attendees are strings; the shared provider comparator sorts
            # object rosters. Keep this authored request comparison fully exact.
            require(request == wanted, "Exact independently authored native " + action + " request differs")
            ledger.mutation(account, event_id, method, body)
            expected_target = {"account": account["id"], "calendar": calendar(account), "event": event_id}
            status = record["outcome"]
            require(status["state"] in ("accepted", "acknowledged"), "Native ordinary action lacks provider acceptance")
            validate_status(status, account, request, status["state"], event_id, ledger)
            if action != "delete":
                projected = record["event"]
                require(projected["account"] == account["id"] and projected["calendar_id"] == calendar(account)
                        and projected["event_id"] == event_id and projected["etag"] == ledger.sources[account["id"]][event_id]["etag"],
                        "Native refreshed event has the wrong source identity/version")
                require(sorted(projected["guest_addresses"]) == sorted(fields["attendees"]), "Native refreshed roster differs from authored fields")
            request.pop("account")
            requests[identity] = {"account": account, "request": request, "target": expected_target, "publication": ledger.publications[-1]}
    require(len(requests) == len(accounts) * 4, "Native scenario did not produce every explicit action")
    return requests


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {key: report.with_suffix(suffix) for key, suffix in {
        "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log", "workspace": ".workspace.log",
        "fixture": ".fixture.json", "sources": ".source.json", "ledger": ".ledger.json", "gui": ".gui.log",
        "gui_report": ".gui.json", "expectation": ".expectation.json"}.items()}
    require(not report.exists() and not any(path.exists() for path in outputs.values()), "Use a new numbered native evidence path")
    result = {"version": 1, "result": "failed", "scenario": "isolated-google-calendar-native", "commands": [],
              "base_unix_secs": BASE_UNIX, "limits": LIMITS,
              "limitations": ["Generated in-memory provider/vault only; no real events or notification delivery.",
                              "Fresh GUI process, with an existing local Watch owner; no populated isolated-engine restart.",
                              "Functional native handlers and independent CLI/source readback only; no benchmark or package acceptance."]}
    children, started = [], time.monotonic()
    try:
        programs = {name: getattr(args, name).resolve(strict=True) for name in ("cli", "engine", "gui")}
        identities = {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()}
        result["executables"] = identities
        sources = source_manifest()
        outputs["sources"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
        accounts = expected_accounts()
        ledger = Ledger(accounts)
        expectation = native_expectation(accounts)
        outputs["expectation"].write_text(json.dumps(expectation, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        with tempfile.TemporaryDirectory(prefix="fuckmail-calendar-native-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {key: logs.enter_context(outputs[key].open("wb")) for key in ("engine", "watch", "personal", "workspace", "gui")}
            env = package_environment()
            env.pop("QT_QUICK_BACKEND", None)
            env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
            env["QT_FORCE_STDERR_LOGGING"] = "1"
            env["QSG_INFO"] = "1"
            env["FM_ENGINE_PATH"] = str(programs["engine"])
            if args.qt_root:
                env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
            engine = None
            try:
                engine = subprocess.Popen([str(programs["engine"]), "--profile", str(profile), "--live-profile", "--verification-google",
                                           "--verification-google-calendar", "--verification-google-calendar-base", str(BASE_UNIX)],
                                          cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=streams["engine"], stderr=subprocess.STDOUT)
                children.append(engine)

                def wait_for(predicate, label, timeout=60):
                    deadline = time.monotonic() + timeout
                    while time.monotonic() < deadline:
                        require(engine.poll() is None, "Isolated engine exited during " + label)
                        value = predicate()
                        if value:
                            return value
                        time.sleep(.1)
                    raise RuntimeError("Timed out: " + label)

                def command(tail, account=None):
                    prefix = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]), "--json"]
                    if account:
                        prefix += ["--account", account["id"]]
                    return prefix + tail

                def cli(tail, account=None):
                    require(engine.poll() is None, "Never reopen an isolated profile using ordinary network code")
                    completed = subprocess.run(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                    require(completed.stdout.strip(), "No CLI response: " + completed.stderr[-400:])
                    data = json.loads(completed.stdout)
                    result["commands"].append({"command": tail, "account": account["key"] if account else None,
                                               "exit_code": completed.returncode, "error": data.get("error", {}).get("code")})
                    require(completed.returncode == 0 and data.get("ok") is True, "CLI failed: " + str(data.get("error")))
                    return data["result"]

                def acquired():
                    with closing(read_database(profile)) as db:
                        states = db.execute("SELECT account,calendar_id,full_generation,retained_errors FROM calendar_sync_state WHERE calendar_id<>''").fetchall()
                        count = db.execute("SELECT COUNT(*) FROM calendar_visible_sources").fetchone()[0]
                    return len(states) == len(accounts) and all(row[2] > 0 and row[3] == 0 for row in states) and count == 14

                wait_for(lambda: fixture_path.is_file() and (profile / "engine.json").is_file(), "isolated startup")
                require(read_json(profile / "engine.json")["pid"] == engine.pid, "Wrong discovery owner")
                initial = read_json(fixture_path)
                require(initial["native_vault"] is False and initial["transport"] == "in_memory_only"
                        and initial["calendar_fixture"]["enabled"] is True and initial["calendar_fixture"]["events_per_account"] == 7
                        and initial["calendar_fixture"]["base_unix_secs"] == BASE_UNIX, "Calendar provider isolation changed")
                watch = subprocess.Popen(command(["watch", "--seconds", "900"]), cwd=scratch, env=env,
                                         stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                owners = []
                for account in accounts:
                    tail = ["google", "sync", "--seconds", "900"]
                    for key, value in LIMITS.items():
                        tail += ["--" + key.replace("_", "-"), str(value)]
                    owner = subprocess.Popen(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    children.append(owner)
                    owners.append(owner)
                wait_for(acquired, "all fourteen independent Calendar sources")
                with closing(read_database(profile)) as db:
                    require(db.execute("PRAGMA user_version").fetchone()[0] == 18, "Wrong shared Calendar schema")
                result["initial_sources"] = verify_retained_sources(profile, ledger)
                require(not journal_snapshot(profile), "Acquisition unexpectedly created authored Calendar operations")
                for owner in owners:
                    owner.terminate()
                for owner in owners:
                    owner.wait(timeout=10)
                wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "CLI sync owners released")
                prefs = cli(["preferences", "show"])
                prefs_file = scratch / "calendar-display.json"
                prefs_file.write_text(json.dumps({"calendar_display_time_zone": "UTC"}), encoding="utf-8")
                result["display_zone_setup"] = {"patch": {"calendar_display_time_zone": "UTC"},
                    "receipt": cli(["preferences", "save", "--preferences-file", str(prefs_file), "--expected-revision", str(prefs["revision"]),
                                    "--request-id", "native-calendar-display-utc"])}
                require(cli(["preferences", "show"])["preferences"]["calendar_display_time_zone"] == "UTC", "Explicit display zone did not persist")
                gui = subprocess.Popen([str(programs["gui"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                                        "--google-calendar-self-test", str(outputs["gui_report"]), "--google-calendar-expectation", str(outputs["expectation"])],
                                       cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=streams["gui"], stderr=subprocess.STDOUT)
                children.append(gui)
                result["gui_exit_code"] = gui.wait(timeout=180)
                require(result["gui_exit_code"] == 0, "Native Calendar failed; inspect retained GUI log/report")
                observed = read_json(outputs["gui_report"])
                require(observed.get("gui_pid") == gui.pid, "Native report belongs to another process")
                requests = validate_native(observed, expectation, accounts, ledger)
                result["native_accounts"] = observed["accounts"]
                outputs["ledger"].write_text(json.dumps({"sources": ledger.sources, "publications": ledger.publications}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
                require(not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "GUI did not release every native sync owner")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "post-native owner report")
                before = read_json(fixture_path)["counters"]
                result["final_sources"] = verify_retained_sources(profile, ledger)
                frozen = journal_snapshot(profile)
                require(len(frozen) == len(requests) == 8, "Native Calendar omitted or invented an authored journal")
                for row in frozen:
                    key = row[0] + "/" + row[1]
                    require(key in requests, "Unknown Calendar journal identity")
                    expected = requests[key]
                    intent, dispatch = json.loads(row[3]), json.loads(row[11])
                    require(intent["version"] == 2 and intent["request"] == expected["request"] and intent["target"] == expected["target"],
                            "Saved native intent differs from independently validated request")
                    require(dispatch["version"] == 2 and dispatch["send_updates"] == "all", "Saved dispatch lost notification policy/version")
                    same(dispatch["baseline"], expected["publication"]["before"], "Frozen native source before mutation")
                    body = deepcopy(expected["publication"]["body"])
                    if body is not None and expected["publication"]["method"] == "POST":
                        del body["id"]
                    same(dispatch["body"], body, "Frozen native provider body")
                    status = cli(["google", "calendar-operation", row[1]], expected["account"])
                    require(status["state"] in ("accepted", "acknowledged"), "Offline native receipt lost provider acceptance")
                    validate_status(status, expected["account"], expected["request"], status["state"], expected["target"]["event"], ledger)
                for account in accounts:
                    cli(["calendar", *WINDOW], account)
                    seen, cursor = set(), None
                    for _ in range(8):
                        tail = ["calendar-operations", "--limit", "2"]
                        if cursor is not None:
                            tail += ["--cursor", json.dumps(cursor)]
                        page = cli(tail, account)
                        for status in page["operations"]:
                            require(status["request_id"] not in seen, "Duplicate native activity receipt")
                            seen.add(status["request_id"])
                        cursor = page["continuation"]
                        if cursor is None:
                            break
                    require(cursor is None, "Native activity pagination exceeded its bounded four-operation scenario")
                    require(seen == {row["request"]["request_id"] for row in requests.values() if row["account"] == account}, "Native activity lost an exact operation")
                require(journal_snapshot(profile) == frozen, "Offline readback modified retained native journals")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "offline Calendar report")
                require(read_json(fixture_path)["counters"] == before, "Offline Calendar readback started provider HTTP")
                watch.terminate()
                watch.wait(timeout=10)
                require(engine.wait(timeout=15) == 0, "Final local owner did not drain the engine")
                final = read_json(fixture_path)
                require(final["status"] == "complete" and not final["errors"] and final["counters"] == before, "Final provider completion changed or failed")
                result["provider_verification"] = verify_publications(final, ledger)
                require(before["calendar_mutations"] == 8 and before["calendar_acknowledged"] == 8 and before["calendar_lost_responses"] == 0,
                        "Unexpected native provider write, retry or certainty count")
                require(source_manifest() == sources and all(sha256(path) == identities[name]["sha256"] for name, path in programs.items()), "Native verification inputs changed")
                result.update({"result": "passed", "provider_counters": before, "journal_rows": len(frozen),
                               "journal_sha256": hashlib.sha256(json.dumps(frozen, ensure_ascii=False).encode()).hexdigest(),
                               "offline_queries_preserved_journal_and_http": True, "final_owner_drained_engine": True})
            finally:
                try:
                    stop_children(children)
                finally:
                    # Retain partial diagnostics before temporary-profile
                    # cleanup even if stopping an owned process fails.
                    try:
                        if fixture_path.is_file():
                            outputs["fixture"].write_bytes(fixture_path.read_bytes())
                    finally:
                        outputs["ledger"].write_text(json.dumps({"sources": ledger.sources, "publications": ledger.publications}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    except Exception as error:
        result["result"] = "failed"
        result["error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {key: sha256(path) for key, path in outputs.items() if path.is_file()}
    report.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cli", "engine", "gui", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--qt-root", type=Path)
    raise SystemExit(run(parser.parse_args()))
