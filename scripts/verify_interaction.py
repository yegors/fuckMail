#!/usr/bin/env python3
"""Measure native navigation/resize/Calendar or capture visuals on owned disposable synthetic profiles."""
import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import sqlite3
import subprocess
import tempfile
import time
import uuid

from dev import package_environment
from verify_search_ingestion import clone_seed, sha256, stop_children


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def snapshot(profile):
    result = {}
    with closing(sqlite3.connect((profile / "store.sqlite3").as_uri() + "?mode=ro", uri=True)) as db:
        # Presentation and read receipts intentionally change. Authorship,
        # retained source, other flags and local captures must not.
        for table in ("drafts", "capture_operations", "attachments", "messages"):
            columns = [r[1] for r in db.execute(f'PRAGMA table_info("{table}")')]
            require(columns, f"Missing expected table: {table}")
            selected = [c for c in columns if table != "messages" or c not in ("unread", "revision")]
            names = ",".join('"' + c.replace('"', '""') + '"' for c in selected)
            digest = hashlib.sha256()
            count = 0
            for row in db.execute(f'SELECT {names} FROM "{table}" ORDER BY rowid'):
                digest.update(json.dumps(row, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
                digest.update(b"\n")
                count += 1
            result[table] = {"rows": count, "columns": selected, "sha256": digest.hexdigest()}
    return result


def verify_navigation(profile, data):
    expected = {"mailbox_click", "mailbox_tab", "mailbox_shift_tab", "conversation_click"}
    classes = data.get("classes", [])
    checked = 0
    with closing(sqlite3.connect((profile / "store.sqlite3").as_uri() + "?mode=ro", uri=True)) as db:
        counts = dict(db.execute("SELECT account,COUNT(DISTINCT conversation_id) FROM messages GROUP BY account"))
        large = {account: count for account, count in counts.items() if count >= 10000}
        if large:
            expected.add("large_mailbox_conversation_click")
            account = data.get("large_mailbox_account")
            require(account in large and large[account] == max(large.values())
                    and data.get("large_mailbox_conversation_count") == large[account],
                    "The additional reader route must use the independently verified largest mailbox")
        require({item.get("class") for item in classes} == expected and len(classes) == len(expected),
                "The report must include all four navigation routes plus the large-mailbox reader route when available")
        for group in classes:
            samples = group.get("samples", [])
            require(group["observations"] == 100 and len(samples) == 100, "Each navigation route needs 100 observations")
            for sample in samples:
                account, selected = sample["account"], sample["selected_id"]
                if group["class"] == "large_mailbox_conversation_click":
                    require(account == data["large_mailbox_account"], "The large reader route left the required mailbox")
                require(account == sample["target_account"], "A sample painted the wrong destination mailbox")
                if sample.get("target_selected_id"):
                    require(selected == sample["target_selected_id"], "A sample painted the wrong selected conversation")
                rows, visible = sample["row_ids"], sample["visible_row_ids"]
                require(rows and len(rows) == len(set(rows)) and visible and set(visible) <= set(rows),
                        "Visible list identities do not match the measured model")
                placeholders = ",".join("?" for _ in rows)
                actual_rows = {r[0] for r in db.execute(
                    f"SELECT id FROM conversations WHERE account=? AND id IN ({placeholders})", [account, *rows])}
                require(actual_rows == set(rows), "A painted row is outside the expected mailbox")
                members = {r[0]: r[1] for r in db.execute(
                    "SELECT id,body FROM messages WHERE account=? AND conversation_id=?", (account, selected))}
                require(members and set(sample["member_ids"]) == set(members), "Reader members differ from stored conversation")
                require(sample["visible_member_ids"] and set(sample["visible_member_ids"]) <= set(members),
                        "Visible reader member is outside the expected conversation")
                message = sample["message_id"]
                require(message in sample["visible_member_ids"], "The measured body is not a visible member")
                require(hashlib.sha256(members[message].encode("utf-8")).hexdigest() == sample["body_sha256"],
                        "Painted message body differs from retained source text")
                if sample.get("body_oracle") == "rich_document_unquoted_text":
                    expected_text = "\n".join(line for line in members[message].replace("\r\n", "\n").replace("\r", "\n").split("\n")
                                              if not line.lstrip().startswith(">"))
                    require(re.sub(r"\s", "", sample["rendered_body_text"]) == re.sub(r"\s", "", expected_text),
                            "Actual rich-text document differs from independently retained message text")
                checked += 1
    return {"samples_checked": checked, "account_rows_members_and_body_hashes": "passed"}


def verify_calendar(profile, data):
    expected = {"view_switch", "month_day_click", "week_step", "month_step", "workspace_switch"}
    classes = data.get("classes", [])
    require({item.get("class") for item in classes} == expected and len(classes) == len(expected),
            "The report must include all five Calendar routes")
    checked = painted = 0
    with closing(sqlite3.connect((profile / "store.sqlite3").as_uri() + "?mode=ro", uri=True)) as db:
        sources = set(db.execute("SELECT account,calendar_id,event_id FROM calendar_sources"))
        for group in classes:
            samples = group.get("samples", [])
            require(group["observations"] == 100 and len(samples) == 100, "Each Calendar route needs 100 observations")
            for sample in samples:
                require(sample["view"] == sample["target_view"] and sample["date"] == sample["target_date"],
                        "A sample painted the wrong Calendar view or date")
                for identity in sample["identities"]:
                    if identity["event_id"]:
                        require((identity["account"], identity["calendar_id"], identity["event_id"]) in sources,
                                "A painted event is outside the stored calendar sources")
                        painted += 1
                checked += 1
    require(painted, "The Calendar routes painted no stored events")
    return {"samples_checked": checked, "painted_events_checked": painted, "view_date_and_event_identities": "passed"}


def run(args):
    visual = args.mode == "visual"
    require(visual or (args.width == 1440 and args.height == 900 and args.text_size == "default" and not args.draft_body and not args.conversation),
            "Width, height, text size and draft-body options apply only to visual mode")
    if visual:
        require(1000 <= args.width <= 3840 and 650 <= args.height <= 2160,
                "Visual dimensions must be within the native screenshot bounds: 1000–3840 by 650–2160")
    report = args.report.resolve()
    output = report.with_suffix(".gui.png" if visual else ".gui.json")
    gui_log = report.with_suffix(".gui.log")
    engine_log = report.with_suffix(".engine.log")
    require(not any(p.exists() for p in (report, output, gui_log, engine_log)), "Choose new evidence paths")
    report.parent.mkdir(parents=True, exist_ok=True)
    result = {"result": "failed", "mode": args.mode, "theme": args.theme,
              "started_unix_ms": time.time_ns() / 1000000, "host": platform.platform(),
              "outputs": {"image" if visual else "gui_report": str(output), "gui_log": str(gui_log), "engine_log": str(engine_log)}}
    if visual:
        result["visual_setup"] = {"width_logical": args.width, "height_logical": args.height,
                                  "text_size": args.text_size, "draft_body": args.draft_body,
                                  "workspace": "mail", "reply_expanded": True}
        result["visual_setup"]["reply_footer"] = args.reply_footer
        result["visual_setup"]["conversation"] = args.conversation or "initial mailbox selection"
    started = time.monotonic()
    children = []
    try:
        bin_dir = args.bin.resolve(strict=True)
        suffix = ".exe" if platform.system() == "Windows" else ""
        programs = {name: bin_dir / (name + suffix) for name in ("fm", "fm-engine", "fuckMail")}
        require(all(p.is_file() for p in programs.values()), "Three native sibling executables are required")
        result["executables_sha256"] = {name: sha256(p) for name, p in programs.items()}
        manifest = next((p / "build-manifest.json" for p in (bin_dir, *list(bin_dir.parents)[:4])
                         if (p / "build-manifest.json").is_file()), None)
        result["package_manifest"] = json.loads(manifest.read_text()) if manifest else None
        env = package_environment()
        env.pop("QT_QUICK_BACKEND", None)
        env.pop("QT_D3D_NO_VBLANK_THREAD", None)
        env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
        env["QSG_INFO"] = "1"
        if args.qt_root:
            require(manifest is None, "Do not add SDK paths to packaged measurements")
            env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
        result["environment"] = {key: env.get(key) for key in ("QT_QPA_PLATFORM", "QT_QUICK_BACKEND", "PATH")}
        with tempfile.TemporaryDirectory(prefix="fuckmail-interaction-") as temporary:
            scratch = Path(temporary).resolve()
            profile = scratch / "synthetic-profile"
            if args.seed:
                seed = args.seed.resolve(strict=True)
                result["source_seed"] = str(seed)
                result["source_seed_sha256"] = sha256(seed / "store.sqlite3")
                result["seed_message_count"] = clone_seed(seed, profile)
            try:
                with engine_log.open("wb") as engine_stream, gui_log.open("wb") as gui_stream, (scratch / "preparation.log").open("wb") as setup_stream:
                    engine = subprocess.Popen([str(programs["fm-engine"]), "--profile", str(profile), "--demo-profile"], cwd=scratch,
                                              env=env, stdin=subprocess.DEVNULL, stdout=engine_stream, stderr=subprocess.STDOUT)
                    children.append(engine)
                    deadline = time.monotonic() + 45
                    while not (profile / "engine.json").exists():
                        require(engine.poll() is None and time.monotonic() < deadline, "Owned engine startup failed")
                        time.sleep(.05)
                    require(json.loads((profile / "engine.json").read_text())["pid"] == engine.pid, "Unexpected engine ownership")
                    watch = subprocess.Popen([str(programs["fm"]), "--profile", str(profile), "--json", "watch", "--seconds", "720"],
                                             cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=setup_stream, stderr=subprocess.STDOUT)
                    children.append(watch)

                    def call(method, params=None, account=None):
                        params_path = scratch / "params.json"
                        params_path.write_text(json.dumps(params or {}, ensure_ascii=False), encoding="utf-8")
                        command = [str(programs["fm"]), "--profile", str(profile), "--json"]
                        if account:
                            command += ["--account", account]
                        command += ["call", method, "--params-file", str(params_path)]
                        completed = subprocess.run(command, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", timeout=30)
                        require(completed.returncode == 0, f"Setup {method} failed: {completed.stderr[-800:]}")
                        reply = json.loads(completed.stdout)
                        require(reply.get("ok") is True, f"Setup method failed: {method}")
                        return reply["result"]

                    accounts = call("accounts")["accounts"]
                    preferences = call("preferences_get")
                    call("preferences_save", {"request_id": str(uuid.uuid4()), "expected_revision": preferences["revision"],
                                              "preferences": {"theme": args.theme, "startup_view": "mail", "last_workspace": "mail",
                                                              "text_size": args.text_size if visual else "default", "timeline_position": "footer"}})
                    for account in accounts:
                        view = call("view_get", account=account["id"])
                        selected = args.conversation if visual and account == accounts[0] else ""
                        if selected:
                            source = call("conversation", {"id": selected}, account["id"])
                            require(source["id"] == selected and source["messages"], "Visual selection needs a real retained conversation")
                        call("view_save", {"request_id": str(uuid.uuid4()), "expected_revision": view["revision"],
                                           "view": {"folder": "inbox", "query": "", "category": "", "filters": {}, "selected": selected,
                                                    "selectedDraft": "", "readerY": 0, "listY": 0, "replyOpen": False}}, account["id"])
                    if visual and args.draft_body:
                        require(accounts, "A visual draft needs a synthetic account")
                        account = accounts[0]["id"]
                        rows = call("conversations", {"folder": "inbox", "grouped": True, "limit": 1}, account)["conversations"]
                        require(rows, "A visual draft needs a conversation in the initial mailbox")
                        conversation = args.conversation or rows[0]["id"]
                        draft = call("draft_get", {"conversation_id": conversation}, account)
                        body = "Synthetic screenshot draft.\nThis local fixture checks button contrast and composer layout."
                        require(draft.get("source_message_id"), "The synthetic reply has no frozen source member")
                        saved = call("draft_save", {"conversation_id": conversation, "body": body, "body_html": None,
                                                    "source_message_id": draft["source_message_id"],
                                                    "expected_revision": draft["revision"], "request_id": str(uuid.uuid4())}, account)
                        require(saved["body"] == body and saved["source_message_id"] == draft["source_message_id"],
                                "The synthetic screenshot draft did not retain its body and source")
                        result["visual_setup"]["draft"] = {"account": account, "conversation_id": conversation,
                                                           "draft_id": saved["draft_id"], "revision": saved["revision"],
                                                           "body_sha256": hashlib.sha256(body.encode("utf-8")).hexdigest()}
                    before = snapshot(profile)
                    result["before"] = before
                    gui_command = [str(programs["fuckMail"]), "--profile", str(profile), "--fixture-clock"]
                    if visual:
                        gui_command += ["--screenshot", str(output), "--screenshot-reply", "--smoke",
                                        "--screenshot-width", str(args.width), "--screenshot-height", str(args.height)]
                        if args.reply_footer:
                            gui_command.append("--screenshot-reply-bottom")
                    else:
                        gui_command += ["--benchmark-" + args.mode + "-output", str(output)]
                    gui = subprocess.Popen(gui_command,
                                           cwd=scratch, env=env, stdin=subprocess.DEVNULL, stdout=gui_stream, stderr=subprocess.STDOUT)
                    children.append(gui)
                    deadline = time.monotonic() + 50
                    if visual:
                        while not output.is_file():
                            require(gui.poll() is None and time.monotonic() < deadline, "GUI screenshot/handoff failed; inspect its log")
                            time.sleep(.02)
                    else:
                        marker = {"navigation": b"Native interaction benchmark prepared", "resize": b"Native resize benchmark prepared",
                                  "calendar": b"Native calendar benchmark prepared"}[args.mode]
                        while marker not in gui_log.read_bytes():
                            require(gui.poll() is None and time.monotonic() < deadline, "GUI preparation/handoff failed; inspect its log")
                            time.sleep(.02)
                    watch.terminate()
                    watch.wait(timeout=5)
                    result["gui_exit_code"] = gui.wait(timeout=660)
                    result["engine_exit_code"] = engine.wait(timeout=15)
                    require(result["gui_exit_code"] in ((0,) if visual else (0, 9)) and result["engine_exit_code"] == 0, "Native measurement or final-owner shutdown failed")
                require(not (profile / "engine.json").exists(), "Engine discovery remains after final owner")
                result["after"] = snapshot(profile)
                require(before == result["after"], "Navigation changed source, flags other than unread, authorship or captures")
                if visual:
                    with output.open("rb") as image_stream:
                        header = image_stream.read(24)
                    require(len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n" and header[12:16] == b"IHDR",
                            "The native screenshot is not a PNG image")
                    width, height = int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
                    require(width > 0 and height > 0, "The native screenshot has empty pixel dimensions")
                    result["image_pixels"] = {"width": width, "height": height}
                    result["measurement_complete"] = True
                    result["visual_review"] = "required: inspect image for layout, contrast, clipping and control placement"
                    result["result"] = "measurement_complete"
                else:
                    data = json.loads(output.read_text(encoding="utf-8"))
                    require(not data.get("error"), "Native measurement reported: " + str(data.get("error")))
                if args.mode == "navigation":
                    result["identity_verification"] = verify_navigation(profile, data)
                    require(type(data.get("budget_passed")) is bool, "Native navigation budget result is missing")
                    require(data["budget_passed"] == (result["gui_exit_code"] == 0), "Exit code and navigation budget disagree")
                    result["classes"] = [{k: group[k] for k in ("class", "observations", "p95_ms", "max_ms")}
                                         for group in data["classes"]]
                elif args.mode == "calendar":
                    result["identity_verification"] = verify_calendar(profile, data)
                    require(type(data.get("budget_passed")) is bool, "Native Calendar budget result is missing")
                    require(data["budget_passed"] == (result["gui_exit_code"] == 0), "Exit code and Calendar budget disagree")
                    result["classes"] = [{k: group[k] for k in ("class", "observations", "p50_ms", "p95_ms", "max_ms")}
                                         for group in data["classes"]]
                elif args.mode == "resize":
                    require(data.get("measurement_complete") is True and len(data.get("surfaces", [])) == 2,
                            "Both native resize traces must complete")
                    result["visual_stability"] = "requires review of separate image and geometry evidence"
                if not visual:
                    result["result"] = "measured" if result["gui_exit_code"] == 9 else "passed"
            finally:
                stop_children(children)
            if args.seed:
                require(sha256(seed / "store.sqlite3") == result["source_seed_sha256"], "The original seed changed")
    except Exception as error:
        result["result"] = "failed"
        result["error"] = str(error)
    finally:
        stop_children(children)
        result["elapsed_seconds"] = round(time.monotonic() - started, 3)
        result["verifier_sha256"] = sha256(Path(__file__))
        result["output_sha256"] = {name: sha256(Path(path)) for name, path in result["outputs"].items() if Path(path).is_file()}
        report.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({k: result[k] for k in ("result", "elapsed_seconds", "error", "gui_exit_code") if k in result}, indent=2))
    return 0 if result["result"] in ("passed", "measurement_complete") else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--mode", choices=("navigation", "resize", "calendar", "visual"), required=True)
    parser.add_argument("--theme", choices=("graphite", "paper", "harbour"), default="graphite")
    parser.add_argument("--width", type=int, default=1440, help="Visual mode: logical window width")
    parser.add_argument("--height", type=int, default=900, help="Visual mode: logical window height")
    parser.add_argument("--text-size", choices=("small", "default", "large"), default="default", help="Visual mode: saved text size")
    parser.add_argument("--draft-body", action="store_true", help="Visual mode: save a fixed synthetic reply before capture")
    parser.add_argument("--reply-footer", action="store_true", help="Visual mode: scroll the expanded reply to its action footer")
    parser.add_argument("--conversation", default="", help="Visual mode: restore this existing conversation from the first synthetic account")
    parser.add_argument("--seed", type=Path)
    parser.add_argument("--qt-root", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
