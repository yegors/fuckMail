#!/usr/bin/env python3
"""Exercise two native development packages against one disposable synthetic profile."""
import argparse
import hashlib
import json
import platform
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

from dev import package_environment


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def contains_fields(actual, expected):
    """Protocol 1 may add query fields; existing field values remain stable."""
    if isinstance(expected, dict):
        return isinstance(actual, dict) and all(key in actual and contains_fields(actual[key], value)
                                                 for key, value in expected.items())
    if isinstance(expected, list):
        return isinstance(actual, list) and len(actual) == len(expected) and all(
            contains_fields(a, e) for a, e in zip(actual, expected))
    return actual == expected


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def package(bin_dir):
    bin_dir = bin_dir.resolve(strict=True)
    suffix = ".exe" if platform.system() == "Windows" else ""
    files = {name: bin_dir / (name + suffix) for name in ("fm", "fm-engine", "fuckmail-gui")}
    require(all(path.is_file() for path in files.values()), "Each package needs GUI, CLI and engine siblings")
    manifest = next((parent / "build-manifest.json" for parent in
                     (bin_dir, *list(bin_dir.parents)[:4]) if (parent / "build-manifest.json").is_file()), None)
    require(manifest is not None, "Package build-manifest.json is required for honest source attribution")
    return files, {"bin": str(bin_dir), "manifest": json.loads(manifest.read_text(encoding="utf-8")),
                   "executable_sha256": {name: sha256(path) for name, path in files.items()}}


class Check:
    def __init__(self, root, env):
        self.root, self.env = root, env
        self.profile = root / "synthetic-upgrade-profile"
        self.children = []
        self.outputs = []

    def spawn(self, program, *args, label):
        output = (self.root / (label + ".log")).open("w+b")
        child = subprocess.Popen([str(program), *map(str, args)], cwd=self.root, env=self.env,
                                 stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT)
        self.outputs.append(output)
        self.children.append(child)
        return child, output

    def pid(self):
        # Discovery is read only to compare ownership. Never print its handshake
        # token or send signals to the PID found in this file.
        try:
            return json.loads((self.profile / "engine.json").read_text(encoding="utf-8"))["pid"]
        except (FileNotFoundError, json.JSONDecodeError):
            return None

    def start_engine(self, files, label):
        child, _ = self.spawn(files["fm-engine"], "--profile", self.profile, label=label)
        deadline = time.monotonic() + 35
        while self.pid() != child.pid:
            require(child.poll() is None, label + " exited before discovery")
            require(time.monotonic() < deadline, label + " startup timed out")
            time.sleep(0.05)
        return child

    def start_owner(self, files, label, seconds=20):
        child, log = self.spawn(files["fm"], "--profile", self.profile, "--json", "watch",
                                "--seconds", str(seconds), label=label)
        deadline = time.monotonic() + 15
        while True:
            require(child.poll() is None, label + " exited before its first status")
            written = Path(log.name).read_bytes()
            if b"\n" in written:
                reply = json.loads(written.split(b"\n", 1)[0])
                require(reply.get("ok") is True, label + " did not establish an explicit session")
                return child
            require(time.monotonic() < deadline, label + " startup timed out")
            time.sleep(0.05)

    def command(self, program, *args, timeout=20):
        result = subprocess.run([str(program), *map(str, args)], cwd=self.root, env=self.env,
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=timeout, encoding="utf-8")
        require(result.returncode == 0, program.name + " command failed (exit " + str(result.returncode) + "): " + (result.stderr + result.stdout)[-1500:])
        return result.stdout

    def call(self, files, method, params=None, account="windscribe"):
        path = self.root / "request.json"
        path.write_text(json.dumps(params or {}, ensure_ascii=False), encoding="utf-8")
        args = ["--profile", self.profile, "--json"]
        if account is not None:
            args += ["--account", account]
        raw = self.command(files["fm"], *args, "call", method, "--params-file", path)
        reply = json.loads(raw)
        require(reply.get("ok") is True, "Protocol method failed: " + method)
        return reply["result"]

    def finish_owners(self, owner, engine):
        require(owner.poll() is None, "Foreground owner ended before the coexistence checks finished")
        require(owner.wait(timeout=50) == 0, "Foreground watch did not end cleanly")
        require(engine.wait(timeout=10) == 0, "Engine did not stop after its final owner")
        require(not (self.profile / "engine.json").exists(), "Stopped engine left active discovery")

    def close(self):
        # Failure cleanup addresses only children returned by our own Popen calls.
        for child in reversed(self.children):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
        for output in self.outputs:
            output.close()


