#!/usr/bin/env python3
"""Exercise ordinary CLI Google drafts against the isolated in-memory engine.

Generated accounts and content only. Optional --send exercises isolated explicit
submissions. No real provider, real delivery, GUI or restart claim.
"""
import argparse
from contextlib import ExitStack, closing
from email import policy
from email.parser import BytesParser
from email.utils import getaddresses
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from verify_google_navigation import LIMITS, expected_accounts, read_database, require, source_manifest
from verify_search_ingestion import sha256, stop_children


def read_json(path):
    require(path.stat().st_size <= 16 * 1024 * 1024, "Fixture report exceeds its bound")
    return json.loads(path.read_text(encoding="utf-8"))


def verify_mime(profile, digest, size, account, body, signature, recipients, attachment):
    require(len(digest) == 64 and all(char in "0123456789abcdef" for char in digest), "Invalid retained hash")
    require(0 < size <= 2 * 1024 * 1024, "Generated draft exceeds fixture bound")
    path = profile / "blobs" / digest
    require(path.stat().st_size == size, "Retained MIME size differs")
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == digest, "Retained MIME hash differs")
    message = BytesParser(policy=policy.default).parsebytes(raw)
    require(not message.defects, "Generated MIME has parser defects")
    require(getaddresses([str(message["From"])])[0][1] == account["email"], "Wrong frozen From account")
    require(str(message["Subject"]) == "Human draft " + account["key"], "Wrong retained subject")
    for name, expected in recipients.items():
        actual = [address for _, address in getaddresses([str(message.get(name, ""))]) if address]
        require(actual == expected, "Wrong retained " + name + " recipients")
    text_parts = [part for part in message.walk() if part.get_content_type() == "text/plain"
                  and part.get_content_disposition() != "attachment"]
    require(len(text_parts) == 1, "Expected exactly one authored text part")
    require(text_parts[0].get_payload(decode=True) == (body + "\n\n-- \n" + signature).encode("utf-8"),
            "Authored text/signature bytes differ")
    # MIME's explicit disposition distinguishes attachments from a nested plain
    # multipart container; EmailMessage.iter_attachments() guesses that the
    # latter is an attachment when the body is a nested multipart/mixed.
    parts = [part for part in message.walk() if part.get_content_disposition() == "attachment"]
    require(sum(not part.is_multipart() for part in message.walk()) == 2,
            "Expected exactly one authored text leaf and one attachment leaf")
    require(len(parts) == 1 and parts[0].get_filename() == attachment[0],
            "Wrong attachment identity: " + repr([(part.get_content_type(), part.get_content_disposition(),
                                                    part.get_filename()) for part in parts]))
    require(parts[0].get_payload(decode=True) == attachment[1], "Attachment bytes differ")
    return {"sha256": digest, "size": size, "text_bytes": len(text_parts[0].get_payload(decode=True)),
            "attachment_sha256": hashlib.sha256(attachment[1]).hexdigest()}


