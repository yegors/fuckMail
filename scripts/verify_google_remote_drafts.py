#!/usr/bin/env python3
"""Verify retained remote Drafts with ordinary CLI/native UI and an isolated provider.

Uses a newly created disposable profile only. No real network/vault, provider
writes, benchmark, package or fresh-engine-restart acceptance is claimed.
"""
import argparse
import base64
from contextlib import ExitStack, closing
from datetime import datetime, timezone
from email import policy
from email.parser import BytesParser
from email.utils import getaddresses
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_navigation import LIMITS, expected_accounts, read_database, read_json, require, source_manifest
from verify_search_ingestion import sha256, stop_children


def expected_source(key, index, version, base):
    """Independent RFC bytes, without calling the Rust/provider implementation."""
    if index == 3:
        return f"Malformed remote draft {key}/0003, version {version}.\r\n".encode()
    date = datetime.fromtimestamp(base - 86400 + index * 60, timezone.utc)
    weekdays = ("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
    months = ("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")
    rfc_date = f"{weekdays[date.weekday()]}, {date.day:02} {months[date.month-1]} {date.year:04} {date:%H:%M:%S} +0000"
    sender = "Other Author <other@example.test>" if index == 4 else "Remote Author <remote@example.test>"
    routing = "" if index == 4 else (
        "To: Reader One <one@example.test>, Reader Two <two@example.test>\r\n"
        "Cc: Copy <copy@example.test>\r\nBcc: Blind <blind@example.test>\r\n")
    headers = (f"From: {sender}\r\n{routing}Subject: Remote {key} draft {index:04} version {version}\r\n"
               f"Date: {rfc_date}\r\nMessage-ID: <remote-{key}-{index:04}-v{version}@example.test>\r\n"
               "In-Reply-To: <prior@example.test>\r\nReferences: <first@example.test> <prior@example.test>\r\n"
               "MIME-Version: 1.0\r\n")
    body = f"Remote cached draft for {key}/{index:04}, version {version}.\r\nSecond authored line."
    if index:
        return (headers + "Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n" + body).encode()
    mixed, alternative = f"remote-{key}-0-v{version}", f"alternative-{key}-v{version}"
    html = f'<p>Remote <strong>{key}</strong> draft 0000 version {version}.</p><img src="cid:remote-image">'
    attachment = bytes((0, 1, 2, 255)) + key.encode() + bytes((version,))
    return (headers + f'Content-Type: multipart/mixed; boundary="{mixed}"\r\n\r\n--{mixed}\r\n'
            + f'Content-Type: multipart/alternative; boundary="{alternative}"\r\n\r\n--{alternative}\r\n'
            + "Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n"
            + body + f"\r\n--{alternative}\r\nContent-Type: text/html; charset=utf-8\r\n"
            + "Content-Transfer-Encoding: 8bit\r\n\r\n" + html + f"\r\n--{alternative}--\r\n--{mixed}\r\n"
            + 'Content-Type: application/octet-stream; name="fixture.bin"\r\n'
            + 'Content-Disposition: inline; filename="fixture.bin"\r\nContent-ID: <remote-image>\r\n'
            + "Content-Transfer-Encoding: base64\r\n\r\n" + base64.b64encode(attachment).decode()
            + f"\r\n--{mixed}--\r\n").encode()


def verify_projection(draft, raw, index):
    require(draft["origin"] == "google_snapshot" and draft["read_only"] is True, "Snapshot lost read-only provenance")
    require(draft.get("google") is None and draft.get("google_send") is None, "Snapshot invented upload or Send state")
    require(draft.get("conversation_id") is None and draft.get("source_message_id") is None, "Snapshot manufactured received mail")
    require(not draft.get("signature_text"), "Snapshot acquired an account signature")
    if index == 3:
        require(draft["remote_source"]["preview_state"] != "complete" and draft["remote_source"]["issues"],
                "Malformed RAW was presented as a complete preview")
        return
    message = BytesParser(policy=policy.default).parsebytes(raw)
    sender = getaddresses(message.get_all("From", []))
    require(len(sender) == 1 and draft["from"] == sender[0][1] and draft["from_name"] == sender[0][0], "Imported From was retargeted")
    for header, field in (("To", "to_recipients"), ("Cc", "cc_recipients"), ("Bcc", "bcc_recipients")):
        expected = [(name, address) for name, address in getaddresses(message.get_all(header, [])) if address]
        actual = [(row.get("name") or "", row["email"]) for row in draft[field]]
        require(actual == expected, "Imported routing differs: " + field)
    require(draft["subject"] == str(message["Subject"]), "Imported subject changed")
    plain = message.get_body(preferencelist=("plain",)) if message.is_multipart() else message
    require(draft["body"].replace("\r\n", "\n").rstrip("\n") == plain.get_content().replace("\r\n", "\n").rstrip("\n"),
            "Readable preview differs from independently decoded text")


def no_writes(fixture):
    require(fixture["native_vault"] is False and fixture["transport"] == "in_memory_only", "Provider isolation changed")
    counters = fixture["counters"]
    for field in ("modify_completed", "draft_created", "draft_updated", "send_completed", "vault_stores", "http_rejected"):
        require(counters[field] == 0, "Unexpected provider/credential action: " + field)
    require(not fixture["errors"] and not fixture["draft_fixture"]["publications"] and not fixture["send_fixture"]["publications"],
            "Import produced a provider write or fixture error")


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {key: report.with_suffix(suffix) for key, suffix in {
        "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log", "workspace": ".workspace.log",
        "fixture": ".fixture.json", "source": ".source.json", "gui": ".gui.log", "gui_report": ".gui.json",
        "expectation": ".expectation.json"}.items()}
    require(not report.exists() and not any(path.exists() for path in outputs.values()), "Use a new numbered evidence path")
    programs = {name: getattr(args, name).resolve(strict=True) for name in ("cli", "engine")}
    if args.gui:
        programs["gui"] = args.gui.resolve(strict=True)
    sources = source_manifest()
    outputs["source"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    identities = {key: {"path": str(path), "sha256": sha256(path)} for key, path in programs.items()}
    result = {"version": 1, "scenario": "isolated-remote-google-drafts", "result": "failed", "executables": identities,
              "limits": LIMITS, "commands": [], "accounts": [], "limitations": [
                  "Generated source and in-memory transport/vault only; no actual Google account or delivery.",
                  "Functional checks only; no navigation/search/resize performance or package acceptance.",
                  "Fresh GUI/CLI cached reopening retains one explicit local Watch engine owner; no fresh-engine restart claim."]}
    children = []
    started = time.monotonic()
    try:
        with tempfile.TemporaryDirectory(prefix="fuckmail-remote-drafts-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {key: logs.enter_context(outputs[key].open("wb")) for key in ("engine", "watch", "personal", "workspace", "gui")}
            env = package_environment()
            env.pop("QT_QUICK_BACKEND", None)
            env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
            env["QSG_INFO"] = "1"
            env["FM_ENGINE_PATH"] = str(programs["engine"])
            if args.qt_root:
                env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
            engine = subprocess.Popen([str(programs["engine"]), "--profile", str(profile), "--live-profile",
                                       "--verification-google", "--verification-google-drafts"], cwd=scratch, env=env,
                                      stdin=subprocess.DEVNULL, stdout=streams["engine"], stderr=subprocess.STDOUT)
            children.append(engine)

            def wait_for(predicate, label, timeout=90):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    require(engine.poll() is None, "Engine exited during " + label)
                    value = predicate()
                    if value:
                        return value
                    time.sleep(.1)
                raise RuntimeError("Timed out: " + label)

            def cli(tail, account=None, refused=None):
                require(engine.poll() is None, "Never relaunch isolated engine against its populated profile")
                command = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]), "--json"]
                if account:
                    command += ["--account", account["id"]]
                reply = subprocess.run(command + tail, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                require(bool(reply.stdout.strip()), "No CLI reply for " + " ".join(tail[:2]) + ": " + reply.stderr[-600:])
                data = json.loads(reply.stdout)
                code = data.get("error", {}).get("code")
                result["commands"].append({"command": tail[:2], "account": account["key"] if account else None,
                                           "exit_code": reply.returncode, "error": code})
                if refused:
                    require(reply.returncode != 0 and data.get("ok") is False and code in refused,
                            "Wrong source refusal: " + str(code))
                    return data["error"]
                require(reply.returncode == 0 and data.get("ok") is True,
                        "CLI failed: " + " ".join(tail[:2]) + ": " + str(data.get("error", reply.stderr[-600:])))
                return data["result"]

            def list_drafts(account):
                rows, offset, expected_total = [], 0, None
                for _ in range(10):
                    page = cli(["draft", "list", "--limit", "17", "--offset", str(offset)], account)
                    require(len(page["drafts"]) <= 17, "Draft page exceeds requested size")
                    expected_total = page["total"] if expected_total is None else expected_total
                    require(page["total"] == expected_total, "Completed inventory list changed while paging")
                    rows.extend(page["drafts"])
                    next_offset = page["next_offset"]
                    if next_offset is None:
                        require(len(rows) == expected_total, "Draft pagination omitted rows")
                        require(len({row["draft_id"] for row in rows}) == len(rows), "Draft pagination duplicated identity")
                        return rows
                    require(next_offset > offset, "Draft page failed to advance")
                    offset = next_offset
                raise RuntimeError("Generated Drafts exceeded bounded page count")

            def export(account, draft, revision, expected, attachment=None):
                tail = ["draft", "source", "--draft-id", draft["draft_id"], "--source-revision", str(revision)]
                if attachment:
                    tail += ["--attachment-id", attachment]
                data = cli(tail, account)
                path = Path(data["path"]).resolve(strict=True)
                # Rust canonicalizes Windows paths with the extended prefix.
                # Compare directory identity, not unlike lexical path forms.
                require(path.parent.parent.samefile(profile / "exports") and path.parent.name == data["sha256"],
                        "Export escaped the disposable private export directory")
                require(data["account"] == account["id"] and data["draft_id"] == draft["draft_id"]
                        and data["source_revision"] == revision and data.get("attachment_id") == attachment,
                        "Export belongs to another source version or account")
                require(path.read_bytes() == expected and data["sha256"] == hashlib.sha256(expected).hexdigest()
                        and data["size"] == len(expected), "Verified source export differs from independent fixture bytes")
                return data

            try:
                wait_for(lambda: (profile / "engine.json").is_file() and fixture_path.is_file(), "isolated startup")
                require(read_json(profile / "engine.json")["pid"] == engine.pid, "Unexpected engine discovery owner")
                initial = read_json(fixture_path)
                require(initial["remote_drafts_fixture"]["enabled"] is True and initial["send_fixture"]["enabled"] is False,
                        "Wrong isolated fixture mode")
                require(initial["status"] == "ready" and initial["counters"]["http_started"] == 0, "Provider work started without ownership")
                base = initial["base_unix_secs"]
                watch = subprocess.Popen([str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                                          "--json", "watch", "--seconds", "900"], cwd=scratch, env=env,
                                         stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                accounts = expected_accounts()
                for account in accounts:
                    cli(["google", "drafts-status"], account)
                    require(not list_drafts(account), "Empty fixture acquired remote snapshots from cached reads")
                    cli(["google", "drafts-refresh"], account, {"sync_not_running"})
                require(read_json(fixture_path)["counters"]["http_started"] == 0, "Cached read or unowned refresh started HTTP")
                owners = []
                for account in accounts:
                    command = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                               "--account", account["id"], "--json", "google", "sync", "--seconds", "900"]
                    for key, value in LIMITS.items():
                        command += ["--" + key.replace("_", "-"), str(value)]
                    owner = subprocess.Popen(command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    owners.append(owner)
                    children.append(owner)

                def complete(account):
                    state = cli(["google", "drafts-status"], account)
                    return state if state["enumeration_complete"] is True else None

                initial_main, source_hashes = {}, set()
                for account in accounts:
                    status = wait_for(lambda: complete(account), "initial remote inventory " + account["key"])
                    require(status["observed_items"] == 52 and status["source_retained"] == 51
                            and status["source_unavailable"] == 1 and status["preview_incomplete"] >= 1,
                            "Inventory confused enumeration, retained RAW and preview completeness")
                    rows = list_drafts(account)
                    by_remote = {row["remote_source"]["provider_draft_id"]: row for row in rows}
                    require(len(by_remote) == len(rows) and 51 <= len(rows) <= 52, "Unexpected remote Drafts identity count")
                    for index in range(52):
                        provider_id = f"rd{index:04}"
                        if index == 2:
                            if provider_id in by_remote:
                                require(by_remote[provider_id]["remote_source"]["source_state"] == "missing", "404 became a false available source")
                            continue
                        row = by_remote[provider_id]
                        source = row["remote_source"]
                        version = 2 if index == 1 else 1
                        raw = expected_source(account["key"], index, version, base)
                        require(source["message_id"] == f"rm{index:04}v{version}" and source["thread_id"] == f"rt{index:04}",
                                "Stored draft identity differs from its fetched version: " + provider_id)
                        digest = hashlib.sha256(raw).hexdigest()
                        require(source["sha256"] == digest and source["size"] == len(raw) and source["source_state"] == "available",
                                "Retained source metadata differs: " + account["key"] + "/" + provider_id)
                        require((profile / "blobs" / digest).read_bytes() == raw, "Retained original RAW differs")
                        source_hashes.add(digest)
                        require(row["origin"] == "google_snapshot" and row["read_only"] is True, "Draft list lost imported provenance")
                        if index in (0, 1, 3, 4):
                            draft = cli(["draft", "show", "--draft-id", row["draft_id"]], account)
                            verify_projection(draft, raw, index)
                            export(account, draft, source["source_revision"], raw)
                            if index == 0:
                                initial_main[account["key"]] = draft
                    result["accounts"].append({"account": account["key"], "initial_status": status, "draft_count": len(rows)})
                no_writes(read_json(fixture_path))
                (profile / "verification-google-draft-version-2").write_text("generated fixture version 2\n", encoding="ascii")
                wait_for(lambda: read_json(fixture_path)["remote_drafts_fixture"]["version_two"], "fixture version transition")
                for account in accounts:
                    refreshed = cli(["google", "drafts-refresh"], account)
                    require(refreshed["generation"] > next(row for row in result["accounts"] if row["account"] == account["key"])["initial_status"]["generation"],
                            "Explicit completed-scan refresh did not begin a new generation")
                expectation = {"version": 1, "accounts": []}
                for account in accounts:
                    wait_for(lambda: complete(account), "changed remote version " + account["key"])
                    old = initial_main[account["key"]]
                    draft = cli(["draft", "show", "--draft-id", old["draft_id"]], account)
                    raw = expected_source(account["key"], 0, 2, base)
                    source_hashes.add(hashlib.sha256(raw).hexdigest())
                    verify_projection(draft, raw, 0)
                    require(draft["remote_source"]["source_revision"] > old["remote_source"]["source_revision"], "Remote replacement did not advance source revision")
                    require(draft["remote_source"]["message_id"] == "rm0000v2" and draft["remote_source"]["thread_id"] == "rt0000",
                            "Refreshed source retained the wrong provider identity")
                    export(account, draft, draft["remote_source"]["source_revision"], raw)
                    export(account, old, old["remote_source"]["source_revision"], expected_source(account["key"], 0, 1, base))
                    history = cli(["draft", "sources", "--draft-id", draft["draft_id"], "--limit", "100"], account)
                    require(any(row["source_revision"] == old["remote_source"]["source_revision"]
                                and row["sha256"] == old["remote_source"]["sha256"] and row["message_id"] == "rm0000v1"
                                and row["thread_id"] == "rt0000" for row in history["sources"]), "Remote replacement lost original source history")
                    parts = draft["attachments"]
                    require(len(parts) == 1, "Expected one exact imported inline attachment")
                    attachment = bytes((0, 1, 2, 255)) + account["key"].encode() + b"\x02"
                    exported = export(account, draft, draft["remote_source"]["source_revision"], attachment, parts[0]["id"])
                    fields = ("draft_id", "origin", "read_only", "from", "from_name", "to_recipients", "cc_recipients", "bcc_recipients", "subject", "body", "remote_source")
                    expectation["accounts"].append({"account": account["id"], "provider_draft_id": "rd0000",
                        "draft": {key: draft[key] for key in fields}, "raw_sha256": hashlib.sha256(raw).hexdigest(),
                        "attachments": [{"id": parts[0]["id"], "sha256": exported["sha256"]}]})
                    body_file = scratch / (account["key"] + "-must-not-save.txt")
                    body_file.write_text("Must never become imported authored work", encoding="utf-8")
                    common = ["--draft-id", draft["draft_id"], "--expected-revision", str(draft["revision"])]
                    cli(["draft", "save", *common, "--body-file", str(body_file), "--request-id", "refused-save"], account, {"draft_read_only"})
                    cli(["draft", "discard", *common, "--request-id", "refused-discard"], account, {"draft_read_only"})
                    cli(["draft", "attach", *common, "--file", str(body_file), "--request-id", "refused-attach"], account, {"draft_read_only"})
                    cli(["draft", "capture", *common, "--request-id", "refused-capture"], account, {"capability_unavailable", "draft_read_only"})
                    require(cli(["draft", "show", "--draft-id", draft["draft_id"]], account) == draft, "Refused edit changed retained draft")
                with closing(read_database(profile)) as db:
                    require(db.execute("PRAGMA user_version").fetchone()[0] == 15, "Wrong live schema")
                    require(db.execute("SELECT count(*) FROM capture_operations").fetchone()[0] == 0, "Import or refused edit fabricated an upload/capture journal")
                    require(db.execute("SELECT count(*) FROM drafts WHERE origin!='google_snapshot' OR conversation_id IS NOT NULL OR source_message_id IS NOT NULL OR google_json IS NOT NULL").fetchone()[0] == 0,
                            "Independent storage has invented authored/twin/received identity")
                    for digest in source_hashes:
                        require(db.execute("SELECT count(*) FROM messages WHERE source_hash=?", [digest]).fetchone()[0] == 0, "Draft source became received mail")
                    result["independent_retained_sources"] = len(source_hashes)
                for owner in owners:
                    owner.terminate()
                    owner.wait(timeout=10)
                wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "all sync owners released")
                offline_cutoff_ms = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= offline_cutoff_ms, "fresh post-release fixture observation")
                wait_for(lambda: (lambda counters: counters["http_started"] == counters["http_completed"] + counters["http_cancelled"])(read_json(fixture_path)["counters"]), "in-flight reads drained")
                offline_before = read_json(fixture_path)["counters"]
                outputs["expectation"].write_text(json.dumps(expectation, indent=2) + "\n", encoding="utf-8")
                for item, account in zip(expectation["accounts"], accounts):
                    reopened = cli(["draft", "show", "--draft-id", item["draft"]["draft_id"]], account)
                    require(all(reopened[key] == value for key, value in item["draft"].items()), "Fresh offline CLI changed cached source")
                    cli(["google", "drafts-refresh"], account, {"sync_not_running"})
                if args.gui:
                    gui = subprocess.Popen([str(programs["gui"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                                            "--google-drafts-self-test", str(outputs["gui_report"]),
                                            "--google-drafts-expectation", str(outputs["expectation"])], cwd=scratch, env=env,
                                           stdin=subprocess.DEVNULL, stdout=streams["gui"], stderr=subprocess.STDOUT)
                    children.append(gui)
                    result["gui_exit_code"] = gui.wait(timeout=120)
                    require(result["gui_exit_code"] == 0 and outputs["gui_report"].is_file(), "Native cached Drafts walkthrough failed")
                    result["gui_result"] = read_json(outputs["gui_report"])
                    require(result["gui_result"]["passed"] is True and not result["gui_result"]["error"]
                            and result["gui_result"]["gui_pid"] == gui.pid, "Native Drafts report did not pass for the launched process")
                offline_after = read_json(fixture_path)["counters"]
                require(offline_after == offline_before, "Cached reopening or source exports started provider work")
                result["offline_provider_counters"] = offline_after
                no_writes(read_json(fixture_path))
                watch.terminate()
                watch.wait(timeout=10)
                require(engine.wait(timeout=10) == 0 and not (profile / "engine.json").exists(), "Final local owner did not stop the engine")
                fixture = read_json(fixture_path)
                no_writes(fixture)
                require(fixture["status"] == "complete", "Fixture did not finish cleanly")
                require(fixture["counters"] == offline_before, "Final provider report changed after sync owners were released")
                require(source_manifest() == sources, "Source changed during paired verification")
                require(all(sha256(path) == identities[key]["sha256"] for key, path in programs.items()), "Executable changed during verification")
                result["provider_counters"] = fixture["counters"]
                result["result"] = "passed"
            finally:
                stop_children(children)
                if fixture_path.is_file():
                    outputs["fixture"].write_bytes(fixture_path.read_bytes())
    except Exception as error:
        result["result"] = "failed"
        result["error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {key: sha256(path) for key, path in outputs.items() if path.is_file()}
    report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--engine", type=Path, required=True)
    parser.add_argument("--gui", type=Path)
    parser.add_argument("--qt-root", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
