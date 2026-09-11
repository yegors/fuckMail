# Build, run and move between machines

This is the Phase 0 developer workflow. The app is Rust engine/CLI plus a C++20 Qt Quick/QML client and SQLite. Python is used only to orchestrate developer tools; the packaged app has no Python, Node.js or JavaScript build dependency.

## First setup

macOS, from the checkout:

```sh
scripts/dev.sh bootstrap
scripts/dev.sh doctor
scripts/dev.sh test
scripts/dev.sh run
```

Windows, from PowerShell in the checkout:

```powershell
./scripts/dev.ps1 bootstrap
./scripts/dev.ps1 doctor
./scripts/dev.ps1 test
./scripts/dev.ps1 run
```

Bootstrap detects existing tools, creates `.tools/bootstrap`, installs locked Python developer dependencies, and downloads the selected Qt desktop kit into `.tools/Qt`. Existing Homebrew Rust is reused when it matches the project version. Otherwise a project-local rustup installation is used without rewriting shell profiles or replacing existing installations.

macOS requires Apple Command Line Tools (`xcode-select --install` if absent). On Windows the wrapper imports an existing VS 2022 C++ environment. During bootstrap it can use `winget` to install missing Visual Studio 2022 C++ Build Tools/Windows SDK and Python 3.11; system installation may show an OS elevation prompt. Existing VS installations without the C++ workload need that workload added. Normal developer commands do not install tools. Initial Windows coverage is x64, not ARM64 or a WSL build.

Ubuntu 24.04 uses the same `scripts/dev.sh` commands after installing a C++ compiler, Python with venv support and the native graphics libraries listed in [the native workflow](../.github/workflows/native.yml). Linux SDK acquisition is included; Arch/Omarchy desktops and other release floors still need their own execution evidence.

## Everyday commands

| Purpose | Mac/Linux | Windows |
| --- | --- | --- |
| Environment versions and paths | `scripts/dev.sh doctor` | `./scripts/dev.ps1 doctor` |
| Incremental build | `scripts/dev.sh build` | `./scripts/dev.ps1 build` |
| Build and run fixture tests | `scripts/dev.sh test` | `./scripts/dev.ps1 test` |
| Build and open GUI | `scripts/dev.sh run` | `./scripts/dev.ps1 run` |
| CLI help | `scripts/dev.sh cli -- --help` | `./scripts/dev.ps1 cli -- --help` |
| Optimized development package | `scripts/dev.sh package --preset release` | `./scripts/dev.ps1 package --preset release` |

GUI/CLI arguments follow `--`, for example `scripts/dev.sh run -- --profile /absolute/local/demo-profile`. Use the same profile for both clients. The engine creates a private profile directory; a new profile's parent must already exist. The normal demo is synthetic and never authenticates with a provider. Do not point test commands at a real mailbox profile.

`--preset dev` is the default; `release` uses optimized Rust and C++ builds. Both use the tracked [CMake presets](../CMakePresets.json), [tool inputs](../scripts/toolchain.json), [Python dependency lock](../scripts/bootstrap-requirements.txt) and Rust toolchain/lockfile. Builds copy `fm` and `fm-engine` beside the GUI executable so engine discovery is the same as in the package. The CLI itself does not load Qt or need a display.

An advanced Qt-only compilation is available with CMake `-DFM_GUI_ONLY=ON` in a separate build directory while editing UI. It deliberately excludes engine integration, tests and installation; it is not a working app or an acceptance result. Standard dev/release presets always build the complete slice.

## Selected tooling and provenance

| Input | Selected version / route |
| --- | --- |
| Rust | 1.88.0, exact `rust-toolchain.toml`; Cargo workspace with locked dependencies |
| Qt | 6.11.2 official binary archives, dynamically linked |
| macOS Qt kit | `clang_64`, installed as `macos`, universal Qt libraries; app built for the current host |
| Windows Qt kit | `win64_msvc2022_64`, VS 2022 x64 compiler/SDK |
| Linux Qt kit | `linux_gcc_64`, Ubuntu 24.04 CI baseline |
| CMake | 3.31.6 Python distribution |
| Ninja | 1.11.1.4 Python distribution (executable reports 1.11.1 with a Kitware suffix) |
| Qt acquisition | aqtinstall 3.3.0 with locked dependencies; no Qt account required |

