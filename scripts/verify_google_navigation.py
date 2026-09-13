#!/usr/bin/env python3
"""Measure unchanged native navigation during real workers using in-memory Google fixtures.

Requires the default-off verification-google engine, ordinary CLI and unchanged GUI.
This is development-runtime evidence, never package or real-provider acceptance.
"""
import argparse
from contextlib import ExitStack, closing
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import uuid

from dev import ROOT, package_environment
from verify_interaction import verify_navigation
from verify_search_ingestion import sha256, stop_children


CONFIG = {"recent_messages_per_account": 60, "recent_threads_per_account": 30,
          "backfill_messages_per_account": 4096, "raw_delay_ms": 16, "max_publications": 4096}
LIMITS = {"gmail_project_units_per_minute": 1000000, "gmail_account_units_per_minute": 500000,
          "calendar_project_units_per_minute": 10000, "calendar_account_units_per_minute": 5000}
ROUTES = {"mailbox_click", "mailbox_tab", "mailbox_shift_tab", "conversation_click"}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def wall_ms():
    return time.time_ns() / 1000000


def read_json(path):
    require(path.stat().st_size <= 16 * 1024 * 1024, "Evidence JSON exceeds its 16 MiB bound")
    return json.loads(path.read_text(encoding="utf-8"))


def read_database(profile):
    db = sqlite3.connect((profile / "store.sqlite3").as_uri() + "?mode=ro", uri=True, timeout=2)
    db.execute("PRAGMA query_only=ON")
    return db


def source_manifest():
    files = set()
    for directory in ("crates", "gui", "scripts"):
        for path in (ROOT / directory).rglob("*"):
            if path.is_file() and (path.suffix in (".rs", ".toml", ".sql", ".cpp", ".h", ".qml", ".py", ".json")
                                   or path.name == "CMakeLists.txt"):
                files.add(path)
    files.update(ROOT / name for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "CMakeLists.txt")
                 if (ROOT / name).is_file())
    return {path.relative_to(ROOT).as_posix(): sha256(path) for path in sorted(files)}


def expected_accounts():
    result = []
    for key in ("personal", "workspace"):
        subject = "verification-google-" + key + "-v1"
        result.append({"id": "google-" + hashlib.sha256(subject.encode()).hexdigest(), "key": key,
                       "subject": subject, "email": key + "@example.test"})
    return result


