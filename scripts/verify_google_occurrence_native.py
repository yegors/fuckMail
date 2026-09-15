"""Validate the native occurrence report against independent fixture sources.

This module performs no process, provider or database operations. The shared
runner owns those lifetimes and the final source/journal/publication readback.
"""
from copy import deepcopy
from datetime import date, timedelta
import json
import re

from verify_google_calendar import calendar, same
from verify_google_limits import LIMITS
from verify_google_navigation import require
from verify_google_occurrence_sources import (
    check_recurring_occurrence, editor_event, native_expectation, normalized_original,
    original_time, validate_resolution, validate_status, verify_window,
)


def _original_from_key(row):
    key = row["occurrence_key"]
    if row["all_day"] is True:
        return {"kind": "date", "date": key["original_start"]}
    require(row["all_day"] is False, "Native occurrence lost its time type")
    return {"kind": "zoned", "instant": key["original_start"], "time_zone": "UTC"}


def _selector(actual, account, source, *, resolved=False, action=None):
    required = {"calendar_id", "series_id", "original_start"}
    if resolved:
        required |= {"account", "event_id"}
        require(actual.get("account") == account["id"], "Editor selector crossed an account")
    if action is not None:
        required |= {"scope", "action"}
        require(resolved and actual.get("scope") == "occurrence" and actual.get("action") == action,
                "Native editor selector changed its explicit single-occurrence action")
    require(required <= set(actual) <= required | {"event_id"}, "Native selector lost or invented fields")
    require(actual["calendar_id"] == calendar(account)
            and actual["series_id"] == source["recurringEventId"]
            and normalized_original(actual["original_start"]) == normalized_original(original_time(source)),
            "Native selector differs from the original provider occurrence")
    if actual["original_start"]["kind"] == "zoned":
        require(actual["original_start"]["time_zone"] == source["originalStartTime"]["timeZone"],
                "Native selector changed the provider original time zone")
    if "event_id" in actual:
        require(actual["event_id"] == source["id"], "Native selector invented an instance hint")


def _identity(row, account, source):
    key = row["occurrence_key"]
    require(set(key) == {"account", "calendar", "series", "original_start"}
            and row["account"] == key["account"] == account["id"]
            and row["calendar_id"] == key["calendar"] == calendar(account)
            and key["series"] == source["recurringEventId"], "Native row crossed a qualified occurrence identity")
    require(json.loads(row["id"]) == key, "Native row ID differs from its full occurrence key")
    require(normalized_original(_original_from_key(row)) == normalized_original(original_time(source)),
            "Native row substituted its current date for its original occurrence")
    require(row["all_day"] is ("date" in source["start"]), "Native row changed date versus instant semantics")


def _rendered(row, account, source):
    _identity(row, account, source)
    expected = editor_event(source)
    for field in ("title", "description", "location", "conference_url", "start", "end", "all_day", "time_zone"):
        same(row.get(field), expected[field], "Native rendered occurrence " + field)
    require(row["date"] == expected["start"][:10] and row.get("busy_only") is False,
            "Native row has the wrong displayed UTC date or visibility")
    eligible = source.get("locked") is not True
    require(row.get("can_resolve_occurrence") is eligible
            and row.get("can_edit") is False and row.get("can_delete") is False,
            "Expanded native row grants editing before provider resolution")
    if eligible:
        _selector(row["occurrence_selector"], account, source)
        _selector(row["recurring_selector"], account, source)
        require(row["recurring_scopes"] == {"edit": ["occurrence", "series", "following"],
                "delete": ["occurrence", "series", "following"], "rsvp": ["occurrence", "series"]},
                "Native scope choices differ from the qualified fixed organizer/self-attendee source")
    else:
        require("occurrence_selector" not in row, "Locked recurring source exposed a resolver selector")
        require("recurring_selector" not in row and not row.get("recurring_scopes"),
                "Locked source acquired a version-four action selector")


def _source_for_row(row, account, ledger):
    key = row["occurrence_key"]
    original = normalized_original(_original_from_key(row))
    matches = [source for source in ledger.sources[account["id"]].values()
               if source.get("recurringEventId") == key["series"]
               and normalized_original(original_time(source)) == original]
    require(len(matches) == 1 and matches[0]["status"] != "cancelled",
            "Native row does not name one independently visible fixture occurrence")
    return matches[0]


