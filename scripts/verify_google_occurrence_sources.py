"""Independent generated-source expectations shared by occurrence CLI/native checks.

No provider calls or performance measurements occur in this module. The fixed
corpus extends the previously verified invitation metadata; it never reads a
provider response to construct its expected original sources.
"""
from copy import deepcopy
from contextlib import closing
from datetime import datetime, timedelta, timezone
import hashlib
import json

from verify_google_calendar import BASE, Ledger, calendar, expected_source, same
from verify_google_limits import LIMITS
from verify_google_navigation import read_database, require


MASTER_COUNTS = (6, 3, 2, 2)
INSTANCE_PREFIXES = ("oi", "od", "oc", "ol")


def utc_text(value):
    return value.strftime("%Y-%m-%dT%H:%M:%SZ")


def expected_master(account, index):
    require(0 <= index < len(MASTER_COUNTS), "Unknown fixed recurring master")
    value = expected_source(account, 0)
    event = f"co{index:04}"
    value.update({"id": event, "etag": f'"{event}-v1"',
                  "summary": f"Occurrence {account['key']} {index:04}",
                  "description": "Preserved generated occurrence details",
                  "recurrence": [f"RRULE:FREQ=DAILY;COUNT={MASTER_COUNTS[index]}"]})
    if index == 1:
        date = BASE.date() + timedelta(days=1)
        value["start"] = {"date": date.isoformat()}
        value["end"] = {"date": (date + timedelta(days=1)).isoformat()}
    else:
        start = BASE + timedelta(hours=index + 1)
        value["start"] = {"dateTime": utc_text(start), "timeZone": "UTC"}
        value["end"] = {"dateTime": utc_text(start + timedelta(minutes=30)), "timeZone": "UTC"}
    if index == 2:
        value["status"] = "cancelled"
    if index == 3:
        value["locked"] = True
    return value


def cancelled_source(source):
    return {"id": source["id"], "status": "cancelled",
            "recurringEventId": source["recurringEventId"],
            "originalStartTime": deepcopy(source["originalStartTime"])}


def expected_instance(account, master, index):
    require(0 <= master < len(MASTER_COUNTS) and 0 <= index < MASTER_COUNTS[master],
            "Unknown fixed recurring instance")
    source = expected_master(account, master)
    event = f"{INSTANCE_PREFIXES[master]}{index:04}"
    source.update({"id": event, "etag": f'"{event}-v1"', "status": "confirmed",
                   "recurringEventId": f"co{master:04}"})
    del source["recurrence"]
    for field in ("start", "end"):
        if master == 1:
            date = BASE.date() + timedelta(days=1 + index + (field == "end"))
            source[field] = {"date": date.isoformat()}
        else:
            point = BASE + timedelta(days=index, hours=master + 1,
                                     minutes=30 if field == "end" else 0)
            source[field] = {"dateTime": utc_text(point), "timeZone": "UTC"}
    source["originalStartTime"] = deepcopy(source["start"])
    if master == 0 and index == 1:
        for field in ("start", "end"):
            point = BASE + timedelta(days=61, hours=1, minutes=30 if field == "end" else 0)
            source[field] = {"dateTime": utc_text(point), "timeZone": "UTC"}
    return cancelled_source(source) if (master, index) == (1, 1) else source


def original_time(source):
    original = source["originalStartTime"]
    if "date" in original:
        return {"kind": "date", "date": original["date"]}
    return {"kind": "zoned", "instant": original["dateTime"],
            "time_zone": original["timeZone"]}


def selector(account, master, index, *, hint=False):
    source = expected_instance(account, master, index)
    value = {"calendar_id": calendar(account), "series_id": f"co{master:04}",
             "original_start": original_time(source)}
    if hint:
        value["event_id"] = source["id"]
    return value


