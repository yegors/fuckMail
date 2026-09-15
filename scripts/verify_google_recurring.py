"""Independent v4 recurrence expectations for the existing isolated process runner.

The expected wire bodies and sources come from authored actions and fixed fixture
constants, never from the provider's response. No real provider is used here.
"""
from contextlib import closing
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import hashlib
import json

from verify_google_calendar import BASE, Ledger, calendar, journal_snapshot, same
from verify_google_limits import LIMITS
from verify_google_navigation import read_database, require
from verify_google_occurrence_sources import (
    OccurrenceLedger, normalized_original, original_time, selector,
    utc_text, verify_retained_occurrences,
)


def stable_id(parts):
    return "f" + hashlib.sha256(json.dumps(parts, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()


def instant(text):
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


def native_expectation(accounts):
    result = {"version": 1, "scenario": "recurring", "limits": deepcopy(LIMITS),
              "calendar_date": BASE.date().isoformat(), "accounts": []}
    for account in accounts:
        day = lambda n: (BASE + timedelta(days=n)).date().isoformat()
        actions = []
        for kind, scope, index in (("edit", "series", 0), ("rsvp", "occurrence", 0), ("rsvp", "series", 2)):
            action = {"kind": kind, "scope": scope, "selector": selector(account, 0, index),
                      "event_id": "co0000" if scope == "series" else "oi0000", "date": day(index), "event": {}}
            if kind == "edit":
                action["fields"] = {"title": f"Series {account['key']} café 日本語"}
            else:
                action["response"] = "tentative" if scope == "occurrence" else "accepted"
            actions.append(action)
        actions.append({"kind": "create", "save_as": "simple", "date": day(10), "fields": {
            "title": f"Repeat {account['key']}", "start": day(10) + "T09:00:00+00:00",
            "end": day(10) + "T09:30:00+00:00", "time_zone": "UTC", "all_day": False,
            "location": "Generated room", "description": "Human authored repeat",
            "recurrence": ["RRULE:FREQ=DAILY;COUNT=3"]}})
        for kind, scope, n in (("edit", "following", 11), ("delete", "series", 10)):
            action = {"kind": kind, "scope": scope, "series_from": "simple", "date": day(n), "event": {},
                      "selector": {"calendar_id": calendar(account), "original_start": {
                          "kind": "zoned", "instant": day(n) + "T09:00:00Z", "time_zone": "UTC"}}}
            if kind == "edit":
                action["fields"] = {"title": f"Following {account['key']}"}
            actions.append(action)
        result["accounts"].append({"account": account["id"], "calendar_id": calendar(account),
                                  "actions": actions, "absent_selectors": [selector(account, 1, 1)]})
    return result


class RecurringLedger(OccurrenceLedger):
    def __init__(self, accounts):
        super().__init__(accounts)
        self.aliases = {a["id"]: {} for a in accounts}
        self.exceptions = {a["id"]: {"oi0001", "od0001"} for a in accounts}

    def mutation(self, account, event, method, body, lost=False):
        result = Ledger.mutation(self, account, event, method, body, lost)
        if method == "DELETE":
            result["after"] = {"id": event, "status": "cancelled"}
            self.sources[account["id"]][event] = deepcopy(result["after"])
        self.listed[account["id"]].add(event)
        before = result["before"] or {}
        if before.get("recurringEventId"):
            self.exceptions[account["id"]].add(event)
        elif method == "PATCH" and event.startswith("co"):
            # Fixed fixture instances inherit a changed master until explicitly
            # excepted. Moved/cancelled instances retain their own exact source.
            for child_id, child in self.sources[account["id"]].items():
                if child.get("recurringEventId") != event or child_id in self.exceptions[account["id"]]:
                    continue
                inherited = deepcopy(result["after"])
                inherited.pop("recurrence", None)
                inherited.update({key: deepcopy(child[key]) for key in ("id", "recurringEventId", "originalStartTime", "start", "end")})
                inherited["etag"] = f'"{child_id}-v{self.versions[account["id"]]}"'
                child.clear()
                child.update(inherited)
        return result

    def selected(self, account, wanted):
        sources = self.sources[account["id"]]
        master = sources[wanted["series_id"]]
        original = normalized_original(wanted["original_start"])
        if master["id"].startswith("co"):
            found = [value for value in sources.values() if value.get("recurringEventId") == master["id"]
                     and normalized_original(original_time(value)) == original]
            require(len(found) == 1, "Independent selector is not a unique fixed instance")
            return deepcopy(found[0])
        start = original[1]
        require(original[0] == "zoned" and wanted["original_start"]["time_zone"] == "UTC", "Unexpected dynamic time type")
        duration = instant(master["end"]["dateTime"]) - instant(master["start"]["dateTime"])
        source = deepcopy(master)
        source.pop("recurrence")
        event = stable_id([master["id"], utc_text(start)])
        event = "fi" + event[1:41]
        source.update({"id": event, "recurringEventId": master["id"],
                       "start": {"dateTime": utc_text(start), "timeZone": "UTC"},
                       "end": {"dateTime": utc_text(start + duration), "timeZone": "UTC"},
                       "originalStartTime": {"dateTime": utc_text(start), "timeZone": "UTC"},
                       "etag": '"' + event + '-' + master["etag"].strip('"') + '"'})
        sources[event] = deepcopy(source)
        return source


def resolved_selector(account, action, ledger):
    value = deepcopy(action.get("selector", {}))
    if action.get("series_from"):
        value["series_id"] = ledger.aliases[account["id"]][action["series_from"]]
    return value


def check_read(value, account, action, selected, master):
    scope = action["scope"]
    source = master if scope == "series" else selected
    keys = {"account", "calendar_id", "event_id", "etag", "recurring", "event"}
    if scope == "following":
        keys.add("following_notice")
        require(isinstance(value["following_notice"], str) and value["following_notice"], "Missing exception-reset notice")
    require(set(value) == keys and value["account"] == account["id"] and value["calendar_id"] == calendar(account)
            and value["event_id"] == source["id"] and value["etag"] == source["etag"], "Recurring read crossed source identity/version")
    context = {"scope": scope, "series_id": master["id"], "expected_master_etag": master["etag"]}
    actual = deepcopy(value["recurring"])
    if scope != "series":
        require(normalized_original(actual.pop("original_start")) == normalized_original(original_time(selected)), "Read changed original start")
    same(actual, context, "Exact action/scope read context")
    event = value["event"]
    require(event["event_id"] == source["id"] and event["etag"] == source["etag"] and event["recurring"] is True
            and event["recurrence"] == master["recurrence"] and event["all_day"] is False
            and event["time_zone"] == "UTC", "Read editor source differs")
    for field, provider in (("title", "summary"), ("description", "description"), ("location", "location")):
        require(event[field] == source.get(provider, ""), "Read changed editable " + field)
    for field in ("start", "end"):
        require(instant(event[field]) == instant(source[field]["dateTime"]), "Read changed event time")
    for kind in ("edit", "delete", "rsvp"):
        require(event["can_" + kind] is (action["kind"] == kind), "Read granted an unselected action")


def record_action(account, action, request, read, ledger):
    request = deepcopy(request)
    if "account" in request:
        require(request.pop("account") == account["id"], "Native request crossed account")
    require(isinstance(request.get("request_id"), str) and 0 < len(request["request_id"]) <= 128, "Missing authored request ID")
    wanted = {"request_id": request["request_id"], "kind": action["kind"], "calendar_id": calendar(account)}
    publications = []
    selected_baseline, master_baseline = None, None
    if action["kind"] == "create":
        fields = deepcopy(action["fields"])
        actual_fields = deepcopy(request["fields"])
        for field in ("start", "end"):
            require(instant(actual_fields[field]) == instant(fields[field]), "Create changed authored time")
            fields[field] = actual_fields[field]
        if "conference_url" in actual_fields:
            fields["conference_url"] = ""
        same(actual_fields, fields, "Exact authored repeat fields")
        wanted["fields"] = fields
        event = stable_id([account["id"], calendar(account), request["request_id"]])
        body = {"id": event, "summary": fields["title"], "description": fields["description"],
                "location": fields["location"], "recurrence": fields["recurrence"],
                "start": {"dateTime": fields["start"], "timeZone": "UTC"},
                "end": {"dateTime": fields["end"], "timeZone": "UTC"}}
        publications.append(ledger.mutation(account, event, "POST", body))
        ledger.aliases[account["id"]][action["save_as"]] = event
    else:
        selection = resolved_selector(account, action, ledger)
        selected = ledger.selected(account, selection)
        master = deepcopy(ledger.sources[account["id"]][selection["series_id"]])
        check_read(read, account, action, selected, master)
        source = master if action["scope"] == "series" else selected
        wanted.update(event_id=source["id"], expected_etag=source["etag"], recurring=deepcopy(read["recurring"]))
        if action["scope"] == "following":
            wanted["recurring"]["reset_future_exceptions"] = True
        if action["kind"] == "edit":
            wanted["fields"] = deepcopy(action["fields"])
        elif action["kind"] == "rsvp":
            wanted["response"] = action["response"]
        if action["scope"] == "following":
            selected_baseline, master_baseline = deepcopy(selected), deepcopy(master)
            require(master["recurrence"] == ["RRULE:FREQ=DAILY;COUNT=3"], "Unexpected split source rule")
            replacement = stable_id([account["id"], calendar(account), request["request_id"], "following"])
            trim = {"recurrence": ["RRULE:COUNT=1;FREQ=DAILY"]}
            publications.append(ledger.mutation(account, master["id"], "PATCH", trim))
            body = {key: deepcopy(selected[key]) for key in ("summary", "description", "location", "start", "end")}
            body.update(id=replacement, summary=action["fields"]["title"], attendees=[], recurrence=["RRULE:COUNT=2;FREQ=DAILY"])
            publications.append(ledger.mutation(account, replacement, "POST", body))
        else:
            body = None if action["kind"] == "delete" else ledger.expected_patch(account, source["id"],
                title=action.get("fields", {}).get("title"), rsvp=action.get("response"))
            publications.append(ledger.mutation(account, source["id"], "DELETE" if action["kind"] == "delete" else "PATCH", body))
    same(request, wanted, "Exact submitted v4 action")
    return {"account": account, "request": request, "publications": publications,
            "publication": publications[-1], "following": action.get("scope") == "following",
            "selected_baseline": selected_baseline, "master_baseline": master_baseline}


def check_status(status, expected):
    request, account = expected["request"], expected["account"]
    pubs = expected["publications"]
    target = request.get("event_id", pubs[0]["event"])
    require(status["intent_version"] == 4 and status["state"] == "acknowledged" and status["local_only"] is False
            and status["request_id"] == request["request_id"] and status["kind"] == request["kind"]
            and status["target"] == {"account": account["id"], "calendar": calendar(account), "event": target}
            and status["dispatch_event_id"] == pubs[-1]["event"] and status["can_resume"] is False,
            "Recurring receipt lost exact intent/dispatch identity or certainty")
    if "recurring" in request:
        same(status["recurring"], request["recurring"], "Receipt scope")
    if expected["following"]:
        require(status["following_complete"] is True and status["step_index"] == 1, "Following receipt did not confirm both steps")
        same(status["steps"], [{"step": name, "state": "acknowledged", "target": pub["event"]}
             for name, pub in zip(("trim", "replacement"), pubs)], "Following step receipts")
    else:
        require(status["step_index"] is None and "steps" not in status, "Single write invented multi-step evidence")
    outcome = status["outcome"]
    require(outcome["kind"] == ("deleted" if request["kind"] == "delete" else "accepted")
            and outcome["evidence_origin"] == "provider_response" and outcome["notification_delivery"] == "unverified"
            and "source" not in outcome, "Public receipt invented delivery or exposed source")


def check_window(rows, ledger, accounts, begin, days):
    start = datetime.fromisoformat(begin).replace(tzinfo=timezone.utc)
    end = start + timedelta(days=days)
    expected = {}
    for account in accounts:
        sources = ledger.sources[account["id"]]
        for master_id in ledger.listed[account["id"]]:
            master = sources[master_id]
            if master.get("status") == "cancelled" or not master.get("recurrence"):
                continue
            parts = dict(part.split("=", 1) for part in master["recurrence"][0][6:].split(";"))
            require(parts["FREQ"] == "DAILY", "This independent scenario only authors daily rules")
            all_day = "date" in master["start"]
            for index in range(int(parts["COUNT"])):
                field = "date" if all_day else "dateTime"
                original = instant(master["start"][field]) + timedelta(days=index)
                current = deepcopy(master)
                for name in ("start", "end"):
                    point = instant(master[name][field]) + timedelta(days=index)
                    current[name][field] = point.date().isoformat() if all_day else utc_text(point)
                matches = [sources[event] for event in ledger.listed[account["id"]]
                           if sources[event].get("recurringEventId") == master_id
                           and normalized_original(original_time(sources[event]))[1] == (original.date().isoformat() if all_day else original)]
                require(len(matches) <= 1, "Duplicate independent exception")
                if matches:
                    current = matches[0]
                if current["status"] == "cancelled":
                    continue
                a, b = (instant(current[name][field]) for name in ("start", "end"))
                if all_day:
                    a, b = a.replace(tzinfo=timezone.utc), b.replace(tzinfo=timezone.utc)
                if b <= start or a >= end:
                    continue
                key = (account["id"], master_id, original.date().isoformat() if all_day else original)
                expected[key] = (current, a, b)
    actual = {}
    for row in rows:
        key = row["occurrence_key"]
        require(key["account"] == row["account"] and key["calendar"] == row["calendar_id"]
                and json.loads(row["id"]) == key and row["busy_only"] is False, "Projected identity/visibility changed")
        original = key["original_start"] if row["all_day"] else instant(key["original_start"])
        qualified = (row["account"], key["series"], original)
        require(qualified not in actual, "Duplicate projected occurrence")
        actual[qualified] = row
    require(set(actual) == set(expected), "Projected occurrence inventory differs from independently expanded sources")
    for key, (source, a, b) in expected.items():
        row = actual[key]
        for field, provider in (("title", "summary"), ("description", "description"), ("location", "location")):
            require(row[field] == source.get(provider, ""), "Projected occurrence changed " + field)
        observed = [instant(row[field]) for field in ("start", "end")]
        if row["all_day"]:
            observed = [point.replace(tzinfo=timezone.utc) for point in observed]
        require(observed == [a, b], "Projected occurrence changed start/end")
        people = [p for p in source.get("attendees", []) if p.get("self")]
        if people:
            require(row["self_response"] == people[0]["responseStatus"], "Explicit instance RSVP was overwritten by a master")
    return len(actual)


def run_cli(accounts, params_call, cli, ledger):
    requests, statuses = {}, {}
    for account, group in zip(accounts, native_expectation(accounts)["accounts"]):
        for index, action in enumerate(group["actions"]):
            request = {"request_id": f"recurring-{account['key']}-{index}", "kind": action["kind"], "calendar_id": calendar(account)}
            read = {}
            if action["kind"] == "create":
                request["fields"] = deepcopy(action["fields"])
            else:
                selection = resolved_selector(account, action, ledger)
                read = params_call("calendar-occurrence", account, dict(selection, scope=action["scope"], action=action["kind"]))
                request.update(event_id=read["event_id"], expected_etag=read["etag"], recurring=deepcopy(read["recurring"]))
                if action["scope"] == "following":
                    request["recurring"]["reset_future_exceptions"] = True
                request.update({key: deepcopy(action[key]) for key in ("fields", "response") if key in action})
            expected = record_action(account, action, request, read, ledger)
            status = params_call("calendar-write", account, request)
            check_status(status, expected)
            same(params_call("calendar-write", account, request), status, "Exact owned replay")
            end = (datetime.fromisoformat(action["date"]) + timedelta(days=42)).date().isoformat()
            page = cli(["calendar", "--combined", "--from", action["date"], "--to", end])
            require(page["coverage"]["complete"] is True and page["continuation"] is None,
                    "Post-action cached CLI window is incomplete")
            check_window(page["events"], ledger, accounts, action["date"], 42)
            key = account["id"] + "/" + request["request_id"]
            requests[key], statuses[key] = expected, status
    return requests, statuses


def validate_native(observed, expectation, accounts, ledger):
    require(observed["version"] == 1 and observed["scenario"] == "recurring" and observed["result"] == "passed"
            and not observed.get("error") and not observed.get("in_progress"), "Native recurring workflow incomplete")
    require(len(observed["accounts"]) == 2, "Native workflow omitted an account")
    requests = {}
    for account, wanted, native in zip(accounts, expectation["accounts"], observed["accounts"]):
        require(native["account"] == account["id"] and native["limits"] == LIMITS and len(native["actions"]) == 6,
                "Native recurring account, ownership limits or action count changed")
        check_window(native["initial_events"], ledger, [account], native["calendar_from"], native["calendar_days"])
        same(native["observed_absent_selectors"], wanted["absent_selectors"], "Native cancellation inspection")
        for action, record in zip(wanted["actions"], native["actions"]):
            require(record["account"] == account["id"] and record["kind"] == action["kind"], "Native action retargeted")
            same(record["selector"], resolved_selector(account, action, ledger), "Native captured selector")
            if action["kind"] != "create":
                require(record["scope"] == action["scope"], "Native action selected another scope")
            expected = record_action(account, action, record["request"], record["resolved_response"], ledger)
            check_status(record["outcome"], expected)
            if action["kind"] == "create":
                require(record["created_series_id"] == ledger.aliases[account["id"]]["simple"], "Native created identity is not deterministic")
            require(record["calendar_from"] == action["date"] and record["calendar_days"] == 42, "Native post-action inspection moved its window")
            check_window(record["post_events"], ledger, [account], record["calendar_from"], record["calendar_days"])
            key = account["id"] + "/" + expected["request"]["request_id"]
            require(key not in requests, "Native action reused an intent identity")
            requests[key] = expected
    return requests


def verify_final(result, accounts, requests, statuses, ledger, profile, cli, params_call):
    result["final_sources"] = verify_retained_occurrences(profile, ledger)
    frozen = journal_snapshot(profile)
    require(len(frozen) == len(requests) == 12 and len(ledger.publications) == 14, "Wrong authored intent/provider attempt counts")
    for row in frozen:
        key = row[0] + "/" + row[1]
        expected = requests[key]
        request, pubs = expected["request"], expected["publications"]
        intent, dispatch, outcome = (json.loads(row[index]) for index in (3, 11, 12))
        same(intent["request"], request, "Immutable recurring authored request")
        require(intent["version"] == dispatch["version"] == 4 and row[5] == "live" and row[6] == "acknowledged"
                and row[8] == len(pubs), "Recurring journal version, certainty or attempts changed")
        require(dispatch["target"]["event"] == pubs[-1]["event"] and dispatch["if_match"] == pubs[-1]["if_match"], "Frozen dispatch changed target/version")
        body = deepcopy(pubs[-1]["body"])
        if pubs[-1]["method"] == "POST":
            body.pop("id")
        same(dispatch["body"], body, "Frozen provider body")
        if request["kind"] != "delete":
            same(outcome["source"], pubs[-1]["after"], "Exact provider ACK source")
        with closing(read_database(profile)) as db:
            raw = db.execute("SELECT recurring_steps_json FROM calendar_write_operations WHERE account=? AND request_id=?", row[:2]).fetchone()[0]
        if expected["following"]:
            steps = json.loads(raw)
            require(len(steps) == 1 and steps[0]["state"] == "acknowledged", "Following lost its first-step evidence")
            same(steps[0]["outcome"]["source"], pubs[0]["after"], "Exact retained trim ACK")
            same(steps[0]["lease"]["dispatch"]["body"], pubs[0]["body"], "Exact retained trim body")
            same(steps[0]["lease"]["dispatch"]["baseline"], expected["master_baseline"], "Exact retained split master")
            same(steps[0]["lease"]["dispatch"]["selected_instance_baseline"], expected["selected_baseline"], "Exact selected instance before trim")
            same(dispatch["selected_instance_baseline"], expected["selected_baseline"], "Exact selected instance after replacement")
        else:
            require(raw is None, "Single write invented following history")
        status = cli(["google", "calendar-operation", row[1]], expected["account"])
        check_status(status, expected)
        if key in statuses:
            same(status, statuses[key], "Offline unchanged receipt")
        statuses[key] = status
        params_call("calendar-write", expected["account"], request, "sync_not_running")
    for account in accounts:
        seen, cursor = set(), None
        for _ in range(8):
            tail = ["calendar-operations", "--limit", "2"]
            if cursor is not None:
                tail += ["--cursor", json.dumps(cursor)]
            page = cli(tail, account)
            for status in page["operations"]:
                key = account["id"] + "/" + status["request_id"]
                require(key not in seen and status == statuses[key], "Activity duplicated or changed receipt")
                seen.add(key)
            cursor = page["continuation"]
            if cursor is None:
                break
        require(cursor is None and len(seen) == 6, "Activity omitted a recurring operation")
    begin = (BASE + timedelta(days=10)).date().isoformat()
    end = (BASE + timedelta(days=14)).date().isoformat()
    page = cli(["calendar", "--combined", "--from", begin, "--to", end])
    require(page["coverage"]["complete"] is True and page["continuation"] is None, "Cached following window incomplete")
    count = check_window(page["events"], ledger, accounts, begin, 4)
    require(count == 4, "Cancelled original series or lost replacement in final cache")
    result.update(journal_rows=len(frozen), cached_windows=[{"from": begin, "to": end, "events": count}])
    return frozen
