#!/usr/bin/env python3
"""Verify a recorded journal-schema upgrade and actual old-engine refusal.

Ordinary standalone engine/CLI executables, not a package or live-account test.
"""
import argparse
from contextlib import closing
import json
from pathlib import Path
import tempfile
import time

from dev import package_environment
from verify_google_navigation import source_manifest as workspace_sources
from verify_google_navigation import read_database
from verify_upgrade import Check, contains_fields, durable_snapshot, require, sha256


def source_manifest():
    # This process check compiles/runs no GUI. Independent native-test edits do
    # not change its Rust or Python inputs.
    root = Path(__file__).resolve().parents[1]
    helpers = {"scripts/verify_draft_snapshot_upgrade.py", "scripts/verify_upgrade.py",
               "scripts/verify_google_navigation.py", "scripts/verify_search_ingestion.py", "scripts/dev.py",
               "scripts/toolchain.json", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
    result = {name: value for name, value in workspace_sources().items()
              if name.startswith("crates/") or name in helpers}
    result.update({path.relative_to(root).as_posix(): sha256(path)
                   for path in (root / "fixtures").rglob("*") if path.is_file()})
    return result


def seed_calendar(check, files):
    """Generate old-engine Calendar receipts through the ordinary CLI boundary."""
    fields = {"title": "Preserved meeting — 日本語", "start": "2026-09-12T09:00:00-04:00",
              "end": "2026-09-12T10:00:00-04:00", "all_day": False,
              "time_zone": "America/Toronto", "location": "Generated room",
              "description": "Human meeting notes", "conference_url": "https://example.test/meeting",
              "attendees": ["you@personal.example", "guest@example.test"]}
    create = {"request_id": "upgrade-calendar-create", "kind": "create", "calendar_id": "primary",
              "fields": fields}
    created = check.call(files, "calendar_write", create, account="personal")
    require(created["state"] == "fake_acknowledged", "Missing old synthetic Calendar create receipt")
    edit = {"request_id": "upgrade-calendar-edit", "kind": "edit", "calendar_id": "primary",
            "event_id": created["target"]["event"], "expected_etag": created["etag"],
            "fields": {**fields, "title": "Retained edited meeting — 日本語"}}
    edited = check.call(files, "calendar_write", edit, account="personal")
    query = {"from": "2026-09-07", "to": "2026-09-14"}
    initial = check.call(files, "calendar", query, account="personal")
    source = next(event for event in initial["events"] if event["event_id"] == "cross-account")
    rsvp = {"request_id": "upgrade-calendar-rsvp", "kind": "rsvp", "calendar_id": "primary",
            "event_id": source["event_id"], "expected_etag": source["etag"], "response": "accepted"}
    responded = check.call(files, "calendar_write", rsvp, account="personal")
    pending = {**create, "request_id": "upgrade-calendar-pending", "prepare_only": True,
               "fields": {**fields, "title": "Prepared unsent meeting"}}
    prepared = check.call(files, "calendar_write", pending, account="personal")
    require(prepared["state"] == "pending", "Missing old pending Calendar receipt")
    return {"query": query, "projection": check.call(files, "calendar", query, account="personal"),
            "requests": [create, edit, rsvp, pending], "receipts": [created, edited, responded, prepared]}


def verify_calendar(check, files, retained):
    for request, receipt in zip(retained["requests"], retained["receipts"], strict=True):
        require(check.call(files, "calendar_write", request, account="personal") == receipt,
                "Migration changed an immutable/pending Calendar receipt")
    require(contains_fields(check.call(files, "calendar", retained["query"], account="personal"),
                            retained["projection"]), "Migration changed the retained Calendar projection")


def verify_account_lifecycle(check, files, retained):
    active = check.call(files, "accounts", account=None)
    all_accounts = check.call(files, "accounts", {"include_removed": True}, account=None)
    require(active == all_accounts and contains_fields(active, retained),
            "Migration hid or altered a previously active account")
    require(len(active["accounts"]) > 0, "Expected old synthetic accounts for lifecycle migration")
    for account in active["accounts"]:
        require(account["lifecycle"] == {"state": "active", "revision": 1, "cleanup": None}
                and account["read_only"] is False,
                "Migration failed to initialize an active account without cleanup work")
    return active


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    # Reserve before launching anything; this report cannot replace an older run.
    with report.open("x", encoding="utf-8") as destination:
        model_upgrade = (args.old_schema, args.new_schema) == (20, 21)
        workflows_upgrade = (args.old_schema, args.new_schema) == (19, 20) or model_upgrade
        occurrence_upgrade = (args.old_schema, args.new_schema) == (18, 19) or workflows_upgrade
        lifecycle_upgrade = (args.old_schema, args.new_schema) == (17, 18) or occurrence_upgrade
        calendar_upgrade = (args.old_schema, args.new_schema) == (16, 17) or lifecycle_upgrade
        scenario = "local-model" if model_upgrade else "calendar-workflows" if workflows_upgrade else "calendar-occurrences" if occurrence_upgrade else "account-lifecycle" if lifecycle_upgrade else "calendar-intents" if calendar_upgrade else "draft-provenance"
        result = {"result": "failed", "scenario": f"schema-{args.old_schema}-to-{args.new_schema}-{scenario}", "version": 5,
                  "limitations": ["Generated synthetic composition/Calendar only; live journals have separate Core fixtures.",
                                  "Standalone engine/CLI source evidence; no GUI/package/performance acceptance."], "artifacts": []}
        started = time.monotonic()
        try:
            old = {"fm": args.old_cli.resolve(strict=True), "fm-engine": args.old_engine.resolve(strict=True)}
            new = {"fm": args.new_cli.resolve(strict=True), "fm-engine": args.new_engine.resolve(strict=True)}
            require((args.old_schema, args.new_schema) in ((14, 15), (15, 16), (16, 17), (17, 18), (18, 19), (19, 20), (20, 21)), "Choose a documented journal-schema upgrade")
            require(old["fm-engine"] != new["fm-engine"], "Use a distinct preserved old-schema engine")
            result["executables"] = {kind: {key: {"path": str(path), "sha256": sha256(path)} for key, path in files.items()}
                                     for kind, files in (("old", old), ("new", new))}
            sources = source_manifest()
            result["sources"] = sources
            with tempfile.TemporaryDirectory(prefix="fuckmail-draft-upgrade-") as temporary:
                check = Check(Path(temporary).resolve(), package_environment())
                check.demo_profile = model_upgrade

                def release(owner, engine):
                    require(owner.poll() is None, "Local owner ended before assertions completed")
                    owner.terminate()
                    owner.wait(timeout=10)
                    require(engine.wait(timeout=10) == 0 and check.pid() is None, "Engine did not release its final local owner")

                try:
                    before_engine = check.start_engine(old, "before-engine")
                    before_owner = check.start_owner(old, "before-owner", seconds=300)
                    status = check.call(old, "status", account=None)
                    require(status["schema_version"] == args.old_schema and status["synthetic"] is True
                            and status["fixture_revision"] == 3, "Expected preserved old-schema synthetic foundation")
                    compatible = check.call(new, "status", account=None)
                    require(compatible == status and check.pid() == before_engine.pid, "New CLI replaced the active compatible old engine")
                    draft = check.call(old, "draft_create", {"mode": "new", "request_id": "upgrade-new"})
                    params = {"draft_id": draft["draft_id"], "expected_revision": draft["revision"], "request_id": "upgrade-save",
                              "subject": "Retained schema migration", "body": "Human authored text\n日本語 — café",
                              "body_html": "<p>Human authored text<br>日本語 — café</p>",
                              "to_recipients": [{"name": "Generated recipient", "email": "recipient@example.test"}],
                              "cc_recipients": [], "bcc_recipients": [{"email": "blind@example.test"}], "attachment_ids": []}
                    saved = check.call(old, "draft_save", params)
                    capture_params = {"draft_id": draft["draft_id"], "expected_revision": saved["revision"],
                                      "request_id": "upgrade-capture", "outcome": "accepted"}
                    captured = check.call(old, "draft_capture", capture_params)
                    current = check.call(old, "draft_get", {"draft_id": draft["draft_id"]})
                    retained_calendar = seed_calendar(check, old) if calendar_upgrade else None
                    retained_accounts = check.call(old, "accounts", account=None) if lifecycle_upgrade else None
                    release(before_owner, before_engine)
                    before = durable_snapshot(check.profile)
                    require(before["schema"] == args.old_schema and before["record_counts"]["capture_operations"] >= 1, "Missing pre-upgrade authored receipt")

                    after_engine = check.start_engine(new, "after-engine")
                    after_owner = check.start_owner(new, "after-owner", seconds=300)
                    upgraded = check.call(new, "status", account=None)
                    require(upgraded["schema_version"] == args.new_schema and upgraded["fixture_revision"] == 3
                            and upgraded["synthetic"] is True, "Migration changed the fixture or wrong schema")
                    recovered = check.call(new, "draft_get", {"draft_id": draft["draft_id"]})
                    require(contains_fields(recovered, current) and recovered["origin"] == "local"
                            and recovered["read_only"] is False and recovered["remote_source"] is None,
                            "Migration altered authored fields or made old draft a remote snapshot")
                    replay = check.call(new, "draft_capture", capture_params)
                    require(replay == captured, "Migration rewrote the exact immutable capture receipt")
                    if retained_calendar is not None:
                        verify_calendar(check, new, retained_calendar)
                    if retained_accounts is not None:
                        migrated_accounts = verify_account_lifecycle(check, new, retained_accounts)
                    release(after_owner, after_engine)
                    after = durable_snapshot(check.profile, before["columns"])
                    require(before["table_digests"] == after["table_digests"], "Old persisted table/column values changed during migration")
                    require(before["retained_blobs"] == after["retained_blobs"], "Migration changed retained MIME or attachment bytes")
                    if workflows_upgrade:
                        with closing(read_database(check.profile)) as db:
                            require(db.execute("SELECT COUNT(*) FROM calendar_reminder_receipts").fetchone()[0] == 0,
                                    "Migration invented reminder receipts")
                            require("recurring_steps_json" in {row[1] for row in db.execute("PRAGMA table_info(calendar_write_operations)")},
                                    "Migration omitted the recurring step receipt column")
                    refusal, refusal_log = check.spawn(old["fm-engine"], "--profile", check.profile, label="old-engine-refusal")
                    require(refusal.wait(timeout=35) != 0 and check.pid() is None, "Old engine opened a schema it cannot safely interpret")
                    require("schema_mismatch" in Path(refusal_log.name).read_text(encoding="utf-8", errors="replace"),
                            "Old engine failed for a reason other than newer-schema refusal")
                    refused = durable_snapshot(check.profile, before["columns"])
                    require(refused["logical_database_sha256"] == after["logical_database_sha256"]
                            and refused["retained_blobs"] == after["retained_blobs"], "Old-engine refusal modified durable data")
                    reopened_engine = check.start_engine(new, "reopened-engine")
                    reopened_owner = check.start_owner(new, "reopened-owner", seconds=300)
                    require(check.call(new, "draft_get", {"draft_id": draft["draft_id"]}) == recovered,
                            "Fresh new engine failed to reopen preserved draft after refusal")
                    require(check.call(new, "draft_capture", capture_params) == captured, "Fresh engine changed immutable replay")
                    if retained_calendar is not None:
                        verify_calendar(check, new, retained_calendar)
                    if retained_accounts is not None:
                        require(verify_account_lifecycle(check, new, retained_accounts) == migrated_accounts,
                                "Fresh engine changed migrated lifecycle metadata")
                    release(reopened_owner, reopened_engine)
                    result["before"] = before
                    result["after"] = after
                    result["refused"] = refused
                    result["draft_id"] = draft["draft_id"]
                    result["capture_receipt"] = captured
                    if retained_calendar is not None:
                        result["calendar"] = retained_calendar
                    if retained_accounts is not None:
                        result["accounts"] = {"before": retained_accounts, "after": migrated_accounts}
                    require(source_manifest() == sources, "Sources changed during migration verification")
                    for kind, files in (("old", old), ("new", new)):
                        require(all(sha256(path) == result["executables"][kind][key]["sha256"] for key, path in files.items()),
                                "Executable changed during migration verification")
                    result["result"] = "passed"
                finally:
                    check.close()
                    for log in check.outputs:
                        source = Path(log.name)
                        target = report.with_suffix("." + source.name)
                        require(not target.exists(), "Never overwrite previous migration logs")
                        target.write_bytes(source.read_bytes())
                        result["artifacts"].append({"name": target.name, "sha256": sha256(target)})
        except Exception as error:
            result["result"] = "failed"
            result["error"] = str(error)
        result["elapsed_seconds"] = time.monotonic() - started
        destination.write(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old-cli", type=Path, required=True)
    parser.add_argument("--old-engine", type=Path, required=True)
    parser.add_argument("--new-cli", type=Path, required=True)
    parser.add_argument("--new-engine", type=Path, required=True)
    parser.add_argument("--old-schema", type=int, choices=(14, 15, 16, 17, 18, 19, 20), default=14)
    parser.add_argument("--new-schema", type=int, choices=(15, 16, 17, 18, 19, 20, 21), default=15)
    parser.add_argument("--report", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
