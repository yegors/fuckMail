# Linux support and end-user installation

Sources checked 2026-09-11. Windows, macOS and Linux (Ubuntu, Arch and Omarchy) are required. Packaging routes below are proposals to validate on clean machines; there are no installers or supported-build results yet. Owner: [Platform](../contracts/platform.md).

## Compatibility is more than a Linux build

Qt documents Windows, macOS and selected Linux configurations. Ubuntu is listed; Arch/Omarchy are not an upstream compatibility promise. We must maintain our own tested distro/compositor matrix and pin the chosen Qt release, compiler, architecture and minimum system-library baseline. The Qt installer binaries' build environment can constrain older Linux systems. [Qt supported platforms](https://doc.qt.io/qt-6/supported-platforms.html).

Test Ubuntu LTS desktop sessions, a recorded current Arch snapshot and a named Omarchy release/configuration. Native Wayland is a required first-class path, including the Omarchy compositor; verify X11/XWayland compatibility where supplied. Clipboard/IME, file dialogs/portals, GPU rendering, fractional scaling, keyring access, notifications and the HTML sandbox all need real desktop tests. A headless CLI must also work without a compositor; credential provisioning/unlocking is a separate setup concern.

Omarchy is a distinct installation/configuration to test, not evidence that a generic Ubuntu pass covers the user. Keep the supported snapshot recorded alongside the [maintained Omarchy project/manual](https://github.com/omacom/omarchy).

## Proposed delivery routes

| Platform | Preferred experiment | End-user outcome |
| --- | --- | --- |
| Windows | Signed installer carrying GUI, engine, CLI and required runtimes; evaluate MSIX/installer CLI discovery | Install once, launch the app and invoke CLI from a fresh terminal without a compiler or Qt SDK |
| macOS | Signed/notarized app distribution with a simple bundled-CLI installation/discovery path | Normal app installation plus a supported CLI launcher; no Homebrew/build-tool prerequisite |
| Ubuntu | Prebuilt .deb with declared runtime dependencies and a documented package/update source | Package manager resolves dependencies; GUI and CLI use one profile |
| Arch / Omarchy | Prebuilt pacman package and documented update channel | No end-user source build, AUR compilation or manual Qt plugin assembly |

These are packaging choices to test, not a promise of every listed store/update channel. Windows supports command execution aliases for packaged CLIs. macOS notarization is part of smooth direct distribution. [Microsoft CLI packaging](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/packaging-cli), [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).

Bundle app-owned runtimes or declare supported system dependencies; users should not install Node, Python, Rust, CMake, Qt developer tools or a model just to read mail. An optional local runner/model has separate explicit setup. Keep first-run OAuth and storage setup clear, verify upgrade/recovery, and preserve user data on normal uninstall unless removal was explicitly chosen.

## Why no universal-package promise yet

Flatpak can provide a common desktop runtime but introduces sandbox/file/IPC boundaries for a host CLI and local runner. It needs a deliberate launcher/profile-sharing design, not a second isolated mailbox database. AppImage can be useful for portability but FUSE and launch/integration prerequisites mean “one downloaded file” does not prove frictionless installation. Test these later if they improve the complete GUI-plus-CLI experience; native packages are the initial recommendation. [Flatpak permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html), [AppImage FUSE notes](https://docs.appimage.org/user-guide/troubleshooting/fuse.html).

Qt WebEngine adds runtime/helper resources, Linux sandbox integration and macOS distribution constraints. Validate those in the package; disabling the HTML sandbox is not an acceptable fix for installation friction. [WebEngine platform notes](https://doc.qt.io/qt-6/qtwebengine-platform-notes.html).

Audit the exact Qt modules and third-party dependencies before choosing the distribution license. Some Qt modules have different licensing terms; “Qt” is not a single blanket grant. Prefer a maintained release and track browser-component updates rather than promising an unspecified LTS entitlement. [Qt licensing](https://doc.qt.io/qt-6/licensing.html).

## Build and validation baseline to investigate first

Keep Cargo and CMake as independent builds joined at test fixtures and package staging. Cargo supports a shared workspace/lockfile for Rust packages; Qt's CMake deployment tooling can help stage GUI dependencies, but still needs platform packaging and runtime verification. Do not add Corrosion, CXX-Qt, a generated Rust/C++ ABI or an npm application pipeline merely to orchestrate separate executables. When implementation begins, record reproducible build commands and pin proven dependencies/toolchains together; no version is selected by this document. [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [Qt CMake deployment](https://doc.qt.io/qt-6/cmake-deployment.html).

Candidate first matrix, subject to available devices and the selected Qt/WebEngine release:

| Target | Build candidate | Required execution environment |
| --- | --- | --- |
| Windows x64 | Stable Rust MSVC target; C++20 with supported MSVC; CMake/Ninja and prebuilt Qt/Quick/WebEngine | Fresh standard-user Windows installation; test shell command discovery, renderer helpers, clipboard/IME, credentials and scaling |
| macOS Apple Silicon | Stable Rust Apple target; Apple Clang/Xcode supported by Qt; matched Qt/renderer artifacts | Actual Mac or appropriate macOS test host, including GUI and Keychain; exercise downloaded-app launch/signing separately from developer launch |
| Ubuntu x64 | Stable Rust GNU target; supported GCC/Clang and Qt build from a controlled LTS baseline | Recorded Ubuntu LTS desktop with Wayland and any claimed X11 path; test also with no display session for CLI |
| Arch x64 | Build/stage against a recorded Arch snapshot; identify declared system libraries and app-private bundled libraries | Fresh Arch desktop with recorded compositor, portal/keyring availability and system packages |
| Omarchy x64 | Validate compatibility of the chosen Arch package on a recorded Omarchy installation | Its actual configured desktop; a generic Arch container pass is insufficient |

Windows ARM, Intel Mac, Linux ARM and exact OS floors remain inventory decisions, not silently promised or rejected architectures. Start with the representative configurations already proposed in Platform; add the founder's actual hardware before declaring supported coverage. Rust target support describes compiler/standard-library support, not Qt, keyring or whole-package compatibility. [Rust target tiers](https://doc.rust-lang.org/rustc/platform-support.html).

The current Qt 6.11 platform page lists Ubuntu 22.04/24.04 and MSVC 2022 among its tested configurations. Its Linux binary notes currently give different glibc statements in different sections. Inspect the actual selected artifacts' linked requirements and run them on the intended minimum system; do not infer a floor from a generic paragraph. WebEngine's requirements can be narrower than Qt's: notably, Windows MinGW and static WebEngine builds are unsupported in the checked documentation. Prefer matched prebuilt Qt modules initially; source-building WebEngine brings upstream Node/Python and native-tool dependencies even though our app has no Node product pipeline. [Qt platform matrix](https://doc.qt.io/qt-6/supported-platforms.html), [WebEngine build constraints](https://doc.qt.io/qt-6/qtwebengine-platform-notes.html).

**Local observation, 2026-09-11:** a read-only PowerShell PATH lookup found Git, but not rustc, cargo, cmake, ninja, cl, qmake/qmake6, qtpaths6 or ctest. This only records that session's command visibility, not machine-wide installation absence. No compiler, SDK or dependency was installed. First execution must inspect existing installations and provision missing tools before claiming the workspace is buildable.

Future CI should separate deterministic Rust/core tests and shared protocol fixtures from Qt model/renderer tests and native package smoke checks. Build Windows/macOS/Linux artifacts early; clean desktop acceptance remains a separate evidence lane for graphics, input, credentials and lifecycle. Cache locked dependencies without masking fresh-checkout failures. Record runner image/tool versions; unavailable Mac/Linux runners are an explicit outstanding gate, not a pass inferred from Windows.

## P0-11: package the feasibility slice early

Prerequisites: later authorization to implement/prototype; compatible engine/CLI/GUI artifacts from P0-10; renderer/composer slice from P0-04; recorded native test machines/VMs and a decision on how dependencies are supplied. Use synthetic mail and a fake credential round trip first. Live OAuth follows the provider packet and account authorization; it is not needed to discover missing Qt libraries.

| Stage | Future package work | Exit evidence |
| --- | --- | --- |
| First smoke package | Stage all three binaries and Qt/QML/WebEngine helpers/resources; create a simple per-platform installer/package using the routes above | A fresh user can launch the fixture GUI and invoke a headless query from a new terminal without developer tools or a model |
| Shared installation/profile | Resolve engine location from the installation, independent of shell working directory; exercise Unicode/spaced paths and same-profile GUI/CLI | One engine/profile, correct runtime discovery and no writes into the program directory; no accidental reliance on developer PATH |
| Upgrade/recovery rehearsal | Install a second fixture build with a deliberate schema/protocol transition, active-client checks and an interrupted migration case | Compatible components upgraded together; old binaries refuse unsupported data; authored fixtures survive recovery and normal uninstall |
| Signing/channel feasibility | Identify Windows signing/alias route, macOS Developer ID/notarization access, and Linux package signing/update source | Concrete prerequisites and actual launch warnings recorded; missing signing credentials remain an open delivery gate |
| MVP finishing | Final signed packages, normal OAuth setup, maintained updates, release notes/licenses and complete required desktop checks | Repeatable clean installation, upgrade/recovery and uninstall evidence across the agreed matrix |

Unsigned local smoke artifacts can prove dependency closure and packaging structure, but do not pass the smooth trusted-distribution gate. Signing accounts/certificates, notarization service access and update hosting are external prerequisites to resolve before final delivery, without installing an always-on application service.

Headless does not imply an unlocked credential store. Verify the chosen Rust credential adapter on Windows/macOS and a real Secret Service provider on each Linux setup; no provider or unlocked session is guaranteed by merely compiling for Linux. An unavailable/locked store must yield an actionable noninteractive error and an explicit setup path. Test GUI and CLI against the same credential references. [Secret Service specification](https://specifications.freedesktop.org/secret-service/latest/).

**Record for each attempt:** artifact/source version, architecture, OS/distro/compositor, toolchain and Qt modules, package size, runtime library requirements, first-run steps, CLI discovery, sandbox/keyring result, signing state and retained-data behavior. Measure installed disk and runtime memory separately. The first fixture package supports the stack decision; polished distribution and the full upgrade matrix remain release work. All checks are planned, not run.
