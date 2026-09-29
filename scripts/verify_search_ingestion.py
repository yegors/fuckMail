#!/usr/bin/env python3
"""Measure native search paints while the same engine ingests synthetic source MIME.

The supplied closed generated seed is read only. All writes target a fresh temporary
copy, and the fixed engine verification mode uses the existing single Store writer.
"""
import argparse
from contextlib import closing, ExitStack
import hashlib
import json
import os
import platform
import shutil
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

from dev import package_environment


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path):
    require(path.stat().st_size <= 4 * 1024 * 1024, "Verification report exceeds its bound")
    return json.loads(path.read_text(encoding="utf-8"))


def read_database(profile):
    return sqlite3.connect((profile / "store.sqlite3").as_uri() + "?mode=ro", uri=True)


def clone_seed(seed, destination):
    require(not (seed / "engine.json").exists(), "Close the generated seed's engine before measuring")
    with closing(read_database(seed)) as db:
        require(db.execute("PRAGMA user_version").fetchone()[0] == 5, "The seed must use schema 5")
        require(str(db.execute("SELECT value FROM meta WHERE key='fixture_revision'").fetchone()[0]) == "2",
                "The seed must be the generated fixture-2 profile")
        require(db.execute("SELECT COUNT(*) FROM messages WHERE id LIKE 'benchmark-%'").fetchone()[0] == 100000,
                "Exactly 100,000 generated benchmark messages are required")
        require(db.execute("SELECT COUNT(*) FROM google_checkpoints").fetchone()[0] == 0,
                "Use a seed without provider checkpoints or earlier verification ingestion")
        before = db.execute("SELECT COUNT(*) FROM messages").fetchone()[0]
        destination.mkdir()
        with closing(sqlite3.connect(destination / "store.sqlite3")) as clone:
            db.backup(clone)
    shutil.copytree(seed / "blobs", destination / "blobs")
    require(not (seed / "engine.json").exists(), "The seed started an engine while cloning")
    return before


def stop_children(children):
    failures = []
    for child in reversed(children):
        try:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired) as error:
            failures.append(f"Owned child {child.pid}: {error}")
    require(not failures, "; ".join(failures))


def retain_ingestion(source, destination):
    if source.exists():
        shutil.copy2(source, destination)


def record_failure(result, error):
    result["result"] = "failed"
    message = str(error)
    result["error"] = result["error"] + "; " + message if result.get("error") else message


def expected_source(remote_id):
    # Independent fixed-source oracle for verification-fixture search mode.
    return ("From: Alex O’Hara <alex.o'hara+build@windscribe.example>\r\n"
            "To: You <you@windscribe.example>\r\n"
            "Subject: Concurrent source verification\r\n"
            "Date: Tue, 01 Sep 2026 12:00:00 +0000\r\n"
            f"Message-ID: <{remote_id}@fixture.example>\r\n"
            "MIME-Version: 1.0\r\n"
            "Content-Type: text/plain; charset=utf-8\r\n"
            "Content-Transfer-Encoding: 8bit\r\n\r\n"
            f"Concurrent source deployment verification {remote_id}\r\n").encode("utf-8")