def durable_snapshot(profile, columns=None):
    # Read-only inspection after the owned engine exits. Compare logical content;
    # harmless SQLite journal/header housekeeping is not a data-reset assertion.
    wal = profile / "store.sqlite3-wal"
    require(not wal.exists() or wal.stat().st_size == 0,
            "Cannot inspect an immutable snapshot while uncheckpointed WAL data exists")
    # On this Mac's Python SQLite runtime, the closed fixture failed mode=ro but
    # immutable inspection passed. It avoids sidecars, and is safe only
    # after the owned writer exited and the WAL check above succeeded.
    db = sqlite3.connect((profile / "store.sqlite3").as_uri() + "?mode=ro&immutable=1", uri=True)
    try:
        schema = db.execute("PRAGMA user_version").fetchone()[0]
        digest = hashlib.sha256("\n".join(db.iterdump()).encode("utf-8")).hexdigest()
        counts = {name: db.execute("SELECT COUNT(*) FROM " + name).fetchone()[0]
                  for name in ("accounts", "conversations", "messages", "attachments", "drafts", "operations", "capture_operations", "views")}
        fixture_revision = db.execute("SELECT value FROM meta WHERE key='fixture_revision'").fetchone()[0]
        if columns is None:
            names = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
            columns = {name: [row[1] for row in db.execute('PRAGMA table_info("' + name.replace('"', '""') + '")')]
                       for name in names}
        table_digests = {}
        for table, names in columns.items():
            quoted = ['"' + name.replace('"', '""') + '"' for name in names]
            projection = ','.join('quote(' + name + ')' for name in quoted)
            sql = 'SELECT ' + projection + ' FROM "' + table.replace('"', '""') + '" ORDER BY ' + ','.join(quoted)
            rows = db.execute(sql).fetchall()
            table_digests[table] = hashlib.sha256(json.dumps(rows, ensure_ascii=False).encode("utf-8")).hexdigest()
    finally:
        db.close()
    blobs = {path.name: sha256(path) for path in sorted((profile / "blobs").iterdir())
             if path.is_file()}
    return {"schema": schema, "logical_database_sha256": digest,
            "record_counts": counts, "retained_blobs": blobs, "fixture_revision": fixture_revision,
            "columns": columns, "table_digests": table_digests}


