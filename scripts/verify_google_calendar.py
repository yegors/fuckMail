#!/usr/bin/env python3
"""Functional two-account CLI invitation checks against the isolated provider.

This is an independent source/request oracle, not a latency benchmark. It never
opens a real profile or permits an isolated profile to use a network runtime.
"""
import argparse
from contextlib import ExitStack, closing
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_navigation import LIMITS, expected_accounts, read_database, require, source_manifest
from verify_google_drafts import read_json
from verify_search_ingestion import sha256, stop_children


BASE = datetime(2026, 9, 13, tzinfo=timezone.utc)
BASE_UNIX = int(BASE.timestamp())
WINDOW = ["--from", "2026-09-13", "--to", "2026-09-15"]
WRITABLE_GUEST = {"email", "displayName", "optional", "responseStatus", "comment", "additionalGuests", "resource"}
READONLY_GUEST = {"id", "self", "organizer", "asyncOperation"}


def calendar(account):
    return f"calendar-{account['key']}@example.test"


def expected_source(account, index):
    """Independent reconstruction of all seven published fixture sources."""
    event = f"ci{index:04}"
    start = BASE + timedelta(hours=index + 1)
    email = account["email"]
    value = {
        "id": event, "etag": f'"{event}-v1"', "status": "confirmed", "eventType": "default",
        "summary": f"Invitation {account['key']} {index:04}",
        "description": "Preserved generated invitation details", "location": "Fixture room",
        "start": {"dateTime": start.strftime("%Y-%m-%dT%H:%M:%SZ"), "timeZone": "UTC"},
        "end": {"dateTime": (start + timedelta(minutes=30)).strftime("%Y-%m-%dT%H:%M:%SZ"), "timeZone": "UTC"},
        "organizer": {"email": email, "self": True}, "creator": {"email": email, "self": True},
        "attendees": [
            {"email": email, "organizer": True, "self": True, "responseStatus": "accepted"},
            {"email": "alpha@example.test", "displayName": "Alpha guest", "responseStatus": "accepted",
             "optional": True, "comment": "Preserved reply", "additionalGuests": 1},
            {"email": "beta@example.test", "displayName": "Beta guest", "responseStatus": "tentative", "resource": False}],
        "reminders": {"useDefault": False, "overrides": [{"method": "popup", "minutes": 15}]},
        "extendedProperties": {"private": {"fixture": "retained"}, "shared": {"invitation": "v1"}},
    }
    if index == 0:
        value.update({"eventLabelId": "fixture-label", "colorId": "3",
                      "conferenceData": {"conferenceSolution": {"key": {"type": "hangoutsMeet"}, "name": "Google Meet"},
                                         "conferenceId": "fixture-meet", "signature": "retained-signature",
                                         "entryPoints": [{"entryPointType": "video", "uri": "https://meet.example.test/fixture"}]},
                      "attachments": [{"fileUrl": "https://drive.example.test/fixture", "title": "Retained attachment",
                                       "mimeType": "application/pdf", "fileId": "fixture-file"}]})
    if index == 2:
        value["attendeesOmitted"] = True
    if index == 3:
        value["attendees"][1]["asyncOperation"] = "inProgress"
    if index == 6:
        value["organizer"] = {"email": "other@example.test", "self": False}
        value["creator"] = deepcopy(value["organizer"])
        value["attendees"][0] = {"email": email, "organizer": False, "self": True, "responseStatus": "needsAction"}
        value["attendees"].append({"email": "other@example.test", "organizer": True, "self": False, "responseStatus": "accepted"})
    return value


def canonical(value):
    """Guest order has no semantic meaning; every other field stays exact."""
    value = deepcopy(value)
    if isinstance(value, dict):
        value = {key: canonical(item) for key, item in value.items()}
        if isinstance(value.get("attendees"), list):
            value["attendees"].sort(key=lambda item: item["email"].lower())
    elif isinstance(value, list):
        value = [canonical(item) for item in value]
    return value


def same(actual, expected, label):
    require(canonical(actual) == canonical(expected), label + " differs from the independent ledger")