def validate_native(observed, expectation, accounts, ledger):
    """Check six explicit ACK actions and return the shared journal lookup map."""
    require(len(accounts) == 2 and len({account["id"] for account in accounts}) == 2,
            "Native occurrence scenario requires both distinct fixture accounts")
    require(expectation == native_expectation(accounts), "Native input differs from independent occurrence constants")
    require(observed.get("version") == 1 and observed.get("scenario") == "occurrence"
            and observed.get("result") == "passed" and not observed.get("error"),
            "Native occurrence walkthrough did not pass")
    rows = observed.get("accounts", [])
    require(len(rows) == len(accounts) and [row.get("account") for row in rows] == [a["id"] for a in accounts],
            "Native occurrence walkthrough omitted, duplicated or crossed an account")
    require(not observed.get("in_progress"), "Native success left an unfinished account")
    require(not ledger.publications, "Native validation must start with the unmodified acquired fixture")
    requests = {}
    for account, wanted_account, native in zip(accounts, expectation["accounts"], rows):
        require(native["limits"] == LIMITS, "Native Settings ownership used different quota limits")
        require(native["calendar_from"] == expectation["calendar_date"] and native["calendar_days"] == 42,
                "Native initial inspection did not cover its actual 42-day Agenda window")
        date_from = native["calendar_from"]
        date_to = (date.fromisoformat(date_from) + timedelta(days=42)).isoformat()
        initial = native["initial_events"]
        verify_window(initial, ledger, [account], date_from, date_to)
        for row in initial:
            _rendered(row, account, _source_for_row(row, account, ledger))
        require(native["observed_absent_selectors"] == wanted_account["absent_selectors"],
                "Native cancellation inspection omitted or changed an expected selector")
        for absent in wanted_account["absent_selectors"]:
            original = normalized_original(absent["original_start"])
            require(absent["calendar_id"] == calendar(account)
                    and date_from <= absent["original_start"]["date"] < date_to
                    and not any(row["occurrence_key"]["series"] == absent["series_id"]
                                and normalized_original(_original_from_key(row)) == original for row in initial),
                    "Sparse cancelled occurrence was visible or outside the inspected window")
        actions = native["actions"]
        require(len(actions) == 3, "Native account did not complete timed edit, all-day edit and cancellation")
        for record, action in zip(actions, wanted_account["actions"]):
            require(record["account"] == account["id"] and record["kind"] == action["kind"]
                    and record["selector"] == action["selector"], "Native action changed its captured account or selector")
            event_id = action["event_id"]
            before = deepcopy(ledger.sources[account["id"]][event_id])
            selected = record["selected_occurrence"]
            _rendered(selected, account, before)
            require(selected["date"] == action["date"], "Native action selected the wrong displayed day")
            response = record["resolved_response"]
            validate_resolution(response, account, before, version=4, action=action["kind"])
            _identity(response["event"], account, before)
            resolved = record["resolved_event"]
            _identity(resolved, account, before)
            require(resolved["id"] == selected["id"], "Resolved editor replaced its selected occurrence key")
            for field, value in response["event"].items():
                same(resolved.get(field), value, "Native resolved editor retained field " + field)
            expected_occurrence = {"series_id": before["recurringEventId"], "original_start": original_time(before),
                                   "expected_master_etag": ledger.sources[account["id"]][before["recurringEventId"]]["etag"]}
            require(record["scope"] == "occurrence" and resolved.get("occurrence") is None
                    and resolved["recurring_action"] == action["kind"] and resolved["following_notice"] == "",
                    "Native resolution mixed legacy or following context into the selected action")
            check_recurring_occurrence(resolved["recurring_context"], expected_occurrence)
            require(resolved["recurring_context"] == response["recurring"], "Editor changed its qualified recurring context")
            _selector(resolved["occurrence_selector"], account, before, resolved=True, action=action["kind"])
            _selector(resolved["recurring_selector"], account, before, resolved=True, action=action["kind"])
            require(resolved["event_id"] == before["id"] and resolved["etag"] == before["etag"],
                    "Native editor used master enrichment as the provider instance")

            request = deepcopy(record["request"])
            request_id = request.get("request_id")
            prefix = "event" if action["kind"] == "edit" else "delete"
            require(isinstance(request_id, str) and len(request_id) <= 128
                    and re.fullmatch(r"gui-" + prefix + r"-[0-9]+-[0-9]+", request_id),
                    "Native action has an unexpected request identity")
            identity = account["id"] + "/" + request_id
            require(identity not in requests, "Native occurrence action reused a request identity")
            check_recurring_occurrence(request["recurring"], expected_occurrence)
            require(request["recurring"] == response["recurring"], "Submitted occurrence differs from the resolved version")
            wanted = {"account": account["id"], "calendar_id": calendar(account), "kind": action["kind"],
                      "event_id": event_id, "expected_etag": before["etag"], "request_id": request_id,
                      "recurring": deepcopy(response["recurring"])}
            if action["kind"] == "edit":
                wanted["fields"] = deepcopy(action["fields"])
                body = ledger.expected_patch(account, event_id, title=action["fields"]["title"])
                method = "PATCH"
            else:
                body, method = None, "DELETE"
            require(request == wanted, "Native request changed fields outside its explicit occurrence action")
            publication = ledger.mutation(account, event_id, method, body)
            require(record["outcome"].get("account") == account["id"], "Native outcome crossed an account")
            validate_status(record["outcome"], account, request, "acknowledged", ledger)
            if action["kind"] == "edit":
                _rendered(record["event"], account, ledger.sources[account["id"]][event_id])
                require(record["event"]["id"] == selected["id"], "Post-edit projection lost the original occurrence identity")
            else:
                require("event" not in record, "Native cancellation retained a visible post-action occurrence")
            require(record["calendar_days"] == 42 and record["calendar_from"] == action["date"],
                    "Native post-action inspection changed its Agenda window")
            after_to = (date.fromisoformat(record["calendar_from"]) + timedelta(days=42)).isoformat()
            verify_window(record["post_events"], ledger, [account], record["calendar_from"], after_to)
            request.pop("account")
            requests[identity] = {"account": account, "request": request, "publication": publication}
    require(len(requests) == len(ledger.publications) == 6
            and all(item["response"] == "acknowledged" for item in ledger.publications),
            "Native scenario did not produce exactly six provider-acknowledged mutations")
    return requests
