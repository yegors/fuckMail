"""Build ordinary Release binaries and test isolated Release engine/CLI paths."""
from datetime import datetime, timezone
from pathlib import Path
import argparse
import json
import os
import re
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/"scripts"))
from verify_google_navigation import source_manifest as all_sources, require
from verify_search_ingestion import sha256

def source_manifest():
    result = {name: value for name, value in all_sources().items() if name.startswith("crates/") or name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}}
    result.update({path.relative_to(ROOT).as_posix():sha256(path) for path in (ROOT/"fixtures").rglob("*") if path.is_file()})
    return result

packet=Path(__file__).resolve().parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("--attempt",type=int,default=1)
parser.add_argument("--stage",choices=("all","ordinary","engine","cli"),default="all")
args=parser.parse_args()
require(1 <= args.attempt <= 99,"Use a bounded numbered attempt")
attempt=f"{args.attempt:02}"
env=os.environ.copy()
env["CARGO_HOME"]=str(ROOT/".tools/cargo")
env["RUSTUP_HOME"]=str(ROOT/".tools/rustup")
cargo=ROOT/".tools/cargo/bin/cargo.exe"
checks=[("rust-production-build-"+attempt,["build","--release","--locked","-p","fm-engine","-p","fm-cli"],
         {"engine":ROOT/"target/release/fm-engine.exe","cli":ROOT/"target/release/fm.exe"}),
        ("feature-tests-"+attempt,["test","--release","--locked","-p","fm-engine","--features","verification-google",
                             "--target-dir","target/verification-google","--bin","fm-engine","--test","verification_profile"],
         {"engine":ROOT/"target/verification-google/release/fm-engine.exe"}),
        ("release-cli-feature-tests-"+attempt,["test","--release","--locked","-p","fm-cli","--features","verification-google",
                             "--target-dir","target/verification-google","--bin","fm","--test","arguments"],
         {"cli":ROOT/"target/verification-google/release/fm.exe"})]
for stage,(stem,command,binaries) in zip(("ordinary","engine","cli"),checks,strict=True):
    if args.stage not in ("all",stage): continue
    log,report,manifest=[packet/(stem+suffix) for suffix in (".log",".result.json",".source.json")]
    require(all(not path.exists() for path in (log,report,manifest)),"Never overwrite a Release attempt")
    before=source_manifest()
    manifest.write_text(json.dumps(before,indent=2)+"\n",encoding="utf-8")
    print("Running "+stem,flush=True)
    started=time.monotonic()
    with log.open("wb") as output:
        completed=subprocess.run([str(cargo),*command],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=output,stderr=subprocess.STDOUT)
    elapsed=time.monotonic()-started
    after=source_manifest()
    changed=sorted(name for name in before.keys()|after.keys() if before.get(name)!=after.get(name))
    text=log.read_text(encoding="utf-8",errors="replace")
    suites=re.findall(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; .*finished in ([\d.]+)s",text,re.MULTILINE)
    result={"status":"passed" if completed.returncode==0 and not changed else "failed","exit_code":completed.returncode,
            "recorded_at_utc":datetime.now(timezone.utc).isoformat(),"elapsed_seconds":elapsed,"command":command,
            "source_manifest_sha256":sha256(manifest),"changed_sources":changed,"log_sha256":sha256(log),
            "executables":{name:{"path":str(path),"sha256":sha256(path),"size":path.stat().st_size} for name,path in binaries.items() if path.is_file()},
            "suites":[{"status":row[0],"passed":int(row[1]),"failed":int(row[2]),"ignored":int(row[3]),"seconds":float(row[4])} for row in suites]}
    report.write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({key:result[key] for key in ("status","exit_code","elapsed_seconds","suites","changed_sources")}),flush=True)
    if result["status"]!="passed": raise SystemExit(1)