def check_upgrade(old, new, report):
    with tempfile.TemporaryDirectory(prefix="fuckmail-upgrade-") as temporary:
        check = Check(Path(temporary).resolve(), package_environment())
        try:
            old_engine = check.start_engine(old, "old-engine")
            old_owner = check.start_owner(old, "old-owner")
            old_status = check.call(old, "status", account=None)
            require(old_status.get("synthetic") is True, "Upgrade check requires synthetic fixture mode")
            report["old_schema"] = old_status["schema_version"]
            require(old_status["schema_version"] == 4, "Phase 1 upgrade requires the preserved schema-4 baseline")
            old_accounts = check.call(old, "accounts", account=None)
            require(len(old_accounts["accounts"]) == 2 and old_status["fixture_revision"] == 1,
                    "Old baseline must be the original two-account fixture 1")
            old_pid = old_engine.pid
            compatible = check.call(new, "status", account=None)
            require(compatible == old_status and check.pid() == old_pid,
                    "Compatible new client replaced or changed the active old engine")
            thread = "windscribe-thread-000"
            detail = check.call(old, "conversation", {"id": thread})
            draft = check.call(old, "draft_get", {"conversation_id": thread})
            body = "Human upgrade draft\nΚαλημέρα — 日本語 — café ☕\nExact authored text survives."
            draft_params = {"conversation_id": thread, "body": body,
                            "expected_revision": draft["revision"], "request_id": "upgrade-draft-once"}
            draft_params.update({"body_html": "<p>Human upgrade draft — 日本語</p>",
                                 "attachment_ids": ["windscribe-attachment-0"]})
            saved = check.call(old, "draft_save", draft_params)
            view_params = {"view": {"query": "maya", "folder": "inbox", "selected": thread,
                                      "readerY": 25, "listY": 15},
                           "expected_revision": 0, "request_id": "upgrade-view-once"}
            saved_view = check.call(old, "view_save", view_params)
            capture_params = {"conversation_id": thread, "expected_revision": saved["revision"],
                              "request_id": "upgrade-capture-once"}
            captured = check.call(old, "draft_capture", capture_params)
            captured_hash = sha256(Path(captured["path"]))
            require(captured["state"] == "accepted" and captured["attempts_started"] == 1,
                    "Old capture must be locally accepted exactly once")
            flag_params = {"id": thread, "message_ids": detail["message_ids"],
                           "expected_revision": detail["revision"], "request_id": "upgrade-flag-once",
                           "read": True, "starred": True}
            flagged = check.call(old, "flag", flag_params)
            require(all(not item["unread"] and item["starred"] for item in flagged["messages"]),
                    "Old package did not persist the explicit independent flags")
            require(check.call(new, "draft_get", {"conversation_id": thread}) == saved,
                    "New client cannot read the old owner's authored draft")
            require(check.call(new, "flag", flag_params) == flagged, "Compatible new client changed an old receipt")
            require(check.pid() == old_pid, "New client replaced the explicit old engine owner")
            check.finish_owners(old_owner, old_engine)
            old_snapshot = durable_snapshot(check.profile)
            report["compatible_coexistence"] = "passed"
            report["old_final_owner_shutdown"] = "passed"
            report["before_upgrade_counts"] = old_snapshot["record_counts"]
            print("Passed compatible coexistence and old final-owner shutdown", flush=True)

            new_engine = check.start_engine(new, "new-engine")
            new_owner = check.start_owner(new, "new-owner")
            upgraded = check.call(new, "status", account=None)
            require(upgraded["schema_version"] > old_status["schema_version"],
                    "Selected packages did not exercise an actual schema upgrade")
            require(upgraded["schema_version"] == 5 and upgraded["fixture_revision"] == 1,
                    "Schema 5 must preserve the original fixture revision")
            current_accounts = check.call(new, "accounts", account=None)
            require([row["id"] for row in current_accounts["accounts"]] == [row["id"] for row in old_accounts["accounts"]],
                    "Migration added, removed or reset accounts")
            report["new_schema"] = upgraded["schema_version"]
            report["fixture_revision_preserved"] = 1
            report["accounts_preserved_without_reseed"] = [row["id"] for row in current_accounts["accounts"]]
            recovered = check.call(new, "draft_get", {"conversation_id": thread})
            require(contains_fields(recovered, saved),
                    "Migration lost or changed the exact Unicode draft")
            require(check.call(new, "draft_save", draft_params) == saved,
                    "Migration changed the immutable old draft receipt")
            require(check.call(new, "flag", flag_params) == flagged,
                    "Migration changed the immutable old flag receipt")
            require(contains_fields(check.call(new, "conversation", {"id": thread}), flagged),
                    "Migration changed retained conversation or flag state")
            require(check.call(new, "view_get") == saved_view, "Migration changed saved navigation")
            require(check.call(new, "view_save", view_params) == saved_view, "Migration changed the old view receipt")
            replay_capture = check.call(new, "draft_capture", capture_params)
            require(replay_capture == captured and sha256(Path(replay_capture["path"])) == captured_hash,
                    "Migration changed capture bytes, operation or attempt count")
            require(check.call(new, "capture_history", {"conversation_id": thread})["total"] == 1,
                    "Migrated capture journal does not expose its original entry")
            check.finish_owners(new_owner, new_engine)
            migrated_snapshot = durable_snapshot(check.profile, old_snapshot["columns"])
            require(migrated_snapshot["table_digests"] == old_snapshot["table_digests"],
                    "Migration changed existing table columns or logical content")
            report["preexisting_logical_table_hashes_preserved"] = len(old_snapshot["table_digests"])
            report["exact_captured_mime_sha256"] = captured_hash
            new_engine = check.start_engine(new, "new-engine-for-gui")
            new_owner = check.start_owner(new, "new-gui-owner", seconds=35)
            check.command(new["fuckmail-gui"], "--smoke", "--profile", check.profile, timeout=30)
            require(check.pid() == new_engine.pid and new_owner.poll() is None,
                    "Closing the upgraded GUI ended the independent CLI owner")
            check.finish_owners(new_owner, new_engine)
            before = durable_snapshot(check.profile)
            require(all(before["retained_blobs"].get(name) == digest
                        for name, digest in old_snapshot["retained_blobs"].items()),
                    "Migration lost or changed a previously retained blob")
            require(all(before["record_counts"][name] == old_snapshot["record_counts"][name]
                        for name in ("accounts", "conversations", "messages", "attachments", "drafts", "capture_operations")),
                    "Migration or GUI startup changed authored/source record counts")
            require(before["record_counts"]["operations"] >= old_snapshot["record_counts"]["operations"],
                    "Migration lost recorded operation receipts")
            report["after_upgrade_counts"] = before["record_counts"]
            report["preexisting_blob_count"] = len(old_snapshot["retained_blobs"])
            report["migration_retains_old_blob_hashes"] = "passed"
            report["migration_and_exact_receipt_replay"] = "passed"
            report["new_gui_with_independent_cli_owner"] = "passed"
            report["new_final_owner_shutdown"] = "passed"
            print("Passed migration, retained bytes, receipt replay and new GUI smoke", flush=True)

            rollback, output = check.spawn(old["fm-engine"], "--profile", check.profile,
                                            label="old-engine-rollback")
            code = rollback.wait(timeout=10)
            output.seek(0)
            diagnostic = output.read().decode("utf-8", errors="replace")
            require(code != 0 and "schema_mismatch" in diagnostic,
                    "Old engine did not explicitly refuse the newer schema")
            require(not (check.profile / "engine.json").exists(), "Refused old engine published discovery")
            require(durable_snapshot(check.profile) == before,
                    "Refused rollback changed logical database state or retained content")
            report["rollback_refused_without_data_reset"] = "passed"

            # Reopen through the new owned engine after the failed old launch.
            final_engine = check.start_engine(new, "new-engine-after-refusal")
            final_owner = check.start_owner(new, "new-owner-after-refusal", seconds=10)
            require(check.call(new, "draft_get", {"conversation_id": thread})["body"] == body,
                    "New package cannot recover the draft after rollback refusal")
            require(check.call(new, "flag", flag_params) == flagged,
                    "New package cannot replay the flag receipt after rollback refusal")
            check.finish_owners(final_owner, final_engine)
            report["new_package_reopens_after_refusal"] = "passed"
            report["old_query_fields_compatible_with_additive_protocol"] = "passed"
            report["draft_utf8_sha256"] = hashlib.sha256(body.encode("utf-8")).hexdigest()
            report["retained_blob_count"] = len(before["retained_blobs"])
        finally:
            check.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old-bin", type=Path, required=True, help="Older package's executable directory")
    parser.add_argument("--new-bin", type=Path, required=True, help="Newer package's executable directory")
    parser.add_argument("--report", type=Path, required=True, help="Write a JSON evidence report")
    args = parser.parse_args()
    report = {"host": platform.platform(), "architecture": platform.machine(),
              "synthetic_only": True, "sdk_environment": "cleared", "result": "failed",
              "verifier_sha256": sha256(Path(__file__)),
              "limits": ["GUI --smoke is launched with the upgraded profile; source honors that option. The independent CLI owner PID stays unchanged, but the script does not instrument the GUI handshake.",
                         "This is unsigned development-package replacement evidence, not an automatic updater, physical input or OS keychain test."]}
    started = time.monotonic()
    try:
        old, report["old_package"] = package(args.old_bin)
        new, report["new_package"] = package(args.new_bin)
        require(old["fm-engine"] != new["fm-engine"], "Select two distinct preserved package locations")
        check_upgrade(old, new, report)
        report["result"] = "passed"
    except (RuntimeError, OSError, ValueError, sqlite3.Error, subprocess.TimeoutExpired) as error:
        report["error"] = str(error)
    finally:
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("Native package upgrade:", report["result"], "—", args.report)
    if report["result"] != "passed":
        raise SystemExit(report.get("error", "Upgrade verification failed"))


if __name__ == "__main__":
    main()