Qt 6.11.2 was released August 18, 2026. The five shared archives are `qtbase`, `qtdeclarative`, `qttools`, `qtsvg` and `qttranslations`; the Linux kit also needs its official `icu` archive. WebEngine is excluded. The current upstream platform matrix lists macOS 13+, Windows with MSVC 2022 and Linux configurations. Our own app coverage is limited to recorded execution evidence, not the whole upstream list. [Qt release](https://www.qt.io/blog/qt-6.11.2-released), [Qt platform matrix](https://doc.qt.io/qt-6/supported-platforms.html).

Qt 6.11 changed Windows repository directories. The pinned aqt release still computes the old path. [The checked-in adapter](../scripts/bootstrap_qt.py) corrects the repository suffix only for Qt 6.11.2 Windows desktop MSVC x64, leaving checksum verification, download, extraction and relocation to aqt. It refuses an unreviewed aqt version and never edits installed dependency files. Revisit it when either pin changes. [Upstream issue 1007](https://github.com/miurahr/aqtinstall/issues/1007).

## CI, packages and handoff

The native workflow runs on `macos-15`, `windows-2022` and `ubuntu-24.04`, using these same scripts. It caches SDKs and incremental Rust output, runs fixture tests and GUI smoke, and uploads development archives plus native logs. Cached state is an optimization; a cold bootstrap must work. A CI build is not verification of the founder's actual PC, compositor, clipboard, IME or keyring.

Packages land in `dist/`. macOS contains `fuckmail-gui.app` with `fm` and `fm-engine` in `Contents/MacOS`; Windows/Linux place the three executables in `bin`. Qt's deployment step stages required libraries/plugins/QML resources. Package smoke clears developer Qt/engine overrides, uses a disposable profile and checks the installed executable. A build manifest records source commit, dirty state, tools and host. Archives are unsigned development artifacts; signing/notarization, a finished installer, PATH integration and full clean-machine qualification remain separate gates.

For a fresh terminal, invoke the bundled CLI by its absolute path or add its containing directory to that terminal's PATH. No global CLI launcher is installed automatically. Move/copy the package directory to install the development build; remove that directory to uninstall. Profiles remain outside installation/build output. This is not a final upgrade or migration system.

Before changing machines, commit and push source, lockfiles, fixtures and accurate status. On the other host, pull the same branch/revision, run bootstrap once, then doctor/test. Never transfer `.tools`, `target`, `build`, `dist`, tokens or live SQLite profiles through Git or a cloud folder. Generate synthetic data locally. Keep any real mail, draft or credential handoff separate from source control.

Qt modules have their own open-source licenses and third-party notices; Rust packages have individual licenses too. Public distribution requires a complete dependency/license inventory and the appropriate notices/source availability, not just these links. The current dependency/source route is inspectable, but this document does not claim the public-release license audit is complete. [Qt licensing](https://doc.qt.io/qt-6/licensing.html), [Qt source archives](https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/).

## Observed setup evidence

2026-09-11, this Mac: macOS 15.7.7, arm64, Apple Clang 17.0.0, SDK 26.2, Rust/Cargo 1.88.0. `scripts/dev.sh bootstrap` and `doctor` passed; CMake detected the native compiler and Qt 6.11.2. The five Mac archives installed in 12.3 seconds; the SDK occupied approximately 1.4 GB. These are acquisition observations, not app import or runtime performance.

The same adapter downloaded, verified and extracted the five Windows MSVC x64 archives on this Mac in 14.3 seconds. That validates archive selection/acquisition only; it does not run Windows code. Linux metadata confirmed `linux_gcc_64`. An Ubuntu CI bootstrap then exposed that the five-archive selection omitted Qt’s separate ICU runtime: `qtpaths` could not load `libicui18n.so.73`. The Linux target now acquires the official `icu` archive and bootstrap repairs a managed SDK missing its three ICU 73 libraries, even when `qtpaths` is already installed. It does not substitute Ubuntu’s different ICU ABI or create compatibility symlinks. An Ubuntu 24.04 x64 container on this arm64 Mac reproduced the exact missing-library error with Qt Core but without that archive; adding the official ICU archive let `qtpaths --qt-version` return `6.11.2`, and `ldd` resolved all three ICU 73 libraries from the Qt kit. This container check used emulation and Ubuntu’s normal GLib runtime; it is loader evidence, not native Linux GUI qualification. Native build, runtime and CI results belong in the current [status](status.md) and Phase 0 evidence, with unavailable desktop checks kept explicit.