class OccurrenceLedger(Ledger):
    def __init__(self, accounts):
        self.sources = {}
        self.listed = {}
        self.calendars = {account["id"]: calendar(account) for account in accounts}
        self.versions = {account["id"]: 1 for account in accounts}
        self.publications = []
        for account in accounts:
            sources = {}
            for master, count in enumerate(MASTER_COUNTS):
                value = expected_master(account, master)
                sources[value["id"]] = value
                for index in range(count):
                    value = expected_instance(account, master, index)
                    sources[value["id"]] = value
            self.sources[account["id"]] = sources
            self.listed[account["id"]] = {f"co{i:04}" for i in range(4)} | {"oi0001", "od0001"}

    def mutation(self, account, event, method, body, lost=False):
        require(method in ("PATCH", "DELETE"), "Occurrence workflow cannot create a series")
        require(event in {"oi0000", "oi0001", "oi0004", "oi0005", "od0000", "od0002"},
                "Unexpected occurrence mutation target")
        expected = super().mutation(account, event, method, body, lost)
        if method == "DELETE":
            expected["after"] = cancelled_source(expected["before"])
            self.sources[account["id"]][event] = deepcopy(expected["after"])
        self.listed[account["id"]].add(event)
        return expected


def editor_event(source):
    require(source["status"] != "cancelled", "A cancelled instance has no editable projection")
    all_day = "date" in source["start"]
    def displayed(field):
        if all_day:
            return source[field]["date"]
        return datetime.fromisoformat(source[field]["dateTime"].replace("Z", "+00:00")).isoformat(timespec="seconds")
    return {"event_id": source["id"], "etag": source["etag"], "title": source["summary"],
            "description": source["description"], "location": source["location"],
            "start": displayed("start"), "end": displayed("end"), "all_day": all_day, "time_zone": "UTC",
            "conference_url": source["conferenceData"]["entryPoints"][0]["uri"],
            "guest_addresses": ["alpha@example.test", "beta@example.test"],
            "protected_attendees": [deepcopy(person) for person in source["attendees"]
                                    if person.get("self") or person.get("organizer")]}


def native_expectation(accounts):
    value = {"version": 1, "scenario": "occurrence", "limits": deepcopy(LIMITS),
             "calendar_date": BASE.date().isoformat(), "accounts": []}
    for account in accounts:
        actions = []
        for master, index, kind in ((0, 0, "edit"), (1, 0, "edit"), (0, 5, "delete")):
            source = expected_instance(account, master, index)
            action = {"kind": kind, "selector": selector(account, master, index),
                      "event_id": source["id"], "date": source["start"].get("date", source["start"].get("dateTime", "")[:10]),
                      "event": editor_event(source)}
            if kind == "edit":
                action["fields"] = {"title": f"Native occurrence {account['key']} {'all-day' if master == 1 else 'timed'}"}
            actions.append(action)
        value["accounts"].append({"account": account["id"], "calendar_id": calendar(account),
                                  "actions": actions, "absent_selectors": [selector(account, 1, 1)]})
    return value


def normalized_original(value):
    require(isinstance(value, dict), "Missing typed original identity")
    if value.get("kind") == "date":
        require(set(value) == {"kind", "date"}, "Unexpected all-day original fields")
        return ("date", value["date"])
    require(value.get("kind") == "zoned" and set(value) == {"kind", "instant", "time_zone"},
            "Unexpected timed original fields")
    return ("zoned", datetime.fromisoformat(value["instant"].replace("Z", "+00:00")))


def check_occurrence(actual, expected):
    require(set(actual) == {"series_id", "original_start", "expected_master_etag"},
            "Occurrence proof leaked or lost public fields")
    require(actual["series_id"] == expected["series_id"]
            and actual["expected_master_etag"] == expected["expected_master_etag"]
            and normalized_original(actual["original_start"]) == normalized_original(expected["original_start"]),
            "Occurrence parent, original identity or master version differs")


