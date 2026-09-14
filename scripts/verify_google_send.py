"""Additional generated-provider Send checks for verify_google_drafts.py --send.

All authored content and recipients are fixed example.test fixture data. The
caller owns the disposable profile, process lifetimes and source/executable log.
"""
from contextlib import closing
from email import policy
from email.parser import BytesParser
from email.utils import parsedate_to_datetime
import json

from verify_google_navigation import read_database, require


def exercise_sends(cli, wait_for, synced, profile, scratch, expected, result, verify_mime, attachment):
    result["submissions"] = []
    require(cli(["status"])["capabilities"]["send"] is True, "Live Send interface is unavailable")
    accounts = cli(["accounts"])["accounts"]
    for item in expected:
        account = item["account"]
        row = next(row for row in accounts if row["id"] == account["id"])
        require(row["google"]["capabilities"]["send"] is True, "Connected fixture account lacks Send capability")
        if not item["recipients"]["To"]:
            item["recipients"] = {"To": ["recipient@example.test"], "Cc": ["carbon@example.test"],
                                  "Bcc": ["hidden@example.test"]}
            recipients = scratch / (account["key"] + "-send-recipients.json")
            recipients.write_text(json.dumps({name.lower() + "_recipients": [{"email": value} for value in values]
                                              for name, values in item["recipients"].items()}), encoding="utf-8")
            item["draft"] = cli(["draft", "save", "--draft-id", item["draft"]["draft_id"],
                                 "--body-file", str(item["body_file"]), "--recipients-file", str(recipients),
                                 "--expected-revision", str(item["draft"]["revision"]),
                                 "--request-id", account["key"] + "-send-recipients"], account)
        twin = wait_for(lambda: synced(item), "send baseline " + account["key"])
        first_request = None
        first_message_id = None
        first_operation = None
        for number in (1, 2):
            if number == 2:
                item["body"] += "\nA separate human message after definitive acceptance."
                item["body_file"].write_text(item["body"], encoding="utf-8", newline="\n")
                item["draft"] = cli(["draft", "save", "--draft-id", item["draft"]["draft_id"],
                                     "--body-file", str(item["body_file"]), "--expected-revision", str(item["draft"]["revision"]),
                                     "--request-id", account["key"] + "-after-send-edit"], account)
                twin = cli(["google", "draft-status", "--draft-id", item["draft"]["draft_id"]], account)
            request_id = account["key"] + "-explicit-send-" + str(number)
            original = {"draft_id": item["draft"]["draft_id"], "expected_revision": item["draft"]["revision"],
                        "expected_google_revision": twin["revision"], "request_id": request_id}
            request = ["google", "send", "--draft-id", item["draft"]["draft_id"],
                       "--expected-revision", str(item["draft"]["revision"]),
                       "--expected-google-revision", str(twin["revision"]), "--request-id", request_id]
            operation = cli(request, account)
            require(operation["account"] == account["id"] and operation["draft_id"] == original["draft_id"]
                    and operation["request_id"] == request_id and operation["request"] == original
                    and operation["google_revision"] == original["expected_google_revision"],
                    "Returned Send does not match independently constructed request/account/draft")
            require(operation["from"] == account["email"] and operation["subject"] == "Human draft " + account["key"],
                    "Wrong frozen header metadata")
            for name, addresses in item["recipients"].items():
                require([row["email"] for row in operation[name.lower() + "_recipients"]] == addresses,
                        "Wrong frozen recipient metadata: " + name)
            require(operation["state"] == "accepted" and operation["operation_is_current"] is True,
                    "Generated Send was not definitively accepted")
            require(operation["draft_revision"] == item["draft"]["revision"]
                    and operation["last_accepted_draft_revision"] == item["draft"]["revision"],
                    "Accepted authored revision differs")
            require(operation["ack"]["message_id"] and operation["ack"]["thread_id"], "Incomplete provider acknowledgment")
            require(operation["provider_draft_id"] == (twin["baseline"]["draft_id"] if number == 1 else None),
                    "Submission reused a consumed or different Google draft")
            verified = verify_mime(profile, operation["upload_sha256"], operation["size"], account,
                                   item["body"], item["signature"], item["recipients"], attachment)
            message = BytesParser(policy=policy.default).parsebytes((profile / "blobs" / operation["upload_sha256"]).read_bytes())
            rfc_message_id = str(message["Message-ID"])
            require(rfc_message_id == operation["message_id"] and rfc_message_id != first_message_id,
                    "Submission RFC Message-ID was not frozen uniquely")
            require(int(parsedate_to_datetime(str(message["Date"])).timestamp()) == operation["created_at"],
                    "MIME Date differs from the frozen timestamp")
            frozen_fields = ("request_id", "draft_revision", "google_revision", "provider_draft_id", "thread_id",
                             "message_id", "upload_sha256", "size", "from", "to_recipients", "cc_recipients",
                             "bcc_recipients", "subject", "created_at", "ack", "state")
            replay = cli(request, account)
            require(all(replay[key] == operation[key] for key in frozen_fields), "Request replay changed frozen Send")
            cached = cli(["google", "send-status", "--request-id", request_id], account)
            require(cached["account"] == account["id"] and cached["draft_id"] == original["draft_id"]
                    and cached["request"] == original and all(cached[key] == operation[key] for key in frozen_fields),
                    "Cached status differs from the exact Send request")
            checked = cli(["google", "send-reconcile", "--request-id", request_id], account)
            require(all(checked[key] == operation[key] for key in frozen_fields), "Read-only inspection changed submission certainty/content")
            evidence = checked["read_evidence"]
            require(evidence["draft_observation"] == "not_checked" and evidence["truncated"] is False,
                    "Wrong bounded Send evidence scope")
            require(any(row["message_id"] == operation["ack"]["message_id"]
                        and row["thread_id"] == operation["ack"]["thread_id"] and "SENT" in row["labels"]
                        for row in evidence["matches"]), "Sent metadata does not match exact acknowledged message")
            current = cli(["draft", "show", "--draft-id", item["draft"]["draft_id"]], account)
            require(current["revision"] == item["draft"]["revision"] and current["body"] == item["body"],
                    "Send modified authored text or revision")
            require(current["google_send"]["request_id"] == request_id, "Draft is missing current Send metadata")
            duplicate = request[:-1] + [request_id + "-duplicate"]
            duplicate[duplicate.index("--expected-google-revision") + 1] = str(checked["current_google_revision"])
            cli(duplicate, account, expected_error="google_send_consumed")
            cli(["google", "send-abandon", "--request-id", request_id, "--expected-revision", str(checked["revision"])],
                account, expected_error="stale_google_work")
            after_refusal = cli(["google", "send-status", "--request-id", request_id], account)
            require(after_refusal == checked, "A refused abandon changed the accepted receipt")
            result["submissions"].append({"account": account["key"], "operation": checked, "mime": verified})
            if number == 1:
                first_request, first_message_id, first_operation = request, rfc_message_id, checked
            else:
                historical = cli(first_request, account)
                require(historical["operation_is_current"] is False
                        and all(historical[key] == first_operation[key] for key in frozen_fields),
                        "Historical replay changed original submission or current identity")
                latest = cli(["draft", "show", "--draft-id", item["draft"]["draft_id"]], account)
                require(latest["google_send"]["request_id"] == request_id and latest["body"] == item["body"],
                        "Historical inspection replaced current metadata/text")
    with closing(read_database(profile)) as db:
        rows = db.execute("SELECT account,id,json,blob_hash FROM capture_operations WHERE execution_kind='google'").fetchall()
        send_rows = [row for row in rows if json.loads(row[2]).get("kind") == "send"]
        require(len(send_rows) == 4, "Expected exactly four durable submission intents")
        expected_rows = {(item["operation"]["account"], item["operation"]["request_id"]): item["operation"]
                         for item in result["submissions"]}
        require(len(expected_rows) == 4 and {(row[0], row[1]) for row in send_rows} == set(expected_rows),
                "Durable submission identities differ from actual CLI receipts")
        for account, request_id, serialized, digest in send_rows:
            row = json.loads(serialized)
            expected_row = expected_rows[(account, request_id)]
            require(row["account"] == account and row["request_id"] == request_id
                    and row["upload_sha256"] == digest == expected_row["upload_sha256"]
                    and all(row[key] == expected_row[key] for key in ("draft_id", "request", "draft_revision", "google_revision",
                            "provider_draft_id", "thread_id", "message_id", "size", "created_at", "ack", "state"))
                    and all(row["headers"][key] == expected_row[key] for key in ("from", "to_recipients", "cc_recipients",
                            "bcc_recipients", "subject")),
                    "Independent Send journal disagrees with CLI")
        require(len(rows) == 9 and all(json.loads(row[2])["state"] == "synced" for row in rows
                                      if json.loads(row[2]).get("kind", "draft") == "draft"),
                "Mixed draft/send journal has extra or unfinished draft work")
        result["independent_send_operations"] = len(send_rows)
        result["independent_all_google_operations_after_send"] = len(rows)


def verify_send_publications(fixture, result):
    require(fixture["counters"]["send_completed"] == 4, "Provider saw a duplicate or missing submission")
    publications = fixture["send_fixture"]["publications"]
    require(len(publications) == 4, "Provider submission trace has wrong length")
    for item in result["submissions"]:
        operation = item["operation"]
        matches = [row for row in publications if row["account"] == operation["account"]
                   and row["message_id"] == operation["ack"]["message_id"]]
        require(len(matches) == 1, "Provider publication must match one account-specific acknowledgment")
        row = matches[0]
        require(row["thread_id"] == operation["ack"]["thread_id"] and row["sha256"] == operation["upload_sha256"]
                and row["size"] == operation["size"] and row["provider_draft_id"] == operation["provider_draft_id"]
                and row["rfc_message_id"] == operation["message_id"].strip("<>"),
                "Provider accepted different content, target or identity")
