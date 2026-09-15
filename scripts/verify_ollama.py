#!/usr/bin/env python3
"""Synthetic Ollama/engine integration: no real accounts, downloads or model service."""
import argparse
import http.server
import json
import pathlib
import socket
import subprocess
import tempfile
import threading
import time

class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True
    calls = 0
    mode = "valid"
    started = threading.Event()
    release = threading.Event()

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self): self.handle_api()
    def do_POST(self): self.handle_api()
    def handle_api(self):
        server = self.server
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if self.path == "/api/version": result = {"version": "fixture"}
        elif self.path == "/api/tags": result = {"models": [{"name":"fixture:latest","digest":"a"*64,"size":5000000}, {"name":"remote:cloud","digest":"b"*64,"size":1}]}
        elif self.path == "/api/show": result = {"capabilities":["completion"],"details":{"format":"gguf"}}
        elif self.path == "/api/chat":
            server.calls += 1
            mode = server.mode
            server.started.set()
            if mode == "delay": server.release.wait(8)
            schema = body["format"]["properties"]
            data = json.loads(body["messages"][-1]["content"])
            if "summary" in schema: output = {"summary":"A design review is discussed.","source_ids":[data["messages"][0]["id"]]}
            else: output = {"sender_kind":"human","category_ids":[data["labels"][0]["id"]] if data["labels"] else [],"category_abstain":False,"reason":"Personal correspondence."}
            if mode == "invalid": output["read"] = True
            if mode == "oversized": output={"summary":"x"*300000,"source_ids":["sample-message"]}
            assert "tools" not in body
            result = {"model":"fixture:latest","done":True,"done_reason":"stop","message":{"content":json.dumps(output)}}
        else:
            self.send_error(404); return
        raw = json.dumps(result).encode()
        try:
            self.send_response(200); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(raw))); self.end_headers(); self.wfile.write(raw)
        except (BrokenPipeError,ConnectionResetError): pass

class Client:
    def __init__(self, discovery):
        self.sock=socket.socket(socket.AF_UNIX); self.sock.settimeout(10); self.sock.connect(discovery["endpoint"])
        self.file=self.sock.makefile("rb"); self.sequence=0
        self.call("hello",{"token":discovery["token"],"client":"ollama-verifier"})
    def close(self): self.file.close(); self.sock.close()
    def call(self, method, params=None, account=None, error=False):
        self.sequence+=1
        req={"version":1,"id":str(self.sequence),"method":method,"params":params or {}}
        if account: req["account"]=account
        self.sock.sendall(json.dumps(req).encode()+b"\n"); result=json.loads(self.file.readline())
        if error: assert "error" in result, result; return result["error"]
        assert "error" not in result,result
        return result["result"]
    def wait(self, job):
        end=time.monotonic()+15
        while time.monotonic()<end:
            result=self.call("assist_job",{"job_id":job})
            if result["status"] not in ("queued","running"): return result
            time.sleep(.04)
        raise AssertionError("Job did not complete")

