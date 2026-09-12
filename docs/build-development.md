# Build, run and move between machines

This is the shared Phase 0/Phase 1 developer workflow. The app is Rust engine/CLI plus a C++20 Qt Quick/QML client and SQLite. Python is used only to orchestrate developer tools; the packaged app has no Python, Node.js or JavaScript build dependency. Phase 1 integration is active; [its execution record](research/phase-1-execution.md) owns current results. The historical host checks below retain their original source and limits.

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

Windows subprocess launches resolve bare tool names against the selected project environment before spawning. This allows package-manifest commands to find project-local Rust even when the terminal has no global `rustc` or `cargo`; no global PATH change is required.

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

GUI/CLI arguments follow `--`, for example `scripts/dev.sh run -- --profile /absolute/local/demo-profile`. Use the same profile for both clients. The engine creates a private profile directory, including missing parent directories. The normal demo is synthetic and never authenticates with a provider. Do not point test commands at a real mailbox profile.

`--preset dev` is the default; `release` uses optimized Rust and C++ builds. Both use the tracked [CMake presets](../CMakePresets.json), [tool inputs](../scripts/toolchain.json), [Python dependency lock](../scripts/bootstrap-requirements.txt) and Rust toolchain/lockfile. Builds copy `fm` and `fm-engine` beside the GUI executable so engine discovery is the same as in the package. The developer launcher reads CMake’s generated executable path, including the different Windows build-directory layout. The CLI itself does not load Qt or need a display.

An advanced Qt-only compilation is available with CMake `-DFM_GUI_ONLY=ON` in a separate build directory while editing UI. It deliberately excludes engine integration, tests and installation; it is not a working app or an acceptance result. Standard dev/release presets always build the complete slice.

## Phase 1 local walkthrough

Use a new disposable profile path for the three-account fixture. Existing profiles are upgraded in place and retain their original accounts, drafts and fixture revision; they are not reseeded. For example, on Windows:

```powershell
./scripts/dev.ps1 run -- --profile C:/Users/Yegor/Projects/Fuckaround/fuckMail/.tools/phase1-walkthrough --fixture-clock
```

On Mac/Linux use `scripts/dev.sh run -- --profile /absolute/local/phase1-walkthrough --fixture-clock`. The optional fixture clock fixes the view at 11 September 2026; ordinary launches use the current clock. Use a genuinely new path to repeat the initial state. All content and submission outcomes in this walkthrough are synthetic/local.

1. In Mail, select Windscribe and open an unread conversation. The deliberate open marks its loaded members read; Delay holds it without a due date, and Done clears both unread and stars. Inbox, Sent and Drafts stay within the selected account. Collapse and expand the counted To review, Delayed and Done headings. Group counts cover the current query; category badges and account totals cover the account, beyond the loaded page.
2. Toggle People or Automated in the category rail; click the selected kind again to clear it. Open a sender's card from the reader to see that sender's account-wide count and latest three conversations. Open a listed conversation, filter to the exact sender, or choose **Write** to create a draft addressed to that sender in the same account.
3. Search for `Maya` or `Frankfurt`, open the search popup's Filters, and combine sender/status/attachment/date conditions. Search matches source text, and a matching earlier message can be revealed in the reader. Clear the query and active filters to restore the normal account view.
4. Open **Q3 infra review: v4 attached**, choose Reply, write your own text, select words and use B/I/U. Add a retained attachment from the attachment menu and edit the reply's signature if desired. New message, Reply all and Forward create independent drafts; edit To/Cc/Bcc and subject, then find each saved item in Drafts. Switch accounts/workspaces and return. The save status must confirm persistence. **Capture locally** records a MIME copy; Sent's local capture history shows its operation and outcome. It does not deliver an email.
5. In Settings → General, try Graphite, Paper and Harbour, then Small, Default and Large text. Set **Open on** to Mail, Calendar or Where I left off. Sync and Notifications remain visibly unavailable. Accounts and Mail contain display names, signatures and row density; existing drafts retain their signature snapshot.
6. In Settings → Calendar, choose visible calendars independently of the mailbox, week start, timeline hours, and **Footer** or **Header** placement for the Mail timeline. Set **New events last** to 30 min, 45 min or 1 h. Browse the timeline, then open Calendar: its day/week/month/agenda views combine permitted details, its grid covers all 24 hours, and its date remains independent of the timeline. Use **New event** to check the chosen default duration and create a nonrecurring event in a writable calendar. Open details to edit or respond for yourself; Local activity records operations without sending invitations.
7. Wait for saved status, close the GUI and reopen the same profile. Check appearance, startup workspace, calendar preferences and authored drafts. Reopening preserves this work; only a genuinely new profile starts from the original fixture.

The CLI takes the same profile. With the executable directory containing `fm` and `fm-engine`, these are read-only checks:

