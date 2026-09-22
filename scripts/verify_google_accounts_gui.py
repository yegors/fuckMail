#!/usr/bin/env python3
"""Native account lifecycle walkthrough with independent generated-source checks.

Uses one new isolated profile, actual GUI/CLI/engine processes and no real Google
account or credential vault. Functional evidence only; never a benchmark.
"""
import argparse
from contextlib import ExitStack, closing
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

from dev import package_environment
from verify_google_account_sources import retained_content, source_inputs, verify_initial_sources
from verify_google_calendar import BASE, BASE_UNIX, WINDOW
from verify_google_drafts import read_json
from verify_google_limits import LIMITS
from verify_google_navigation import expected_accounts, read_database, require
from verify_search_ingestion import sha256, stop_children


PROVIDER_COUNTER_EXCLUSIONS = ("vault_loads", "vault_stores")


def verify_restore_provider_quiet(baseline, final, gui_started_unix_ms, gui_finished_unix_ms):
    """Compare a fresh pre-Restore native observation with the drained engine."""
    require(isinstance(baseline, dict) and set(baseline) == {
        "updated_unix_ms", "captured_after_unix_ms", "counters"},
        "Native walkthrough omitted the exact pre-Restore provider observation")
    captured_after = baseline["captured_after_unix_ms"]
    updated = baseline["updated_unix_ms"]
    final_updated = final["updated_unix_ms"]
    require(all(type(value) is int and value > 0 for value in (
        gui_started_unix_ms, gui_finished_unix_ms, captured_after, updated, final_updated))
        and gui_started_unix_ms <= captured_after <= updated <= gui_finished_unix_ms
        and final_updated >= updated,
        "Pre-Restore provider observation was stale or outside the actual native run")
    before, after = baseline["counters"], final["counters"]
    require(isinstance(before, dict) and isinstance(after, dict) and set(before) == set(after)
            and set(PROVIDER_COUNTER_EXCLUSIONS) <= set(before)
            and all(type(value) is int and value >= 0 for counters in (before, after) for value in counters.values()),
            "Pre-Restore and final provider counter schemas differ or contain invalid values")
    require(before.get("http_started", 0) > 0
            and before["http_started"] == sum(before.get(key, 0) for key in (
                "http_completed", "http_cancelled", "http_rejected"))
            and before.get("raw_active") == 0 and before.get("raw_held_active") == 0,
            "Pre-Restore provider observation was not taken after acquired work drained")
    for key in before:
        if key not in PROVIDER_COUNTER_EXCLUSIONS:
            require(after[key] == before[key], "Restore or later archive work changed provider counter: " + key)


def native_expectation(profile, cli):
    value = {"version": 1, "calendar_date": BASE.date().isoformat(),
             "cli_path": str(cli), "limits": LIMITS, "accounts": []}
    for account in expected_accounts():
        key = account["key"]
        value["accounts"].append({"account": account["id"], "identity": key, "email": account["email"],
            "renamed_email": "renamed-" + account["email"],
            "draft": {"subject": "Native retained " + key + " — 日本語",
                      "body": "Saved human text for " + key + ".\nUnicode café 日本語.",
                      "dirty_body": "Unsaved human changes for " + key + ".\nKeep this exact buffer — 日本語.",
                      "recipient": "recipient-" + key + "@example.test",
                      "pending_recipient": "unfinished-" + key + "-address"}})
    return value


