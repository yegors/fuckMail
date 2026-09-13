"""Verify the synthetic Calendar upgrade seed against preserved schema-16 binaries."""
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts"))
from dev import package_environment
from verify_draft_snapshot_upgrade import seed_calendar, verify_calendar
from verify_upgrade import Check, durable_snapshot, require, sha256

packet = Path(__file__).resolve().parent
files = {name: packet / "baseline" / (name + ".exe") for name in ("fm", "fm-engine")}
report = packet / "calendar-baseline-seed-01.json"
result = {"result": "failed", "schema": 16, "scope": "Preserved old engine/CLI and generated synthetic Calendar receipts only",
          "executables": {key: {"path": str(path), "sha256": sha256(path)} for key, path in files.items()},
          "helper_sha256": sha256(ROOT / "scripts/verify_draft_snapshot_upgrade.py")}
with report.open("x", encoding="utf-8") as output:
    try:
        with tempfile.TemporaryDirectory(prefix="fm-calendar-upgrade-seed-") as temporary:
            check = Check(Path(temporary), package_environment())
            try:
                engine = check.start_engine(files, "baseline-engine")
                owner = check.start_owner(files, "baseline-owner", seconds=300)
                retained = seed_calendar(check, files)
                verify_calendar(check, files, retained)
                result["retained"] = retained
                owner.terminate()
                owner.wait(timeout=10)
                require(engine.wait(timeout=10) == 0 and check.pid() is None, "Final old owner did not drain")
                result["snapshot"] = durable_snapshot(check.profile)
                require(result["snapshot"]["schema"] == 16, "Wrong preserved baseline schema")
            finally:
                check.close()
                for log in check.outputs:
                    source = Path(log.name)
                    target = packet / ("calendar-baseline-seed-01." + source.name)
                    with target.open("xb") as destination:
                        destination.write(source.read_bytes())
        result["result"] = "passed"
    except Exception as error:
        result["error"] = str(error)
    json.dump(result, output, indent=2)
    output.write("\n")
print(json.dumps({key: result[key] for key in ("result", "error") if key in result}))
raise SystemExit(0 if result["result"] == "passed" else 1)
