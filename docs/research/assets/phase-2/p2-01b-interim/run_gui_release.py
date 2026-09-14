import argparse
import json
import platform
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_native as native

parser = argparse.ArgumentParser()
parser.add_argument("--attempt", required=True)
args = parser.parse_args()
stem = "gui-release-build-" + args.attempt
def source_inventory():
    result = {path.relative_to(native.ROOT).as_posix(): native.digest(path)
              for path in sorted((native.ROOT / "gui").rglob("*")) if path.is_file()}
    result["CMakeLists.txt"] = native.digest(native.ROOT / "CMakeLists.txt")
    return result

sources = source_inventory()
native.write(native.OUT / (stem + ".source.json"), sources)
start = time.monotonic()
code = native.run([native.d.VENV_BIN / ("cmake" + native.d.SUFFIX), "--build", "build/p2-08a-gui-release",
                   "--target", "fuckmail-gui", "--parallel", "8"], native.OUT / (stem + ".log"))
path = native.ROOT / "build/p2-08a-gui-release/fuckmail-gui.exe"
result = {"kind": "development-gui-only-release", "build_dir": "build/p2-08a-gui-release",
          "host": platform.platform(), "source_inventory": stem + ".source.json", "build_passed": code == 0,
          "tests_run": False, "binary": {"path": path.relative_to(native.ROOT).as_posix(),
                                         "sha256": native.digest(path), "size": path.stat().st_size},
          "elapsed_seconds": time.monotonic() - start,
          "source_unchanged": source_inventory() == sources}
native.write(native.OUT / (stem + ".result.json"), result)
print(json.dumps(result, indent=2), flush=True)
raise SystemExit(0 if code == 0 and result["source_unchanged"] else 1)