def main():
    parser=argparse.ArgumentParser(); parser.add_argument("--engine",required=True); args=parser.parse_args()
    server=Server(("127.0.0.1",0),Handler); threading.Thread(target=server.serve_forever,daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="fm-ollama-") as root:
        profile=pathlib.Path(root)/"profile"; profile.mkdir(mode=0o700)
        with open(pathlib.Path(root)/"engine.log","w") as log:
            process=subprocess.Popen([str(pathlib.Path(args.engine).resolve()),"--profile",str(profile),"--demo-profile"],stdout=log,stderr=log)
            try:
                deadline=time.monotonic()+20
                while not (profile/"engine.json").exists():
                    assert process.poll() is None,"Engine startup failed"
                    assert time.monotonic()<deadline,"No discovery"
                    time.sleep(.05)
                discovery=json.loads((profile/"engine.json").read_text()); a=Client(discovery); b=Client(discovery)
                assert a.call("status")["schema_version"]==21
                state=a.call("assist_settings_get"); settings=state["settings"]
                assert not settings["accounts"] and server.calls==0
                settings["endpoint"]="http://127.0.0.1:"+str(server.server_port)
                catalog=a.call("assist_discover",{"endpoint":settings["endpoint"]})
                assert len(catalog["models"])==2 and not catalog["models"][1]["eligible"]
                model={"name":"fixture:latest","digest":"a"*64}
                settings["summary_model"]=model;settings["classification_model"]=model
                settings["accounts"]={"windscribe":{"summaries":True,"classification":False,"descriptions":{}}}
                a.call("assist_settings_save",{"expected_revision":0,"settings":settings})
                assert server.calls==0,"Saving settings triggered inference"
                summary_only={"task":"summary","conversation_id":"windscribe-thread-001","message_id":"windscribe-message-001-000"}
                independent_summary=a.call("assist_start",summary_only,"windscribe")["job_id"]
                assert a.wait(independent_summary)["status"]=="completed"
                assert a.call("assist_start",dict(summary_only,task="classification"),"windscribe",error=True)["code"]=="assistance_disabled"
                settings["accounts"]["windscribe"]["classification"]=True
                a.call("assist_settings_save",{"expected_revision":1,"settings":settings})
                a.call("assist_start",{"task":"summary","sample":True,"read":True},error=True)
                remote=json.loads(json.dumps(settings));remote["summary_model"]={"name":"remote:cloud","digest":"b"*64}
                blocked=a.call("assist_start",{"task":"summary","sample":True,"settings":remote})["job_id"]
                assert a.wait(blocked)["status"]=="failed" and server.calls==1

                assert b.call("assist_settings_get")["settings"]==settings
                params={"task":"classification","conversation_id":"windscribe-thread-001","message_id":"windscribe-message-001-000"}
                before=a.call("conversation",{"id":params["conversation_id"]},"windscribe")
                job=a.call("assist_start",params,"windscribe")["job_id"]
                assert b.call("assist_job",{"job_id":job},error=True)["code"]=="not_found"
                assert a.wait(job)["status"]=="completed"
                after=a.call("conversation",{"id":params["conversation_id"]},"windscribe")
                assert [(m["unread"],m["starred"]) for m in before["messages"]]==[(m["unread"],m["starred"]) for m in after["messages"]]
                suggestions=a.call("assist_results",{"conversation_id":params["conversation_id"]},"windscribe")["items"]
                suggestion=next(r for r in suggestions if r["task"]=="classification" and r["target"]==params["message_id"])
                label=suggestion["output"]["category_ids"][0]
                a.call("assist_accept",{"conversation_id":params["conversation_id"],"message_id":params["message_id"],"category_id":label,"request_id":"ollama-accept-label"},"windscribe")
                accepted=a.call("conversation",{"id":params["conversation_id"]},"windscribe")
                assert label in next(m for m in accepted["messages"] if m["id"]==params["message_id"])["categories"]
                assert [(m["id"],m["categories"]) for m in before["messages"] if m["id"]!=params["message_id"]]==[(m["id"],m["categories"]) for m in accepted["messages"] if m["id"]!=params["message_id"]]
                assert [(m["unread"],m["starred"]) for m in before["messages"]]==[(m["unread"],m["starred"]) for m in accepted["messages"]]
                assert not next(r for r in a.call("assist_results",{"conversation_id":params["conversation_id"]},"windscribe")["items"] if r["task"]=="classification")["stale"]
                after=accepted
                summary=a.call("assist_start",dict(params,task="summary"),"windscribe")["job_id"]
                assert a.wait(summary)["items"][0]["output"]["summary"]
                settings["summary_prompt"]+="\nKeep it brief."
                a.call("assist_settings_save",{"expected_revision":2,"settings":settings})
                current=a.call("assist_results",{"conversation_id":params["conversation_id"]},"windscribe")["items"]
                assert not next(r for r in current if r["task"]=="classification")["stale"],"Summary settings invalidated independent classification"
                assert next(m for m in a.call("conversation",{"id":params["conversation_id"]},"windscribe")["messages"] if m["id"]==params["message_id"])["kind"]=="people"
                cli=pathlib.Path(args.engine).resolve().with_name("fm")
                typed=subprocess.run([str(cli),"--profile",str(profile),"--engine",str(pathlib.Path(args.engine).resolve()),"--json","assist","test","summary"],capture_output=True,text=True,timeout=20)
                assert typed.returncode==0,typed.stdout+typed.stderr
                assert json.loads(typed.stdout)["result"]["status"]=="completed"
                server.mode="invalid"
                invalid=a.call("assist_start",{"task":"classification","sample":True})["job_id"]
                assert a.wait(invalid)["status"]=="failed"
                server.mode="oversized"
                huge=a.call("assist_start",{"task":"summary","sample":True})["job_id"]
                assert a.wait(huge)["status"]=="failed"
                server.mode="delay";server.started.clear();server.release.clear()
                late=a.call("assist_start",params,"windscribe")["job_id"]
                assert server.started.wait(5)
                member=next(m for m in after["messages"] if m["id"]==params["message_id"])
                b.call("assist_correct",{"conversation_id":params["conversation_id"],"message_id":member["id"],"kind":"automated","expected_revision":member["revision"]},"windscribe")
                server.release.set(); assert a.wait(late)["status"]=="failed"
                assert a.call("assist_results",{"conversation_id":params["conversation_id"]},"windscribe")["items"][0]["stale"]
                server.started.clear();server.release.clear()
                canceled=a.call("assist_start",{"task":"summary","sample":True})["job_id"]
                assert server.started.wait(5)
                a.call("assist_cancel",{"job_id":canceled}); assert a.wait(canceled)["status"]=="canceled"
                server.release.set();time.sleep(.1);server.started.clear();server.release.clear()
                disconnected=a.call("assist_start",{"task":"summary","sample":True})["job_id"]
                assert server.started.wait(5)
                a.close();server.release.set();server.mode="valid"
                assert b.call("status")["schema_version"]==21
                independent=b.call("assist_start",{"task":"summary","sample":True})["job_id"]
                assert b.wait(independent)["status"]=="completed"
                b.close();assert process.wait(timeout=10)==0
                print(json.dumps({"passed":True,"checks":["off has no inference","discovery/remote exclusion","settings parity","session isolation","summary-only operation","independent task settings","classification/flags","explicit label acceptance/flags","summary coverage","invalid output","in-flight correction","cancellation","independent owner","final shutdown"],"inference_requests":server.calls}))
            finally:
                if process.poll() is None:process.terminate();process.wait(timeout=10)
    server.shutdown()
if __name__=="__main__":main()
