#!/usr/bin/env python3
"""Functional CLI/source verification against the isolated draft-actions provider.

Only a new disposable generated profile is used. No real accounts, benchmark,
package or populated isolated-engine restart is exercised.
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
import platform
import re
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_navigation import LIMITS, expected_accounts, read_database, read_json, require, source_manifest
from verify_google_remote_drafts import expected_source, verify_projection
from verify_search_ingestion import sha256, stop_children


FIRST_EDIT_BODY = "First authored edit after adoption.\nUnicode 日本語, café."


def action_source(account, index, base, version=1):
    source_index = (10, 0, 11, 12, 13, 14)[index]
    raw = expected_source(account["key"], source_index, version, base)
    if index != 2:
        raw = raw.replace(b"From: Remote Author <remote@example.test>\r\n",
                          f"From: Primary Author <{account['email']}>\r\n".encode(), 1)
    return raw


def routing(message, header):
    return [(name, address) for name, address in getaddresses(message.get_all(header, [])) if address]


def file_parts(message):
    # The formatter can nest multipart/mixed body containers. Only leaf MIME
    # parts are files; iter_attachments() also yields those nested containers.
    return [part for part in message.walk() if not part.is_multipart()
            and (part.get_content_disposition() in ("attachment", "inline") or part.get_filename() is not None)]


def verify_wire(raw, account, index, base, copied, body=None):
    """Compare decoded wire content to independently generated input, not Core's projection."""
    message = BytesParser(policy=policy.default).parsebytes(raw)
    original = BytesParser(policy=policy.default).parsebytes(action_source(account, index, base))
    require(all(not part.defects for part in message.walk()), "Uploaded MIME is malformed")
    expected_from = [(account["email"], account["email"])] if copied else routing(original, "From")
    require(routing(message, "From") == expected_from, "Uploaded MIME changed the expected From")
    for header in ("To", "Cc", "Bcc"):
        require(routing(message, header) == routing(original, header), "Uploaded MIME lost " + header)
    require(str(message["Subject"]) == str(original["Subject"]), "Uploaded MIME changed the subject")
    files = file_parts(message)
    bodies = [part for part in message.walk() if not part.is_multipart() and part not in files]
    require(len(bodies) == 1 and bodies[0].get_content_type() == "text/plain", "Upload added HTML or lost the sole plain body")
    expected_body = original.get_body(preferencelist=("plain",)).get_content() if body is None else body
    require(bodies[0].get_content().replace("\r\n", "\n") == expected_body.replace("\r\n", "\n"),
            "Uploaded MIME differs from the complete authored text")
    expected_files = file_parts(original)
    require(len(files) == len(expected_files), "Upload lost or invented a file")
    for actual, expected in zip(files, expected_files):
        require(actual.get_payload(decode=True) == expected.get_payload(decode=True)
                and actual.get_filename() == expected.get_filename()
                and actual.get_content_type() == expected.get_content_type(), "Uploaded attachment content or metadata differs")
        require(actual.get_content_disposition() == "attachment" and actual.get("Content-ID") is None,
                "Converted file retained inline/CID semantics")
    for header in ("In-Reply-To", "References"):
        actual = message.get(header)
        expected = None if copied else original.get(header)
        require((str(actual).split() if actual is not None else None)
                == (str(expected).split() if expected is not None else None), "Uploaded MIME changed " + header)
    return message