def check_recurring_occurrence(actual, expected):
    require(set(actual) == {"scope", "series_id", "original_start", "expected_master_etag"}
            and actual["scope"] == "occurrence", "Recurring action escaped the single-occurrence scope")
    check_occurrence({key: value for key, value in actual.items() if key != "scope"}, expected)
    if actual["original_start"]["kind"] == "zoned":
        require(actual["original_start"]["time_zone"] == expected["original_start"]["time_zone"],
                "Recurring occurrence changed the qualified original time zone")


def validate_resolution(response, account, source, *, version=3, action=None):
    require(version in (3, 4), "Unknown occurrence resolution version")
    context_key = "recurring" if version == 4 else "occurrence"
    require(set(response) == {"account", "calendar_id", "event_id", "etag", context_key, "event"},
            "Occurrence response leaked raw authority or omitted editor data")
    require(response["account"] == account["id"] and response["calendar_id"] == calendar(account)
            and response["event_id"] == source["id"] and response["etag"] == source["etag"],
            "Resolved provider instance crossed an identity or version")
    expected = {"series_id": source["recurringEventId"], "original_start": original_time(source),
                "expected_master_etag": f'"{source["recurringEventId"]}-v1"'}
    if version == 4:
        require(action in ("edit", "delete"), "Native occurrence resolution needs its exact authored action")
        check_recurring_occurrence(response["recurring"], expected)
        master = expected_master(account, int(source["recurringEventId"][2:]))
        require(response["event"]["recurring"] is True and response["event"]["recurrence"] == master["recurrence"],
                "Resolved version-four editor lost its retained master rule")
    else:
        check_occurrence(response["occurrence"], expected)
    for key, value in editor_event(source).items():
        same(response["event"].get(key), value, "Resolved editor field " + key)
    if version == 4:
        require(response["event"]["can_edit"] is (action == "edit")
                and response["event"]["can_delete"] is (action == "delete")
                and response["event"]["can_edit_attendees"] is (action == "edit")
                and response["event"]["can_rsvp"] is False, "Version-four resolver changed its selected action authority")
    else:
        require(response["event"]["can_edit"] is True and response["event"]["can_delete"] is True
                and response["event"]["can_rsvp"] is False, "Resolved editor grants wrong occurrence actions")


def verify_retained_occurrences(profile, ledger):
    expected = {(account, event): ledger.sources[account][event]
                for account, events in ledger.listed.items() for event in events}
    with closing(read_database(profile)) as db:
        rows = db.execute("SELECT account,calendar_id,event_id,json FROM calendar_published_sources").fetchall()
        actual = {(account, event): {"calendar": cal, "source": json.loads(raw)}
                  for account, cal, event, raw in rows}
        require(len(actual) == len(rows), "Published occurrence source identities crossed calendars")
        require(set(actual) == set(expected), "Retained occurrence inventory differs from independent list/publication ledger")
        for key, source in expected.items():
            require(actual[key]["calendar"] == ledger.calendars[key[0]],
                    "Retained occurrence crossed a calendar")
            same(actual[key]["source"], source, "Exact retained occurrence " + str(key))
        visible = {(account, event) for account, event in db.execute("SELECT account,event_id FROM calendar_visible_sources")}
        # This view means current inventory membership, not rendered events.
        # Acquisition retains supplied cancellations in it; direct mutation
        # publication marks its cancelled target hidden. Expansion suppresses
        # both representations and is independently checked by verify_window.
        hidden = {(row["account"], row["event"]) for row in ledger.publications if row["method"] == "DELETE"}
        require(visible == set(expected) - hidden, "Current source membership differs from acquisition/publication evidence")
    active = sum(source["status"] != "cancelled" for source in expected.values())
    return {"retained_sources": len(expected), "active_sources": active, "query_sources": len(visible),
            "cancelled_sources": len(expected) - active}


