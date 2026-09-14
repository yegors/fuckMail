"""Independent generated-source and archive preservation oracles for P2-01B.

Read-only SQL and retained-file checks; no provider or native credential access.
"""
from contextlib import closing
import hashlib
import json

from dev import ROOT
from verify_google_calendar import BASE_UNIX, calendar, expected_source, same
from verify_google_navigation import (expected_accounts, expected_message, read_database,
                                      require, source_manifest, verify_timestamp)
from verify_search_ingestion import sha256


def source_inputs(native=False):
    inputs = {name: digest for name, digest in source_manifest().items()
              if native or not name.startswith("gui/") and name != "CMakeLists.txt"}
    inputs.update({path.relative_to(ROOT).as_posix(): sha256(path)
                   for path in (ROOT / "fixtures").rglob("*") if path.is_file()})
    return inputs


def verify_initial_sources(profile):
    """Reconstruct all four MIME sources and two Calendar objects independently."""
    accounts = {account["id"]: account for account in expected_accounts()}
    expected = {(account, remote): expected_message(value, remote, BASE_UNIX)
                for account, value in accounts.items() for remote in ("vr0000", "vr0001")}
    checked = []
    with closing(read_database(profile)) as db:
        require(db.execute("PRAGMA user_version").fetchone()[0] == 18, "Wrong account lifecycle schema")
        meta = dict(db.execute("SELECT key,value FROM meta"))
        require(str(meta["fixture_revision"]) == "0" and str(meta["profile_kind"]) == "1",
                "Expected the new empty-live-derived isolated profile")
        rows = db.execute("""SELECT m.account,m.remote_id,m.id,m.conversation_id,m.body,m.subject,m.source_hash,
            m.sent_at,m.sender_name,m.sender_email,m.recipient,m.unread,m.starred,m.inbox,m.sent,m.trashed,
            g.remote_thread_id,g.provider_labels_json,g.deleted,c.source_state,c.index_state,
            c.attachment_state,c.issue,c.source_size,c.internal_date_ms,c.source_hash
            FROM messages m JOIN google_message_state g ON g.account=m.account AND g.message_id=m.id
            JOIN google_mail_coverage c ON c.account=m.account AND c.remote_id=m.remote_id
            ORDER BY m.account,m.remote_id""").fetchall()
        names = ("account", "remote_id", "id", "conversation_id", "body", "subject", "source_hash", "sent_at",
                 "sender_name", "sender_email", "recipient", "unread", "starred", "inbox", "sent", "trashed",
                 "remote_thread_id", "provider_labels_json", "deleted", "source_state", "index_state",
                 "attachment_state", "issue", "source_size", "internal_date_ms", "coverage_hash")
        seen = set()
        for values in rows:
            row = dict(zip(names, values, strict=True))
            key = (row["account"], row["remote_id"])
            require(key in expected and key not in seen, "Unexpected or duplicate acquired message")
            seen.add(key)
            wanted = expected[key]
            for field in ("id", "conversation_id", "body", "subject", "source_hash", "remote_thread_id", "internal_date_ms"):
                require(row[field] == wanted[field], "Independent message mismatch: " + field)
            verify_timestamp(key, row["sent_at"], wanted["internal_date_ms"])
            for field, value in {"sender_name": "Verification Sender", "sender_email": "sender@example.test",
                                 "recipient": accounts[row["account"]]["email"], "starred": 0, "inbox": 1,
                                 "sent": 0, "trashed": 0, "deleted": 0, "source_state": "retained",
                                 "index_state": "indexed", "attachment_state": "complete", "issue": None}.items():
                require(row[field] == value, "Independent acquired message state mismatch: " + field)
            require(row["unread"] in (0, 1) and set(json.loads(row["provider_labels_json"]))
                    in ({"INBOX"}, {"INBOX", "UNREAD"}), "Unexpected acquired review/label state")
            raw = profile / "blobs" / wanted["source_hash"]
            require(row["coverage_hash"] == wanted["source_hash"] and row["source_size"] == len(wanted["source"])
                    and raw.is_file() and raw.read_bytes() == wanted["source"], "Exact acquired RAW bytes differ")
            checked.append({"account": row["account"], "remote_id": row["remote_id"],
                            "source_sha256": wanted["source_hash"], "source_size": len(wanted["source"]),
                            "conversation_id": wanted["conversation_id"]})
        require(seen == set(expected), "The account fixture did not acquire all four expected sources")
        require(dict(db.execute("SELECT account,count(*) FROM conversations GROUP BY account"))
                == {account: 1 for account in accounts}, "Expected exactly one acquired thread per account")
        calendar_rows = db.execute("SELECT account,calendar_id,event_id,json FROM calendar_visible_sources").fetchall()
        require(len(calendar_rows) == 2, "Expected one acquired Calendar event per account")
        calendar_seen = set()
        for account, calendar_id, event, raw in calendar_rows:
            require(account in accounts and account not in calendar_seen
                    and calendar_id == calendar(accounts[account]) and event == "ci0000",
                    "Wrong independently expected Calendar identity")
            calendar_seen.add(account)
            same(json.loads(raw), expected_source(accounts[account], 0), "Initial account Calendar source")
        require(calendar_seen == set(accounts), "A generated Calendar account was omitted")
    return {"messages": checked, "calendar_sources": len(calendar_seen), "threads": len(accounts)}


def retained_content(profile):
    """Hash exact source/authored data, excluding allowed lifecycle metadata changes."""
    tables = ("messages", "conversations", "attachments", "drafts", "capture_operations",
              "calendar_sources", "calendar_visible_sources", "google_draft_sources", "google_message_state",
              "google_mail_coverage", "calendar_write_operations")
    result = {"tables": {}, "blobs": {}}
    with closing(read_database(profile)) as db:
        existing = {row[0] for row in db.execute("SELECT name FROM sqlite_schema WHERE type IN ('table','view')")}
        for table in tables:
            require(table in existing, "Expected retained content table is missing: " + table)
            columns = [row[1] for row in db.execute(f'PRAGMA table_info("{table}")')]
            rows = db.execute(f'SELECT * FROM "{table}"').fetchall()
            encoded = sorted(json.dumps(row, separators=(",", ":"), ensure_ascii=False) for row in rows)
            value = json.dumps({"columns": columns, "rows": encoded}, separators=(",", ":"), ensure_ascii=False).encode()
            result["tables"][table] = {"rows": len(rows), "sha256": hashlib.sha256(value).hexdigest()}
        # Removal adds operation rows; every pre-existing non-removal receipt
        # remains part of the preservation comparison.
        rows = db.execute("SELECT account,request_id,fingerprint,result_json FROM operations "
                          "WHERE coalesce(json_extract(result_json,'$.kind'),'')<>'account_remove' "
                          "ORDER BY account,request_id").fetchall()
        result["tables"]["non_removal_operations"] = {"rows": len(rows), "sha256": hashlib.sha256(
            json.dumps(rows, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()}
    for path in sorted((profile / "blobs").rglob("*")):
        if path.is_file():
            result["blobs"][path.relative_to(profile / "blobs").as_posix()] = {
                "sha256": sha256(path), "size": path.stat().st_size}
    return result
