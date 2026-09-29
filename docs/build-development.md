# Build and run from source

fuckMail is a Rust engine (`fm-engine`) and CLI (`fm`), a C++20 Qt Quick app (`fuckmail-gui`) and a SQLite store. Python runs only the developer scripts; the packaged app needs no Python or Node.js.

All developer commands go through [scripts/dev.py](../scripts/dev.py), via `scripts/dev.sh` (macOS, Linux) or `scripts/dev.ps1` (Windows PowerShell). Both take the same commands and options.

## Requirements

Every platform needs Git, Python 3.9 or newer, and internet access for the first bootstrap. Bootstrap supplies Rust, Qt, CMake and Ninja.

- **Windows (x64):** Visual Studio 2022 C++ Build Tools with the Windows SDK. `dev.ps1` loads the VS 2022 compiler environment per command without changing your PATH. If Visual Studio or Python is missing, `bootstrap` installs the Build Tools (C++ workload) or Python 3.11 through `winget`. An existing Visual Studio without the C++ workload needs it added by hand. ARM64 and WSL are not supported.
- **macOS:** Apple's Command Line Tools (`xcode-select --install`). The app targets macOS 13 or later, built for the host's architecture.
- **Ubuntu 24.04 (x64):** the packages CI installs (from [the native workflow](../.github/workflows/native.yml)), plus `python3-venv`:

```sh
sudo apt-get install -y build-essential python3-venv libgl1-mesa-dev libegl1 libopengl0 \
  libxkbcommon-dev libxkbcommon-x11-0 libxcb-cursor0 libxcb-icccm4 libxcb-keysyms1 \
  libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libxcb-render-util0
```

Other Linux distributions, including Arch, are untested.

## First setup

```sh
scripts/dev.sh bootstrap              # once per checkout
scripts/dev.sh doctor                 # check tool versions
scripts/dev.sh run --preset release
```

On Windows, use `./scripts/dev.ps1` with the same arguments from PowerShell in the checkout.

Bootstrap installs into the ignored `.tools/` directory and leaves system installs alone:

| Tool | Pinned version | Location |
| --- | --- | --- |
| CMake, Ninja, aqtinstall | 3.31.6, 1.11.1.4, 3.3.0 | Python venv in `.tools/bootstrap` |
| Qt | 6.11.2: qtbase, qtdeclarative, qttools, qtsvg, qttranslations, Qt Multimedia (Linux adds ICU) | `.tools/Qt/6.11.2/<kit>` |
| Rust | 1.88.0 with rustfmt and clippy | A matching `rustc` on PATH, else a local rustup in `.tools/rustup` and `.tools/cargo` |
| Inno Setup (Windows) | 7.1.0, SHA-256 checked | `.tools/innosetup-7.1.0` |