def same_publications(actual, expected, fields, label):
    project = lambda rows: sorted(json.dumps({field: row[field] for field in fields}, sort_keys=True) for row in rows)
    require(project(actual) == project(expected), "Unexpected exact provider " + label + " publications")


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {key: report.with_suffix(suffix) for key, suffix in {
        "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log",
        "workspace": ".workspace.log", "fixture": ".fixture.json", "sources": ".source.json"}.items()}
    if args.gui:
        outputs.update({key: report.with_suffix(suffix) for key, suffix in {
            "gui": ".gui.log", "gui_report": ".gui.json", "expectation": ".expectation.json"}.items()})
    require(not report.exists() and not any(p.exists() for p in outputs.values()), "Use a new numbered evidence path")
    programs = {name: getattr(args, name).resolve(strict=True) for name in ("cli", "engine")}
    if args.gui:
        programs["gui"] = args.gui.resolve(strict=True)
    identities = {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()}
    sources = source_manifest()
    outputs["sources"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    result = {"result": "failed", "scenario": "isolated-google-draft-actions-native" if args.gui else "isolated-google-draft-actions", "version": 1,
              "executables": identities, "commands": [], "accounts": [], "limits": LIMITS,
              "limitations": ["Generated in-memory provider and vault only; no real mail or delivery.",
                              "Functional CLI/source and optional native checks; no performance, package or engine-restart acceptance."]}
    children = []
    started = time.monotonic()
    try:
        with tempfile.TemporaryDirectory(prefix="fuckmail-draft-actions-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {key: logs.enter_context(outputs[key].open("wb")) for key in ("engine", "watch", "personal", "workspace")}
            env = package_environment()
            if args.gui:
                streams["gui"] = logs.enter_context(outputs["gui"].open("wb"))
                env.pop("QT_QUICK_BACKEND", None)
                env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
                env["QSG_INFO"] = "1"
                env["FM_ENGINE_PATH"] = str(programs["engine"])
                if args.qt_root:
                    env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
            engine = subprocess.Popen([str(programs["engine"]), "--profile", str(profile), "--live-profile",
                                       "--verification-google", "--verification-google-draft-actions"],
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
                require(engine.poll() is None, "Never reopen a populated isolated profile using ordinary network code")
                output = subprocess.run(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                require(bool(output.stdout.strip()), "No CLI response: " + output.stderr[-400:])
                data = json.loads(output.stdout)
                error = data.get("error", {}).get("code")
                result["commands"].append({"command": tail[:2], "account": account["key"] if account else None,
                                           "exit_code": output.returncode, "error": error})
                if refused:
                    require(output.returncode != 0 and data.get("ok") is False and error in refused,
                            "Unexpected refusal: " + str(data.get("error")))
                    return data["error"]
                require(output.returncode == 0 and data.get("ok") is True, "CLI action failed: " + str(data.get("error")))
                return data["result"]

            def source_args(draft):
                return ["--draft-id", draft["draft_id"], "--source-revision", str(draft["remote_source"]["source_revision"])]

            def export(account, draft, raw):
                data = cli(["draft", "source", *source_args(draft)], account)
                path = Path(data["path"]).resolve(strict=True)
                require(path.parent.parent.samefile(profile / "exports") and path.parent.name == data["sha256"], "Export escaped its private directory")
                require(data["account"] == account["id"] and data["draft_id"] == draft["draft_id"]
                        and data["source_revision"] == draft["remote_source"]["source_revision"]
                        and data["sha256"] == hashlib.sha256(raw).hexdigest() and data["size"] == len(raw)
                        and path.read_bytes() == raw, "Retained source differs from independent expected RAW")

            def synced(account, draft):
                status = cli(["google", "draft-status", "--draft-id", draft["draft_id"]], account)
                return status if status["state"] == "synced" and status["baseline"]["draft_revision"] == draft["revision"] else None

            expected_uploads, expected_sends, expected_deletions = [], [], []
            expected_reads, retained_versions = [], []

            def upload_bytes(digest, size):
                require(isinstance(digest, str) and re.fullmatch(r"[0-9a-f]{64}", digest) is not None,
                        "Invalid retained upload hash")
                raw = (profile / "blobs" / digest).read_bytes()
                require(len(raw) == size and hashlib.sha256(raw).hexdigest() == digest,
                        "Retained upload does not match its immutable hash and size")
                return raw

            def record_upload(account, status, index, copied, body=None):
                baseline = status["baseline"]
                target = baseline["draft_id"]
                require(re.fullmatch(r"vd[0-9]{4}", target) is not None if copied else target == "ra0000",
                        "Upload selected the wrong created/adopted target")
                thread = "vdt-" + target if copied else "rat0000"
                require(baseline["thread_id"] == thread, "Upload changed its provider thread")
                raw = upload_bytes(baseline["upload_sha256"], baseline["size"])
                verify_wire(raw, account, index, base, copied, body)
                require(baseline["observed_sha256"] == baseline["upload_sha256"],
                        "Generated provider readback differs from its exact accepted upload")
                row = {"account": account["id"], "draft_id": target, "message_id": baseline["message_id"],
                       "thread_id": thread, "size": len(raw), "sha256": baseline["upload_sha256"]}
                expected_reads.append(row)
                expected_uploads.append({**row, "kind": "create" if copied else "update"})

            try:
                wait_for(lambda: fixture_path.is_file() and (profile / "engine.json").is_file(), "startup")
                require(read_json(profile / "engine.json")["pid"] == engine.pid, "Wrong discovery owner")
                initial = read_json(fixture_path)
                require(initial["native_vault"] is False and initial["transport"] == "in_memory_only", "Provider isolation changed")
                base = initial["base_unix_secs"]
                watch = subprocess.Popen(command(["watch", "--seconds", "900"]), cwd=scratch, env=env,
                                         stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                accounts = expected_accounts()
                owners = []
                for account in accounts:
                    tail = ["google", "sync", "--seconds", "900"]
                    for key, value in LIMITS.items():
                        tail.extend(["--" + key.replace("_", "-"), str(value)])
                    owner = subprocess.Popen(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    owners.append(owner)
                    children.append(owner)
                snapshots = {}
                retained = []
                for account in accounts:
                    wait_for(lambda: cli(["google", "drafts-status"], account)["enumeration_complete"], "inventory " + account["key"])
                    page = cli(["draft", "list", "--limit", "100"], account)
                    require(page["total"] == 6 and page["next_offset"] is None, "Wrong action-fixture inventory")
                    rows = {row["remote_source"]["provider_draft_id"]: row for row in page["drafts"]}
                    require(set(rows) == {f"ra{i:04}" for i in range(6)}, "Wrong action source identities")
                    snapshots[account["key"]] = rows
                    for index in range(6):
                        draft = rows[f"ra{index:04}"]
                        raw = action_source(account, index, base)
                        export(account, draft, raw)
                        retained.append((account, draft, raw))
                        expected_reads.append({"account": account["id"], "draft_id": f"ra{index:04}",
                                               "message_id": f"ram{index:04}v1", "thread_id": f"rat{index:04}",
                                               "sha256": hashlib.sha256(raw).hexdigest(), "size": len(raw)})
                    if args.gui:
                        continue
                    source = rows["ra0000"]
                    assessment = cli(["draft", "authoring", *source_args(source)], account)
                    require(assessment["adopt"]["allowed"] is True, "Supported plain source refused")
                    adopted = cli(["google", "draft-adopt", *source_args(source), "--request-id", "adopt-main"], account)
                    require(adopted["read_only"] is False and adopted["origin"] == "local", "Adoption did not create authored work")
                    require(adopted["google"].get("baseline") is None, "Adoption fabricated an upload baseline")
                    state = cli(["google", "draft-status", "--draft-id", adopted["draft_id"]], account)
                    require(state["state"] == "adopted" and state.get("baseline") is None,
                            "Adoption alone uploaded or fabricated an acknowledged baseline")
                    if account["key"] == "personal":
                        body_file = scratch / "first-edit.txt"
                        body_file.write_text(FIRST_EDIT_BODY, encoding="utf-8")
                        edited = cli(["draft", "save", "--draft-id", adopted["draft_id"], "--body-file", str(body_file),
                                      "--expected-revision", str(adopted["revision"]), "--request-id", "first-edit"], account)
                        uploaded = wait_for(lambda: synced(account, edited), "adopted first edit")
                        require(uploaded["baseline"]["draft_id"] == "ra0000", "First edit created a different Google draft")
                        record_upload(account, uploaded, 0, False, FIRST_EDIT_BODY)
                    else:
                        sent = cli(["google", "send", "--draft-id", adopted["draft_id"], "--expected-revision", str(adopted["revision"]),
                                    "--expected-google-revision", str(state["revision"]), "--request-id", "adopted-immediate-send"], account)
                        require(sent["state"] == "accepted" and sent["provider_draft_id"] == "ra0000", "Immediate Send lost the observed Google target")
                        require(sent["thread_id"] == "rat0000" and sent["ack"]["thread_id"] == "rat0000",
                                "Immediate Send changed its adopted thread")
                        raw = upload_bytes(sent["upload_sha256"], sent["size"])
                        mime = verify_wire(raw, account, 0, base, False)
                        require(str(mime["Message-ID"]) == sent["message_id"], "Send changed its frozen RFC Message-ID")
                        require(re.fullmatch(r"<[^<>\s]+>", sent["message_id"]) is not None,
                                "Send omitted its bounded bracketed RFC Message-ID")
                        expected_sends.append({"account": account["id"], "provider_draft_id": "ra0000",
                                               "message_id": sent["ack"]["message_id"], "thread_id": "rat0000",
                                               "rfc_message_id": sent["message_id"][1:-1], "size": len(raw),
                                               "sha256": sent["upload_sha256"], "kind": "draft_send"})
                    for index in (1, 2):
                        original = rows[f"ra{index:04}"]
                        assessment = cli(["draft", "authoring", *source_args(original)], account)
                        require(assessment["copy"]["allowed"] is True and assessment["adopt"]["allowed"] is False,
                                "Conversion-only source eligibility is incorrect")
                        cli(["google", "draft-adopt", *source_args(original), "--request-id", f"refuse-adopt-{index}"],
                            account, {"draft_source_unsupported"})
                        tail = ["draft", "copy", *source_args(original), "--request-id", f"copy-{index}",
                                "--from-policy", "primary", "--content-policy", "plain_text",
                                "--inline-policy", "attachments", "--thread-policy", "omit"]
                        copied = cli(tail, account)
                        require(cli(tail, account) == copied, "Copy replay changed its original receipt")
                        message = BytesParser(policy=policy.default).parsebytes(action_source(account, index, base))
                        require(copied["from"] == account["email"] and not copied.get("signature_text")
                                and not copied.get("body_html") and copied["subject"] == str(message["Subject"]), "Copy conversion or identity differs")
                        for header, field in (("To", "to_recipients"), ("Cc", "cc_recipients"), ("Bcc", "bcc_recipients")):
                            require([(r.get("name") or "", r["email"]) for r in copied[field]] == routing(message, header), "Copy lost routing")
                        plain = message.get_body(preferencelist=("plain",))
                        require(copied["body"].replace("\r\n", "\n").rstrip("\n") == plain.get_content().replace("\r\n", "\n").rstrip("\n"), "Copy lost text")
                        baseline = wait_for(lambda: synced(account, copied), "copied draft autosync")
                        require(baseline["baseline"]["draft_id"] not in rows, "Copy overwrote an original Google source")
                        record_upload(account, baseline, index, True)
                    receipts = []
                    for index, expected_state in ((4, "acceptance_unknown"), (5, "deleted")):
                        original = rows[f"ra{index:04}"]
                        request = f"delete-{index}"
                        tail = ["google", "draft-delete", *source_args(original), "--request-id", request]
                        receipt = cli(tail, account)
                        require(receipt["state"] == expected_state, "Delete outcome lost certainty distinction")
                        require(receipt["account"] == account["id"] and receipt["draft_id"] == original["draft_id"]
                                and receipt["request_id"] == request and receipt["provider_draft_id"] == f"ra{index:04}"
                                and receipt["source_revision"] == original["remote_source"]["source_revision"]
                                and receipt["source_sha256"] == hashlib.sha256(action_source(account, index, base)).hexdigest(),
                                "Delete receipt crossed its frozen account/source/target")
                        expected_deletions.append({"account": account["id"], "draft_id": f"ra{index:04}", "existed": True,
                                                   "response": "lost_response" if index == 4 else "acknowledged"})
                        require(cli(tail, account)["state"] == expected_state, "Delete replay changed mutation certainty")
                        if index == 4:
                            checked = cli(["google", "draft-delete-reconcile", "--request-id", request], account)
                            require(checked["state"] == "acceptance_unknown", "Absence fabricated deletion acknowledgment")
                            require(checked["read_evidence"] == {"draft_exists": False, "deletion_proven": False,
                                                                  "source": "draft_get", "observed": None},
                                    "Deletion reconciliation omitted its non-conclusive absence evidence")
                            cli(["draft", "copy", *source_args(original), "--request-id", "blocked-copy",
                                 "--from-policy", "primary", "--content-policy", "plain_text",
                                 "--inline-policy", "attachments", "--thread-policy", "omit"],
                                account, {"google_draft_delete_pending", "google_draft_pending"})
                            cli(["google", "draft-delete-abandon", "--request-id", request,
                                 "--expected-revision", str(checked["revision"])], account, {"stale_google_work"})
                        receipts.append({"request_id": request, "state": expected_state})
                    result["accounts"].append({"account": account["key"], "receipts": receipts})
                if args.gui:
                    for owner in owners:
                        owner.terminate()
                        owner.wait(timeout=10)
                    wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "CLI owners released before native stage")
                    expected = {"version": 1, "limits": LIMITS, "accounts": []}
                    for account in accounts:
                        rows = snapshots[account["key"]]
                        item = {"account": account["id"]}
                        for action, target in (("copy", "ra0001"), ("adopt", "ra0000"), ("delete", "ra0005")):
                            source = rows[target]["remote_source"]
                            item[action] = {"provider_draft_id": target, "source_revision": source["source_revision"],
                                            "source_sha256": source["sha256"]}
                        original = cli(["draft", "show", "--draft-id", rows["ra0001"]["draft_id"]], account)
                        verify_projection(original, action_source(account, 1, base), 0)
                        with closing(read_database(profile)) as db:
                            identity = db.execute("SELECT a.email,a.name,m.revision FROM accounts a JOIN account_metadata m ON m.account=a.id WHERE a.id=?",
                                                  (account["id"],)).fetchone()
                        require(identity is not None and identity[0] == account["email"] and identity[1] == account["email"]
                                and isinstance(identity[2], int) and identity[2] == 0, "Unexpected generated primary From identity")
                        item["copy"]["primary_from"] = {"email": identity[0], "name": identity[1], "revision": identity[2]}
                        item["copy"]["expected_draft"] = {
                            **{key: original[key] for key in ("subject", "body", "to_recipients", "cc_recipients", "bcc_recipients")},
                            "from": account["email"], "signature_text": "", "body_html": None,
                        }
                        expected["accounts"].append(item)
                    outputs["expectation"].write_text(json.dumps(expected, indent=2) + "\n", encoding="utf-8")
                    gui = subprocess.Popen([str(programs["gui"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                                            "--google-draft-actions-self-test", str(outputs["gui_report"]),
                                            "--google-draft-actions-expectation", str(outputs["expectation"])],
                                           cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                           stdout=streams["gui"], stderr=subprocess.STDOUT)
                    children.append(gui)
                    require(gui.wait(timeout=180) == 0, "Native draft actions failed; inspect the retained GUI log")
                    observed = read_json(outputs["gui_report"])
                    require(observed["version"] == 1 and observed["result"] == "passed", "Native report is not a passing action run")
                    native_accounts = {row["account"]: row for row in observed["accounts"]}
                    require(set(native_accounts) == {account["id"] for account in accounts}, "Native actions missed or crossed an account")
                    for account, expected_account in zip(accounts, expected["accounts"]):
                        native = native_accounts[account["id"]]
                        copied = cli(["draft", "show", "--draft-id", native["copy"]["draft_id"]], account)
                        for key, value in expected_account["copy"]["expected_draft"].items():
                            require(copied.get(key) == value, "Native copy changed expected " + key)
                        copied_state = synced(account, copied)
                        require(copied_state is not None, "Native closed its owner before the copied draft synchronized")
                        record_upload(account, copied_state, 1, True)
                        adopted = cli(["google", "draft-status", "--draft-id", native["adopt"]["draft_id"]], account)
                        require(adopted["state"] == "adopted" and adopted.get("baseline") is None, "Native adoption performed an unauthored upload")
                        deletion = cli(["google", "draft-delete-status", "--request-id", native["delete"]["request_id"]], account)
                        require(deletion["state"] == "deleted" and deletion["provider_draft_id"] == "ra0005", "Native deletion crossed its frozen target")
                        source = snapshots[account["key"]]["ra0005"]
                        require(deletion["account"] == account["id"] and deletion["draft_id"] == source["draft_id"]
                                and deletion["source_revision"] == source["remote_source"]["source_revision"]
                                and deletion["source_sha256"] == hashlib.sha256(action_source(account, 5, base)).hexdigest(),
                                "Native deletion crossed its frozen account/source")
                        expected_deletions.append({"account": account["id"], "draft_id": "ra0005",
                                                   "existed": True, "response": "acknowledged"})
                        require(native["exports"], "Native workflow omitted original-source export")
                        for exported in native["exports"]:
                            matched = [index for index in range(6) if snapshots[account["key"]][f"ra{index:04}"]["remote_source"]["source_revision"] == exported["source_revision"]]
                            require(len(matched) == 1 and exported["account"] == account["id"]
                                    and exported.get("attachment_id") is None, "Native export crossed its expected account/source")
                            raw = action_source(account, matched[0], base)
                            path = Path(exported["path"]).resolve(strict=True)
                            require(path.parent.parent.samefile(profile / "exports")
                                    and path.parent.name == exported["sha256"]
                                    and exported["sha256"] == hashlib.sha256(raw).hexdigest()
                                    and exported["size"] == len(raw) and path.read_bytes() == raw,
                                    "Native original-source export differs from independent RAW")
                        result["accounts"].append({"account": account["key"], "native": native,
                                                   "receipts": [{"request_id": deletion["request_id"], "state": "deleted"}]})
                else:
                    (profile / "verification-google-draft-actions-version-2").write_text("generated version transition\n", encoding="ascii")
                    wait_for(lambda: read_json(fixture_path)["draft_actions_fixture"]["version_two"], "changed remote source")
                    for account in accounts:
                        stale = snapshots[account["key"]]["ra0003"]
                        adoption_error = cli(["google", "draft-adopt", *source_args(stale), "--request-id", "stale-adopt"],
                                             account, {"google_draft_conflict", "stale_source_revision"})
                        require(adoption_error.get("details", {}).get("draft_authoring_invocation_finished_without_commit") is True,
                                "Fresh stale adoption omitted its completed-invocation recovery proof")
                        deletion_error = cli(["google", "draft-delete", *source_args(stale), "--request-id", "stale-delete"],
                                             account, {"google_draft_conflict", "stale_source_revision"})
                        require(deletion_error.get("details", {}).get("draft_delete_invocation_finished_before_dispatch") is True,
                                "Fresh stale deletion omitted its completed-invocation recovery proof")
                        cli(["google", "draft-delete-status", "--request-id", "stale-delete"], account, {"not_found"})
                        history = cli(["draft", "sources", "--draft-id", stale["draft_id"], "--limit", "100"], account)
                        require(history["account"] == account["id"] and history["draft_id"] == stale["draft_id"]
                                and history["total"] == 2 and len(history["sources"]) == 2 and history["next_offset"] is None,
                                "Changed-source refusal failed to retain exactly both remote versions")
                        versions = {}
                        for version in (1, 2):
                            matches = [source for source in history["sources"] if source["message_id"] == f"ram0003v{version}"]
                            require(len(matches) == 1, "Retained history lost the exact contained-message version")
                            source = matches[0]
                            raw = action_source(account, 3, base, version)
                            require(source["provider_draft_id"] == "ra0003" and source["thread_id"] == "rat0003"
                                    and source["source_state"] == "available" and source["preview_state"] == "complete"
                                    and source["sha256"] == hashlib.sha256(raw).hexdigest() and source["size"] == len(raw)
                                    and source["is_current"] is (version == 2), "Retained remote version has incorrect identity/content")
                            versions[version] = source
                            version_draft = {**stale, "remote_source": source}
                            export(account, version_draft, raw)
                            if version == 2:
                                retained_versions.append((account, version_draft, raw))
                                expected_reads.append({"account": account["id"], "draft_id": "ra0003",
                                                       "message_id": "ram0003v2", "thread_id": "rat0003",
                                                       "sha256": source["sha256"], "size": len(raw)})
                        require(versions[1]["source_revision"] == stale["remote_source"]["source_revision"]
                                and versions[2]["source_revision"] > versions[1]["source_revision"],
                                "Changed source replaced or rewound its immutable original revision")
                for owner in owners:
                    if owner.poll() is None:
                        owner.terminate()
                        owner.wait(timeout=10)
                wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "sync owners released")
                cutoff = time.time_ns() // 1_000_000
                wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "post-release provider report")
                before = read_json(fixture_path)["counters"]
                for account, draft, raw in retained + retained_versions:
                    export(account, draft, raw)
                for item, account in zip(result["accounts"], accounts):
                    for receipt in item["receipts"]:
                        require(cli(["google", "draft-delete-status", "--request-id", receipt["request_id"]], account)["state"] == receipt["state"], "Offline receipt changed")
                require(read_json(fixture_path)["counters"] == before, "Offline inspection started provider work")
                with closing(read_database(profile)) as db:
                    require(db.execute("PRAGMA user_version").fetchone()[0] == 16, "Wrong schema")
                    rows = db.execute("SELECT json FROM capture_operations WHERE execution_kind='google' AND json_extract(json,'$.kind')='draft_delete'").fetchall()
                    require(len(rows) == (2 if args.gui else 4), "Missing or fabricated deletion journals")
                watch.terminate()
                watch.wait(timeout=10)
                require(engine.wait(timeout=15) == 0, "Final engine owner did not close")
                final = read_json(fixture_path)
                require(final["status"] == "complete" and not final["errors"] and final["counters"] == before, "Final provider receipt changed or failed")
                require(before["draft_delete_attempts"] == (2 if args.gui else 4) and before["draft_delete_unknown"] == (0 if args.gui else 2)
                        and before["draft_deleted"] == 2 and before["vault_stores"] == 0,
                        "Unexpected automatic deletion retry, completion claim or credential write")
                require(before["draft_created"] == (2 if args.gui else 4) and before["draft_updated"] == (0 if args.gui else 1)
                        and before["send_completed"] == (0 if args.gui else 1), "Adoption uploaded without an edit or created an unintended provider draft/Send")
                require(final["draft_actions_fixture"]["enabled"] is True
                        and final["draft_actions_fixture"]["containers_per_account"] == 6
                        and final["draft_actions_fixture"]["version_two"] is (not bool(args.gui)),
                        "Wrong generated draft-actions scenario")
                result["expected_provider_publications"] = {"drafts": expected_uploads, "sends": expected_sends,
                                                            "deletions": expected_deletions}
                same_publications(final["draft_fixture"]["publications"], expected_uploads,
                                  ("account", "draft_id", "message_id", "thread_id", "size", "sha256", "kind"), "draft upload")
                same_publications(final["send_fixture"]["publications"], expected_sends,
                                  ("account", "provider_draft_id", "message_id", "thread_id", "rfc_message_id", "size", "sha256", "kind"), "Send")
                same_publications(final["draft_actions_fixture"]["deletions"], expected_deletions,
                                  ("account", "draft_id", "existed", "response"), "deletion")
                observed_reads = final["draft_actions_fixture"]["reads"]
                require(all(row in observed_reads for row in expected_reads),
                        "Actual provider GET trace omitted a verified original/upload/changed source")
                require(len({(row["account"], row["draft_id"]) for row in expected_uploads}) == len(expected_uploads),
                        "Separate authored operations unexpectedly reused a provider draft")
                require(source_manifest() == sources and all(sha256(path) == identities[name]["sha256"] for name, path in programs.items()), "Tested inputs changed during verification")
                result["provider_counters"] = before
                result["retained_source_count"] = len(retained)
                result["retained_changed_source_count"] = len(retained_versions)
                result["verified_provider_publications"] = {"drafts": expected_uploads, "sends": expected_sends,
                                                            "deletions": expected_deletions}
                result["result"] = "passed"
            finally:
                stop_children(children)
                if fixture_path.is_file():
                    outputs["fixture"].write_bytes(fixture_path.read_bytes())
    except Exception as error:
        result["result"] = "failed"
        result["error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {name: sha256(path) for name, path in outputs.items() if path.is_file()}
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