def roster_patch(before, guests):
    old = {person["email"].lower(): person for person in before["attendees"]}
    protected = [person for person in old.values() if person.get("self") or person.get("organizer")]
    wanted = {email.lower() for email in guests}
    removable = {email for email, person in old.items() if not person.get("self") and not person.get("organizer")}
    if wanted == removable:
        return None
    people = protected + [old.get(email, {"email": email, "responseStatus": "needsAction"}) for email in sorted(wanted)]
    return [{key: deepcopy(value) for key, value in person.items() if key in WRITABLE_GUEST} for person in people]


class Ledger:
    def __init__(self, accounts):
        self.sources = {account["id"]: {f"ci{i:04}": expected_source(account, i) for i in range(7)} for account in accounts}
        self.versions = {account["id"]: 1 for account in accounts}
        self.publications = []

    def expected_patch(self, account, event, title=None, guests=None, rsvp=None):
        before = self.sources[account["id"]][event]
        body = {}
        if title is not None and title != before["summary"]:
            body["summary"] = title
        if guests is not None:
            people = roster_patch(before, guests)
            if people is not None:
                body["attendees"] = people
        if rsvp is not None:
            body.update({"attendeesOmitted": True, "attendees": [{"email": account["email"], "responseStatus": rsvp}]})
        if "eventLabelId" in before:
            body["eventLabelId"] = before["eventLabelId"]
        elif "colorId" in before:
            body["colorId"] = before["colorId"]
        return body

    def mutation(self, account, event, method, body, lost=False):
        before = deepcopy(self.sources[account["id"]].get(event))
        query = {"sendUpdates": "all"}
        if method != "DELETE":
            query.update({"conferenceDataVersion": "1", "supportsAttachments": "true"})
            if "eventLabelId" in body:
                query["eventLabelVersion"] = "1"
        after = deepcopy(body if method == "POST" else before)
        if method == "DELETE":
            after["status"] = "cancelled"
        elif method == "PATCH":
            for key, value in body.items():
                if key == "attendeesOmitted":
                    continue
                if key == "attendees" and body.get("attendeesOmitted"):
                    require(len(value) == 1, "Only the authenticated self RSVP is partial")
                    matching = [person for person in after[key] if person["email"].lower() == account["email"].lower()]
                    require(len(matching) == 1 and matching[0].get("self"), "The independent self identity is ambiguous")
                    matching[0]["responseStatus"] = value[0]["responseStatus"]
                elif key == "attendees":
                    previous = {person["email"].lower(): person for person in before[key]}
                    after[key] = deepcopy(value)
                    for person in after[key]:
                        for field in READONLY_GUEST:
                            if field in previous.get(person["email"].lower(), {}):
                                person[field] = previous[person["email"].lower()][field]
                else:
                    after[key] = deepcopy(value)
        self.versions[account["id"]] += 1
        after["etag"] = f'"{event}-v{self.versions[account["id"]]}"'
        if method == "POST":
            after.update({"organizer": {"email": account["email"], "self": True}, "status": "confirmed"})
        expected = {"account": account["id"], "calendar": calendar(account), "event": event, "method": method,
                    "if_match": before["etag"] if before is not None else None, "query": query,
                    "body": deepcopy(body), "before": before, "after": after,
                    "response": "lost_response" if lost else "acknowledged", "expected_quota_units": 3 if method == "PATCH" else 1}
        self.publications.append(expected)
        self.sources[account["id"]][event] = after
        return expected


def journal_snapshot(profile):
    with closing(read_database(profile)) as db:
        return db.execute("SELECT account,request_id,fingerprint,intent_json,result_json,execution_mode,state,operation_revision,attempt,connection_generation,permission_epoch,dispatch_json,outcome_json FROM calendar_write_operations ORDER BY account,request_id").fetchall()


def read_visible_sources(profile):
    with closing(read_database(profile)) as db:
        result = {}
        for account, cal, event, raw in db.execute("SELECT account,calendar_id,event_id,json FROM calendar_visible_sources"):
            require((account, event) not in result, "Unexpected duplicate fixture event across calendars")
            result[(account, event)] = {"calendar": cal, "source": json.loads(raw)}
        return result


