#!/usr/bin/env python3
"""One native developer workflow on macOS, Windows and Linux; no app Python dependency."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools"
CONFIG = json.loads((ROOT / "scripts/toolchain.json").read_text())
HOST = platform.system()
WINDOWS = HOST == "Windows"
SUFFIX = ".exe" if WINDOWS else ""
VENV = TOOLS / "bootstrap"
VENV_BIN = VENV / ("Scripts" if WINDOWS else "bin")
PYTHON = VENV_BIN / ("python.exe" if WINDOWS else "python")


def run(*args, env=None, cwd=ROOT, capture=False, timeout=None):
    args = [str(arg) for arg in args]
    print("+ " + subprocess.list2cmdline(args), flush=True)
    result = subprocess.run(args, cwd=str(cwd), env=env, check=True,
                            text=True, stdout=subprocess.PIPE if capture else None, timeout=timeout)
    return result.stdout.strip() if capture else None


def rust_version():
    content = (ROOT / "rust-toolchain.toml").read_text()
    match = re.search(r'channel\s*=\s*"([0-9.]+)"', content)
    if not match:
        raise RuntimeError("rust-toolchain.toml must name an exact Rust version")
    return match.group(1)


def environment():
    if HOST not in CONFIG["targets"]:
        raise RuntimeError("Unsupported developer host: " + HOST)
    target = CONFIG["targets"][HOST]
    qt = Path(os.environ.get("FM_QT_ROOT", TOOLS / "Qt" / CONFIG["qt"] / target["qt_dir"]))
    env = os.environ.copy()
    local_cargo = TOOLS / "cargo" / "bin"
    env["PATH"] = os.pathsep.join([str(VENV_BIN), str(local_cargo), str(qt / "bin"), env.get("PATH", "")])
    if (TOOLS / "rustup").exists():
        env["RUSTUP_HOME"] = str(TOOLS / "rustup")
        env["CARGO_HOME"] = str(TOOLS / "cargo")
    env["FM_QT_ROOT"] = str(qt)
    return env


def bootstrap():
    if sys.version_info < (3, 9):
        raise RuntimeError("Python 3.9+ is required for developer tooling")
    if WINDOWS and platform.machine().lower() not in ("amd64", "x86_64"):
        raise RuntimeError("This initial Windows kit is x64; ARM64 needs its own recorded native kit")
    if HOST == "Linux" and platform.machine().lower() not in ("amd64", "x86_64"):
        raise RuntimeError("This initial Linux kit is x64")
    if HOST == "Darwin":
        run("xcrun", "--find", "clang++")
    TOOLS.mkdir(exist_ok=True)
    if not PYTHON.exists():
        run(sys.executable, "-m", "venv", VENV)
    requirements = ROOT / "scripts/bootstrap-requirements.txt"
    digest = hashlib.sha256(requirements.read_bytes()).hexdigest()
    stamp = VENV / ".requirements-sha256"
    if not stamp.exists() or stamp.read_text() != digest:
        run(PYTHON, "-m", "pip", "install", "--disable-pip-version-check", "-r", requirements)
        stamp.write_text(digest)
    env = environment()
    target = CONFIG["targets"][HOST]
    qt = Path(env["FM_QT_ROOT"])
    qtpaths = qt / "bin" / ("qtpaths" + SUFFIX)
    extra = target.get("extra_archives", [])
    # Some official kits put mandatory runtime libraries in separate archives.
    # Repair the managed SDK even when an earlier partial install has qtpaths.
    missing = [name for name in target.get("required_sdk_files", []) if not (qt / name).exists()]
    managed = "FM_QT_ROOT" not in os.environ
    if not qtpaths.exists() or (managed and missing):
        if not managed:
            raise RuntimeError("FM_QT_ROOT does not contain the selected Qt kit; unset it to provision the project kit")
        archives = extra if qtpaths.exists() else CONFIG["qt_archives"] + extra
        run(PYTHON, ROOT / "scripts/bootstrap_qt.py", "install-qt", target["aqt_host"], "desktop", CONFIG["qt"],
            target["aqt_arch"], "-O", TOOLS / "Qt", "--archives", *archives, env=env)
        missing = [name for name in target.get("required_sdk_files", []) if not (qt / name).exists()]
        if missing:
            raise RuntimeError("Official Qt kit is incomplete after acquisition: " + ", ".join(missing))
    selected = rust_version()
    compiler = shutil.which("rustc", path=env["PATH"])
    actual = run(compiler, "--version", env=env, capture=True).split()[1] if compiler else None
    if actual != selected:
        # Keep the existing Homebrew/global Rust installation and shell profile intact.
        env["RUSTUP_HOME"] = str(TOOLS / "rustup")
        env["CARGO_HOME"] = str(TOOLS / "cargo")
        installer = TOOLS / ("rustup-init.exe" if WINDOWS else "rustup-init.sh")
        url = ("https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe"
               if WINDOWS else "https://sh.rustup.rs")
        print("Downloading the official rustup installer to", installer, flush=True)
        urllib.request.urlretrieve(url, installer)
        args = [installer] if WINDOWS else ["sh", installer]
        run(*args, "-y", "--no-modify-path", "--profile", "minimal", "--default-toolchain", selected, env=env)
    doctor()


def doctor():
    env = environment()
    print("Host:", platform.platform(), platform.machine())
    print("Qt kit:", env["FM_QT_ROOT"])
    print("Source:", ROOT)
    failed = []
    checks = [("git", "--version"), ("rustc", "--version"), ("cargo", "--version"),
              ("cmake", "--version"), ("ninja", "--version"), ("qtpaths", "--qt-version")]
    for executable, argument in checks:
        found = shutil.which(executable, path=env["PATH"])
        if not found:
            print("MISSING:", executable)
            failed.append(executable)
            continue
        output = run(found, argument, env=env, capture=True).splitlines()[0]
        print(output, "at", found)
        expected = rust_version() if executable == "rustc" else CONFIG.get(executable)
        if executable == "ninja":
            # The Python distribution's fourth component is its packaging version.
            expected = ".".join(CONFIG["ninja"].split(".")[:3])
        if executable == "qtpaths":
            expected = CONFIG["qt"]
        if expected and expected not in output:
            failed.append(executable + " version (expected " + expected + ")")
    compiler = "cl.exe" if WINDOWS else "c++"
    print("Native C++ compiler:", shutil.which(compiler, path=env["PATH"]) or "MISSING")
    if not shutil.which(compiler, path=env["PATH"]):
        failed.append(compiler)
    if HOST == "Darwin":
        run("xcrun", "--show-sdk-version", env=env)
    if failed:
        raise RuntimeError("Environment needs setup: " + ", ".join(failed) + ". Run bootstrap.")


def configure_build(preset):
    doctor()
    env = environment()
    run(VENV_BIN / ("cmake" + SUFFIX), "--preset", preset, env=env)
    run(VENV_BIN / ("cmake" + SUFFIX), "--build", "--preset", preset, "--parallel", env=env)


def gui_binary(preset, installed=None):
    if installed is None:
        metadata = ROOT / "build" / preset / "gui-executable-path.txt"
        if not metadata.exists():
            raise RuntimeError("GUI build path is unavailable; run build first")
        return Path(metadata.read_text(encoding="utf-8").strip())
    base = Path(installed)
    if HOST == "Darwin":
        return base / "fuckmail-gui.app" / "Contents" / "MacOS" / "fuckmail-gui"
    return base / "bin" / ("fuckmail-gui" + SUFFIX)


def package_environment():
    """The installed product cannot depend on the developer SDK or shell setup."""
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(("FM_", "QT_", "QML", "QSG_", "DYLD_", "LD_LIBRARY_PATH")):
            env.pop(key, None)
    if WINDOWS:
        system = Path(env.get("SystemRoot", r"C:\Windows"))
        env["PATH"] = os.pathsep.join([str(system / "System32"), str(system)])
        env["QT_QPA_FONTDIR"] = str(system / "Fonts")
    else:
        env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env["QT_FORCE_STDERR_LOGGING"] = "1"
    return env


def engine_running(pid):
    if WINDOWS:
        import ctypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.restype = ctypes.c_void_p
        kernel.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        kernel.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        kernel.CloseHandle.argtypes = [ctypes.c_void_p]
        handle = kernel.OpenProcess(0x00100000, False, pid)  # SYNCHRONIZE
        if not handle:
            if ctypes.get_last_error() == 87:  # Process no longer exists.
                return False
            raise RuntimeError("Cannot verify packaged engine process exit")
        try:
            return kernel.WaitForSingleObject(handle, 0) == 0x102
        finally:
            kernel.CloseHandle(handle)
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def verify_engine_stopped(profile):
    discovery = profile / "engine.json"
    # Read only the PID for this local synthetic test; never log the handshake token.
    try:
        pid = json.loads(discovery.read_text())["pid"]
    except FileNotFoundError:
        pid = None
    deadline = time.monotonic() + 8
    while discovery.exists() or (pid is not None and engine_running(pid)):
        if time.monotonic() >= deadline:
            raise RuntimeError("Packaged engine remained after its final explicit client closed")
        time.sleep(0.05)
    print("Verified final-client engine shutdown:", profile.name, flush=True)


def verify_package(preset, stage):
    env = package_environment()
    with tempfile.TemporaryDirectory(prefix="fuckmail-package-check-") as scratch:
        scratch = Path(scratch).resolve()
        relocated = scratch / "relocated package"
        shutil.copytree(stage, relocated, symlinks=True)
        gui = gui_binary(preset, relocated)
        cli = gui.parent / ("fm" + SUFFIX)
        # A fresh terminal gets system PATH only, with no Rust or Qt SDK overrides.
        help_text = run(cli, "--help", env=env, cwd=relocated, capture=True, timeout=10)
        if "mail" not in help_text or "calendar" not in help_text:
            raise RuntimeError("Packaged CLI help is incomplete")
        profile = scratch / "cli-profile"
        reply = json.loads(run(cli, "--profile", profile, "--account", "windscribe", "--json",
                               "mail", "list", "--query", "Maya", "--field", "sender", "--limit", "5",
                               env=env, cwd=relocated, capture=True, timeout=45))
        rows = reply.get("result", {}).get("conversations", [])
        if not reply.get("ok") or not 1 <= len(rows) <= 5:
            raise RuntimeError("Packaged CLI sender query did not return bounded real-store results")
        verify_engine_stopped(profile)
        gui_profile = scratch / "gui-profile"
        smoke_env = env.copy()
        if HOST == "Darwin":
            smoke_env["DYLD_PRINT_LIBRARIES"] = "1"
        command = [str(gui), "--smoke", "--profile", str(gui_profile)]
        print("+ " + subprocess.list2cmdline(command), flush=True)
        smoke = subprocess.run(command, cwd=relocated, env=smoke_env, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45)
        log_dir = ROOT / "build" / "verification"
        log_dir.mkdir(parents=True, exist_ok=True)
        (log_dir / "package-runtime.log").write_text(smoke.stdout + smoke.stderr)
        if smoke.returncode:
            print(smoke.stdout + smoke.stderr)
            raise RuntimeError("Relocated packaged GUI smoke failed")
        if HOST == "Darwin":
            sdk = environment()["FM_QT_ROOT"]
            if sdk in smoke.stderr or "QtCore.framework" not in smoke.stderr:
                raise RuntimeError("Runtime trace did not prove Qt loaded from the relocated package")
            qt_lines = [line for line in smoke.stderr.splitlines() if "Qt" in line and ".framework/" in line]
            if not qt_lines or any(str(relocated) not in line for line in qt_lines):
                raise RuntimeError("A Qt framework loaded from outside the relocated package")
        verify_engine_stopped(gui_profile)
        run(gui, "--self-test", env=env, cwd=relocated, timeout=90)
        print("Relocated package passed: SDK-free GUI smoke/self-test, fresh-terminal CLI help/query, final engine shutdown", flush=True)


def package(preset):
    configure_build(preset)
    env = environment()
    stage = ROOT / "dist" / (HOST.lower() + "-" + platform.machine().lower())
    # The stage contains only build output. Profiles never live beneath dist.
    if stage.exists():
        shutil.rmtree(stage)
    run(VENV_BIN / ("cmake" + SUFFIX), "--install", ROOT / "build" / preset, "--prefix", stage, env=env)
    (stage / "README.txt").write_text(
        "fuckMail Phase 0 development build (unsigned)\n\n"
        "Contains synthetic mail only; no provider sign-in or real sending.\n"
        "Launch fuckmail-gui (inside the .app on macOS, bin elsewhere).\n"
        "The same directory contains the headless fm CLI and fm-engine.\n"
        "Use fm --help for CLI commands. Profiles are stored outside the package.\n"
        "Copy the folder/app to install; remove it to uninstall. User data is preserved.\n"
        "This is a development artifact, not a signed/notarized public release.\n",
        encoding="utf-8")
    guide = ROOT / "docs/build-development.md"
    if guide.exists():
        shutil.copy2(guide, stage / "BUILD-AND-LICENSE-NOTES.md")
    source = run("git", "rev-parse", "HEAD", env=env, capture=True)
    dirty = bool(run("git", "status", "--porcelain", env=env, capture=True))
    manifest = {"source": source, "source_dirty": dirty, "host": platform.platform(),
                "arch": platform.machine(), "preset": preset, "toolchain": CONFIG,
                "rust": run("rustc", "--version", env=env, capture=True),
                "cargo": run("cargo", "--version", env=env, capture=True)}
    (stage / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    verify_package(preset, stage)
    archive_base = ROOT / "dist" / ("fuckmail-" + HOST.lower() + "-" + platform.machine().lower())
    archive = shutil.make_archive(str(archive_base), "zip" if WINDOWS else "gztar", root_dir=stage.parent, base_dir=stage.name)
    print("Development package:", archive)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["bootstrap", "doctor", "build", "test", "run", "cli", "package"])
    parser.add_argument("--preset", choices=["dev", "release"], default="dev")
    args, extra = parser.parse_known_args()
    if extra and extra[0] == "--":
        extra = extra[1:]
    if extra and args.command not in ("run", "cli"):
        parser.error("extra arguments are accepted only by run and cli")
    os.chdir(ROOT)
    if args.command == "bootstrap":
        bootstrap()
    elif args.command == "doctor":
        doctor()
    elif args.command == "build":
        configure_build(args.preset)
    elif args.command == "test":
        configure_build(args.preset)
        run(VENV_BIN / ("ctest" + SUFFIX), "--preset", args.preset, env=environment())
    elif args.command == "run":
        configure_build(args.preset)
        run(gui_binary(args.preset), *extra, env=environment())
    elif args.command == "cli":
        profile = "debug" if args.preset == "dev" else "release"
        binary = ROOT / "target" / profile / ("fm" + SUFFIX)
        if not binary.exists():
            raise RuntimeError("CLI is not built; run build first")
        run(binary, *extra, env=environment())
    elif args.command == "package":
        package(args.preset)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        print("ERROR:", error, file=sys.stderr)
        sys.exit(1)
