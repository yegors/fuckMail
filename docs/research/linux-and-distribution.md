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

## Next experiment

P0-11: install GUI/engine/CLI on clean supported systems with no developer tools, discover the CLI, authenticate, render mail, launch a headless query, upgrade and uninstall while checking retained data. Include Ubuntu, Arch and Omarchy desktop sessions rather than a Linux container alone. Record package size, first-run steps, dependency resolution, CLI PATH/alias behavior, signing/update mechanism and any required system integration.
