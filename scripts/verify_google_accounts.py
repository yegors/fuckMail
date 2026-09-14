#!/usr/bin/env python3
"""Exercise CLI account lifecycle against a fresh isolated loopback OAuth fixture.

No real Google, browser, native credential vault, benchmark or populated-profile
restart is used. Every auth action uses the feature CLI's exact fixture probe.
"""
import argparse
from contextlib import ExitStack, closing
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_account_sources import BASE_UNIX, retained_content, source_inputs, verify_initial_sources
from verify_google_calendar import WINDOW
from verify_google_navigation import LIMITS, expected_accounts, expected_message, read_database, read_json, require
from verify_search_ingestion import sha256, stop_children


FULL_SCOPES = {"openid", "email", "https://www.googleapis.com/auth/gmail.modify",
               "https://www.googleapis.com/auth/calendar.events",
               "https://www.googleapis.com/auth/calendar.calendarlist.readonly"}
READ_SCOPES = {"openid", "email", "https://www.googleapis.com/auth/gmail.readonly"}
HOLD_STORE = "verification-google-auth-hold-store"
FAIL_CLEANUP = "verification-google-cleanup-fail"


def authored_projection(value):
    """Only immutable human-authored fields; archive and Google status may change."""
    fields = ("account", "draft_id", "revision", "mode", "from", "from_name", "subject",
              "to_recipients", "cc_recipients", "bcc_recipients", "body", "body_html",
              "signature_text", "attachment_ids", "attachments", "discarded", "origin")
    return {field: value[field] for field in fields}