def verify_retained_sources(profile, ledger):
    accounts = {account["id"]: account for account in expected_accounts()}
    expected = {(account_id, event): {"calendar": calendar(accounts[account_id]), "source": source}
                for account_id, items in ledger.sources.items() for event, source in items.items()
                if source["status"] != "cancelled"}
    actual = read_visible_sources(profile)
    require(set(actual) == set(expected), "Visible Calendar identities differ from the independent ledger")
    for key, source in expected.items():
        same(actual[key], source, "Cached source " + str(key))
    deleted = [(account, event) for account, items in ledger.sources.items()
               for event, source in items.items() if source["status"] == "cancelled"]
    with closing(read_database(profile)) as db:
        for account, event in deleted:
            require(db.execute("SELECT COUNT(*) FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",
                               (account, calendar(accounts[account]), event)).fetchone()[0] == 1,
                    "Deleted source was physically discarded instead of retained with journal evidence")
    return {"visible_sources": len(expected), "hidden_retained_sources": len(deleted)}


def verify_publications(fixture_report, ledger):
    actual = fixture_report["calendar_fixture"]["publications"]
    require(len(actual) == len(ledger.publications), "Unexpected provider mutation or automatic replay")
    for account in expected_accounts():
        same([row for row in actual if row["account"] == account["id"]],
             [row for row in ledger.publications if row["account"] == account["id"]], "Exact ordered provider publications")
    before = fixture_report["counters"]
    total = len(ledger.publications)
    lost = sum(row["response"] == "lost_response" for row in ledger.publications)
    require(before["calendar_mutation_attempts"] == before["calendar_mutations"] == total
            and before["calendar_lost_responses"] == lost and before["calendar_acknowledged"] == total - lost,
            "Wrong mutation-attempt count, certainty or automatic replay")
    require(before["vault_stores"] == 0 and all(before[key] == 0 for key in
            ("raw_started", "modify_completed", "draft_created", "draft_updated", "draft_read",
             "draft_list", "draft_delete_attempts", "send_completed", "sent_read")), "Calendar workflow performed Gmail or credential writes")
    return {"publications": total, "mutation_attempts": total, "lost_responses": lost,
            "acknowledged_responses": total - lost}


