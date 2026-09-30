#!/usr/bin/env python3
"""End-to-end local model pipeline check against synthetic mail and a
fake same-machine model server. No real accounts, mail or model service.

Runs the built engine and CLI: settings, one explicit analysis, the queue
(newest first, pause, Catch up, model down, failing email, owner close), the
Test button and the OpenAI-compatible provider.
"""
import argparse
import http.server
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import threading
import time

ACCOUNT = "windscribe"
SEPARATOR = "\n@@\n"


class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True
    mode = "valid"
    chats = []  # (path, email) per generation request
    other = []  # every non-generation path
    bodies = []


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.handle_api()

    def do_POST(self):
        self.handle_api()

    def handle_api(self):
        server = self.server
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if self.path == "/api/version":
            result = {"version": "fixture"}
        elif self.path == "/api/tags":
            result = {"models": [{"name": "fixture:latest", "digest": "a" * 64, "size": 5000000},
                                 {"name": "remote:cloud", "digest": "b" * 64, "size": 1}]}
        elif self.path == "/api/show":
            result = {"capabilities": ["completion"], "details": {"format": "gguf"}}
        elif self.path == "/v1/models":
            result = {"data": [{"id": "fixture:latest"}]}
        elif self.path in ("/api/chat", "/v1/chat/completions"):
            if server.mode == "down":
                self.send_error(500)
                return
            if server.mode == "slow":
                time.sleep(.25)
            labels, email = body["messages"][1]["content"].split(SEPARATOR)
            labels, email = json.loads(labels), json.loads(email)
            server.chats.append((self.path, email))
            server.bodies.append(body)
            output = {"summary": "You are asked to review " + email["subject"][:40] + ".",
                      "label": labels[0]["id"] if labels else None, "sender": "human"}
            if server.mode == "invalid" or (server.mode == "invalid-first" and len(server.chats) == 1):
                output["read"] = True
            content = json.dumps(output)
            if self.path == "/api/chat":
                result = {"model": body["model"], "done": True, "done_reason": "stop", "eval_count": 20,
                          "eval_duration": 200_000_000, "message": {"content": content}}
            else:
                result = {"model": body["model"], "usage": {"completion_tokens": 20},
                          "choices": [{"index": 0, "finish_reason": "stop",
                                       "message": {"role": "assistant", "content": content}}]}
        else:
            self.send_error(404)
            return
        if self.path not in ("/api/chat", "/v1/chat/completions"):
            server.other.append(self.path)
        raw = json.dumps(result).encode()
        try:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass


class Client:
    def __init__(self, discovery):
        self.sock = None
        if os.name == "nt":
            self.file = open(discovery["endpoint"], "r+b", buffering=0)
        else:
            self.sock = socket.socket(socket.AF_UNIX)
            self.sock.settimeout(10)
            self.sock.connect(discovery["endpoint"])
            self.file = self.sock.makefile("rb")
        self.sequence = 0
        self.call("hello", {"token": discovery["token"], "client": "local-model-verifier"})

    def close(self):
        self.file.close()
        if self.sock:
            self.sock.close()

    def call(self, method, params=None, account=None, error=False):
        self.sequence += 1
        req = {"version": 1, "id": str(self.sequence), "method": method, "params": params or {}}
        if account:
            req["account"] = account
        data = json.dumps(req).encode() + b"\n"
        if self.sock:
            self.sock.sendall(data)
        else:
            self.file.write(data)
        result = json.loads(self.file.readline())
        if error:
            assert "error" in result, result
            return result["error"]
        assert "error" not in result, result
        return result["result"]

    def wait(self, job):
        end = time.monotonic() + 15
        while time.monotonic() < end:
            result = self.call("assist_job", {"job_id": job})
            if result["status"] not in ("queued", "running"):
                return result
            time.sleep(.04)
        raise AssertionError("Job did not complete")

    def progress(self):
        rows = self.call("assist_progress", {"complete": True})["accounts"]
        return next(r for r in rows if r["account"] == ACCOUNT)


