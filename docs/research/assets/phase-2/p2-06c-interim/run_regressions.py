"""Serial existing-workflow functional regressions for P2-06C."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
CLI = ROOT / "target/release/fm.exe"
AUTH_CLI = ROOT / "target/verification-google/release/fm.exe"
ENGINE = ROOT / "target/verification-google/release/fm-engine.exe"
ORDINARY = ROOT / "target/release/fm-engine.exe"
GUI = ROOT / "build/p2-08a-gui-release/fuckmail-gui.exe"
QT = ROOT / ".tools/Qt/6.11.2/msvc2022_64"
common = ["--cli", CLI, "--engine", ENGINE]
native = ["--gui", GUI, "--qt-root", QT]
checks = [
    ("clicalendar-01", "verify_google_calendar.py", [*common, "--ordinary-engine", ORDINARY]),
    ("nativecalendar-01", "verify_google_calendar_gui.py", [*common, *native]),
    ("cliaccounts-01", "verify_google_accounts.py", [*common, "--auth-cli", AUTH_CLI, "--ordinary-engine", ORDINARY]),
    ("nativeaccounts-01", "verify_google_accounts_gui.py", [*common, *native]),
    ("draftsend-01", "verify_google_drafts.py", [*common, "--send"]),
    ("draftactions-01", "verify_google_draft_actions.py", common),
    ("remotedrafts-01", "verify_google_remote_drafts.py", [*common, *native]),
]
for name, script, arguments in checks:
    print("Running " + name, flush=True)
    command = [sys.executable, "-B", ROOT / "scripts" / script, *arguments, "--report", OUT / (name + ".json")]
    completed = subprocess.run([str(value) for value in command], cwd=ROOT)
    if completed.returncode:
        raise SystemExit(completed.returncode)