def validate_status(status, account, request, state, event, ledger):
    require(status["request_id"] == request["request_id"] and status["kind"] == request["kind"]
            and status["target"] == {"account": account["id"], "calendar": calendar(account), "event": event}
            and status["intent_version"] == 2 and status["local_only"] is False
            and status["state"] == state, "Wrong scoped versioned Calendar status")
    if state in ("accepted", "acknowledged"):
        require(status["outcome"]["kind"] == ("deleted" if request["kind"] == "delete" else "accepted")
                and status["outcome"]["evidence_origin"] == "provider_response"
                and status["outcome"]["notification_delivery"] == "unverified", "Provider ACK claimed notification delivery")
    if state == "reconciled":
        require(status["outcome"]["kind"] == "observed_state" and status["outcome"]["matches_intent"] is True
                and status["outcome"]["event_cancelled"] is (request["kind"] == "delete")
                and status["outcome"]["notification_delivery"] == "unverified"
                and "evidence_origin" not in status["outcome"], "Read observation became a fabricated provider response")
        require(status["outcome"]["observed_etag"] == ledger.sources[account["id"]][event]["etag"], "Read evidence names the wrong source revision")


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {key: report.with_suffix(suffix) for key, suffix in {
        "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log", "workspace": ".workspace.log",
        "fixture": ".fixture.json", "sources": ".source.json", "ledger": ".ledger.json", "refusal": ".refusal.log"}.items()}
    require(not report.exists() and not any(path.exists() for path in outputs.values()), "Use a new numbered evidence path")
    programs = {name: getattr(args, name).resolve(strict=True) for name in ("cli", "engine")}
    if args.ordinary_engine:
        programs["ordinary_engine"] = args.ordinary_engine.resolve(strict=True)
    identities = {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()}
    sources = source_manifest()
    outputs["sources"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    accounts = expected_accounts()
    ledger = Ledger(accounts)
    result = {"result": "failed", "scenario": "isolated-google-calendar-cli", "version": 1,
              "base_unix_secs": BASE_UNIX, "window": {"from": "2026-09-13", "to": "2026-09-15"},
              "executables": identities, "commands": [], "accounts": [], "limits": LIMITS,
              "expected_provider_publications": ledger.publications,
              "limitations": ["Generated in-memory provider/vault only; no live events or notification delivery.",
                              "Functional CLI/source checks only; no performance, packages or populated-engine restart acceptance.",
                              "Attendee arrays are compared as exact identity sets; all other source/request fields remain exact."]}
    children = []
    started = time.monotonic()
    requests, statuses = {}, {}

    def save_ledger():
        outputs["ledger"].write_text(json.dumps({"sources": ledger.sources, "publications": ledger.publications,
                                                "requests": requests, "statuses": statuses}, indent=2) + "\n", encoding="utf-8")

    try:
        with tempfile.TemporaryDirectory(prefix="fuckmail-calendar-invitations-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {key: logs.enter_context(outputs[key].open("wb")) for key in ("engine", "watch", "personal", "workspace")}
            env = package_environment()
            engine = subprocess.Popen([str(programs["engine"]), "--profile", str(profile), "--live-profile",
                                       "--verification-google", "--verification-google-calendar",
                                       "--verification-google-calendar-base", str(BASE_UNIX)],
                                      cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                      stdout=streams["engine"], stderr=subprocess.STDOUT)
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

            def cli(tail, account=None, refused=None):
                require(engine.poll() is None, "Never reopen a populated isolated profile through a CLI query")
                output = subprocess.run(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                require(bool(output.stdout.strip()), "No CLI response: " + output.stderr[-400:])
                data = json.loads(output.stdout)
                error = data.get("error", {}).get("code")
                result["commands"].append({"command": tail[:2], "account": account["key"] if account else None,
                                           "exit_code": output.returncode, "error": error})
                if refused:
                    require(output.returncode != 0 and data.get("ok") is False and error == refused,
                            "Unexpected refusal: " + str(data.get("error")))
                    return data["error"]
                require(output.returncode == 0 and data.get("ok") is True, "CLI action failed: " + str(data.get("error")))
                return data["result"]

            def visible():
                return {key: value["source"] for key, value in read_visible_sources(profile).items()}

            def acquired():
                with closing(read_database(profile)) as db:
                    rows = db.execute("SELECT account,calendar_id,full_generation,retained_errors FROM calendar_sync_state WHERE calendar_id<>''").fetchall()
                    return len(rows) == 2 and all(generation > 0 and errors == 0 for _, _, generation, errors in rows) and len(visible()) == 14

            def check_cache():
                try:
                    return verify_retained_sources(profile, ledger)
                except Exception:
                    result["source_mismatch_snapshot"] = [
                        {"account": account, "event": event, **value}
                        for (account, event), value in sorted(read_visible_sources(profile).items())
                    ]
                    raise

            def write(account, request, body=None, refused=None, lost=False):
                key = account["id"] + "/" + request["request_id"]
                require(key not in requests, "The test accidentally reused a new request ID")
                requests[key] = deepcopy(request)
                save_ledger()
                file = scratch / f"params-{len(requests):03}.json"
                file.write_text(json.dumps(request) + "\n", encoding="utf-8")
                response = cli(["google", "calendar-write", "--params-file", str(file)], account, refused)
                if refused:
                    retained = cli(["google", "calendar-operation", request["request_id"]], account)
                    validate_status(retained, account, request, "prepared", request["event_id"], ledger)
                    statuses[key] = retained
                    save_ledger()
                    return response
                event = response["target"]["event"]
                method = {"create": "POST", "delete": "DELETE", "edit": "PATCH", "rsvp": "PATCH"}[request["kind"]]
                if method == "POST":
                    require(re.fullmatch(r"f[0-9a-f]{64}", event) is not None and all(event not in items for items in ledger.sources.values()),
                            "Created Calendar identity is invalid or reused")
                    body = {**body, "id": event}
                else:
                    require(event == request["event_id"], "Mutation retargeted a different event")
                ledger.mutation(account, event, method, body, lost)
                save_ledger()  # Expected evidence survives any subsequent oracle failure.
                validate_status(response, account, request, "unknown" if lost else "acknowledged", event, ledger)
                statuses[key] = response
                if lost:
                    replay = cli(["google", "calendar-write", "--params-file", str(file)], account)
                    require(replay == response, "Exact unknown replay changed the immutable attempt")
                    observed = cli(["google", "calendar-reconcile", request["request_id"]], account)
                    validate_status(observed, account, request, "reconciled", event, ledger)
                    require(cli(["google", "calendar-write", "--params-file", str(file)], account) == observed,
                            "A reconciled request was submitted again")
                    statuses[key] = observed
                save_ledger()
                check_cache()
                return statuses[key]

            def request(account, name, kind, event, fields=None, response=None, etag=None):
                value = {"request_id": name, "kind": kind, "calendar_id": calendar(account), "event_id": event,
                         "expected_etag": etag or ledger.sources[account["id"]][event]["etag"]}
                if fields is not None:
                    value["fields"] = fields
                if response is not None:
                    value["response"] = response
                return value

            try:
                wait_for(lambda: fixture_path.is_file() and (profile / "engine.json").is_file(), "startup")
                require(read_json(profile / "engine.json")["pid"] == engine.pid, "Wrong discovery owner")
                initial = read_json(fixture_path)
                require(initial["native_vault"] is False and initial["transport"] == "in_memory_only"
                        and initial["base_unix_secs"] == BASE_UNIX and initial["accounts"] == accounts
                        and initial["calendar_fixture"]["enabled"] is True, "Calendar provider isolation or identity changed")
                watch = subprocess.Popen(command(["watch", "--seconds", "900"]), cwd=scratch, env=env,
                                         stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                preferences = cli(["preferences", "show"])
                preferences_file = scratch / "calendar-preferences.json"
                preferences_file.write_text(json.dumps({"calendar_display_time_zone": "UTC"}) + "\n", encoding="utf-8")
                cli(["preferences", "save", "--preferences-file", str(preferences_file),
                     "--expected-revision", str(preferences["revision"]), "--request-id", "calendar-display-utc"])
                owners = []
                for account in accounts:
                    tail = ["google", "sync", "--seconds", "900"]
                    for key, value in LIMITS.items():
                        tail.extend(["--" + key.replace("_", "-"), str(value)])
                    owner = subprocess.Popen(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    owners.append(owner)
                    children.append(owner)
                wait_for(acquired, "complete initial Calendar acquisition")
                save_ledger()
                initial_visible = visible()
                for account in accounts:
                    for index in range(7):
                        require(initial_visible[(account["id"], f"ci{index:04}")] == expected_source(account, index),
                                "Initial acquired source differs from independently reconstructed JSON")
                result["independently_verified_initial_sources"] = 14
                for account in accounts:
                    page = cli(["calendar", *WINDOW], account)
                    require(len(page["events"]) == 7, "Cached Calendar window omitted a generated event")
                    for number, (name, guests, title) in enumerate([
                        ("add-gamma", ["alpha@example.test", "beta@example.test", "gamma@example.test"], None),
                        ("title-noop-guests", ["gamma@example.test", "beta@example.test", "alpha@example.test"], "Human changed invitation title"),
                        ("remove-alpha", ["beta@example.test", "gamma@example.test"], None),
                        ("remove-all", [], None),
                    ]):
                        fields = {"attendees": guests}
                        if title is not None:
                            fields["title"] = title
                        body = ledger.expected_patch(account, "ci0000", title, guests)
                        if number == 1:
                            require("attendees" not in body, "No-op roster unexpectedly changes the provider array")
                        write(account, request(account, name, "edit", "ci0000", fields), body)
                    write(account, request(account, "stale-etag", "edit", "ci0000", {"title": "Unsent stale text"}, etag='"ci0000-v1"'), refused="stale_revision")
                    write(account, request(account, "duplicate-guests", "edit", "ci0000", {"attendees": ["alpha@example.test", "ALPHA@example.test"]}), refused="invalid_request")
                    write(account, request(account, "protected-guest", "edit", "ci0000", {"attendees": [account["email"]]}), refused="invalid_request")
                    for event in ("ci0002", "ci0003"):
                        write(account, request(account, event + "-roster-refusal", "edit", event, {"attendees": []}), refused="incomplete_attendees")
                        title = "Title only preserves unsupported roster " + event
                        write(account, request(account, event + "-title", "edit", event, {"title": title}), ledger.expected_patch(account, event, title))
                    guests = ["alpha@example.test", "beta@example.test", "gamma@example.test"]
                    write(account, request(account, "lost-invitation", "edit", "ci0001", {"attendees": guests}), ledger.expected_patch(account, "ci0001", guests=guests), lost=True)
                    write(account, request(account, "lost-delete", "delete", "ci0004"), lost=True)
                    write(account, request(account, "ack-delete", "delete", "ci0005"))
                    write(account, request(account, "self-rsvp", "rsvp", "ci0006", response="accepted"), ledger.expected_patch(account, "ci0006", rsvp="accepted"))
                    write(account, request(account, "foreign-organizer", "edit", "ci0006", {"title": "Cannot edit another organizer copy"}), refused="unsupported_operation")
                    fields = {"title": "New explicit invitation " + account["key"], "start": "2026-09-14T12:00:00Z",
                              "end": "2026-09-14T13:00:00Z", "all_day": False, "time_zone": "UTC", "attendees": ["delta@example.test"]}
                    body = {"summary": fields["title"], "start": {"dateTime": fields["start"], "timeZone": "UTC"},
                            "end": {"dateTime": fields["end"], "timeZone": "UTC"}, "attendees": [{"email": "delta@example.test", "responseStatus": "needsAction"}]}
                    created = write(account, {"request_id": "create-invitation", "kind": "create", "calendar_id": calendar(account), "fields": fields}, body)
                    write(account, request(account, "delete-created", "delete", created["target"]["event"]))
                    result["accounts"].append({"id": account["id"], "key": account["key"], "calendar": calendar(account),
                                               "original_sources": 7, "publications": 12, "lost_responses": 2, "refusals": 6})
                check_cache()
                for owner in owners:
                    require(owner.poll() is None, "An explicit sync owner ended prematurely")
                    owner.terminate()
                    owner.wait(timeout=10)
                wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "sync owners released")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "post-release provider report")
                before = read_json(fixture_path)["counters"]
                frozen_journal = journal_snapshot(profile)
                require(len(frozen_journal) == len(requests) == 36, "Unexpected missing or invented Calendar intent")
                for row in frozen_journal:
                    key = row[0] + "/" + row[1]
                    intent = json.loads(row[3])
                    require(intent["version"] == 2 and intent["request"] == requests[key], "Retained original intent changed")
                    if row[11] is not None:
                        dispatch = json.loads(row[11])
                        publication = next(item for item in ledger.publications if item["account"] == row[0]
                                           and item["event"] == intent["target"]["event"] and item["if_match"] == dispatch["if_match"])
                        require(dispatch["version"] == 2 and dispatch["send_updates"] == "all", "Dispatch lost its version or notification policy")
                        same(dispatch["baseline"], publication["before"], "Frozen pre-mutation source")
                        expected_body = deepcopy(publication["body"])
                        if publication["method"] == "POST":
                            del expected_body["id"]
                        same(dispatch["body"], expected_body, "Frozen dispatch body")
                for account in accounts:
                    cli(["calendar", *WINDOW], account)
                    cursor, seen = None, set()
                    while True:
                        tail = ["calendar-operations", "--limit", "3"]
                        if cursor is not None:
                            tail += ["--cursor", json.dumps(cursor)]
                        page = cli(tail, account)
                        for status in page["operations"]:
                            key = account["id"] + "/" + status["request_id"]
                            require(key not in seen and status == statuses[key], "History changed or duplicated a scoped receipt")
                            seen.add(key)
                            require(cli(["google", "calendar-operation", status["request_id"]], account) == status, "Offline status changed")
                        cursor = page["continuation"]
                        if cursor is None:
                            break
                    require(seen == {key for key in requests if key.startswith(account["id"] + "/")}, "History hid retained refusal or completed intent")
                cli(["calendar-sources"])
                require(journal_snapshot(profile) == frozen_journal, "Offline queries changed journal bytes")
                result["journal_rows"] = len(frozen_journal)
                result["journal_sha256"] = hashlib.sha256(json.dumps(frozen_journal, ensure_ascii=False).encode()).hexdigest()
                with closing(read_database(profile)) as db:
                    require(db.execute("PRAGMA user_version").fetchone()[0] == 20, "Wrong shared schema")
                    for publication in ledger.publications:
                        if publication["method"] == "DELETE":
                            require(db.execute("SELECT COUNT(*) FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",
                                               (publication["account"], publication["calendar"], publication["event"])).fetchone()[0] == 1,
                                    "Deleted source was physically discarded instead of retained with journal evidence")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "offline provider report")
                require(read_json(fixture_path)["counters"] == before, "Offline Calendar inspection started HTTP")
                watch.terminate()
                watch.wait(timeout=10)
                require(engine.wait(timeout=15) == 0, "Final Watch owner did not drain the engine")
                final = read_json(fixture_path)
                require(final["status"] == "complete" and not final["errors"] and final["counters"] == before, "Final provider completion changed or failed")
                save_ledger()
                require(len(ledger.publications) == 24, "The expected CLI scenario did not complete")
                result["verified_provider"] = verify_publications(final, ledger)
                result["provider_counters"] = before
                result["offline_queries_preserved_journal_and_http"] = True
                result["final_owner_drained_engine"] = True
                # The feature build's unflagged guard precedes the profile
                # marker. Exercise that guard and both actual marker paths.
                require("ordinary_engine" in programs, "All three isolation refusal cases require --ordinary-engine")
                refusal_cases = [
                    ("feature_unflagged", "engine", [], "isolated_build_guard",
                     "fm-engine: This isolated build requires --verification-google and a new disposable live profile"),
                    ("feature_flagged", "engine", ["--verification-google"], "isolation_marker",
                     "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine"),
                    ("ordinary_unflagged", "ordinary_engine", [], "isolation_marker",
                     "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine"),
                ]
                result["restart_refusals"] = []
                for case, name, flags, diagnostic_kind, expected_diagnostic in refusal_cases:
                    prior_journal = journal_snapshot(profile)
                    require(prior_journal == frozen_journal, "Journal changed before a refused restart")
                    refusal = subprocess.run([str(programs[name]), "--profile", str(profile), "--live-profile", *flags],
                                             cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                             stderr=subprocess.STDOUT, timeout=15)
                    after_journal = journal_snapshot(profile)
                    entry = {"case": case, "executable": name, "flags": flags, "exit_code": refusal.returncode,
                             "diagnostic_kind": diagnostic_kind, "output": refusal.stdout.decode("utf-8", errors="replace"),
                             "journal_unchanged": after_journal == frozen_journal,
                             "journal_before_sha256": hashlib.sha256(json.dumps(prior_journal, ensure_ascii=False).encode()).hexdigest(),
                             "journal_after_sha256": hashlib.sha256(json.dumps(after_journal, ensure_ascii=False).encode()).hexdigest()}
                    result["restart_refusals"].append(entry)
                    with outputs["refusal"].open("a", encoding="utf-8") as stream:
                        stream.write(json.dumps(entry, ensure_ascii=False) + "\n")
                    require(refusal.returncode != 0 and entry["output"].splitlines() == [expected_diagnostic],
                            "Refusal did not produce the exact " + diagnostic_kind + " diagnostic: " + case)
                    require(entry["journal_unchanged"], "Refused restart touched the shared journal: " + case)
                require(source_manifest() == sources and all(sha256(path) == identities[name]["sha256"] for name, path in programs.items()),
                        "Runtime or verifier sources/executables changed during verification")
                result["result"] = "passed"
            finally:
                try:
                    stop_children(children)
                except Exception as cleanup_error:
                    result["cleanup_error"] = str(cleanup_error)
                save_ledger()
                if fixture_path.is_file():
                    outputs["fixture"].write_bytes(fixture_path.read_bytes())
    except Exception as error:
        result["result"] = "failed"
        result["error"] = str(error)
    if result.get("cleanup_error"):
        result["result"] = "failed"
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {name: sha256(path) for name, path in outputs.items() if path.is_file()}
    report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--engine", type=Path, required=True)
    parser.add_argument("--ordinary-engine", type=Path, required=True, help="Prove ordinary marker refusal alongside both isolated-build refusal paths")
    parser.add_argument("--report", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