def validate_native(observed, expectation, profile):
    require(observed.get("version") == 1 and observed.get("result") == "passed" and not observed.get("error"),
            "Native account walkthrough did not pass")
    rows = observed.get("accounts", [])
    require([row.get("account") for row in rows] == [row["account"] for row in expectation["accounts"]],
            "Native walkthrough omitted or crossed an account")
    require(observed.get("browser_handoffs") == 3 and observed.get("fixture_step_count") == 3,
            "Expected two explicit native Adds and one Restore")
    require(observed.get("active_account_count") == 0 and observed.get("archive_account_count") == 2,
            "Native last-account removal did not leave two discoverable archives")
    receipts = {}
    for expected, row in zip(expectation["accounts"], rows, strict=True):
        require(row["add_lifecycle"] == {"state": "active", "revision": 1, "cleanup": None},
                "Native Add did not create one fresh active identity")
        require(row["acquired"]["message_count"] == 2 and row["acquired"]["event_ids"] == ["ci0000"],
                "Native Settings sync did not acquire both mail and Calendar")
        archive = row["archive"]
        require(archive["unread_before"] == archive["unread_after"], "Opening archived mail marked it reviewed")
        require(archive["draft"]["account"] == expected["account"] and archive["draft"]["account_archived"] is True
                and archive["draft"]["read_only"] is True, "Native archive draft is not account-scoped read-only data")
        for receipt in row["removals"]:
            operation = receipt["operation"]
            key = (expected["account"], receipt["request_id"])
            require(key not in receipts and operation["account"] == key[0] and operation["request_id"] == key[1]
                    and operation["kind"] == "account_remove" and operation["retention"] == "archive",
                    "Native removal lost its exact retained operation identity")
            receipts[key] = operation
    external = observed["external_removal"]
    external_key = (external["account"], external["request_id"])
    require(external_key[0] == expectation["accounts"][1]["account"], "Independent CLI removed the wrong native account")
    if external_key in receipts:
        require(receipts[external_key] == external["operation"], "CLI/native removal receipts disagree")
    else:
        receipts[external_key] = external["operation"]
    require(len(receipts) == 4, "Expected failed cleanup, fresh retry and both final removals")
    personal, workspace = expectation["accounts"]
    buffer = observed["retained_buffer"]
    require(buffer["account"] == workspace["account"] and buffer["draft"]["body"] == workspace["draft"]["dirty_body"],
            "External removal lost the independently authored unsaved body")
    require(buffer["recipient_inputs"]["bcc_recipients"] == workspace["draft"]["pending_recipient"],
            "External removal lost the exact unfinished recipient input")
    for field in ("keep_here_preserved", "navigation_guarded", "close_guarded", "explicit_discard_completed"):
        require(observed["disposition"][field] is True, "Native unsaved buffer disposition failed: " + field)
    with closing(read_database(profile)) as db:
        life = db.execute("SELECT id,email,lifecycle_state,lifecycle_revision,cleanup_json FROM accounts ORDER BY rowid").fetchall()
        require([row[0] for row in life] == [personal["account"], workspace["account"]], "Unexpected native stored account identity")
        for row, expected in zip(life, (personal, workspace), strict=True):
            require(row[1] == (expected["renamed_email"] if expected["identity"] == "personal" else expected["email"])
                    and row[2] == "removed" and row[3] > 1 and json.loads(row[4])["state"] == "complete",
                    "Native removal/restore did not persist its truthful final lifecycle")
        frozen_fields = ("from_name", "identity_revision", "signature_text", "body_html", "source_message_id",
                         "source_hash", "conversation_id", "mode", "revision", "created_at", "updated_at", "legacy_reply")
        drafts = db.execute("SELECT account,draft_id,body,routing_json,discarded,origin,attachments_json,"
                            + ",".join(frozen_fields) + " FROM drafts").fetchall()
        require(len(drafts) == 2, "Native lifecycle lost or duplicated an authored draft")
        by_account = {row[0]: row for row in drafts}
        for expected, observed_row in zip(expectation["accounts"], rows, strict=True):
            row = by_account[expected["account"]]
            routing = json.loads(row[3])
            before_removal = observed_row["saved_before_remove"] if expected["identity"] == "personal" else observed_row["saved_draft"]
            wanted_body = expected["draft"]["dirty_body"] if expected["identity"] == "personal" else expected["draft"]["body"]
            require(row[1] == observed_row["saved_draft"]["draft_id"] and row[2] == wanted_body
                    and routing["subject"] == expected["draft"]["subject"] and row[4] == 0 and row[5] == "local"
                    and json.loads(row[6]) == [], "Saved native composition differs from independent human input")
            require(before_removal["account"] == expected["account"] and before_removal["draft_id"] == row[1]
                    and before_removal["body"] == wanted_body and routing["from"] == expected["email"],
                    "Native restore changed an existing draft's frozen sender or saved identity")
            require(routing == {key: before_removal[key] for key in routing}
                    and all(value == before_removal[field] for field, value in zip(frozen_fields, row[7:], strict=True)),
                    "Native lifecycle altered frozen routing, authored metadata or source fields")
            require([item["email"] for item in routing["to_recipients"]] == [expected["draft"]["recipient"]]
                    and routing["cc_recipients"] == [] and routing["bcc_recipients"] == [],
                    "Native removal changed recipients or saved incomplete unsent recipient text")
        operations = db.execute("SELECT account,request_id,result_json FROM operations "
                                "WHERE json_extract(result_json,'$.kind')='account_remove'").fetchall()
        require({(row[0], row[1]): json.loads(row[2]) for row in operations} == receipts,
                "Stored removal operations differ from the actual native/CLI receipts")
        require(db.execute("SELECT count(*) FROM capture_operations").fetchone()[0] == 0
                and db.execute("SELECT count(*) FROM calendar_write_operations").fetchone()[0] == 0,
                "Account lifecycle created an unsolicited Send or Calendar operation")
    return {"removal_operations": len(receipts), "retained_drafts": len(drafts), "accounts": len(rows)}