def verify_arrivals(profile, before, ingestion):
    sequences = [batch["sequence"] for batch in ingestion["batches"]]
    require(len(set(sequences)) == len(sequences)
            and all(type(sequence) is int and 0 <= sequence < 3000 for sequence in sequences)
            and all(batch["committed_messages"] == 25 for batch in ingestion["batches"]),
            "Committed batches must have distinct bounded identities and exactly 25 messages")
    expected_ids = {f"verification-search-{sequence:04}-{member:02}"
                    for sequence in sequences for member in range(25)}
    seen = set()
    with closing(read_database(profile)) as db:
        db.row_factory = sqlite3.Row
        after = db.execute("SELECT COUNT(*) FROM messages").fetchone()[0]
        rows = db.execute("""SELECT m.*, g.remote_thread_id, g.provider_labels_json, g.deleted
            FROM messages m LEFT JOIN google_message_state g ON g.account=m.account AND g.message_id=m.id
            WHERE m.remote_id LIKE 'verification-search-%'""")
        for row in rows:
            remote_id = row["remote_id"]
            require(remote_id in expected_ids and remote_id not in seen,
                    f"Unexpected or duplicate committed source identity: {remote_id}")
            seen.add(remote_id)
            body = f"Concurrent source deployment verification {remote_id}"
            expected = {"account": "windscribe", "sender_name": "Alex O’Hara",
                        "sender_email": "alex.o'hara+build@windscribe.example",
                        "recipient": "you@windscribe.example", "subject": "Concurrent source verification",
                        "body": body + "\r\n", "sent_at": "2026-09-01T12:00:00Z",
                        "unread": 1, "starred": 0, "inbox": 1, "sent": 0,
                        "remote_thread_id": f"verification-thread-{remote_id}", "deleted": 0}
            for field, value in expected.items():
                require(row[field] == value, f"Arrival {remote_id} has an incorrect {field}")
            require(sorted(json.loads(row["provider_labels_json"])) == ["INBOX", "UNREAD"],
                    f"Arrival {remote_id} has incorrect provider labels")
            source = expected_source(remote_id)
            expected_hash = hashlib.sha256(source).hexdigest()
            require(row["source_hash"] == expected_hash,
                    f"Arrival {remote_id} does not reference its independently expected source MIME")
            blob = profile / "blobs" / expected_hash
            require(blob.is_file() and blob.stat().st_size == len(source) and sha256(blob) == expected_hash,
                    f"Arrival {remote_id} has missing or changed source MIME bytes")
        # MATCH checks the actual FTS postings, not just a stored FTS content row.
        indexed = db.execute("""SELECT COUNT(*) FROM message_fts JOIN messages m ON m.rowid=message_fts.rowid
            WHERE message_fts MATCH 'body:deployment' AND m.account='windscribe'
              AND m.remote_id LIKE 'verification-search-%'
              AND message_fts.sender=m.sender_name || ' ' || m.sender_email
              AND message_fts.subject=m.subject AND message_fts.body=m.body""").fetchone()[0]
    require(seen == expected_ids and after - before == len(seen) == indexed == ingestion["committed_messages"],
            "Independent source identities/bytes, normalized messages and FTS postings do not match committed ingestion")
    return after, indexed