def validate_status(status, account, request, state, ledger):
    event = request["event_id"]
    version = 4 if "recurring" in request else 3
    require(status["request_id"] == request["request_id"] and status["kind"] == request["kind"]
            and status["target"] == {"account": account["id"], "calendar": calendar(account), "event": event}
            and status["intent_version"] == version and status["local_only"] is False and status["state"] == state,
            "Wrong scoped Calendar status version or identity")
    if version == 4:
        require("occurrence" not in request, "Version-four request mixed two scope grammars")
        occurrence = {key: value for key, value in request["recurring"].items() if key != "scope"}
        check_recurring_occurrence(status["recurring"], occurrence)
        require(status["recurring"] == request["recurring"] and status["dispatch_kind"] == request["kind"]
                and status["dispatch_event_id"] == event and status["step_index"] is None
                and status["can_resume"] is False and "steps" not in status and "following_complete" not in status,
                "Single-occurrence receipt invented following steps, resume permission or a different dispatch")
        check_occurrence(status["occurrence"], occurrence)
    else:
        require("recurring" not in status, "Version-three receipt acquired a version-four scope")
        check_occurrence(status["occurrence"], request["occurrence"])
    outcome = status["outcome"]
    require(not any(key in outcome for key in ("source", "master_read_proof", "publication_fence")),
            "Public activity leaked retained provider source or internal proof")
    if state == "acknowledged":
        require(outcome["kind"] == ("deleted" if request["kind"] == "delete" else "accepted")
                and outcome["evidence_origin"] == "provider_response"
                and outcome["notification_delivery"] == "unverified", "Provider ACK invented notification delivery")
        if request["kind"] == "delete" and version == 3:
            require(outcome["source_origin"] == "dispatched_occurrence_identity", "DELETE ACK claimed a fetched event")
        if version == 4:
            require("source_origin" not in outcome, "Version-four public ACK invented an unprojected source-origin field")
    elif state == "reconciled":
        source = ledger.sources[account["id"]][event]
        require(outcome["kind"] == "observed_state" and outcome["matches_intent"] is True
                and outcome["event_cancelled"] is (request["kind"] == "delete")
                and outcome["observed_etag"] == source.get("etag")
                and ((version == 3 and outcome.get("source_origin") == "fetched")
                     or (version == 4 and "source_origin" not in outcome))
                and outcome["notification_delivery"] == "unverified" and "evidence_origin" not in outcome,
                "Read reconciliation invented acknowledgment, ETag or notification delivery")


def verify_window(rows, ledger, accounts, date_from, date_to):
    window_from = datetime.fromisoformat(date_from).replace(tzinfo=timezone.utc)
    window_to = datetime.fromisoformat(date_to).replace(tzinfo=timezone.utc)
    require(window_from < window_to, "Occurrence inspection needs a positive date window")
    expected = {}
    for account in accounts:
        for master in (0, 1, 3):
            for index in range(MASTER_COUNTS[master]):
                source = ledger.sources[account["id"]][f"{INSTANCE_PREFIXES[master]}{index:04}"]
                if source["status"] == "cancelled":
                    continue
                projected = editor_event(source)
                start, end = [datetime.fromisoformat(projected[field].replace("Z", "+00:00")) for field in ("start", "end")]
                if projected["all_day"]:
                    start, end = start.replace(tzinfo=timezone.utc), end.replace(tzinfo=timezone.utc)
                if not (end > window_from and start < window_to):
                    continue
                original = original_time(source)
                identity = original.get("date") if original["kind"] == "date" else datetime.fromisoformat(
                    original["instant"].replace("Z", "+00:00")).isoformat(timespec="seconds")
                expected[(account["id"], source["recurringEventId"], identity)] = projected
    actual = {}
    for row in rows:
        key = row["occurrence_key"]
        identity = (key["account"], key["series"], key["original_start"])
        require(identity not in actual, "Duplicate rendered occurrence identity")
        require(key["calendar"] == row["calendar_id"] == ledger.calendars.get(key["account"]),
                "Rendered occurrence crossed a calendar")
        actual[identity] = row
    require(set(actual) == set(expected), "Rendered window resurrected a cancellation, omitted a move or invented an occurrence")
    for identity, expected_row in expected.items():
        row = actual[identity]
        for key in ("title", "description", "location", "conference_url", "start", "end", "all_day", "time_zone"):
            same(row.get(key), expected_row[key], "Rendered occurrence field " + key)
    return len(expected)