def run(args):
    report = args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    outputs = {key: report.with_suffix(suffix) for key, suffix in {
        "engine": ".engine.log", "watch": ".watch.log", "gui": ".gui.log", "fixture": ".fixture.json",
        "gui_report": ".gui.json", "failure_png": ".gui.failure.png",
        "expectation": ".expectation.json", "sources": ".source.json"}.items()}
    require(not report.exists() and not any(path.exists() for path in outputs.values()), "Use a new numbered evidence path")
    result = {"version": 1, "result": "failed", "scenario": "isolated-google-account-lifecycle-native",
              "base_unix_secs": BASE_UNIX, "limits": LIMITS, "commands": [],
              "limitations": ["Generated in-memory provider/vault; no real account, token, invitation or Send.",
                              "Programmatic native controls and independent readback; no physical-input or performance acceptance.",
                              "Development executables only; no package or populated isolated-engine restart acceptance."]}
    children = []
    started = time.monotonic()
    with report.open("x", encoding="utf-8") as destination:
        try:
            programs = {key: getattr(args, key).resolve(strict=True) for key in ("engine", "cli", "gui")}
            result["executables"] = {key: {"path": str(path), "sha256": sha256(path)} for key, path in programs.items()}
            sources = source_inputs(native=True)
            outputs["sources"].write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
            with tempfile.TemporaryDirectory(prefix="fuckmail-accounts-native-") as temporary, ExitStack() as logs:
                scratch = Path(temporary).resolve()
                profile = scratch / "empty-live-profile"
                fixture_path = profile / "verification-google-report.json"
                streams = {key: logs.enter_context(outputs[key].open("wb")) for key in ("engine", "watch", "gui")}
                env = package_environment()
                env.pop("QT_QUICK_BACKEND", None)
                env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
                env["QT_FORCE_STDERR_LOGGING"] = "1"
                env["QSG_INFO"] = "1"
                env["FM_ENGINE_PATH"] = str(programs["engine"])
                if args.qt_root:
                    env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
                engine = subprocess.Popen([str(programs["engine"]), "--profile", str(profile), "--live-profile",
                    "--verification-google", "--verification-google-accounts", "--verification-google-accounts-base", str(BASE_UNIX)],
                    cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=streams["engine"], stderr=subprocess.STDOUT)
                children.append(engine)

                def wait_for(predicate, label, timeout=30):
                    deadline = time.monotonic() + timeout
                    while time.monotonic() < deadline:
                        require(engine.poll() is None, "Engine exited during " + label)
                        value = predicate()
                        if value:
                            return value
                        time.sleep(.1)
                    raise RuntimeError("Timed out: " + label)

                def command(tail, account=None):
                    prefix = [str(programs["cli"]), "--profile", str(profile), "--engine", str(programs["engine"]), "--json"]
                    return prefix + (["--account", account] if account else []) + tail

                def cli(tail, account=None):
                    require(engine.poll() is None, "Do not reopen a populated isolated profile")
                    reply = subprocess.run(command(tail, account), cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=45)
                    data = json.loads(reply.stdout)
                    result["commands"].append({"command": tail[:2], "account": account, "exit_code": reply.returncode,
                                               "error": data.get("error", {}).get("code")})
                    require(reply.returncode == 0 and data.get("ok") is True, "Offline CLI read failed: " + str(data.get("error")))
                    return data["result"]

                try:
                    wait_for(lambda: fixture_path.is_file() and (profile / "engine.json").is_file(), "new fixture startup")
                    require(read_json(profile / "engine.json")["pid"] == engine.pid, "Wrong isolated discovery owner")
                    initial = read_json(fixture_path)
                    fixture = initial["account_fixture"]
                    require(initial["transport"] == "in_memory_only" and initial["native_vault"] is False
                            and fixture["enabled"] is True and fixture["mode"] == "accounts-v1"
                            and fixture["initial_bound_accounts"] == 0 and not fixture["credential_accounts"]
                            and initial["counters"]["http_started"] == 0, "Account fixture did not start isolated and empty")
                    watch = subprocess.Popen(command(["watch", "--seconds", "900"]), cwd=scratch, env=env,
                        stdin=subprocess.DEVNULL, stdout=streams["watch"], stderr=subprocess.STDOUT)
                    children.append(watch)
                    require(cli(["accounts", "--include-removed"])["accounts"] == [], "Native Add was bypassed by pre-bound accounts")
                    preferences = cli(["preferences", "show"])
                    preference_file = scratch / "calendar-display.json"
                    preference_file.write_text(json.dumps({"calendar_display_time_zone": "UTC"}), encoding="utf-8")
                    result["display_zone_setup"] = cli(["preferences", "save", "--preferences-file", str(preference_file),
                        "--expected-revision", str(preferences["revision"]), "--request-id", "native-accounts-display-utc"])
                    require(cli(["preferences", "show"])["preferences"]["calendar_display_time_zone"] == "UTC",
                            "Explicit fixture Calendar display zone did not persist")
                    expected = native_expectation(profile, programs["cli"])
                    outputs["expectation"].write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
                    result["gui_started_unix_ms"] = time.time_ns() // 1_000_000
                    gui = subprocess.Popen([str(programs["gui"]), "--profile", str(profile), "--engine", str(programs["engine"]),
                        "--account-lifecycle-self-test", str(outputs["gui_report"]),
                        "--account-lifecycle-expectation", str(outputs["expectation"])],
                        cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=streams["gui"], stderr=subprocess.STDOUT)
                    children.append(gui)
                    result["gui_exit_code"] = gui.wait(timeout=180)
                    result["gui_finished_unix_ms"] = time.time_ns() // 1_000_000
                    require(result["gui_exit_code"] == 0, "Native lifecycle failed; inspect retained native log/report")
                    observed = read_json(outputs["gui_report"])
                    require(observed.get("gui_pid") == gui.pid, "Native report belongs to another process")
                    result["native"] = validate_native(observed, expected, profile)
                    result["sources_checked"] = verify_initial_sources(profile)
                    frozen = retained_content(profile)
                    cutoff = time.time_ns() // 1_000_000
                    wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "post-native provider baseline")
                    before_queries = read_json(fixture_path)
                    require(cli(["accounts"])["accounts"] == [], "Removed native accounts remain in active tabs")
                    require(len(cli(["accounts", "--include-removed"])["accounts"]) == 2, "Native archives cannot be discovered")
                    require(cli(["calendar", "--combined", *WINDOW])["events"] == [], "Removed events remain in combined Calendar")
                    require(not any(row["active"] for row in cli(["google", "sync-status"])["accounts"]), "Native sync owner leaked")
                    for account in expected_accounts():
                        require(len(cli(["calendar", *WINDOW], account["id"])["events"]) == 1,
                                "Explicit cached archive Calendar lost its source")
                        cli(["mail", "list"], account["id"])
                        cli(["draft", "list"], account["id"])
                    require(retained_content(profile) == frozen, "Offline archive queries changed retained content or receipts")
                    result["retained_content"] = frozen
                    cutoff = time.time_ns() // 1_000_000
                    wait_for(lambda: read_json(fixture_path)["updated_unix_ms"] >= cutoff, "final fixture counters")
                    final = read_json(fixture_path)
                    require(final["counters"] == before_queries["counters"]
                            and final["account_fixture"] == before_queries["account_fixture"],
                            "Offline archive queries started provider or credential work")
                    result["offline_queries_preserved_content_and_provider"] = True
                    account_fixture = final["account_fixture"]
                    require(not final["errors"] and not account_fixture["errors"] and not account_fixture["credential_accounts"]
                            and account_fixture["active_callbacks"] == 0 and account_fixture["active_vault_calls"] == 0,
                            "Native lifecycle left credentials/work active or reported a fixture error")
                    for key, expected_count in {"auth_attempts": 3, "callbacks_sent": 3, "callbacks_denied": 0,
                        "token_exchanges": 3, "vault_store_attempts": 3, "vault_store_completed": 3,
                        "vault_remove_attempts": 4, "vault_remove_completed": 3, "vault_remove_failed": 1}.items():
                        require(account_fixture[key] == expected_count, "Actual native lifecycle counter differs: " + key)
                    for key in ("modify_completed", "draft_created", "draft_updated", "draft_deleted", "send_completed", "calendar_mutations"):
                        require(final["counters"][key] == 0, "Native account work caused an unsolicited provider write: " + key)
                    result["account_fixture"] = account_fixture
                    result["provider_counters"] = final["counters"]
                    watch.terminate()
                    watch.wait(timeout=10)
                    require(engine.wait(timeout=20) == 0 and not (profile / "engine.json").exists(), "Final owner did not release the engine")
                    completed_fixture = read_json(fixture_path)
                    require(completed_fixture["status"] == "complete" and not completed_fixture["errors"]
                            and not completed_fixture["account_fixture"]["errors"]
                            and completed_fixture["counters"] == final["counters"]
                            and completed_fixture["account_fixture"] == account_fixture,
                            "Final engine drain changed provider/credential counters or reported an error")
                    provider_before_restore = observed.get("provider_before_restore")
                    verify_restore_provider_quiet(provider_before_restore, completed_fixture,
                        result["gui_started_unix_ms"], result["gui_finished_unix_ms"])
                    result["provider_before_restore"] = provider_before_restore
                    result["provider_after_restore"] = {"updated_unix_ms": completed_fixture["updated_unix_ms"],
                                                        "counters": completed_fixture["counters"]}
                    result["provider_counter_exclusions"] = list(PROVIDER_COUNTER_EXCLUSIONS)
                    result["final_owner_drained_engine"] = True
                    require(source_inputs(native=True) == sources, "Native verification inputs changed during the run")
                    require(all(sha256(path) == result["executables"][key]["sha256"] for key, path in programs.items()),
                            "A tested executable changed during the run")
                    result["result"] = "passed"
                finally:
                    try:
                        stop_children(children)
                    finally:
                        if fixture_path.is_file():
                            outputs["fixture"].write_bytes(fixture_path.read_bytes())
        except Exception as error:
            result["result"] = "failed"
            result["error"] = str(error)
        result["elapsed_seconds"] = time.monotonic() - started
        result["artifacts"] = {key: {"name": path.name, "sha256": sha256(path)} for key, path in outputs.items() if path.is_file()}
        destination.write(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "native", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("engine", "cli", "gui", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--qt-root", type=Path)
    raise SystemExit(run(parser.parse_args()))