def run(args):
    report = args.report.resolve()
    outputs = {name: report.with_suffix(suffix) for name, suffix in {
        "gui_report": ".gui.json", "ingestion_report": ".ingestion.json",
        "gui_log": ".gui.log", "engine_log": ".engine.log"}.items()}
    require(not any(path.exists() for path in (report, *outputs.values())),
            "Use a new report name; earlier measurements must be preserved")
    result = {"result": "failed", "started_unix_ms": time.time_ns() / 1000000,
              "host": None, "package_manifest": None, "executables_sha256": {},
              "verifier_sha256": None, "source_seed": str(args.seed), "source_database_sha256": None,
              "outputs": {name: str(path) for name, path in outputs.items()}}
    children = []
    started = time.monotonic()
    try:
        report.parent.mkdir(parents=True, exist_ok=True)
        result["host"] = platform.platform()
        bin_dir = args.bin.resolve(strict=True)
        suffix = ".exe" if platform.system() == "Windows" else ""
        files = {name: bin_dir / (name + suffix) for name in ("fm", "fm-engine", "fuckMail")}
        require(all(path.is_file() for path in files.values()), "GUI, engine and CLI siblings are required")
        manifest_path = next((parent / "build-manifest.json" for parent in (bin_dir, *list(bin_dir.parents)[:4])
                              if (parent / "build-manifest.json").exists()), None)
        result["package_manifest"] = read_json(manifest_path) if manifest_path else None
        result["executables_sha256"] = {name: sha256(path) for name, path in files.items()}
        result["verifier_sha256"] = sha256(Path(__file__))
        seed = args.seed.resolve(strict=True)
        result["source_seed"] = str(seed)
        result["source_database_sha256"] = sha256(seed / "store.sqlite3")
        env = package_environment()
        env.pop("QT_QUICK_BACKEND", None)
        env.pop("QT_D3D_NO_VBLANK_THREAD", None)
        env["QSG_INFO"] = "1"
        env["QT_QPA_PLATFORM"] = {"Windows": "windows", "Darwin": "cocoa"}.get(platform.system(), "xcb")
        if args.qt_root:
            require(manifest_path is None, "SDK paths must not be added to a packaged measurement")
            env["PATH"] = str(args.qt_root.resolve(strict=True) / "bin") + os.pathsep + env["PATH"]
        result["environment"] = {key: env.get(key) for key in
                                 ("QT_QPA_PLATFORM", "QT_QUICK_BACKEND", "QT_D3D_NO_VBLANK_THREAD", "PATH")}
        with tempfile.TemporaryDirectory(prefix="fuckmail-search-ingestion-") as scratch, ExitStack() as cleanup:
            scratch = Path(scratch).resolve()
            profile = scratch / "generated-profile"
            ingestion_path = profile / "verification-ingestion.json"
            # ExitStack is LIFO: stop only our children, retain even a partial
            # report, then let TemporaryDirectory remove the disposable clone.
            cleanup.callback(retain_ingestion, ingestion_path, outputs["ingestion_report"])
            cleanup.callback(stop_children, children)
            before = clone_seed(seed, profile)
            result["initial_message_count"] = before
            result["cloned_database_sha256"] = sha256(profile / "store.sqlite3")
            with outputs["engine_log"].open("w", encoding="utf-8") as engine_log, \
                    outputs["gui_log"].open("w", encoding="utf-8") as gui_log:
                engine = subprocess.Popen([str(files["fm-engine"]), "--profile", str(profile),
                                           "--verification-fixture", "search"], cwd=scratch, env=env,
                                          stdin=subprocess.DEVNULL, stdout=engine_log, stderr=subprocess.STDOUT)
                children.append(engine)
                deadline = time.monotonic() + 35
                while not (profile / "engine.json").exists() or not ingestion_path.exists():
                    require(engine.poll() is None, "Verification engine exited before startup")
                    require(time.monotonic() < deadline, "Verification engine startup timed out")
                    time.sleep(.05)
                armed = read_json(ingestion_path)
                require(armed.get("mode") == "search" and armed.get("engine_pid") == engine.pid
                        and read_json(profile / "engine.json").get("pid") == engine.pid,
                        "Search verification scenario was not armed in the owned engine")
                gui = subprocess.Popen([str(files["fuckMail"]), "--profile", str(profile), "--fixture-clock",
                                        "--benchmark-search-output", str(outputs["gui_report"])],
                                       cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                       stdout=gui_log, stderr=subprocess.STDOUT)
                children.append(gui)
                result["gui_exit_code"] = gui.wait(timeout=600)
                result["engine_exit_code"] = engine.wait(timeout=15)
            require(result["gui_exit_code"] in (0, 9) and result["engine_exit_code"] == 0,
                    "Native measurement or engine shutdown failed; inspect the retained logs")
            gui_data, ingestion = read_json(outputs["gui_report"]), read_json(ingestion_path)
            result["budget_passed"] = gui_data.get("budget_passed") is True
            require(not ingestion.get("errors") and ingestion.get("messages_per_batch") == 25
                    and ingestion.get("interval_ms") == 100 and ingestion.get("max_batches") == 3000
                    and ingestion.get("mode") == "search" and ingestion.get("version") == 1
                    and ingestion.get("engine_pid") == engine.pid and ingestion.get("status") == "complete",
                    "The fixed ingestion workload failed, changed, or did not complete in the owned engine")
            batches = ingestion["batches"]
            require(ingestion.get("scheduler_version") == 2
                    and ingestion.get("scheduler") == {"max_resident_work": 32,
                        "foreground_limit": 16, "background_limit": 16,
                        "sources_per_step": 1, "foreground_burst_limit": 4},
                    "The bounded cooperative scheduling configuration changed")
            require(len(batches) == ingestion["committed_batches"] and len(batches) > 0,
                    "Committed batch reporting is incomplete")
            require(sum(batch["committed_messages"] for batch in batches) == ingestion["committed_messages"],
                    "Committed message reporting is inconsistent")
            after, indexed = verify_arrivals(profile, before, ingestion)
            result["final_message_count"], result["independently_indexed_arrivals"] = after, indexed
            classes = []
            queries = gui_data["queries"]
            require([query["class"] for query in queries] ==
                    ["rare subject", "rare body", "grouped first50", "combined filters first50"],
                    "The native report must contain all four measured search classes")
            for query in queries:
                windows = query["sample_windows"]
                require(len(windows) == query["observations"] == 100, "Every class needs 100 timed paint windows")
                require(all(window["input_unix_ms"] <= window["paint_unix_ms"] for window in windows)
                        and all(earlier["paint_unix_ms"] <= later["input_unix_ms"]
                                for earlier, later in zip(windows, windows[1:])),
                        "Sample wall-clock windows changed order; ingestion overlap cannot be established")
                begin, end = windows[0]["input_unix_ms"], windows[-1]["paint_unix_ms"]
                overlapping = [batch for batch in batches
                               if batch["cooperative"]["commit_finished_unix_ms"] >= begin
                               and batch["cooperative"]["commit_started_unix_ms"] <= end]
                require(len(overlapping) >= 2, "A search class did not overlap continuing committed ingestion")
                require(ingestion["first_scheduled_unix_ms"] <= begin
                        and ingestion["producer_last_attempt_unix_ms"] + ingestion["interval_ms"] >= end,
                        "The bounded ingestion producer stopped before the measured class finished")
                classes.append({"class": query["class"], "p95_ms": query["p95_ms"],
                                "first_input_unix_ms": begin, "last_paint_unix_ms": end,
                                "overlapping_committed_batches": len(overlapping),
                                "overlapping_committed_messages": sum(batch["committed_messages"] for batch in overlapping)})
            result.update(queries=classes, committed_messages=ingestion["committed_messages"],
                          committed_batches=ingestion["committed_batches"],
                           workload="25 source MIME messages scheduled every 100 ms through the same bounded engine queue and GooglePageStore writer; independent exact source identities and MIME hashes/bytes, normalized fields/flags and FTS MATCH counts verified after final-owner shutdown.")
            require(result["budget_passed"] and result["gui_exit_code"] == 0,
                    "Native paint latency did not meet its unchanged 200 ms budget; source and overlap checks passed")
            result["result"] = "passed"
    except Exception as error:
        record_failure(result, error)
    finally:
        try:
            stop_children(children)
        except Exception as error:
            record_failure(result, error)
        result["elapsed_seconds"] = round(time.monotonic() - started, 3)
        result["output_sha256"] = {}
        for name, path in outputs.items():
            try:
                if path.exists():
                    result["output_sha256"][name] = sha256(path)
            except Exception as error:
                record_failure(result, error)
        try:
            report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        except OSError as error:
            record_failure(result, f"Cannot write {report}: {error}")
    print(json.dumps({key: result[key] for key in ("result", "elapsed_seconds", "queries", "error") if key in result}, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", required=True, type=Path, help="Directory containing the three native executables")
    parser.add_argument("--seed", required=True, type=Path, help="Closed generated 100k fixture profile; never modified")
    parser.add_argument("--report", required=True, type=Path, help="New report filename; existing evidence is never overwritten")
    parser.add_argument("--qt-root", type=Path, help="Explicit SDK for a development binary only; packaged runs reject this option")
    raise SystemExit(run(parser.parse_args()))
