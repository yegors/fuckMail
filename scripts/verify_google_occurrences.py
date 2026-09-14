#!/usr/bin/env python3
"""Verify generated two-account occurrence workflows, with optional native GUI.

Only a fresh marked in-memory-provider profile is used. This is functional
source/journal verification, not a performance or real-account test.
"""
import argparse
from contextlib import ExitStack, closing
from copy import deepcopy
from datetime import timedelta
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_calendar import BASE, BASE_UNIX, calendar, journal_snapshot, same, verify_publications
from verify_google_drafts import read_json
from verify_google_navigation import LIMITS, expected_accounts, read_database, require, source_manifest
from verify_google_occurrence_native import validate_native
from verify_google_occurrence_sources import (
    OccurrenceLedger, native_expectation, selector, validate_resolution, validate_status,
    verify_journals, verify_retained_occurrences, verify_window,
)
from verify_search_ingestion import sha256, stop_children


def digest_rows(rows):
    return hashlib.sha256(json.dumps(rows, ensure_ascii=False).encode()).hexdigest()


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    suffixes = {"engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log",
                "workspace": ".workspace.log", "fixture": ".fixture.json", "sources": ".source.json",
                "ledger": ".ledger.json", "refusal": ".refusal.log"}
    if args.gui:
        suffixes.update(gui=".gui.log", gui_report=".gui.json", expectation=".expectation.json")
    outputs = {key: report.with_suffix(suffix) for key, suffix in suffixes.items()}
    require(not report.exists() and not any(path.exists() for path in outputs.values()),
            "Use a fresh numbered occurrence evidence path")
    result = {"version": 1, "result": "failed",
              "scenario": "isolated-google-occurrences-" + ("native" if args.gui else "cli"),
              "commands": [], "base_unix_secs": BASE_UNIX, "limits": LIMITS,
              "limitations": ["Generated in-memory provider/vault only; no real account or notification delivery.",
                              "Functional source/journal checks; no benchmark, package or physical-input acceptance.",
                              "Cached checks keep an explicit local Watch owner; marked profiles cannot restart."]}
    accounts = expected_accounts()
    ledger = OccurrenceLedger(accounts)
    requests, statuses, children = {}, {}, []
    started = time.monotonic()

    def save_ledger():
        outputs["ledger"].write_text(json.dumps({"sources": ledger.sources,
            "listed": {key: sorted(value) for key, value in ledger.listed.items()},
            "publications": ledger.publications, "requests": requests, "statuses": statuses},
            ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    try:
        names = ["cli", "engine", "ordinary_engine"] + (["gui"] if args.gui else [])
        programs = {name: getattr(args, name).resolve(strict=True) for name in names}
        identities = {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()}
        result["executables"] = identities
        sources = source_manifest()
        outputs["sources"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
        with tempfile.TemporaryDirectory(prefix="fuckmail-occurrences-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {key: logs.enter_context(outputs[key].open("wb"))
                       for key in ("engine", "watch", "personal", "workspace")}
            env = package_environment()
            if args.gui:
                streams["gui"] = logs.enter_context(outputs["gui"].open("wb"))
                env.pop("QT_QUICK_BACKEND", None)
                env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
                env["QT_FORCE_STDERR_LOGGING"], env["QSG_INFO"] = "1", "1"
                env["FM_ENGINE_PATH"] = str(programs["engine"])
                if args.qt_root:
                    env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
            engine = subprocess.Popen([str(programs["engine"]), "--profile", str(profile), "--live-profile",
                "--verification-google", "--verification-google-occurrences",
                "--verification-google-occurrences-base", str(BASE_UNIX)], cwd=scratch, env=env,
                stdin=subprocess.DEVNULL, stdout=streams["engine"], stderr=subprocess.STDOUT)
            children.append(engine)
            owners = []
            sequence = 0

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

            def cli(tail, account=None, refused=None):
                require(engine.poll() is None, "Never reopen a populated isolated profile through a CLI query")
                completed = subprocess.run(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                require(completed.stdout.strip(), "Missing CLI reply: " + completed.stderr[-400:])
                value = json.loads(completed.stdout)
                error = value.get("error", {}).get("code")
                result["commands"].append({"command": tail[:2], "account": account["key"] if account else None,
                                           "exit_code": completed.returncode, "error": error})
                if refused:
                    require(completed.returncode != 0 and value.get("ok") is False and error == refused,
                            "Expected " + refused + ": " + str(value.get("error")))
                    return value["error"]
                require(completed.returncode == 0 and value.get("ok") is True,
                        "CLI operation failed: " + str(value.get("error")))
                return value["result"]

            def params_call(method, account, value, refused=None):
                nonlocal sequence
                sequence += 1
                path = scratch / f"params-{sequence:04}.json"
                path.write_text(json.dumps(value, ensure_ascii=False) + "\n", encoding="utf-8")
                return cli(["google", method, "--params-file", str(path)], account, refused)

            def pause_owners():
                for owner in owners:
                    require(owner.poll() is None, "Explicit sync owner ended prematurely")
                    owner.terminate()
                for owner in owners:
                    owner.wait(timeout=10)
                owners.clear()
                wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]),
                         "all sync owners released")

            def acquired():
                with closing(read_database(profile)) as db:
                    rows = db.execute("SELECT full_generation,retained_errors FROM calendar_sync_state WHERE calendar_id<>''").fetchall()
                    count = db.execute("SELECT COUNT(*) FROM calendar_published_sources").fetchone()[0]
                return len(rows) == 2 and all(generation > 0 and errors == 0 for generation, errors in rows) and count == 12

            def lookup(account, master, index, hint=False):
                before = journal_snapshot(profile)
                value = params_call("calendar-occurrence", account, selector(account, master, index, hint=hint))
                source = ledger.sources[account["id"]][value["event_id"]]
                expected_id = ("oi", "od", "oc", "ol")[master] + f"{index:04}"
                require(value["event_id"] == expected_id, "Lookup selected a sibling provider instance")
                validate_resolution(value, account, source)
                require(journal_snapshot(profile) == before, "Lookup created or changed an authored operation")
                return value

            def write(account, master, index, kind, lost=False):
                resolved = lookup(account, master, index, hint=(master == 0 and index in (4, 5)))
                event = resolved["event_id"]
                request = {"request_id": f"occurrence-{kind}-{event}", "kind": kind,
                           "calendar_id": calendar(account), "event_id": event,
                           "expected_etag": resolved["etag"], "occurrence": resolved["occurrence"]}
                body = None
                if kind == "edit":
                    title = f"CLI occurrence {account['key']} {event} café 日本語"
                    request["fields"] = {"title": title}
                    body = ledger.expected_patch(account, event, title=title)
                key = account["id"] + "/" + request["request_id"]
                publication = ledger.mutation(account, event, "PATCH" if kind == "edit" else "DELETE", body, lost)
                requests[key] = {"account": account, "request": deepcopy(request), "publication": publication}
                save_ledger()
                status = params_call("calendar-write", account, request)
                validate_status(status, account, request, "unknown" if lost else "acknowledged", ledger)
                statuses[key] = status
                require(params_call("calendar-write", account, request) == status, "Exact owned replay changed the retained attempt")
                if lost:
                    status = cli(["google", "calendar-reconcile", request["request_id"]], account)
                    validate_status(status, account, request, "reconciled", ledger)
                    require(params_call("calendar-write", account, request) == status, "Reconciled intent was submitted again")
                    statuses[key] = status
                save_ledger()

            try:
                wait_for(lambda: fixture_path.is_file() and (profile / "engine.json").is_file(), "isolated startup")
                require(read_json(profile / "engine.json")["pid"] == engine.pid, "Wrong discovery owner")
                initial = read_json(fixture_path)
                require(initial["native_vault"] is False and initial["transport"] == "in_memory_only"
                        and initial["accounts"] == accounts and initial["base_unix_secs"] == BASE_UNIX
                        and initial["occurrence_fixture"] == {"enabled": True, "masters_per_account": 4,
                            "instances_per_account": 13, "initial_sources_per_account": 6, "page_size": 2},
                        "Occurrence fixture isolation or fixed corpus changed")
                watch = subprocess.Popen(command(["watch", "--seconds", "900"]), cwd=scratch, env=env,
                    stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                prefs = cli(["preferences", "show"])
                pref_file = scratch / "display-zone.json"
                pref_file.write_text(json.dumps({"calendar_display_time_zone": "UTC"}), encoding="utf-8")
                cli(["preferences", "save", "--preferences-file", str(pref_file), "--expected-revision",
                     str(prefs["revision"]), "--request-id", "occurrence-display-utc"])
                require(cli(["preferences", "show"])["preferences"]["calendar_display_time_zone"] == "UTC",
                        "Explicit UTC display preference did not persist")
                for account in accounts:
                    tail = ["google", "sync", "--seconds", "900"]
                    for key, value in LIMITS.items():
                        tail += ["--" + key.replace("_", "-"), str(value)]
                    owner = subprocess.Popen(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    owners.append(owner)
                    children.append(owner)
                wait_for(acquired, "complete initial recurring-source acquisition")
                result["initial_sources"] = verify_retained_occurrences(profile, ledger)
                require(result["initial_sources"] == {"retained_sources": 12, "active_sources": 8, "query_sources": 12, "cancelled_sources": 4}
                        and not journal_snapshot(profile), "Acquisition changed source identities or created an authored intent")
                with closing(read_database(profile)) as db:
                    require(db.execute("PRAGMA user_version").fetchone()[0] == 19, "Wrong shared occurrence schema")
                if args.gui:
                    pause_owners()
                    expectation = native_expectation(accounts)
                    outputs["expectation"].write_text(json.dumps(expectation, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
                    gui = subprocess.Popen([str(programs["gui"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                        "--google-calendar-self-test", str(outputs["gui_report"]), "--google-calendar-expectation", str(outputs["expectation"])],
                        cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=streams["gui"], stderr=subprocess.STDOUT)
                    children.append(gui)
                    result["gui_exit_code"] = gui.wait(timeout=180)
                    require(result["gui_exit_code"] == 0, "Native occurrence walkthrough failed; inspect retained GUI evidence")
                    observed = read_json(outputs["gui_report"])
                    require(observed["gui_pid"] == gui.pid, "Native report belongs to another process")
                    requests = validate_native(observed, expectation, accounts, ledger)
                    result["native_accounts"] = observed["accounts"]
                else:
                    for account in accounts:
                        wrong = selector(account, 0, 0)
                        wrong["event_id"] = "oi0002"
                        missing = selector(account, 0, 0)
                        missing["original_start"]["instant"] = "2026-12-31T01:00:00Z"
                        for value, error in ((wrong, "scope_mismatch"), (selector(account, 3, 0), "unsupported_operation"),
                                             (selector(account, 1, 1), "invalid_request"), (missing, "calendar_occurrence_unresolved")):
                            before = journal_snapshot(profile)
                            params_call("calendar-occurrence", account, value, error)
                            require(journal_snapshot(profile) == before, "Refused lookup changed an authored journal")
                        for master, index, kind, lost in ((0, 0, "edit", False), (0, 1, "edit", True),
                                (1, 0, "edit", False), (1, 2, "delete", False), (0, 4, "delete", True), (0, 5, "delete", False)):
                            write(account, master, index, kind, lost)
                    pause_owners()
                require(not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]),
                        "A native or CLI sync owner survived the workflow")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "post-owner fixture report")
                counters = read_json(fixture_path)["counters"]
                result["final_sources"] = verify_retained_occurrences(profile, ledger)
                require(result["final_sources"] == {"retained_sources": 18 if args.gui else 22,
                        "active_sources": 12, "query_sources": 16, "cancelled_sources": 6 if args.gui else 10}, "Wrong final retained occurrence inventory")
                frozen = journal_snapshot(profile)
                result["journal_rows"] = verify_journals(frozen, requests)
                require(len(frozen) == (6 if args.gui else 12), "Unexpected number of explicit occurrence mutations")
                for row in frozen:
                    key = row[0] + "/" + row[1]
                    expected = requests[key]
                    require(row[5] == "live" and row[8] == 1 and row[6] in ("acknowledged", "reconciled"),
                            "Occurrence journal repeated an attempt or lost terminal certainty")
                    outcome = json.loads(row[12])
                    publication = expected["publication"]
                    if row[6] == "reconciled" or publication["method"] == "PATCH":
                        same(outcome["source"], publication["after"], "Exact retained provider outcome source")
                    else:
                        require(outcome["source_origin"] == "dispatched_occurrence_identity", "DELETE ACK lost its truthful provenance")
                        with closing(read_database(profile)) as db:
                            raw = db.execute("SELECT observed_json FROM calendar_event_state WHERE account=? AND calendar_id=? AND event_id=?",
                                             (row[0], publication["calendar"], publication["event"])).fetchone()[0]
                        same(json.loads(raw), {"kind": "acknowledged_occurrence_cancellation", "request_id": row[1],
                                             "source": publication["after"]}, "ACK-derived cancellation observation envelope")
                    status = cli(["google", "calendar-operation", row[1]], expected["account"])
                    validate_status(status, expected["account"], expected["request"], row[6], ledger)
                    if key in statuses:
                        require(status == statuses[key], "Offline receipt changed after the owned workflow")
                    statuses[key] = status
                    params_call("calendar-write", expected["account"], expected["request"], "sync_not_running")
                for account in accounts:
                    seen, cursor = set(), None
                    for _ in range(8):
                        tail = ["calendar-operations", "--limit", "2"]
                        if cursor is not None:
                            tail += ["--cursor", json.dumps(cursor)]
                        page = cli(tail, account)
                        for status in page["operations"]:
                            key = account["id"] + "/" + status["request_id"]
                            require(key not in seen and status == statuses[key], "Activity duplicated or changed a retained receipt")
                            seen.add(key)
                        cursor = page["continuation"]
                        if cursor is None:
                            break
                    require(cursor is None and seen == {key for key, value in requests.items() if value["account"] == account},
                            "Activity omitted an occurrence operation")
                result["cached_windows"] = []
                for begin, end in ((0, 7), (61, 63)):
                    date_from, date_to = [(BASE + timedelta(days=days)).date().isoformat() for days in (begin, end)]
                    page = cli(["calendar", "--combined", "--from", date_from, "--to", date_to])
                    require(page["coverage"]["complete"] is True and page["continuation"] is None,
                            "Final cached occurrence window is incomplete")
                    count = verify_window(page["events"], ledger, accounts, date_from, date_to)
                    result["cached_windows"].append({"from": date_from, "to": date_to, "events": count})
                require(journal_snapshot(profile) == frozen, "Offline queries or ownerless replay changed journal bytes")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "offline fixture report")
                require(read_json(fixture_path)["counters"] == counters, "Offline cached inspection started provider HTTP")
                watch.terminate()
                watch.wait(timeout=10)
                require(engine.wait(timeout=15) == 0, "Final Watch owner failed to drain the engine")
                final = read_json(fixture_path)
                require(final["status"] == "complete" and not final["errors"] and final["counters"] == counters,
                        "Final provider completion changed or failed")
                result["provider_verification"] = verify_publications(final, ledger)
                require(len(ledger.publications) == (6 if args.gui else 12)
                        and counters["calendar_lost_responses"] == (0 if args.gui else 4), "Wrong provider mutation or uncertainty counts")
                cases = [("feature_unflagged", "engine", [], "isolated_build_guard",
                          "fm-engine: This isolated build requires --verification-google and a new disposable live profile"),
                         ("feature_flagged", "engine", ["--verification-google"], "isolation_marker",
                          "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine"),
                         ("ordinary_unflagged", "ordinary_engine", [], "isolation_marker",
                          "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine")]
                result["restart_refusals"] = []
                for case, name, flags, diagnostic, wanted in cases:
                    require(journal_snapshot(profile) == frozen, "Journal changed before refused restart")
                    stopped = subprocess.run([str(programs[name]), "--profile", str(profile), "--live-profile", *flags],
                        cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=15)
                    entry = {"case": case, "executable": name, "flags": flags, "exit_code": stopped.returncode,
                             "diagnostic_kind": diagnostic, "output": stopped.stdout.decode("utf-8", errors="replace"),
                             "journal_unchanged": journal_snapshot(profile) == frozen,
                             "journal_before_sha256": digest_rows(frozen), "journal_after_sha256": digest_rows(journal_snapshot(profile))}
                    result["restart_refusals"].append(entry)
                    with outputs["refusal"].open("a", encoding="utf-8") as stream:
                        stream.write(json.dumps(entry) + "\n")
                    require(stopped.returncode != 0 and entry["output"].splitlines() == [wanted] and entry["journal_unchanged"],
                            "Stopped isolated profile refusal changed its diagnostic or durable data")
                require(source_manifest() == sources and all(sha256(path) == identities[name]["sha256"] for name, path in programs.items()),
                        "Runtime/verifier source or executable changed during verification")
                result.update(result="passed", provider_counters=counters, journal_sha256=digest_rows(frozen),
                              offline_queries_preserved_journal_and_http=True, final_owner_drained_engine=True)
            finally:
                try:
                    stop_children(children)
                finally:
                    save_ledger()
                    if fixture_path.is_file():
                        outputs["fixture"].write_bytes(fixture_path.read_bytes())
    except Exception as error:
        result["result"], result["error"] = "failed", str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {key: sha256(path) for key, path in outputs.items() if path.is_file()}
    report.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cli", "engine", "ordinary-engine", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--gui", type=Path)
    parser.add_argument("--qt-root", type=Path)
    raise SystemExit(run(parser.parse_args()))