Pins live in [scripts/toolchain.json](../scripts/toolchain.json), [rust-toolchain.toml](../rust-toolchain.toml) and `scripts/bootstrap-requirements.txt`. Qt comes from the official archives through aqtinstall (no Qt account needed); [scripts/bootstrap_qt.py](../scripts/bootstrap_qt.py) works around [aqtinstall#1007](https://github.com/miurahr/aqtinstall/issues/1007) for Qt 6.11.2 on Windows, so revisit it when either pin changes. To use your own Qt 6.11.2 kit with Qt Multimedia, set `FM_QT_ROOT`.

## Everyday commands

| Command | What it does |
| --- | --- |
| `doctor` | Prints tool versions and paths; fails if one is missing or wrong (default for `dev.ps1`) |
| `build` | Configures and builds incrementally |
| `test` | Builds, then runs the whole CTest suite |
| `run` | Builds, then starts the GUI |
| `cli` | Runs the already-built `fm` |
| `package` | Builds and checks a relocatable app, then writes an installer to `dist/` |

`--preset dev` (the default) is a Debug build of the C++ and Rust code in `build/dev` and `target/debug`, and runs several times slower. `--preset release` builds optimized code in `build/release` and `target/release`; use it daily, for judging performance and for packages. Both come from [CMakePresets.json](../CMakePresets.json).

Arguments after `--` go to the GUI (`run`) or CLI (`cli`):

```sh
scripts/dev.sh run --preset release -- --demo-profile
scripts/dev.sh cli --preset release -- --demo-profile --account windscribe mail list --limit 5
```

Builds copy `fm` and `fm-engine` next to the GUI executable, as packages do. The CLI needs no Qt or display. `fm --help` and `fuckMail --help` list every option; `--fixture-clock` pins the GUI to the demo data's date, 11 September 2026.

For quick UI compiles, configure a separate build directory with `-DFM_GUI_ONLY=ON`. It builds only the Qt client, so the result can't run on its own.

## Profiles

A profile is one directory: the SQLite store (`store.sqlite3`, with `-wal`/`-shm` files while open), retained message bytes in `blobs/` and other local state. Profiles live outside the program and build folders; building, installing, upgrading and uninstalling never touch them.

The default **live** profile is `fuckMail/default` under `%LOCALAPPDATA%` on Windows, `~/Library/Application Support` on macOS, and `$XDG_DATA_HOME` (else `~/.local/share`) on Linux. It starts empty and holds the Google accounts you connect. `--demo-profile` opens `fuckMail/demo-v1` in the same place: the synthetic accounts described in [fixtures/README.md](../fixtures/README.md), which never connect to Google.

- `--profile DIR` picks another directory (absolute for `fm`). It opens whatever profile is there, or creates an empty live one; add `--demo-profile` to create demo data instead.
- GUI and CLI share one engine per profile, which exits after the last client disconnects.
- To back up or move a profile, close every client and copy the whole directory, never `store.sqlite3` alone.
- Schema37 profiles upgrade in place to schema38. Profiles older than37, and profiles newer than the running engine, are refused. For a profile older than the baseline, move its folder aside, start again and reconnect your accounts.
- Don't point tests or development commands at a profile with real mail. Tests use temporary profiles.

## Google sign-in setup

fuckMail uses the Gmail and Google Calendar APIs. Each build therefore needs a Google Cloud OAuth client of type **Desktop app**. Whoever builds the app sets this up once; people who sign in never handle the client file.

1. In the [Google Cloud console](https://console.cloud.google.com/), create a project and enable the **Gmail API** and the **Google Calendar API**.
2. Configure the OAuth consent screen. For personal use, choose the **External** audience, leave the app in **Testing**, and add each Google account you will sign in with as a test user.
3. Create an OAuth client ID of type **Desktop app** and download its JSON file. Keep it outside the repository.
4. Register it with the build (use `python` instead of `python3` on Windows), then rebuild:

```sh
python3 scripts/configure_google.py /absolute/private/path/to/desktop-client.json
scripts/dev.sh build --preset release
```

The app requests `openid`, `email`, `https://mail.google.com/`, `calendar.events`, `calendar.calendarlist.readonly` and `calendar.events.freebusy`. Read-only access uses `gmail.readonly` and `calendar.events.readonly` instead. Gmail's scopes are *restricted*, which has two consequences:

- While the app is in Testing, only listed test users can sign in (up to 100), and Google expires their sign-in after 7 days, so you reconnect weekly.
- Offering a build to the general public requires Google's verification for restricted scopes.

Sign-in happens in the system browser. The engine receives the callback on a temporary `http://127.0.0.1:<port>/oauth2/callback` listener, so there is no web callback URL or hosted service. In the app, use **Settings → Accounts → Connect Google account**, enter the email and continue with Google. Reconnect and Restore reuse the saved account identity. Sign-in alone does not start sync: choose **Start sync** for each account.

The script stores only the client registration fields, in the ignored `.tools/google-oauth-client.json`. CMake copies that file beside the engine in builds and packages, so **a package you share contains your client ID and secret**. Google treats desktop client secrets as non-confidential, but share packages only with people you are happy to have use your Cloud project. An absolute `FM_GOOGLE_CLIENT_CONFIG` path overrides the file, including when you run an engine directly. Never commit the file or put access or refresh tokens in it. A build without it reports Google sign-in as unavailable. CI builds carry no credentials.

The CLI shares the same setup:

```sh
fm --profile /absolute/private/profile google setup-status
fm --profile /absolute/private/profile google connect --email you@gmail.com
```

Use `--account ID google connect --email ADDRESS --reauthorize` for explicit reconnect; restore also requires the current archive revision. `--client-config` remains a developer override. The email is a Google hint; verified subject and granted permissions determine the connected account. Existing profiles retain their client binding. The app registration must match after a move/reinstall; another project's registration cannot refresh old grants.

## Running tests

`scripts/dev.sh test --preset release` builds and runs every CTest test, as CI does:

- `rust-test-build` compiles the Rust test binaries; `rust-workspace` runs `cargo test --workspace --locked`.
- `gui-smoke`, `gui-live-empty`, `gui-integration`, `gui-attachments` and `gui-startup-recovery` run the built app with the real engine on disposable profiles.
- The other `gui-*` tests are Qt unit and UI tests in [gui/CMakeLists.txt](../gui/CMakeLists.txt).

GUI tests need no display: CTest runs them with `QT_QPA_PLATFORM=offscreen` and Qt Quick's software backend. Linux still needs the libraries listed above.

`test` doesn't forward arguments to CTest. For one test, run the CTest from `.tools/bootstrap` in the checkout with the Qt kit's `bin` on PATH (Windows GUI tests need it to load Qt), for example `ctest --preset release -R gui-smoke`.

Known timing issues: `gui-startup-recovery` can time out when cargo rebuilds inside its 90-second limit (it passes alone), the link-hover check in `gui-html-view` fails intermittently, and some GUI timing checks run near their limits in Debug builds, so judge the suite on release.

[CONTRIBUTING.md](../CONTRIBUTING.md) lists the checks to run before sending a change.

## Packaging

`scripts/dev.sh package --preset release` builds in its own tree, `build/<preset>-package`, so it works while an app from `build/release` is running. It stages the app in `dist/<os>-<arch>` with install notes, this guide as `BUILD-AND-LICENSE-NOTES.md` and a `build-manifest.json` (source commit, toolchain). It then tests a relocated copy with a system-only PATH and writes one file to `dist/`:

- **Windows:** `fuckMail-<version>-<commit>-windows-amd64-setup.exe`, built with Inno Setup from [scripts/windows-installer.iss](../scripts/windows-installer.iss). It installs for the current user in `%LOCALAPPDATA%\Programs\fuckMail` without an admin prompt, with a Start menu entry, an optional desktop icon and an uninstaller under Settings → Apps. Upgrades replace the program files. The MSVC runtime DLLs ship beside the executables, because a per-user install can't run `vc_redist`.
- **macOS:** `fuckMail-<version>-<commit>-macos-<arch>.dmg` with `fuckMail.app` and an Applications shortcut, made with `hdiutil`. The script mounts the image and verifies the app's code signature. `fm` and `fm-engine` are in `fuckMail.app/Contents/MacOS`.
- **Linux:** `fuckMail-<version>-<commit>-linux-<arch>.tar.gz` of the staged folder; start `bin/fuckMail`.

The version comes from `project(fuckMail VERSION ...)` in [CMakeLists.txt](../CMakeLists.txt). On Windows and Linux the executables are in `bin/`. Nothing puts `fm` on PATH; call it by its full path.

**Unsigned builds.** Installers are not signed yet. On Windows, SmartScreen warns: choose **More info → Run anyway**. On macOS, open System Settings → Privacy & Security and choose **Open Anyway** (on macOS 14 and earlier, Control-click the app and choose Open). After each update macOS may ask whether `fm-engine` can use your keychain; choose **Always Allow**. Before adding Developer ID signing and notarization, move `google-oauth-client.json` out of `Contents/MacOS`, which should hold only code.

**App icon.** The app, window, Dock and installer icon comes from `gui/app-icon/app-icon.png`. When it changes, run `python scripts/make_app_icons.py` to regenerate the committed `.ico` and `.icns`; this one script needs Pillow, which the build does not. The source is 128 px, so the largest sizes are upscaled; a 1024 px source would make them sharp. The `.dmg` keeps the standard disk-image icon.

**Licenses.** fuckMail is MIT-licensed ([LICENSE](../LICENSE)). [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) lists every redistributed component with its license. Packages install those texts, plus `THIRD-PARTY-RUST.txt` generated from `Cargo.lock` by `scripts/third_party_notices.py`, into their `licenses` folder. Add new bundled components to that file. [Qt licensing](https://doc.qt.io/qt-6/licensing.html), [Qt source archives](https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/).

## Continuous integration

[The native workflow](../.github/workflows/native.yml) runs `bootstrap`, `test --preset release` and `package --preset release` on `macos-15`, `windows-2022` and `ubuntu-24.04` for every push and pull request, plus the hygiene check, `cargo fmt` and strict `cargo clippy`.

## Windows renderer workaround

On Windows, [gui/main.cpp](../gui/main.cpp) sets `QT_D3D_NO_VBLANK_THREAD=1` before Qt starts unless it is already set. Qt 6.11.2's DXGI VBlank notifier can stop repaints on some machines; this uses Qt's timer fallback instead, with D3D11 and vsync still on. Recheck it when Qt changes.

## Moving between machines and repository hygiene

Commit and push source, lockfiles and fixtures. On the other machine, pull, run `bootstrap` once, then `doctor` and `test`. Run `scripts/configure_google.py` again there, because the client file lives in the ignored `.tools/`.

Never commit or sync through Git or a cloud folder: `.tools/`, `target/`, `build/`, `dist/`, profiles, SQLite files, OAuth client files or tokens. `.gitignore` covers them. Before committing, run `python3 scripts/check_repo_hygiene.py` (`python` on Windows); it flags tracked generated output, oversized files and raw logs or archives under `docs/`. CI runs it too.
