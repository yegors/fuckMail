import hashlib
import json
import pathlib
import subprocess
import shutil
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from scripts import dev

name, scope, *command = sys.argv[1:]
out = pathlib.Path(__file__).resolve().parent
tested = {command[index + 1] for index, value in enumerate(command[:-1]) if value in ("-p", "--package")}

def inventory():
    paths = [ROOT / item for item in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]
    for package in scope.split(","):
        for p in (ROOT / "crates" / package).rglob("*"):
            if not p.is_file() or not (p.suffix in (".rs", ".sql", ".json") or p.name == "Cargo.toml"):
                continue
            relative = p.relative_to(ROOT / "crates" / package)
            if package not in tested and ("tests" in relative.parts or p.name == "tests.rs" or p.name.endswith("_tests.rs")):
                continue
            paths.append(p)
    paths.extend(p for p in (ROOT / "fixtures").rglob("*") if p.is_file())
    return {p.relative_to(ROOT).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(set(paths))}
source = inventory()
for suffix in ("log", "source.json", "result.json"):
    if (out / f"{name}.{suffix}").exists():
        raise SystemExit("refusing to replace an existing attempt")
(out / f"{name}.source.json").write_text(json.dumps(source, indent=2) + "\n")
start = time.monotonic()
environment = dev.environment()
command[0] = shutil.which(command[0], path=environment["PATH"]) or command[0]
with (out / f"{name}.log").open("w", encoding="utf-8") as log:
    try:
        code = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT).returncode
    except OSError as error:
        log.write(f"Tool launch failed before compilation: {type(error).__name__}: {error}\n")
        code = 127
after = inventory()
changed = sorted(p for p in source.keys() | after.keys() if source.get(p) != after.get(p))
report = {"command":command,"scope":scope,"dependency_test_sources_excluded":sorted(set(scope.split(","))-tested),"exit_code":code,"elapsed_seconds":time.monotonic()-start,"source_count":len(source),"source_changes":changed,"status":"passed" if code == 0 and not changed else "failed","source_sha256":hashlib.sha256((out / f"{name}.source.json").read_bytes()).hexdigest(),"log_sha256":hashlib.sha256((out / f"{name}.log").read_bytes()).hexdigest()}
(out / f"{name}.result.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps(report))
raise SystemExit(0 if report["status"]=="passed" else 1)