def until(check, seconds=20, message="condition"):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.05)
    raise AssertionError("Timed out waiting for " + message)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", required=True)
    args = parser.parse_args()
    engine = pathlib.Path(args.engine).resolve()
    cli = engine.with_name("fm.exe" if os.name == "nt" else "fm")
    server = Server(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    checks = []
    with tempfile.TemporaryDirectory(prefix="fm-local-model-") as root:
        profile = pathlib.Path(root) / "profile"
        profile.mkdir(mode=0o700)

        def fm(*argv):
            done = subprocess.run([str(cli), "--profile", str(profile), "--engine", str(engine), "--json", *argv],
                                  capture_output=True, text=True, timeout=30)
            assert done.returncode == 0, done.stdout + done.stderr
            return json.loads(done.stdout)["result"]

        with open(pathlib.Path(root) / "engine.log", "w") as log:
            process = subprocess.Popen([str(engine), "--profile", str(profile), "--demo-profile"], stdout=log, stderr=log)
            try:
                until(lambda: (profile / "engine.json").exists() or process.poll() is not None, 60, "engine start")
                assert process.poll() is None, "Engine startup failed"
                discovery = json.loads((profile / "engine.json").read_text())
                a, b = Client(discovery), Client(discovery)

                state = a.call("assist_settings_get")
                settings = state["settings"]
                assert "prompt" not in settings and settings["system"] and "{{messages}}" in settings["user"]
                settings["endpoint"] = "http://127.0.0.1:" + str(server.server_port)
                catalog = a.call("assist_discover", {"endpoint": settings["endpoint"]})
                assert [m["eligible"] for m in catalog["models"]] == [True, False]
                settings["model"] = {"name": "fixture:latest", "digest": "a" * 64}
                settings["system"] = "Only my system rules."
                settings["user"] = "{{labels}}" + SEPARATOR + "{{messages}}"
                others = [r["id"] for r in a.call("accounts")["accounts"] if r["id"] != ACCOUNT]
                settings["accounts"] = {o: {"summaries": False, "classification": False} for o in others}
                a.call("assist_settings_save", {"expected_revision": state["revision"], "settings": settings})
                assert not server.chats, "Saving settings ran inference"
                checks.append("settings: system/user, no inference on save, cloud model excluded")

                thread = "windscribe-thread-001"
                message = next(m for m in a.call("conversation", {"id": thread}, ACCOUNT)["messages"] if not m["sent"])
                before = (message["unread"], message["starred"])
                other_before = list(server.other)
                job = a.call("assist_start", {"conversation_id": thread, "message_id": message["id"]}, ACCOUNT)["job_id"]
                assert b.call("assist_job", {"job_id": job}, error=True)["code"] == "not_found"
                done = a.wait(job)
                assert done["status"] == "completed", done
                body = server.bodies[-1]
                assert body["messages"][0] == {"role": "system", "content": "Only my system rules."}
                assert body["think"] is False and body["stream"] is False and body["keep_alive"]
                assert body["format"]["properties"]["sender"]["enum"] == ["human", "bot"]
                assert None in body["format"]["properties"]["label"]["enum"]
                email = server.chats[-1][1]
                assert set(email) == {"from", "to", "subject", "date", "text", "shortened"}
                after = next(m for m in a.call("conversation", {"id": thread}, ACCOUNT)["messages"] if m["id"] == message["id"])
                assert (after["unread"], after["starred"]) == before, "Analysis changed review state"
                items = a.call("assist_results", {"conversation_id": thread}, ACCOUNT)["items"]
                assert {i["task"] for i in items if i["target"] == message["id"]} == {"summary", "classification"}
                # The model is checked once, not around every email.
                first_checks = server.other[len(other_before):]
                job = a.call("assist_start", {"conversation_id": thread, "message_id": message["id"]}, ACCOUNT)["job_id"]
                assert a.wait(job)["status"] == "completed"
                assert server.other[len(other_before) + len(first_checks):] == [], "Model checks repeated per email"
                checks.append("explicit analysis: one request per email, review state unchanged, both results saved")

                # The queue: newest first, until everything is saved.
                total = a.progress()["eligible"]
                chats_before = len(server.chats)
                assert a.call("assist_watch")["watching"]
                until(lambda: a.progress()["queued"] == 0, 60, "queue to drain")
                dates = [e["date"] for _, e in server.chats[chats_before:]]
                assert dates == sorted(dates, reverse=True), "Queue is not newest first"
                assert len(dates) == total - 1, (len(dates), total)
                assert a.progress()["done"] == total
                assert a.progress()["seconds_per_message"] is not None
                checks.append("queue: every eligible email once, newest first, progress and speed")

                # Pause and resume through the CLI.
                fm("--account", ACCOUNT, "assist", "pause")
                assert a.progress()["paused"] is True
                fm("--account", ACCOUNT, "assist", "resume")
                assert a.progress()["paused"] is False
                checks.append("CLI pause/resume")

                # Catch up: history before a cutoff waits until asked for.
                row = next(r for r in a.call("accounts")["accounts"] if r["id"] == ACCOUNT)
                a.call("assist_settings_get")
                sqlite = __import__("sqlite3").connect(str(profile / "store.sqlite3"), timeout=5)
                with sqlite:
                    sqlite.execute("DELETE FROM local_model_results WHERE account=?", (ACCOUNT,))
                    sqlite.execute("INSERT OR REPLACE INTO meta(key,value) VALUES(?,?)",
                                   ("local_model_history_cutoff:" + ACCOUNT, int(time.time() * 1000)))
                sqlite.close()
                del row
                progress = a.progress()
                assert progress["queued"] == 0 and progress["unprocessed"] == total, progress
                counts = fm("--account", ACCOUNT, "assist", "backlog")
                assert counts["ranges"]["all"] == total, counts
                chats_before = len(server.chats)
                server.mode = "slow"
                fm("--account", ACCOUNT, "assist", "catch-up", "--range", "all")
                until(lambda: len(server.chats) >= chats_before + 3, 30, "catch-up start")
                # New mail during Catch up goes next.
                now = time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime())
                sqlite = __import__("sqlite3").connect(str(profile / "store.sqlite3"), timeout=5)
                with sqlite:
                    sqlite.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) SELECT account,'arrival-1',conversation_id,'arrival-1',sender_name,sender_email,recipient,'Arrived during catch-up',body,?,unread,starred,inbox,sent,source_hash FROM messages WHERE account=? AND id=?", (now, ACCOUNT, message["id"]))
                sqlite.close()
                arrived = len(server.chats)
                until(lambda: any(e["subject"] == "Arrived during catch-up" for _, e in server.chats[arrived:]), 30, "new mail")
                position = next(i for i, (_, e) in enumerate(server.chats[arrived:]) if e["subject"] == "Arrived during catch-up")
                assert position <= 1, "New mail waited behind Catch up: position %d" % position
                server.mode = "valid"
                until(lambda: a.progress()["done"] == total + 1, 60, "catch-up")
                dates = [e["date"] for _, e in server.chats[chats_before:] if e["subject"] != "Arrived during catch-up"]
                assert dates == sorted(dates, reverse=True) and len(dates) == total
                total += 1
                checks.append("Catch up: waits behind the cutoff, runs newest first through the CLI, and new mail goes next")

                # Model down: the queue holds and nothing is marked failed.
                sqlite = __import__("sqlite3").connect(str(profile / "store.sqlite3"), timeout=5)
                with sqlite:
                    sqlite.execute("DELETE FROM local_model_results WHERE account=? AND target IN (SELECT DISTINCT target FROM local_model_results WHERE account=? ORDER BY target LIMIT 4)", (ACCOUNT, ACCOUNT))
                sqlite.close()
                server.mode = "down"
                b.call("assist_catch_up", {"range": "all"}, ACCOUNT)
                held = until(lambda: a.progress()["model_error"], 20, "model error")
                assert held["code"] == "assistance_unavailable", held
                progress = a.progress()
                assert progress["failed"] == 0 and progress["queued"] > 0, progress
                server.mode = "valid"
                # Saving settings clears the error and wakes the queue at once.
                state = a.call("assist_settings_get")
                a.call("assist_settings_save", {"expected_revision": state["revision"], "settings": state["settings"]})
                until(lambda: a.progress()["queued"] == 0, 30, "recovery")
                assert a.progress()["model_error"] is None
                checks.append("model down: queue holds, no failures, resumes")

                # One bad email: two attempts, then it leaves the queue; the rest continue.
                sqlite = __import__("sqlite3").connect(str(profile / "store.sqlite3"), timeout=5)
                with sqlite:
                    sqlite.execute("DELETE FROM local_model_results WHERE account=? AND target IN (SELECT DISTINCT target FROM local_model_results WHERE account=? ORDER BY target LIMIT 2)", (ACCOUNT, ACCOUNT))
                sqlite.close()
                server.mode = "invalid"
                b.call("assist_catch_up", {"range": "all"}, ACCOUNT)
                until(lambda: a.progress()["failed"] == 2, 30, "failures")
                assert a.progress()["queued"] == 0
                server.mode = "valid"
                failed = a.call("assist_results", {"conversation_id": thread}, ACCOUNT)["failures"]
                assert all(f["attempts"] >= 2 for f in failed)
                checks.append("failing email: two attempts, then skipped until Try again")

                # Test button with unsaved settings.
                draft = json.loads(json.dumps(settings))
                draft["system"] = "Unsaved rules."
                job = a.call("assist_start", {"sample": True, "sample_case": "receipt", "settings": draft})["job_id"]
                result = a.wait(job)
                assert result["status"] == "completed" and result["items"][0]["metrics"]["output_tokens"] == 20
                assert server.bodies[-1]["messages"][0]["content"] == "Unsaved rules."
                remote = json.loads(json.dumps(settings))
                remote["model"] = {"name": "remote:cloud", "digest": "b" * 64}
                chats = len(server.chats)
                job = a.call("assist_start", {"sample": True, "settings": remote})["job_id"]
                assert a.wait(job)["status"] == "failed" and len(server.chats) == chats, "Cloud model ran"
                checks.append("Test: unsaved settings, timings, cloud model refused")

                # OpenAI-compatible provider.
                state = b.call("assist_settings_get")
                compatible = state["settings"]
                compatible["provider"] = "openai_compatible"
                compatible["model"]["digest"] = ""
                compatible["endpoint"] += "/v1"
                b.call("assist_settings_save", {"expected_revision": state["revision"], "settings": compatible})
                job = b.call("assist_start", {"conversation_id": thread, "message_id": message["id"]}, ACCOUNT)["job_id"]
                assert b.wait(job)["status"] == "completed"
                assert server.chats[-1][0] == "/v1/chat/completions"
                assert server.bodies[-1]["response_format"]["json_schema"]["strict"] is True
                checks.append("OpenAI-compatible provider")

                # Closing every client stops the queue.
                a.close()
                b.close()
                assert process.wait(timeout=15) == 0
                checks.append("owners closed: engine exits cleanly")
                print(json.dumps({"passed": True, "checks": checks, "requests": len(server.chats)}))
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)
    server.shutdown()


if __name__ == "__main__":
    main()