def expected_message(account, remote_id, base):
    match = re.fullmatch(r"v([rb])(\d{4})", remote_id)
    require(match is not None, "Unexpected fixture source identity: " + remote_id)
    index = int(match[2])
    recent = match[1] == "r"
    require(index < (60 if recent else 4096), "Fixture source index exceeds the fixed workload")
    kind, thread = ("recent", index // 2) if recent else ("backfill", index)
    seconds = base - 86400 + index * 60 if recent else base - 90 * 86400 - index * 60
    date = datetime.fromtimestamp(seconds, timezone.utc)
    # English names are fixed explicitly: host locale must not change MIME bytes.
    weekdays, months = ("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"), (
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")
    rfc_date = f"{weekdays[date.weekday()]}, {date.day:02} {months[date.month - 1]} {date.year:04} {date:%H:%M:%S} +0000"
    key = account["key"]
    subject = f"Verification {key} {kind} {thread:04}"
    body = (f"Generated verification mail for {key}/{kind}/{index:04}. This fixed source exists only in the disposable acquisition fixture."
            + " Cached mail must stay responsive." * 32)
    source = ("\r\n".join(("From: Verification Sender <sender@example.test>", "To: " + account["email"],
                            "Subject: " + subject, "Date: " + rfc_date,
                            f"Message-ID: <v1-{key}-{kind}-{index:04}@example.test>", "MIME-Version: 1.0",
                            "Content-Type: text/plain; charset=utf-8", "Content-Transfer-Encoding: 8bit"))
              + "\r\n\r\n" + body).encode("utf-8")
    remote_thread = ("tr" if recent else "tb") + f"{thread:04}"
    identity = lambda kind, remote: "gmail-" + kind + "-" + hashlib.sha256(
        (account["id"] + "\0" + remote).encode()).hexdigest()
    return {"source": source, "source_hash": hashlib.sha256(source).hexdigest(), "body": body,
            "subject": subject, "remote_thread_id": remote_thread, "recent": recent,
            "id": identity("message", remote_id), "conversation_id": identity("thread", remote_thread),
            "sent_at": date.strftime("%Y-%m-%dT%H:%M:%SZ"), "internal_date_ms": seconds * 1000}


def mismatch(key, field, actual, expected):
    # Retain useful diagnostics even for bounded body fields without flooding
    # the report; this fixture contains generated data only.
    def display(value):
        text = repr(value)
        return text if len(text) <= 320 else text[:320] + "..."
    return f"Independent fixture mismatch: {key} {field}: actual={display(actual)}, expected={display(expected)}"


def verify_timestamp(key, actual, expected_ms):
    expected = datetime(1970, 1, 1, tzinfo=timezone.utc) + timedelta(milliseconds=expected_ms)
    diagnostic = mismatch(key, "sent_at", actual, f"UTC instant {expected.isoformat()} / {expected_ms} epoch milliseconds")
    require(isinstance(actual, str) and re.fullmatch(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?(?:Z|[+-]\d{2}:\d{2})", actual), diagnostic)
    try:
        parsed = datetime.fromisoformat(actual.replace("Z", "+00:00"))
    except ValueError as error:
        raise RuntimeError(diagnostic) from error
    # Verify the independently generated instant and UTC normalization; equivalent
    # whole-second and .000 spellings must not cause a false failure.
    require(parsed.utcoffset() == timedelta(0) and parsed == expected, diagnostic)


class Publications:
    """Read-only SQL observations bracket real commits independently of engine reporting."""
    def __init__(self, profile):
        self.profile = profile
        self.samples = []

    def poll(self):
        began = wall_ms()
        with closing(read_database(self.profile)) as db:
            rows = dict(db.execute("SELECT account,count(*) FROM messages GROUP BY account"))
        ended = wall_ms()
        require(not self.samples or began >= self.samples[-1]["end_unix_ms"],
                "Wall clock moved backwards during publication measurement")
        value = {"start_unix_ms": began, "end_unix_ms": ended, "counts": rows}
        if self.samples:
            require(all(rows.get(account, 0) >= count for account, count in self.samples[-1]["counts"].items()),
                    "Acquisition removed a stable fixture source")
        self.samples.append(value)
        require(len(self.samples) <= 40000, "Publication observations exceeded the bounded run")
        return rows

    def increments(self):
        for before, after in zip(self.samples, self.samples[1:]):
            for account, count in after["counts"].items():
                previous = before["counts"].get(account, 0)
                if count > previous:
                    # The earlier read may have taken its snapshot after start;
                    # using start→end encloses the possible commit conservatively.
                    yield {"account": account, "start_unix_ms": before["start_unix_ms"],
                           "end_unix_ms": after["end_unix_ms"], "new_messages": count - previous}


def validate_fixture(data, engine_pid):
    require(data.get("version") == 1 and data.get("scenario") == "google-navigation"
            and data.get("engine_pid") == engine_pid and data.get("profile_kind") == "live"
            and data.get("fixture_revision") == 0, "Unexpected verification engine identity or scenario")
    require(data.get("transport") == "in_memory_only" and data.get("native_vault") is False,
            "Verification must have in-memory transport and no native vault")
    require(data.get("config") == CONFIG and not data.get("errors"), "Fixture configuration changed or reported an error")
    require(sorted(data.get("accounts", []), key=lambda row: row["key"]) == expected_accounts(),
            "Verification account identities differ from the independent fixture")
    require(type(data.get("base_unix_secs")) is int and data["base_unix_secs"] % 60 == 0,
            "Fixture date origin must be a UTC minute")


def verify_sources(profile, fixture):
    accounts = {account["id"]: account for account in expected_accounts()}
    checked, by_account, receipts = [], {}, {}
    with closing(read_database(profile)) as db:
        db.row_factory = sqlite3.Row
        require(db.execute("PRAGMA user_version").fetchone()[0] == 12, "Schema boundary changed")
        meta = dict(db.execute("SELECT key,value FROM meta"))
        require(str(meta["fixture_revision"]) == "0" and str(meta["profile_kind"]) == "1", "Disposable profile is not empty-live derived")
        require({r[0] for r in db.execute("SELECT id FROM accounts")} == set(accounts), "Unexpected account in disposable profile")
        for table in ("drafts", "capture_operations", "attachments", "calendar_write_operations"):
            require(db.execute(f'SELECT count(*) FROM "{table}"').fetchone()[0] == 0,
                    "Navigation created or imported unexpected authored/attachment content: " + table)
        rows = db.execute("""SELECT m.*,g.remote_thread_id,g.provider_labels_json,g.deleted,
            c.source_state,c.index_state,c.attachment_state,c.issue,c.source_size,c.internal_date_ms,
            c.source_hash AS coverage_hash FROM messages m
            LEFT JOIN google_message_state g ON g.account=m.account AND g.message_id=m.id
            LEFT JOIN google_mail_coverage c ON c.account=m.account AND c.remote_id=m.remote_id
            ORDER BY m.account,m.remote_id""")
        seen = set()
        for row in rows:
            key = (row["account"], row["remote_id"])
            require(row["account"] in accounts and key not in seen, "Unexpected/duplicate account-bound message")
            seen.add(key)
            expected = expected_message(accounts[row["account"]], row["remote_id"], fixture["base_unix_secs"])
            for field in ("id", "conversation_id", "remote_thread_id", "body", "subject", "source_hash", "internal_date_ms"):
                require(row[field] == expected[field], mismatch(key, field, row[field], expected[field]))
            verify_timestamp(key, row["sent_at"], expected["internal_date_ms"])
            for field, value in {"sender_name": "Verification Sender", "sender_email": "sender@example.test",
                                 "recipient": accounts[row["account"]]["email"], "starred": 0, "inbox": 1,
                                 "sent": 0, "trashed": 0, "deleted": 0, "source_state": "retained",
                                 "index_state": "indexed", "attachment_state": "complete", "issue": None}.items():
                require(row[field] == value, mismatch(key, field, row[field], value))
            require(row["unread"] in (0, 1) and (expected["recent"] or row["unread"] == 0), "Unexpected read state")
            labels = set(json.loads(row["provider_labels_json"]))
            require(labels in ({"INBOX"}, {"INBOX", "UNREAD"}) and (expected["recent"] or labels == {"INBOX"}),
                    "Provider state gained an unexpected label")
            blob = profile / "blobs" / expected["source_hash"]
            require(row["coverage_hash"] == expected["source_hash"] and row["source_size"] == len(expected["source"])
                    and blob.is_file() and blob.read_bytes() == expected["source"], "Retained MIME bytes/coverage differ from generated expectation")
            by_account[row["account"]] = by_account.get(row["account"], 0) + 1
            checked.append({"account": row["account"], "remote_id": row["remote_id"], "source_sha256": expected["source_hash"],
                            "body_sha256": hashlib.sha256(expected["body"].encode()).hexdigest()})
        for account in accounts:
            require(all((account, f"vr{index:04}") in seen for index in range(60)), "Stable recent sources are incomplete")
            require(60 < by_account.get(account, 0) <= 4156, "Both accounts need actual bounded historical acquisition")
        require(db.execute("SELECT count(*) FROM google_mail_coverage").fetchone()[0] == len(seen),
                "A provider source is retained without complete normalized/indexed verification")
        indexed = db.execute("""SELECT count(*) FROM message_fts JOIN messages m ON m.rowid=message_fts.rowid
            WHERE message_fts MATCH 'body:acquisition' AND message_fts.sender=m.sender_name || ' ' || m.sender_email
            AND message_fts.subject=m.subject AND message_fts.body=m.body""").fetchone()[0]
        require(indexed == len(seen) == db.execute("SELECT count(*) FROM message_fts").fetchone()[0],
                "FTS content/postings do not match every independently verified source")
        for row in db.execute("""SELECT account,part_key,applied_revision,count(*) FROM google_live_page_items
                WHERE applied_revision IS NOT NULL GROUP BY account,part_key,applied_revision"""):
            receipts[(row[0], row[1])] = {"revision": row[2], "item_count": row[3]}
        require(all(r[0] < 10000 for r in db.execute("SELECT count(*) FROM conversations GROUP BY account")),
                "Acquisition exceeded the unchanged four-route oracle workload")
    reported = {}
    for receipt in fixture["publications"]:
        key = (receipt["account"], receipt["receipt_key"])
        require(key not in reported and receipt["start_unix_ms"] <= receipt["end_unix_ms"], "Duplicate/invalid durable receipt report")
        reported[key] = {field: receipt[field] for field in ("revision", "item_count")}
    require(reported and reported == receipts, "Engine reported publications differ from durable Core part receipts")
    return {"messages": len(seen), "by_account": by_account, "fts_postings": indexed,
            "durable_part_receipts": len(receipts), "verified_sources": checked}


def verify_overlap(gui, fixture, observations):
    require({group["class"] for group in gui["classes"]} == ROUTES and len(gui["classes"]) == 4,
            "The unchanged benchmark must report exactly four routes")
    increments = list(observations.increments())
    result = []
    for group in gui["classes"]:
        samples = group["samples"]
        require(group["observations"] == len(samples) == 100, "Each route needs all 100 correct paints")
        times = sorted(sample["input_to_paint_ms"] for sample in samples)
        require(all(type(value) in (int, float) and math.isfinite(value) and value >= 0 for value in times),
                "Paint latency samples must be finite and nonnegative")
        p95 = times[math.ceil((len(times) - 1) * .95)]
        require(abs(p95 - group["p95_ms"]) < .000001 and group["budget_passed"] == (p95 <= 100),
                "Reported p95/budget differs from the retained 100 raw samples")
        require(all(s["input_unix_ms"] <= s["paint_unix_ms"] for s in samples)
                and all(a["paint_unix_ms"] <= b["input_unix_ms"] for a, b in zip(samples, samples[1:])),
                "Navigation wall-clock windows are not ordered")
        begin, end = samples[0]["input_unix_ms"], samples[-1]["paint_unix_ms"]
        sql = [row for row in increments if begin <= row["start_unix_ms"] <= row["end_unix_ms"] <= end]
        durable = [row for row in fixture["publications"] if begin <= row["start_unix_ms"] <= row["end_unix_ms"] <= end]
        for account in (row["id"] for row in expected_accounts()):
            require(any(row["account"] == account for row in sql) and any(row["account"] == account for row in durable),
                    "Both accounts must publish durable mail during route: " + group["class"])
        result.append({"class": group["class"], "first_input_unix_ms": begin, "last_paint_unix_ms": end,
                       "independent_sql_increments": sql, "durable_receipt_count": len(durable),
                       "durable_item_count": sum(row["item_count"] for row in durable)})
    return result


def cleanup_owned(scratch, owner):
    require(scratch.is_absolute() and scratch.resolve() == scratch and scratch.name.startswith("fuckmail-google-navigation-"),
            "Refusing cleanup outside the allocated verification root")
    require((scratch / "runner-owner").read_text(encoding="ascii") == owner, "Disposable directory ownership changed")
    require(not scratch.is_symlink(), "Disposable root became a link")
    shutil.rmtree(scratch)


def run(args):
    report = args.report.resolve()
    outputs = {name: report.with_suffix(suffix) for name, suffix in {
        "gui_report": ".gui.json", "fixture_report": ".fixture.json", "publications": ".sql-observations.json",
        "source_manifest": ".source.json", "gui_log": ".gui.log", "engine_log": ".engine.log",
        "personal_cli_log": ".personal-cli.log", "workspace_cli_log": ".workspace-cli.log"}.items()}
    require(not any(path.exists() for path in (report, *outputs.values())), "Choose new evidence paths; prior results are never overwritten")
    report.parent.mkdir(parents=True, exist_ok=True)
    result = {"result": "failed", "scenario": "google-navigation", "version": 1,
              "runtime": "development; not package acceptance", "started_unix_ms": wall_ms(),
              "host": {"platform": platform.platform(), "machine": platform.machine(), "python": sys.version,
                       "logical_cpus": os.cpu_count()}, "theme": args.theme, "simulated_confirmed_limits": LIMITS,
              "outputs": {key: str(path) for key, path in outputs.items()}}
    started, children, scratch, observations = time.monotonic(), [], None, None
    owner = str(uuid.uuid4())
    try:
        programs = {name: path.resolve(strict=True) for name, path in
                    (("gui", args.gui), ("cli", args.cli), ("engine", args.engine))}
        require(all(path.is_file() for path in programs.values()), "Three explicit native executables are required")
        result["executables"] = {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()}
        initial_sources = source_manifest()
        outputs["source_manifest"].write_text(json.dumps(initial_sources, indent=2) + "\n", encoding="utf-8")
        env = package_environment()
        env.pop("QT_QUICK_BACKEND", None)
        env.pop("QT_D3D_NO_VBLANK_THREAD", None)
        env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
        env["QSG_INFO"] = "1"
        if args.qt_root:
            env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
        result["environment"] = {key: env.get(key) for key in ("QT_QPA_PLATFORM", "QT_QUICK_BACKEND", "PATH")}
        scratch = Path(tempfile.mkdtemp(prefix="fuckmail-google-navigation-")).resolve()
        (scratch / "runner-owner").write_text(owner, encoding="ascii")
        profile = scratch / "empty-live-profile"
        fixture_path = profile / "verification-google-report.json"
        with ExitStack() as logs:
            streams = {name: logs.enter_context(path.open("wb")) for name, path in outputs.items() if name.endswith("_log")}
            engine_command = [str(programs["engine"]), "--profile", str(profile), "--live-profile", "--verification-google"]
            result["engine_command"] = engine_command
            engine = subprocess.Popen(engine_command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                      stdout=streams["engine_log"], stderr=subprocess.STDOUT)
            children.append(engine)

            def wait_for(predicate, seconds, message, required=()):
                deadline = time.monotonic() + seconds
                while True:
                    require(engine.poll() is None and all(child.poll() is None for child in required), message + ": process exited")
                    if observations:
                        observations.poll()
                    if predicate():
                        return
                    require(time.monotonic() < deadline, message + ": timed out")
                    time.sleep(.025)

            wait_for(lambda: (profile / "engine.json").is_file() and fixture_path.is_file(), 45, "Verification startup")
            require(read_json(profile / "engine.json")["pid"] == engine.pid, "Unexpected engine owns disposable discovery")
            fixture = read_json(fixture_path)
            validate_fixture(fixture, engine.pid)
            require(fixture["status"] == "ready" and fixture["counters"]["http_started"] == 0,
                    "Fixture must await explicit sync owners without provider work")
            observations = Publications(profile)
            require(not observations.poll(), "Verification bootstrap imported mail before explicit sync")

            def call(method, params=None, account=None):
                require(engine.poll() is None, "Never relaunch a verification engine after its owner exits")
                params_path = scratch / ("params-" + str(uuid.uuid4()) + ".json")
                params_path.write_text(json.dumps(params or {}), encoding="utf-8")
                command = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]), "--json"]
                if account:
                    command += ["--account", account]
                command += ["call", method, "--params-file", str(params_path)]
                reply = subprocess.run(command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=30)
                require(reply.returncode == 0, "IPC preparation failed: " + method + ": " + reply.stderr[-800:])
                data = json.loads(reply.stdout)
                require(data.get("ok") is True, "IPC operation failed: " + method)
                return data["result"]

            owners = []
            for account in expected_accounts():
                command = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                           "--account", account["id"], "--json", "google", "sync"]
                for name, value in LIMITS.items():
                    command += ["--" + name.replace("_", "-"), str(value)]
                command += ["--seconds", "900"]
                process = subprocess.Popen(command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                           stdout=streams[account["key"] + "_cli_log"], stderr=subprocess.STDOUT)
                children.append(process)
                owners.append(process)
            def recent_ready():
                with closing(read_database(profile)) as db:
                    counts = dict(db.execute("SELECT account,count(*) FROM messages WHERE remote_id GLOB 'vr[0-9][0-9][0-9][0-9]' GROUP BY account"))
                    return counts == {row["id"]: 60 for row in expected_accounts()}
            wait_for(recent_ready, 90, "Both accounts' stable recent mail", owners)
            status = call("google_sync_status")
            require(status["configured"] is True and status["limits"] == LIMITS
                    and len(status["accounts"]) == 2 and all(row["active"] and not row["owned"] for row in status["accounts"]),
                    "Two CLI-owned sync jobs must be active with the explicitly supplied limits")
            preferences = call("preferences_get")
            call("preferences_save", {"request_id": str(uuid.uuid4()), "expected_revision": preferences["revision"],
                                      "preferences": {"theme": args.theme, "startup_view": "mail", "last_workspace": "mail",
                                                      "text_size": "default", "timeline_position": "footer"}})
            gui_command = [str(programs["gui"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                           "--live-profile", "--benchmark-navigation-output", str(outputs["gui_report"])]
            result["gui_command"] = gui_command
            gui = subprocess.Popen(gui_command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                   stdout=streams["gui_log"], stderr=subprocess.STDOUT)
            children.append(gui)
            wait_for(lambda: gui.poll() is not None, 660, "Native navigation", owners)
            result["gui_exit_code"] = gui.returncode
            require(gui.returncode in (0, 9), "Native GUI failed; inspect retained raw output")
            gui_data = read_json(outputs["gui_report"])
            require(gui_data.get("result") == "measured" and not gui_data.get("error"), "Native navigation did not complete")
            baseline = observations.poll()
            wait_for(lambda: all(observations.samples[-1]["counts"].get(row["id"], 0) > baseline[row["id"]]
                                 for row in expected_accounts()), 15, "GUI close must preserve both CLI acquisitions", owners)
            result["gui_close_lifetime"] = {"before": baseline, "after": observations.samples[-1]["counts"], "passed": True}
            owners[0].terminate()
            owners[0].wait(timeout=5)
            # Allow the first account's already-admitted short commit to settle.
            settling = time.monotonic()
            wait_for(lambda: time.monotonic() - settling >= 1, 3, "First-owner drain", (owners[1],))
            baseline = observations.poll()
            remaining = expected_accounts()[1]["id"]
            wait_for(lambda: observations.samples[-1]["counts"].get(remaining, 0) > baseline[remaining],
                     15, "Independent remaining CLI acquisition", (owners[1],))
            require(observations.samples[-1]["counts"][expected_accounts()[0]["id"]] == baseline[expected_accounts()[0]["id"]],
                    "Released account continued publishing after its bounded drain")
            result["first_owner_exit"] = {"before": baseline, "after": observations.samples[-1]["counts"], "passed": True}
            (profile / "verification-google-hold-raw").touch(exist_ok=False)
            wait_for(lambda: read_json(fixture_path)["counters"]["raw_held_active"] >= 1,
                     15, "Deterministic final-owner cancellation barrier", (owners[1],))
            final_owner_end = wall_ms()
            owners[1].terminate()
            owners[1].wait(timeout=5)
            result["engine_exit_code"] = engine.wait(timeout=20)
            result["final_owner_exit"] = {"owner_end_unix_ms": final_owner_end, "engine_end_unix_ms": wall_ms(),
                                           "cli_exit_codes": [child.returncode for child in owners], "method": "owned CLI process termination / IPC EOF"}
            require(engine.returncode == 0 and not (profile / "engine.json").exists(), "Final-owner exit did not cleanly stop engine/remove discovery")
        fixture = read_json(fixture_path)
        validate_fixture(fixture, engine.pid)
        require(fixture["status"] == "complete", "Verification engine did not complete its final report")
        counters = fixture["counters"]
        require(counters["raw_cancelled"] >= 1 and counters["raw_active"] == counters["raw_held_active"] == 0
                and counters["http_rejected"] == counters["vault_stores"] == 0
                and counters["vault_loads"] > 0 and counters["modify_completed"] > 0,
                "Final cancellation, transport isolation or vault isolation failed")
        result["credential_verification"] = {"native_vault": False, "synthetic_vault_loads": counters["vault_loads"],
                                             "synthetic_vault_stores": counters["vault_stores"],
                                             "unknown_account_or_transport_errors": fixture["errors"]}
        result["source_verification"] = verify_sources(profile, fixture)
        result["identity_verification"] = verify_navigation(profile, gui_data)
        result["overlap_verification"] = verify_overlap(gui_data, fixture, observations)
        result["classes"] = [{key: group[key] for key in ("class", "observations", "p95_ms", "max_ms", "budget_passed")}
                             for group in gui_data["classes"]]
        result["budget_passed"] = gui_data.get("budget_passed")
        require(gui_data.get("p95_budget_ms") == 100 and type(result["budget_passed"]) is bool
                and result["budget_passed"] == (gui.returncode == 0), "Native unchanged budget/exit-code contract failed")
        require(source_manifest() == initial_sources and all(sha256(path) == result["executables"][name]["sha256"]
                                                            for name, path in programs.items()), "Runtime/source inputs changed during measurement")
        require(result["budget_passed"], "Native navigation exceeded the unchanged 100 ms p95 budget; correctness/overlap results are retained")
        result["result"] = "passed"
    except Exception as error:
        result["error"] = str(error)
    finally:
        stopped = True
        try:
            stop_children(children)
        except Exception as error:
            stopped = False
            result["result"] = "failed"
            result["cleanup_error"] = str(error)
        if scratch:
            fixture_path = scratch / "empty-live-profile" / "verification-google-report.json"
            try:
                if fixture_path.is_file():
                    shutil.copyfile(fixture_path, outputs["fixture_report"])
                require(stopped, "Owned child remained active; disposable directory retained")
                cleanup_owned(scratch, owner)
            except Exception as error:
                result["result"] = "failed"
                result["cleanup_error"] = str(error)
                result["retained_disposable_root"] = str(scratch)
        if observations:
            outputs["publications"].write_text(json.dumps(observations.samples, separators=(",", ":")) + "\n", encoding="utf-8")
        result["elapsed_seconds"] = time.monotonic() - started
        result["verifier_sha256"] = sha256(Path(__file__))
        result["outputs_sha256"] = {name: sha256(path) for name, path in outputs.items() if path.is_file()}
        report.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "error", "elapsed_seconds", "gui_exit_code") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gui", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--engine", type=Path, required=True)
    parser.add_argument("--qt-root", type=Path, help="Optional Qt SDK root for the explicitly identified development runtime")
    parser.add_argument("--theme", choices=("graphite", "paper", "harbour"), default="graphite")
    parser.add_argument("--report", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
