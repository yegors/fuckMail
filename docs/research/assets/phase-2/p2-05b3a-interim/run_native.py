import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
import scripts.dev as d

OUT = ROOT / "build/verification/p2-05b3a"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def source_manifest():
    paths = [ROOT / p for p in ("CMakeLists.txt", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]
    for directory in ("gui", "crates", "fixtures", "scripts"):
        paths.extend(p for p in (ROOT / directory).rglob("*") if p.is_file() and "__pycache__" not in p.parts)
    return {p.relative_to(ROOT).as_posix(): digest(p) for p in sorted(set(paths))}


def binaries():
    paths = list((ROOT / "build/dev").glob("*.exe"))
    paths += list((ROOT / "target/debug/deps").glob("sessions-*.exe"))
    paths += [ROOT / "target/debug" / p for p in ("fm-engine.exe", "fm.exe")]
    return {p.relative_to(ROOT).as_posix(): {"size": p.stat().st_size, "sha256": digest(p)}
            for p in sorted(set(paths)) if p.is_file()}


def run(args, log):
    with log.open("x", encoding="utf-8") as stream:
        command = "+ " + subprocess.list2cmdline([str(p) for p in args])
        print(command, flush=True)
        stream.write(command + "\n")
        stream.flush()
        child = subprocess.Popen([str(p) for p in args], cwd=ROOT, env=d.environment(),
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
        for line in child.stdout:
            print(line, end="", flush=True)
            stream.write(line)
            stream.flush()
        return child.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=("build", "ctest"))
    parser.add_argument("--attempt", default="01")
    parser.add_argument("--integration-watchdog", type=int, choices=(150, 180), default=150)
    parser.add_argument("--aggregate-watchdog-adjusted-under", choices=("D030",))
    args = parser.parse_args()
    if (args.integration_watchdog == 180) != (args.aggregate_watchdog_adjusted_under == "D030"):
        parser.error("The 180-second aggregate watchdog requires explicit D030 attribution")
    stem = "gui-dev-" + ("build-" if args.phase == "build" else "ctest-") + args.attempt
    source_path = OUT / ("gui-dev-source-" + args.attempt + ".json") if args.phase == "build" else OUT / (stem + ".source.json")
    result_path = OUT / ("gui-dev-result-" + args.attempt + ".json") if args.phase == "build" else OUT / (stem + ".result.json")
    if source_path.exists() or result_path.exists() or (OUT / (stem + ".log")).exists():
        raise RuntimeError("This numbered native attempt already exists")
    sources = source_manifest()
    before = binaries()
    write(source_path, sources)
    before_path = OUT / (stem + ".binaries-before.json")
    after_path = OUT / (stem + ".binaries-after.json")
    write(before_path, before)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    clock = time.monotonic()
    if args.phase == "build":
        command = [d.VENV_BIN / ("cmake" + d.SUFFIX), "--build", "build/dev", "--parallel", "8"]
    else:
        configured = json.loads(subprocess.check_output(
            [str(d.VENV_BIN / ("ctest" + d.SUFFIX)), "--test-dir", "build/dev", "--show-only=json-v1"],
            cwd=ROOT, env=d.environment(), text=True))
        integration_test = next(test for test in configured["tests"] if test["name"] == "gui-integration")
        timeout = next(prop["value"] for prop in integration_test["properties"] if prop["name"] == "TIMEOUT")
        if timeout != args.integration_watchdog:
            raise RuntimeError("Configured integration timeout differs from the recorded allowance")
        command = [d.VENV_BIN / ("ctest" + d.SUFFIX), "--test-dir", "build/dev", "-R", "^gui-", "-V", "--output-on-failure",
                   "--output-junit", OUT / (stem + ".junit.xml")]
    exit_code = run(command, OUT / (stem + ".log"))
    elapsed = time.monotonic() - clock
    finished = datetime.datetime.now(datetime.timezone.utc).isoformat()
    after = binaries()
    write(after_path, after)
    changed_sources = [p for p, h in sources.items() if not (ROOT / p).is_file() or digest(ROOT / p) != h]
    changed_binaries = [p for p in sorted(before.keys() | after.keys()) if before.get(p) != after.get(p)]
    result = {"status": "build_passed" if args.phase == "build" and exit_code == 0 else "passed" if exit_code == 0 else "failed",
              "configuration": "build/dev: Debug, FM_GUI_ONLY=OFF, BUILD_TESTING=TRUE", "host": platform.platform(),
              "source_manifest": source_path.name, "source_count": len(sources), "source_unchanged": not changed_sources,
              "changed_source_paths": changed_sources, "changed_binary_paths": changed_binaries, "binaries": after,
              "wall_elapsed_seconds": elapsed, "exit_code": exit_code}
    meta = {"exit_code": exit_code, "started_utc": started, "finished_utc": finished, "elapsed_seconds": elapsed,
            "native_selection": "^gui-" if args.phase == "ctest" else None,
            "includes_rust_build_fixture": args.phase == "ctest", "reruns_rust_workspace_tests": False,
            "watchdogs_unchanged": not bool(args.aggregate_watchdog_adjusted_under),
            "aggregate_watchdog_adjusted_under": args.aggregate_watchdog_adjusted_under,
            "aggregate_watchdog_seconds": args.integration_watchdog,
            "primary_watchdog_seconds": 120, "child_watchdog_seconds": 45,
            "primary_and_child_watchdogs_unchanged": True, "all_assertions_preserved": True,
            "QT_MESSAGE_PATTERN": os.environ.get("QT_MESSAGE_PATTERN")}
    write(OUT / (stem + ".meta.json"), meta)
    if args.phase == "build":
        result["tests_run"] = False
        result["source_unchanged_after_build"] = not changed_sources
        result["build_log"] = stem + ".log"
    else:
        last = ROOT / "build/dev/Testing/Temporary/LastTest.log"
        shutil.copyfile(last, OUT / (stem + ".LastTest.log"))
        root = ET.parse(OUT / (stem + ".junit.xml")).getroot()
        cases = [{"name": case.attrib["name"], "status": case.attrib.get("status"), "seconds": float(case.attrib.get("time", 0))}
                 for case in root.iter("testcase")]
        failures = sum(1 for case in root.iter("testcase") if case.find("failure") is not None or case.find("error") is not None)
        log = (OUT / (stem + ".log")).read_text(encoding="utf-8")
        diagnostics = [{"line": i, "text": line} for i, line in enumerate(log.splitlines(), 1)
                       if re.search(r"DelegateModel::cancel|warning C\d+|(?:^|\s)(?:warning:|QML [^:]+:|QQml\w*:|qt\.[\w.]+:)", line)]
        total = re.search(r"Total Test time \(real\) =\s*([\d.]+) sec", log)
        integration = next((case["seconds"] for case in cases if case["name"] == "gui-integration"), 0)
        result.update(tests=len(cases), native_tests=sum(c["name"].startswith("gui-") for c in cases),
                      build_fixtures=sum(c["name"] == "rust-test-build" for c in cases), failures=failures, cases=cases,
                      ctest_elapsed_seconds=float(total.group(1)) if total else None, integration_seconds=integration,
                      integration_watchdog_seconds=args.integration_watchdog,
                      integration_watchdog_margin_seconds=args.integration_watchdog-integration,
                      diagnostic_count=len(diagnostics), diagnostics=diagnostics,
                      limits=["Normal Windows development runtime; no package or navigation-performance acceptance",
                              "Includes Rust --workspace --no-run build fixture and one explicit startup-recovery Rust test; does not rerun workspace tests",
                              "Raw native output and matching diagnostics retained; no warning-free claim"])
        result["actual_delegate_model_cancel_emissions"] = (OUT / (stem + ".LastTest.log")).read_text(encoding="utf-8").count("DelegateModel::cancel")
        primary = re.search(r"t=\s*([\d.]+)\] GUI self-test passed:", log)
        diagnostic = {"QT_MESSAGE_PATTERN": os.environ.get("QT_MESSAGE_PATTERN"),
                      "watchdog_seconds": args.integration_watchdog,
                      "source_changes": bool(changed_sources),
                      "aggregate_watchdog_adjusted_under": args.aggregate_watchdog_adjusted_under,
                      "aggregate_watchdog_seconds": args.integration_watchdog,
                      "primary_watchdog_seconds": 120, "child_watchdog_seconds": 45,
                      "primary_and_child_watchdogs_unchanged": True, "all_assertions_preserved": True,
                      "primary_success_qt_elapsed_seconds": float(primary.group(1)) if primary else None,
                      "primary_approximate_margin_seconds": 120-float(primary.group(1)) if primary else None,
                      "limits": ["Qt elapsed time starts at the first Qt message; primary margin is approximate",
                                 "D030 changes the aggregate harness allowance only; no startup or input-to-paint performance correction is claimed",
                                 "Failed native01 and native02 remain retained"]}
        write(OUT / (stem + ".diagnostic.json"), diagnostic)
    artifact_paths = [source_path, before_path, after_path, OUT / (stem + ".log"), OUT / (stem + ".meta.json")]
    if args.phase == "ctest":
        artifact_paths += [OUT / (stem + ".LastTest.log"), OUT / (stem + ".junit.xml"), OUT / (stem + ".diagnostic.json")]
    result["artifacts_sha256"] = {p.name: digest(p) for p in artifact_paths}
    write(result_path, result)
    print(json.dumps({k: v for k, v in result.items() if k not in ("binaries", "cases", "artifacts_sha256")}, indent=2), flush=True)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