def verify_journals(rows, requests):
    require(len(rows) == len(requests), "Missing or invented occurrence intent")
    for row in rows:
        key = row[0] + "/" + row[1]
        require(key in requests, "Unexpected occurrence journal identity")
        expected = requests[key]
        request, publication = expected["request"], expected["publication"]
        intent, dispatch = json.loads(row[3]), json.loads(row[11])
        target = {"account": row[0], "calendar": publication["calendar"], "event": publication["event"]}
        version = 4 if "recurring" in request else 3
        if version == 4:
            require("occurrence" not in request, "Version-four journal mixed scope grammars")
            occurrence = {key: value for key, value in request["recurring"].items() if key != "scope"}
            check_recurring_occurrence(dispatch["recurring"], occurrence)
            require(dispatch["recurring"] == request["recurring"] and set(dispatch) == {
                "version", "kind", "target", "body", "if_match", "send_updates", "baseline", "base_revision",
                "event_label_version", "recurring", "occurrence", "original_start_provider", "master_proof", "lifecycle_revision"},
                "Single-occurrence dispatch lost its exact version-four shape or acquired a second mutation step")
        else:
            occurrence = request["occurrence"]
        require(intent == {"version": version, "request": request, "target": target}, "Original versioned occurrence intent bytes changed")
        require(dispatch["version"] == version and dispatch["kind"] == request["kind"] and dispatch["target"] == target
                and dispatch["if_match"] == publication["if_match"] and dispatch["send_updates"] == "all",
                "Frozen occurrence dispatch lost version, target or conditional/notification semantics")
        same(dispatch["baseline"], publication["before"], "Full frozen provider instance")
        same(dispatch["body"], publication["body"], "Exact frozen occurrence patch")
        require(dispatch["occurrence"] == occurrence
                and dispatch["original_start_provider"] == publication["before"]["originalStartTime"],
                "Frozen provider original identity changed")
        stamp = dispatch["master_proof"]
        require(set(stamp) == {"account", "calendar", "series", "etag", "source_sha256", "recurrence_sha256",
                               "source_generation", "calendar_revision", "lifecycle_revision", "connection_generation",
                               "permission_epoch", "zone", "access"}, "Master proof lost bounded metadata shape")
        master = expected_master(expected["account"], int(occurrence["series_id"][2:]))
        canonical_hash = lambda value: hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
        require(stamp["account"] == row[0] and stamp["calendar"] == target["calendar"]
                and stamp["series"] == master["id"] and stamp["etag"] == master["etag"]
                and stamp["source_sha256"] == canonical_hash(master) and stamp["recurrence_sha256"] == canonical_hash(master["recurrence"])
                and stamp["zone"] == "UTC" and stamp["access"] == "owner"
                and all(isinstance(stamp[field], int) and stamp[field] > 0 for field in
                        ("source_generation", "calendar_revision", "lifecycle_revision", "connection_generation", "permission_epoch")),
                "Frozen master proof differs from qualified generated source/authority")
        if version == 4:
            require(dispatch["lifecycle_revision"] == stamp["lifecycle_revision"]
                    and dispatch["base_revision"] == stamp["calendar_revision"]
                    and dispatch["event_label_version"] == (1 if isinstance(publication["body"], dict)
                                                            and "eventLabelId" in publication["body"] else 0),
                    "Version-four dispatch lost its lifecycle, source revision or label-version fence")
    return len(rows)