def profile_files(profile):
    return {path.relative_to(profile).as_posix(): {"sha256": sha256(path), "size": path.stat().st_size}
            for path in sorted(profile.rglob("*")) if path.is_file()}


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {key: report.with_suffix(suffix) for key, suffix in {
        "engine": ".engine.log", "watch": ".watch.log", "personal": ".personal.log",
        "workspace": ".workspace.log", "fixture": ".fixture.json", "source": ".source.json",
        "ledger": ".ledger.json", "ordinary_refusal": ".ordinary-refusal.log",
        "feature_refusal": ".feature-refusal.log"}.items()}
    require(not report.exists() and not any(path.exists() for path in outputs.values()),
            "Use a new numbered evidence path")
    programs = {name: getattr(args, name).resolve(strict=True)
                for name in ("cli", "auth_cli", "engine", "ordinary_engine")}
    identities = {name: {"path": str(path), "sha256": sha256(path)} for name, path in programs.items()}
    sources = source_inputs()
    outputs["source"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    ledger = {"commands": [], "lifecycle": [], "auth_steps": [], "preservation_checks": []}
    result = {"version": 1, "scenario": "isolated-google-accounts-cli", "result": "failed",
              "base_unix_secs": BASE_UNIX, "executables": identities, "limits": LIMITS,
              "limitations": ["Generated loopback OAuth and in-memory HTTP/vault only; no real Google or native credential access.",
                              "Functional CLI and independent source checks; no performance or package acceptance.",
                              "Removed archives are retained locally; remote deletion and revocation are not exercised."]}
    children = []
    started = time.monotonic()
    try:
        with tempfile.TemporaryDirectory(prefix="fuckmail-accounts-cli-") as temporary, ExitStack() as logs:
            scratch = Path(temporary).resolve()
            profile = scratch / "empty-live-profile"
            fixture_path = profile / "verification-google-report.json"
            streams = {key: logs.enter_context(outputs[key].open("wb"))
                       for key in ("engine", "watch", "personal", "workspace")}
            env = package_environment()
            fixture_args = ["--profile", str(profile), "--live-profile", "--verification-google",
                            "--verification-google-accounts", "--verification-google-accounts-base", str(BASE_UNIX)]
            engine = subprocess.Popen([str(programs["engine"]), *fixture_args], cwd=scratch, env=env,
                                      stdin=subprocess.DEVNULL, stdout=streams["engine"], stderr=subprocess.STDOUT)
            children.append(engine)

            def wait_for(predicate, label, seconds=20):
                deadline = time.monotonic() + seconds
                while time.monotonic() < deadline:
                    require(engine.poll() is None, "Isolated engine exited during " + label)
                    value = predicate()
                    if value:
                        return value
                    time.sleep(.05)
                raise RuntimeError("Timed out: " + label)

            def fixture():
                return read_json(fixture_path)

            def fresh_fixture():
                cutoff = time.time_ns() // 1_000_000
                return wait_for(lambda: (value if (value := fixture())["updated_unix_ms"] >= cutoff else None),
                                "fresh sanitized fixture report")

            def command(tail, account=None, auth=False):
                command = [str(programs["auth_cli" if auth else "cli"]), "--profile", str(profile),
                           "--engine", str(programs["engine"]), "--json"]
                if auth:
                    command.append("--verification-google-auth")
                if account:
                    command += ["--account", account["id"]]
                return command + tail

            def cli(tail, account=None, refused=None, auth=False):
                require(engine.poll() is None, "Never relaunch a populated isolated profile")
                reply = subprocess.run(command(tail, account, auth), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=35)
                # Never copy OAuth URLs, callbacks, raw stdout or stderr into this report.
                try:
                    data = json.loads(reply.stdout)
                except (ValueError, TypeError) as error:
                    raise RuntimeError("CLI omitted a bounded JSON receipt for " + " ".join(tail[:2])) from error
                code = data.get("error", {}).get("code")
                ledger["commands"].append({"command": tail[:2], "account": account["key"] if account else None,
                                           "feature_auth_cli": auth, "exit_code": reply.returncode, "error": code})
                if refused:
                    require(reply.returncode != 0 and data.get("ok") is False and code in refused,
                            "Unexpected CLI refusal for " + " ".join(tail[:2]) + ": " + str(code))
                    return data["error"]
                require(reply.returncode == 0 and data.get("ok") is True,
                        "CLI action failed: " + " ".join(tail[:2]) + ": " + str(code))
                return data["result"]

            def call(method, params=None, account=None, refused=None):
                path = scratch / "bounded-command.json"
                path.write_text(json.dumps(params or {}, ensure_ascii=False), encoding="utf-8")
                return cli(["call", method, "--params-file", str(path)], account, refused)

            def rows(include_removed=False):
                return cli(["accounts", *(["--include-removed"] if include_removed else [])])["accounts"]

            def account_row(account):
                found = [row for row in rows(True) if row["id"] == account["id"]]
                require(len(found) == 1, "Missing exact account identity")
                return found[0]

            def lifecycle(account, label):
                value = account_row(account)["lifecycle"]
                ledger["lifecycle"].append({"label": label, "account": account["id"], "value": value})
                return value

            def step(number, identity, outcome="success", grant="full", renamed=False):
                value = {"step": number, "identity": identity, "outcome": outcome, "grant": grant, "renamed": renamed}
                (profile / "verification-google-account-step.json").write_text(json.dumps(value), encoding="utf-8")
                ledger["auth_steps"].append(value)

            def sign_in(account=None, restore_revision=None, refused=None):
                tail = ["google", "restore" if restore_revision is not None else "connect",
                        "--client-config", str(profile / "verification-google-client.json")]
                if restore_revision is not None:
                    tail += ["--expected-revision", str(restore_revision)]
                return cli(tail, account, refused, auth=True)

            def remove(account, request, revision):
                value = cli(["account", "remove", "--expected-revision", str(revision),
                             "--request-id", request, "--retention", "archive"], account)
                require(value["account"] == account["id"] and value["request_id"] == request
                        and value["operation"]["kind"] == "account_remove"
                        and value["operation"]["retention"] == "archive", "Wrong immutable removal identity")
                ledger["lifecycle"].append({"label": request, "account": account["id"], "value": value["lifecycle"],
                                             "operation": value["operation"], "operation_is_current": value["operation_is_current"]})
                return value

            drafts = {}

            def verify_draft(account, archived):
                wanted = drafts[account["key"]]
                draft = cli(["draft", "show", "--draft-id", wanted["draft_id"]], account)
                require(authored_projection(draft) == wanted["projection"], "Saved authored draft changed across lifecycle")
                require(draft["account_archived"] is archived and draft["read_only"] is archived,
                        "Draft archive affordance differs from lifecycle")
                destination = scratch / (account["key"] + "-export-" + str(len(ledger["commands"])) + ".bin")
                exported = cli(["attachment", wanted["attachment_id"], "--output", str(destination)], account)
                require(destination.read_bytes() == wanted["attachment_bytes"]
                        and exported["sha256"] == hashlib.sha256(wanted["attachment_bytes"]).hexdigest(),
                        "Archived attachment export differs from independent original bytes")

            def preserve(label, baseline):
                actual = retained_content(profile)
                require(actual == baseline, "Retained data changed during " + label)
                ledger["preservation_checks"].append({"label": label, "passed": True,
                    "sha256": hashlib.sha256(json.dumps(actual, sort_keys=True).encode()).hexdigest()})

            try:
                wait_for(lambda: fixture_path.is_file() and (profile / "engine.json").is_file(), "empty fixture startup")
                initial = fixture()
                accounts = expected_accounts()
                personal, workspace = accounts
                require(initial["accounts"] == accounts and initial["base_unix_secs"] == BASE_UNIX
                        and initial["account_fixture"]["mode"] == "accounts-v1"
                        and initial["account_fixture"]["initial_bound_accounts"] == 0
                        and initial["native_vault"] is False and initial["transport"] == "in_memory_only",
                        "Wrong isolated account fixture contract")
                watch = subprocess.Popen(command(["watch", "--seconds", "900"]), cwd=scratch, env=env,
                                         stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                children.append(watch)
                require(rows() == [] and rows(True) == [], "Fixture did not start with zero bound accounts")
                step(1, "personal", outcome="denied")
                sign_in(refused={"permission_denied"})
                denied = fresh_fixture()["account_fixture"]
                require(rows() == [] and denied["vault_store_attempts"] == 0 and denied["token_exchanges"] == 0,
                        "Denied Add created an account or stored a grant")
                step(2, "personal")
                require(sign_in()["account"] == personal["id"], "Personal Add changed immutable identity")
                step(3, "workspace", grant="mail_readonly")
                require(sign_in()["account"] == workspace["id"], "Workspace Add changed immutable identity")
                reduced = account_row(workspace)
                require(set(reduced["google"]["granted_scopes"]) == READ_SCOPES and reduced["google"]["capabilities"]["mail_read"] is True
                        and reduced["google"]["capabilities"]["mail_write"] is False and reduced["google"]["capabilities"]["send"] is False
                        and reduced["google"]["capabilities"]["calendar_write"] is False, "Partial grant invented write capability")
                step(4, "workspace")
                require(sign_in(workspace)["account"] == workspace["id"], "Scoped reconnect changed identity")
                for account in accounts:
                    row = account_row(account)
                    require(set(row["google"]["granted_scopes"]) == FULL_SCOPES and row["google"]["connection_state"] == "connected"
                            and row["google"]["capabilities"]["mail_write"] is True and row["google"]["capabilities"]["calendar_write"] is True,
                            "Full verified grant or capability publication differs")
                require(fresh_fixture()["counters"]["http_started"] == 0, "Sign-in started provider acquisition")
                preferences = cli(["preferences", "show"])
                preferences_file = scratch / "preferences.json"
                preferences_file.write_text(json.dumps({"calendar_display_time_zone": "UTC"}), encoding="utf-8")
                cli(["preferences", "save", "--preferences-file", str(preferences_file),
                     "--expected-revision", str(preferences["revision"]), "--request-id", "display-utc"])
                owners = []
                for account in accounts:
                    tail = ["google", "sync", "--seconds", "900"]
                    for key, value in LIMITS.items():
                        tail += ["--" + key.replace("_", "-"), str(value)]
                    owner = subprocess.Popen(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=streams[account["key"]], stderr=subprocess.STDOUT)
                    children.append(owner)
                    owners.append(owner)

                def acquired():
                    with closing(read_database(profile)) as db:
                        return (dict(db.execute("SELECT account,count(*) FROM messages GROUP BY account"))
                                == {a["id"]: 2 for a in accounts}
                                and dict(db.execute("SELECT account,count(*) FROM calendar_visible_sources GROUP BY account"))
                                == {a["id"]: 1 for a in accounts})

                wait_for(acquired, "four exact RAW messages and two Calendar sources", 45)
                result["initial_sources"] = verify_initial_sources(profile)
                for account in accounts:
                    cli(["google", "sync-stop"], account)
                wait_for(lambda: not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "owned sync pause")
                for owner in owners:
                    require(owner.wait(timeout=8) == 0, "Owned sync CLI did not finish after Pause")
                require(len(cli(["calendar", "--combined", *WINDOW])["events"]) == 2, "Combined active Calendar omitted sources")

                for account in accounts:
                    key = account["key"]
                    draft = cli(["draft", "create", "--request-id", key + "-create"], account)
                    routing = {"to_recipients": [{"name": "Récepteur 日本語", "email": key + "-to@example.test"}],
                               "cc_recipients": [{"name": "Copy", "email": "copy@example.test"}],
                               "bcc_recipients": [{"name": "Blind", "email": key + "-blind@example.test"}]}
                    body = "Retained human draft for " + key + ".\nCafé, 日本語 and 🌍 remain exact.\nSecond line."
                    html = "<p>Retained <strong>" + key + "</strong> — café 日本語 🌍.</p>"
                    signature = "Signature " + key + " — é\nHuman authored."
                    files = {}
                    for field, value in {"body": body, "html": html, "signature": signature,
                                         "recipients": json.dumps(routing, ensure_ascii=False)}.items():
                        files[field] = scratch / (key + "-" + field + ".txt")
                        files[field].write_text(value, encoding="utf-8", newline="\n")
                    tail = ["draft", "save", "--draft-id", draft["draft_id"], "--subject", "Archive " + key + " — 日本語",
                            "--body-file", str(files["body"]), "--html-file", str(files["html"]),
                            "--signature-file", str(files["signature"]), "--recipients-file", str(files["recipients"]),
                            "--expected-revision", str(draft["revision"]), "--request-id", key + "-save"]
                    draft = cli(tail, account)
                    require(draft["body"] == body and draft["body_html"] == html and draft["signature_text"] == signature
                            and all(draft[field] == value for field, value in routing.items()), "Saved human fields differ from inputs")
                    attachment_bytes = bytes((0, 1, 2, 255)) + ("archive-file/" + key + "/日本語\n").encode()
                    attachment_file = scratch / ("Pièce-" + key + "-日本語.bin")
                    attachment_file.write_bytes(attachment_bytes)
                    attached = cli(["draft", "attach", "--draft-id", draft["draft_id"], "--file", str(attachment_file),
                                    "--expected-revision", str(draft["revision"]), "--request-id", key + "-attach"], account)
                    attachment = attached["attachment"]
                    require(attachment["account"] == account["id"] and attachment["message_id"] is None
                            and attachment["name"] == attachment_file.name and attachment["mime_type"] == "application/octet-stream"
                            and attachment["size"] == len(attachment_bytes)
                            and attachment["sha256"] == hashlib.sha256(attachment_bytes).hexdigest(), "Standalone file import differs")
                    draft = attached["draft"]
                    require(draft["attachment_ids"] == [attachment["id"]], "Imported draft selection differs")
                    drafts[key] = {"draft_id": draft["draft_id"], "projection": authored_projection(draft),
                                   "attachment_id": attachment["id"], "attachment_bytes": attachment_bytes}
                    verify_draft(account, False)
                baseline = retained_content(profile)
                outputs["ledger"].write_text(json.dumps({**ledger, "retained_baseline": baseline}, indent=2) + "\n", encoding="utf-8")
                before_provider = fresh_fixture()["counters"]
                require(all(before_provider[key] == 0 for key in ("modify_completed", "draft_created", "draft_updated", "send_completed", "calendar_mutations")),
                        "Paused human draft work contacted the provider")

                preview = cli(["account", "remove-preview"], personal)
                require(preview["retained"] == {"messages": 2, "drafts": 1} and preview["remote_data_changed"] is False,
                        "Removal preview omitted retained work or claimed remote change")
                (profile / FAIL_CLEANUP).touch(exist_ok=False)
                first_revision = preview["lifecycle"]["revision"]
                failed = remove(personal, "personal-remove-failed", first_revision)
                require(failed["lifecycle"]["state"] == "removed" and failed["lifecycle"]["cleanup"]["state"] == "failed",
                        "Native cleanup failure was presented as active or complete")
                (profile / FAIL_CLEANUP).unlink()
                failed_counters = fresh_fixture()["account_fixture"]
                require(failed_counters["vault_remove_failed"] == 1 and personal["id"] in failed_counters["credential_accounts"],
                        "Failed native cleanup concealed its retained credential")
                require([row["id"] for row in rows()] == [workspace["id"]] and len(rows(True)) == 2,
                        "Removal did not separate active accounts from archives")
                require(len(cli(["calendar", "--combined", *WINDOW])["events"]) == 1
                        and len(cli(["calendar", *WINDOW], personal)["events"]) == 1, "Archive Calendar scope or combined exclusion differs")
                verify_draft(personal, True)
                thread = expected_message(personal, "vr0000", BASE_UNIX)["conversation_id"]
                mail = cli(["mail", "read", thread], personal)
                require(mail["id"] == thread and len(mail["messages"]) == 2, "Explicit archive mail read omitted members")
                cli(["draft", "create", "--request-id", "removed-create"], personal, {"account_removed"})
                call("draft_save", {"draft_id": drafts["personal"]["draft_id"], "body": "Refused new text",
                                    "expected_revision": drafts["personal"]["projection"]["revision"], "request_id": "removed-save"}, personal, {"account_removed"})
                cli(["mail", "review", thread, "--request-id", "removed-review"], personal, {"account_removed"})
                preserve("removed cached reads and refused mutations", baseline)
                retry_revision = failed["lifecycle"]["revision"]
                removed = remove(personal, "personal-remove-retry", retry_revision)
                require(removed["lifecycle"]["cleanup"]["state"] == "complete", "Explicit cleanup retry did not complete")
                counters = fresh_fixture()["account_fixture"]
                require(counters["vault_remove_attempts"] == 2 and personal["id"] not in counters["credential_accounts"],
                        "Cleanup retry did not prove local credential absence")
                historical = remove(personal, "personal-remove-failed", first_revision)
                require(historical["operation"] == failed["operation"] and historical["operation_is_current"] is False,
                        "Historical removal evidence changed")
                sign_in(personal, refused={"account_removed"})
                step(5, "personal")
                sign_in(refused={"account_removed"})
                stores_before = fresh_fixture()["account_fixture"]["vault_store_attempts"]
                step(6, "workspace")
                sign_in(personal, removed["lifecycle"]["revision"], {"identity_mismatch"})
                require(lifecycle(personal, "wrong-subject restore") == removed["lifecycle"]
                        and fresh_fixture()["account_fixture"]["vault_store_attempts"] == stores_before,
                        "Wrong-subject restore changed the archive or credential")

                (profile / HOLD_STORE).touch(exist_ok=False)
                step(7, "personal")
                tail = ["google", "restore", "--client-config", str(profile / "verification-google-client.json"),
                        "--expected-revision", str(removed["lifecycle"]["revision"])]
                cancelled = subprocess.Popen(command(tail, personal, True), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                children.append(cancelled)
                wait_for(lambda: fixture()["account_fixture"]["phase"] == "store_held_after_write"
                         and fixture()["account_fixture"]["active_vault_calls"] == 1, "restore held after native persistence")
                require(personal["id"] in fixture()["account_fixture"]["credential_accounts"], "Restore hold occurred before record mutation")
                cancelled.terminate()
                cancelled.communicate(timeout=5)
                (profile / HOLD_STORE).unlink()
                pending = wait_for(lambda: (life if (life := account_row(personal)["lifecycle"])["cleanup"]["state"] == "pending"
                                           and fixture()["account_fixture"]["active_vault_calls"] == 0 else None),
                                   "cancelled restore drain and durable pending cleanup")
                require(pending["state"] == "removed" and personal["id"] in fixture()["account_fixture"]["credential_accounts"],
                        "Cancelled stored restore falsely reported active or credential absence")
                ledger["lifecycle"].append({"label": "cancelled-after-store", "account": personal["id"], "value": pending,
                                             "owned_cli_exit_code": cancelled.returncode})
                cleaned = remove(personal, "personal-cancelled-restore-cleanup", pending["revision"])
                require(cleaned["lifecycle"]["cleanup"]["state"] == "complete", "Cancelled restore cleanup did not complete")
                step(8, "personal", renamed=True)
                restored = sign_in(personal, cleaned["lifecycle"]["revision"])
                require(restored["account"] == personal["id"], "Restoration changed immutable account identity")
                row = account_row(personal)
                require(row["email"] == "renamed-personal@example.test" and row["lifecycle"]["state"] == "active"
                        and set(row["google"]["granted_scopes"]) == FULL_SCOPES, "Same-subject renamed restore did not publish exact identity/grants")
                verify_draft(personal, False)
                require(not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "Restore automatically started sync")
                historical = remove(personal, "personal-remove-retry", retry_revision)
                require(historical["operation"] == removed["operation"] and historical["operation_is_current"] is False
                        and historical["lifecycle"]["state"] == "active", "Historical completed replay removed the restored account")
                require(fresh_fixture()["account_fixture"]["vault_remove_attempts"] == 3,
                        "Historical replay deleted the restored credential")
                preserve("same-subject restore and historical replay", baseline)
                for account in accounts:
                    final_remove = remove(account, account["key"] + "-final-remove", lifecycle(account, "before-final-remove")["revision"])
                    require(final_remove["lifecycle"]["cleanup"]["state"] == "complete", "Final local credential cleanup failed")
                    verify_draft(account, True)
                require(rows() == [] and len(rows(True)) == 2, "Last removal did not leave an empty active account list")
                require(cli(["calendar", "--combined", *WINDOW])["events"] == [], "Combined Calendar exposed removed accounts")
                require(cli(["calendar-sources"])["calendars"] == [], "Default Calendar catalog exposed removed accounts")
                for account in accounts:
                    require(len(cli(["calendar", *WINDOW], account)["events"]) == 1, "Exact archive Calendar source was lost")
                    page = cli(["mail", "list", "--folder", "all"], account)
                    require(len(page["conversations"]) == 1 and page["conversations"][0]["message_count"] == 2,
                            "Exact archived conversation disappeared")
                preserve("both removed and archive exports", baseline)
                settled = fresh_fixture()
                for key, value in before_provider.items():
                    if key not in ("vault_loads", "vault_stores"):
                        require(settled["counters"][key] == value, "Lifecycle/archive access triggered provider work: " + key)
                auth_counts = settled["account_fixture"]
                require(auth_counts["consumed_steps"] == list(range(1, 9)) and auth_counts["auth_attempts"] == 8
                        and auth_counts["callbacks_sent"] == 8 and auth_counts["callbacks_denied"] == 1
                        and auth_counts["token_exchanges"] == 7 and auth_counts["jwks_reads"] == 7
                        and auth_counts["vault_store_attempts"] == 5 and auth_counts["vault_store_completed"] == 5
                        and auth_counts["vault_remove_attempts"] == 5 and auth_counts["vault_remove_completed"] == 4
                        and auth_counts["vault_remove_failed"] == 1 and auth_counts["credential_accounts"] == []
                        and auth_counts["active_callbacks"] == 0 and auth_counts["active_vault_calls"] == 0,
                        "Actual OAuth/vault attempt counts differ or work survived its owner")
                watch.terminate()
                watch.wait(timeout=8)
                require(engine.wait(timeout=15) == 0 and not (profile / "engine.json").exists(),
                        "Final explicit owner did not drain and remove discovery")
                final = fixture()
                require(final["status"] == "complete" and not final["errors"] and not final["account_fixture"]["errors"]
                        and final["counters"] == settled["counters"] and final["account_fixture"] == auth_counts,
                        "Final authoritative fixture result changed or failed")
                before_refusal = profile_files(profile)
                for name, executable, tail in (
                    ("ordinary_refusal", programs["ordinary_engine"], ["--profile", str(profile), "--live-profile"]),
                    ("feature_refusal", programs["engine"], fixture_args),
                ):
                    with outputs[name].open("wb") as output:
                        refused = subprocess.run([str(executable), *tail], cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                                 stdout=output, stderr=subprocess.STDOUT, timeout=8)
                    require(refused.returncode == 1 and outputs[name].read_text(encoding="utf-8").strip()
                            == "fm-engine: Google verification profiles cannot be opened by an ordinary or restarted engine",
                            "Marked stopped profile did not produce an explicit isolation refusal")
                    require(profile_files(profile) == before_refusal and not (profile / "engine.json").exists(),
                            "Refused startup mutated the retained marked profile")
                preserve("refused ordinary and feature restart", baseline)
                result["provider_counters"] = final["counters"]
                result["account_fixture"] = auth_counts
                result["retained_content"] = baseline
                result["archive_count"] = 2
                result["draft_count"] = 2
                result["source_sha256"] = sha256(outputs["source"])
                require(source_inputs() == sources and all(sha256(path) == identities[name]["sha256"]
                                                          for name, path in programs.items()), "Tested source or executables changed")
                result["result"] = "passed"
            except Exception as error:
                result["result"] = "failed"
                result["error"] = str(error)
                raise
            finally:
                try:
                    try:
                        # Only fixture-created markers in this newly allocated profile.
                        for marker in (HOLD_STORE, FAIL_CLEANUP):
                            path = profile / marker
                            if path.is_file():
                                path.unlink()
                    finally:
                        stop_children(children)
                finally:
                    try:
                        if fixture_path.is_file():
                            outputs["fixture"].write_bytes(fixture_path.read_bytes())
                    finally:
                        outputs["ledger"].write_text(json.dumps(ledger, indent=2) + "\n", encoding="utf-8")
    except Exception as error:
        result["result"] = "failed"
        if "error" in result and result["error"] != str(error):
            result["cleanup_error"] = str(error)
        else:
            result["error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    result["outputs_sha256"] = {name: sha256(path) for name, path in outputs.items() if path.is_file()}
    report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True, help="Ordinary CLI for all local/provider commands")
    parser.add_argument("--auth-cli", type=Path, required=True, help="Isolated-feature CLI used only for explicit sign-in")
    parser.add_argument("--engine", type=Path, required=True, help="Isolated account-fixture engine")
    parser.add_argument("--ordinary-engine", type=Path, required=True, help="Ordinary engine for marked-profile refusal")
    parser.add_argument("--report", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