```text
fm --profile /absolute/local/phase1-walkthrough --json accounts
fm --profile /absolute/local/phase1-walkthrough --account windscribe --json mail list --grouped --limit 50
fm --profile /absolute/local/phase1-walkthrough --account windscribe --json draft list
fm --profile /absolute/local/phase1-walkthrough --account windscribe --json draft history
fm --profile /absolute/local/phase1-walkthrough --json calendar --combined --from 2026-09-11 --to 2026-09-12
fm --profile /absolute/local/phase1-walkthrough --json preferences show
```

Use a native Windows path in place of the illustrative absolute path. `mail read ID` is a query and does not mark the conversation reviewed; `mail review`, `mail done` and `mail delay` are explicit actions. Run `fm --help` and the command's `--help` for mutation parameters. The [fixture API](../fixtures/README.md) defines exact fields and limits. [Phase 1 execution evidence](research/phase-1-execution.md) distinguishes executed checks from remaining acceptance work.

The Windows package at clean source `8743162` passes relocated runtime, preserved schema4/schema5 upgrade/refusal and packaged concurrent-search checks. It is `dist/fuckmail-windows-amd64.zip`, with a preserved copy under `dist/baselines/fuckmail-windows-amd64-schema7-8743162.zip`. Its staged GUI runs without rebuilding or the SDK. The package's `build-manifest.json` records its exact source; [current execution evidence](research/phase-1-execution.md#p1-09-clean-windows-package--8743162) records the hashes and limits. To build later source, run `./scripts/dev.ps1 package --preset release` and retain that run's own checks. Use a fresh synthetic profile for the updated walkthrough:

```powershell
./dist/windows-amd64/bin/fuckmail-gui.exe --profile C:/Users/Yegor/Projects/Fuckaround/fuckMail/.tools/phase1-update2-walkthrough --fixture-clock
```