def run(args):
    report = args.report.resolve()
    require(not report.exists(), "Use a new numbered report path")
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {name: report.with_suffix("." + name + ".log")
               for name in ("engine", "watch", "personal", "workspace")}
    fixture_output = report.with_suffix(".fixture.json")
    require(not fixture_output.exists() and all(not path.exists() for path in outputs.values()),
            "Never overwrite an earlier attempt")
    programs = {name: getattr(args, name).resolve(strict=True) for name in ("engine", "cli")}
    sources = source_manifest()
    result = {"result": "failed", "scenario": "isolated-google-send-cli" if args.send else "isolated-google-drafts-cli", "version": 2,
              "executables": {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()},
              "sources": sources, "limits": LIMITS, "accounts": [], "commands": [],
              "limitations": ["Generated in-memory provider; no live account, credential or real send.",
                              "CLI integration; no GUI, navigation, package or restart acceptance.",
                              "Provider echoes RAW; real-provider transformations remain unverified."]}
    children = []
    started = time.monotonic()
    try:
        # This library owns the newly created directory and removes it only after
        # all processes started below have stopped. No supplied profile is opened.
        with tempfile.TemporaryDirectory(prefix="fuckmail-google-drafts-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {name: logs.enter_context(path.open("wb")) for name, path in outputs.items()}
            env = os.environ.copy()
            env["FM_ENGINE_PATH"] = str(programs["engine"])
            engine_command = [str(programs["engine"]), "--profile", str(profile), "--live-profile", "--verification-google"]
            if args.send:
                engine_command.append("--verification-google-send")
            engine = subprocess.Popen(engine_command,
                                      cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=streams["engine"], stderr=subprocess.STDOUT)
            children.append(engine)

            def wait_for(predicate, label, timeout=30):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    require(engine.poll() is None, "Engine ended during " + label)
                    value = predicate()
                    if value:
                        return value
                    time.sleep(0.1)
                raise RuntimeError("Timed out: " + label)

            def cli(tail, account=None, expected_error=None):
                command = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]), "--json"]
                if account:
                    command += ["--account", account["id"]]
                reply = subprocess.run(command + tail, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                if expected_error:
                    data = json.loads(reply.stdout)
                    require(reply.returncode == 1 and data.get("ok") is False
                            and data.get("error", {}).get("code") == expected_error,
                            "Expected explicit CLI refusal " + expected_error)
                    result["commands"].append({"command": tail[:2], "account": account["key"] if account else None,
                                               "expected_error": expected_error})
                    return data["error"]
                require(reply.returncode == 0, "CLI failed: " + " ".join(tail[:2]) + ": " + reply.stderr[-600:])
                data = json.loads(reply.stdout)
                require(data.get("ok") is True, "CLI operation did not succeed")
                result["commands"].append({"command": tail[:2], "account": account["key"] if account else None})
                return data["result"]

            try:
                wait_for(lambda: (profile / "engine.json").is_file() and fixture_path.is_file(), "isolated startup")
                require(read_json(profile / "engine.json")["pid"] == engine.pid, "Unexpected engine discovery")
                initial = read_json(fixture_path)
                require(initial["transport"] == "in_memory_only" and initial["native_vault"] is False,
                        "Expected isolated provider and synthetic vault")
                require(initial["send_fixture"]["enabled"] == args.send, "Wrong explicit send fixture mode")
                require(initial["status"] == "ready" and initial["counters"]["http_started"] == 0,
                        "Provider work started before ownership")
                # Watch keeps only the engine alive while local drafts are prepared.
                watch = subprocess.Popen([str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                                          "--json", "watch", "--seconds", "900"], cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                         stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                expected = []
                attachment_bytes = bytes(range(256)) * 32
                filename = "Human attachment 日本語.bin"
                attachment_file = scratch / filename
                attachment_file.write_bytes(attachment_bytes)
                for account in expected_accounts():
                    body = "Human-authored " + account["key"] + " draft.\nUnicode 日本語, café."
                    signature = "Verification signature " + account["key"]
                    body_file, signature_file = scratch / (account["key"] + ".txt"), scratch / (account["key"] + "-signature.txt")
                    body_file.write_text(body, encoding="utf-8", newline="\n")
                    signature_file.write_text(signature, encoding="utf-8", newline="\n")
                    recipients = {"To": [], "Cc": [], "Bcc": []} if account["key"] == "personal" else {
                        "To": ["recipient@example.test"], "Cc": ["carbon@example.test"], "Bcc": ["hidden@example.test"]}
                    recipient_file = scratch / (account["key"] + "-recipients.json")
                    recipient_file.write_text(json.dumps({name.lower() + "_recipients": [{"email": value} for value in values]
                                                         for name, values in recipients.items()}), encoding="utf-8")
                    draft = cli(["draft", "create", "--request-id", account["key"] + "-create"], account)
                    draft = cli(["draft", "save", "--draft-id", draft["draft_id"], "--body-file", str(body_file),
                                 "--signature-file", str(signature_file), "--subject", "Human draft " + account["key"],
                                 "--recipients-file", str(recipient_file), "--expected-revision", str(draft["revision"]),
                                 "--request-id", account["key"] + "-save"], account)
                    imported = cli(["draft", "attach", "--draft-id", draft["draft_id"], "--file", str(attachment_file),
                                    "--expected-revision", str(draft["revision"]), "--request-id", account["key"] + "-attach"], account)
                    expected.append({"account": account, "draft": imported["draft"], "body": body, "signature": signature,
                                     "recipients": recipients, "body_file": body_file})
                require(read_json(fixture_path)["counters"]["http_started"] == 0, "Local draft preparation started provider work")
                require(not cli(["google", "sync-status"])["accounts"], "Local commands unexpectedly acquired a Google sync owner")
                owners = []
                for item in expected:
                    account = item["account"]
                    command = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                               "--account", account["id"], "--json", "google", "sync", "--seconds", "900"]
                    for name, value in LIMITS.items():
                        command += ["--" + name.replace("_", "-"), str(value)]
                    owner = subprocess.Popen(command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    children.append(owner)
                    owners.append(owner)

                def synced(item):
                    status = cli(["google", "draft-status", "--draft-id", item["draft"]["draft_id"]], item["account"])
                    return status if status["state"] == "synced" and status["baseline"]["draft_revision"] == item["draft"]["revision"] else None

                for item in expected:
                    account, draft = item["account"], item["draft"]
                    first = wait_for(lambda: synced(item), "first draft upload " + account["key"])
                    verified = verify_mime(profile, first["baseline"]["upload_sha256"], first["operation"]["size"], account,
                                           item["body"], item["signature"], item["recipients"], (filename, attachment_bytes))
                    noop = cli(["google", "draft-sync", "--draft-id", draft["draft_id"], "--expected-revision", str(draft["revision"]),
                                "--request-id", account["key"] + "-already-saved"], account)
                    require(noop["baseline"] == first["baseline"], "An already synchronized revision changed its provider baseline")
                    item["body"] += "\nAnother deliberate local edit."
                    item["body_file"].write_text(item["body"], encoding="utf-8", newline="\n")
                    item["draft"] = cli(["draft", "save", "--draft-id", draft["draft_id"], "--body-file", str(item["body_file"]),
                                          "--expected-revision", str(draft["revision"]), "--request-id", account["key"] + "-edit"], account)
                    second = wait_for(lambda: synced(item), "replacement draft upload " + account["key"])
                    require(second["baseline"]["draft_id"] == first["baseline"]["draft_id"], "Replacement changed stable draft identity")
                    require(second["baseline"]["message_id"] != first["baseline"]["message_id"], "Replacement failed to advance content identity")
                    require(second["baseline"]["observed_sha256"] == second["baseline"]["upload_sha256"], "Echo fixture RAW differs from upload")
                    updated = verify_mime(profile, second["baseline"]["upload_sha256"], second["operation"]["size"], account,
                                          item["body"], item["signature"], item["recipients"], (filename, attachment_bytes))
                    checked = cli(["google", "draft-reconcile", "--draft-id", draft["draft_id"]], account)
                    require(checked["baseline"] == second["baseline"] and checked["state"] == "synced", "Read reconciliation changed retained content")
                    result["accounts"].append({"account": account["key"], "draft_id": draft["draft_id"],
                                               "first": first, "final": checked, "first_mime": verified, "final_mime": updated})

                with closing(read_database(profile)) as db:
                    require(db.execute("PRAGMA user_version").fetchone()[0] == 18, "Expected schema 18")
                    rows = db.execute("SELECT account,id,json,blob_hash FROM capture_operations WHERE execution_kind='google'").fetchall()
                    require(len(rows) == 4, "Expected exactly one create and update per generated account")
                    for account, request_id, record, digest in rows:
                        record = json.loads(record)
                        require(record["account"] == account and record["request_id"] == request_id and record["upload_sha256"] == digest,
                                "Independent operation identity/hash differs")
                        require(record["state"] == "synced", "Independent operation journal is unfinished")
                    result["independent_google_operations"] = len(rows)
                if args.send:
                    from verify_google_send import exercise_sends
                    exercise_sends(cli, wait_for, synced, profile, scratch, expected, result, verify_mime,
                                   (filename, attachment_bytes))
                for owner in owners + [watch]:
                    owner.terminate()
                    owner.wait(timeout=10)
                require(engine.wait(timeout=10) == 0 and not (profile / "engine.json").exists(), "Final-owner exit did not stop the engine")
                fixture = read_json(fixture_path)
                require(fixture["status"] == "complete" and not fixture["errors"], "Fixture ended with errors")
                require(fixture["counters"]["draft_created"] == 2
                        and fixture["counters"]["draft_updated"] == (3 if args.send else 2),
                        "Provider saw duplicate or missing draft writes")
                if args.send:
                    from verify_google_send import verify_send_publications
                    verify_send_publications(fixture, result)
                else:
                    require(fixture["counters"]["send_completed"] == 0, "Draft-only fixture submitted mail")
                require(fixture["counters"]["vault_stores"] == 0 and fixture["native_vault"] is False, "Unexpected credential write")
                publications = fixture["draft_fixture"]["publications"]
                for item in result["accounts"]:
                    final = item["final"]
                    require(any(row["account"] == final["account"] and row["draft_id"] == final["baseline"]["draft_id"]
                                and row["message_id"] == final["baseline"]["message_id"] and row["sha256"] == item["final_mime"]["sha256"]
                                for row in publications), "Independent provider publication does not match accepted baseline")
                require(source_manifest() == sources, "Runtime sources changed during verification")
                result["provider_counters"] = fixture["counters"]
                result["result"] = "passed"
            finally:
                stop_children(children)
                if fixture_path.is_file():
                    fixture_output.write_bytes(fixture_path.read_bytes())
    except Exception as error:
        result["result"] = "failed"
        result["error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {path.name: sha256(path) for path in [*outputs.values(), fixture_output] if path.is_file()}
    report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--engine", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--send", action="store_true", help="Enable bounded generated submissions in the isolated provider")
    raise SystemExit(run(parser.parse_args()))