Use `dist/windows-amd64/bin/fm.exe` with that same profile for the paired CLI. Historical schema4/schema5 packages are preserved under `dist/baselines`; [the c34b82e record](research/phase-1-execution.md#final-windows-development-package--c34b82e) retains their earlier runtime scope. Use separate disposable profiles with old packages; an old engine must refuse a profile already upgraded to a newer incompatible schema. Final-source CI and physical-input limits remain explicit.

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

The native workflow runs on `macos-15`, `windows-2022` and `ubuntu-24.04`, using these same scripts. It caches SDKs and incremental Rust output, runs fixture tests and GUI smoke, and uploads development archives plus native logs. Cached state is an optimization; a cold bootstrap must work. Explicit restore/save steps keep successfully acquired SDKs and compiled Rust output even when a later runtime test fails; failure status is preserved and all tests still run. SDKs are saved only after successful bootstrap. See the [cache action documentation](https://github.com/actions/cache/tree/v4#using-a-combination-of-restore-and-save-actions). CTest prepares Rust test binaries in a separate `rust-test-build` setup fixture (900-second compile limit), then runs the expanded workspace under a 600-second aggregate execution watchdog. This keeps a cold optimized test build from consuming the runtime limit; application latency targets are unchanged. GUI checks set `QT_FORCE_STDERR_LOGGING=1` so native Windows failures are retained in CTest output. [Qt logging environment](https://doc.qt.io/qtcreator/creator-how-set-project-environment.html). A CI build is not verification of the founder's actual PC, compositor, clipboard, IME or keyring.

Packages land in `dist/`. macOS contains `fuckmail-gui.app` with `fm` and `fm-engine` in `Contents/MacOS`; Windows/Linux place the three executables in `bin`. Qt's deployment step stages required libraries/plugins/QML resources. Package smoke clears developer Qt/engine overrides, uses a disposable profile and checks the installed executable. A build manifest records source commit, dirty state, tools and host. Archives are unsigned development artifacts; signing/notarization, a finished installer, PATH integration and full clean-machine qualification remain separate gates.

For a fresh terminal, invoke the bundled CLI by its absolute path or add its containing directory to that terminal's PATH. No global CLI launcher is installed automatically. Move/copy the package directory to install the development build; remove that directory to uninstall. Profiles remain outside installation/build output. This is not a final upgrade or migration system.

The portable `scripts/verify_upgrade.py` checks two preserved packages with different schema versions. Pass each directory containing the three executables, plus a report path; on macOS that directory is inside `fuckmail-gui.app/Contents/MacOS`, on Windows/Linux it is `bin`:

```sh
python3 scripts/verify_upgrade.py --old-bin /absolute/old/package/bin --new-bin /absolute/new/package/bin --report build/verification/package-upgrade.json
```

On Windows use `python` and native absolute paths. The default baseline is schema 4/fixture 1; add `--expected-old-schema 5` for the preserved schema 5/fixture 2 package. Both upgrade to schema 6 while preserving the original fixture revision and accounts. Choose a new report filename for each run; existing reports are refused. The script creates its own disposable synthetic profile; it has no existing-profile argument. It checks compatible old/new client coexistence, explicit owner shutdown, Unicode draft/flag/receipt preservation, schema migration, the new GUI against the upgraded profile and refusal of an incompatible old engine without resetting data. It starts and cleans up only its own subprocess handles; discovery PIDs are never terminated. The JSON report preserves the exact package manifests, including dirty state, and executable hashes. A report counts as evidence only after an actual passing run; this developer check is not an automatic updater or physical keychain/input qualification.

Before changing machines, commit and push source, lockfiles, fixtures and accurate status. On the other host, pull the same branch/revision, run bootstrap once, then doctor/test. Never transfer `.tools`, `target`, `build`, `dist`, tokens or live SQLite profiles through Git or a cloud folder. Generate synthetic data locally. Keep any real mail, draft or credential handoff separate from source control.

Qt modules have their own open-source licenses and third-party notices; Rust packages have individual licenses too. Public distribution requires a complete dependency/license inventory and the appropriate notices/source availability, not just these links. The current dependency/source route is inspectable, but this document does not claim the public-release license audit is complete. [Qt licensing](https://doc.qt.io/qt-6/licensing.html), [Qt source archives](https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/).

## Observed setup evidence

2026-09-12, home Windows PC: Phase 1 runtime `1132c44` passed all nine release CTests in 156.16 seconds, relocated SDK-free GUI/CLI checks, full native selftest with actual GUI restart, and the two-package schema 4→5 upgrade/refusal in 92.422 seconds. Reviewed native captures cover 1440×900, 1920×1080 and 1160×700 logical sizes at the host's 125% scaling. [Phase 1 evidence](research/phase-1-execution.md) records hashes, measurements and remaining final-source CI/physical checks.

The Windows GUI sets `QT_D3D_NO_VBLANK_THREAD=1` before Qt starts unless the caller already supplied that variable. This pinned Qt 6.11.2 workaround uses Qt's normal timer update fallback after the native D3D11 renderer stalled on this host; D3D11, threaded rendering and presentation vsync remain enabled. It needs rechecking when Qt changes. See the [renderer diagnosis](research/phase-1-execution.md#expanded-acceptance-and-windows-renderer-correction); no system-wide environment change is needed.

2026-09-11, home Windows PC: Windows 11 Pro build 26200 x64, Ryzen 7 9800X3D, 128 GB installed RAM and RTX 5090. Existing Python 3.11.9, VS Build Tools 2022 17.14/MSVC 19.44 and Windows SDK 10.0.26100.0 were reused. Bootstrap installed the pinned project-local tools; doctor, dev/release builds and all eight CTests in each preset passed. Packaging exposed and drove the project-local executable-lookup correction above. The corrected release package passes relocated SDK-free GUI/CLI checks and native Windows/D3D11 rendering with system-only PATH. A schema-4 archive is retained under `dist/baselines/`; exact provenance, hashes, logs and limits are in [Windows handoff evidence](research/phase-0-execution.md#home-windows-handoff-before-phase-1). Phase 1 implementation has not started.

Final Phase 0 runtime source `204fa62` passed all eight CTests and relocated packaged GUI/CLI checks on native macOS 15 ARM64, Windows Server 2022 x64 and Ubuntu 24.04 x64 CI. The native Mac desktop walkthrough and actual packaged schema 2→4 upgrade/refusal checks also pass. See [complete execution evidence](research/phase-0-execution.md). This does not qualify the physical home Windows PC, Linux desktops or OS keyrings.

2026-09-11, this Mac: macOS 15.7.7, arm64, Apple Clang 17.0.0, SDK 26.2, Rust/Cargo 1.88.0. `scripts/dev.sh bootstrap` and `doctor` passed; CMake detected the native compiler and Qt 6.11.2. The five Mac archives installed in 12.3 seconds; the SDK occupied approximately 1.4 GB. These are acquisition observations, not app import or runtime performance.

The same adapter downloaded, verified and extracted the five Windows MSVC x64 archives on this Mac in 14.3 seconds. That validates archive selection/acquisition only; it does not run Windows code. Linux metadata confirmed `linux_gcc_64`. An Ubuntu CI bootstrap then exposed that the five-archive selection omitted Qt’s separate ICU runtime: `qtpaths` could not load `libicui18n.so.73`. The Linux target now acquires the official `icu` archive and bootstrap repairs a managed SDK missing its three ICU 73 libraries, even when `qtpaths` is already installed. It does not substitute Ubuntu’s different ICU ABI or create compatibility symlinks. An Ubuntu 24.04 x64 container on this arm64 Mac reproduced the exact missing-library error with Qt Core but without that archive; adding the official ICU archive let `qtpaths --qt-version` return `6.11.2`, and `ldd` resolved all three ICU 73 libraries from the Qt kit. This container check used emulation and Ubuntu’s normal GLib runtime; it is loader evidence, not native Linux GUI qualification. Native build, runtime and CI results belong in the current [status](status.md) and Phase 0 evidence, with unavailable desktop checks kept explicit.
