# GUI

The desktop client: a Qt Quick (QML) front end over a C++20 backend, built as the
`fuckmail-gui` target by [CMakeLists.txt](CMakeLists.txt). It holds no mail data of its
own; everything comes from the Rust engine (`fm-engine`), which the `fm` CLI also uses.

## Architecture

- **QML front end.** `main.cpp` loads `Main.qml` from the `FuckMail` QML module. Screens
  and controls are plain `.qml` files in this folder; `Theme.qml` (palettes Graphite,
  Paper, Terminal and Harbour, text sizes) and `ShortcutRegistry.qml` are singletons.
- **C++ backend exposed to QML.** `MailBackend` is set as the `backend` context property
  and `DebugConsole` as `debugDiagnostics`. QML reads their properties and calls their
  `Q_INVOKABLE` methods. A few native items are registered as QML types: `FmIconItem`,
  `FmHtmlView`, `HatchItem` and `ReplyDocument`.
- **Local IPC to the engine.** `MailBackend` finds or launches `fm-engine` for the profile,
  reads its private discovery file `engine.json` (checked by `IpcTrust`), verifies the
  peer process and user, and talks JSON over a `QLocalSocket` (a Unix socket or Windows
  named pipe). The main connection keeps one request in flight with a bounded queue,
  and generation counters drop replies that belong to an older view.
- **Separate connections for slow work.** Google operations, attachment imports, the
  HTML reader's calls and cancellable conversation reads each use a `GoogleOperation`,
  so they never block the main queue.
- **No direct data access.** The GUI never opens the SQLite store or talks to Google
  itself (the debug console only reports the store's file size). Closing the GUI ends
  its session; it never kills an engine that a CLI session is still using.

## Where things live

| Area | Files |
| --- | --- |
| App entry and shell | `main.cpp` (command line, profile and engine choice, test modes), `Main.qml`, `StartupOverlay.qml`, `TitleBar.h` with `TitleBar.cpp` (Windows/Linux) and `TitleBar.mm` (macOS) |
| Backend | `MailBackend.h` / `MailBackend.cpp` (connection, request queue, mail list, reader, draft saving). Its other methods are split by feature: `AccountLifecycleBackend`, `CompositionBackend`, `DraftAttachmentBackend`, `DraftTransferBackend`, `DiscardUndoBackend`, `SendUndoBackend`, `OutgoingBackend`, `RemoteMailBackend`, `GoogleBackend`, `GoogleDraftBackend`, `GoogleSendBackend`, `RemindersAndPreferencesBackend`. `GoogleOperation` and `IpcTrust` handle connections |
| List models | `ConversationListModel` (account, conversation and message lists, updated in place so delegates survive refreshes); `DebugLogModel` in `DebugConsole.h` |
| Calendar | `CalendarLayout` (overlap columns, midnight clipping, all-day lanes), `CalendarOperations.cpp`, `CalendarSearchBackend.cpp`, `CalendarUndoBackend.cpp`; `CalendarView.qml`, `EventEditor.qml`, `EventDetails.qml`, `TodayStrip.qml` (the Mail timeline, hatched with `HatchItem`) |
| Composer | `ReplyComposer.qml`, `RecipientField.qml`, and `ReplyDocument`, which bridges the editor's `QTextDocument` to plain text plus restricted HTML |
| HTML messages | `FmHtmlView` (tiled QQuickItem), `HtmlDocument` (layout and painting on [litehtml](third_party/litehtml/VENDORED.md), built by `cmake/litehtml.cmake`), `HtmlContent` (engine calls for HTML, images, blocklist, proxy), `HtmlLinks.h`, `SearchMatch.h`, `ReceivedBody.h`; `HtmlMessageBody.qml`. The engine sanitises HTML and fetches images; the view loads nothing itself |
| Icons | `FmIconItem` behind `FmIcon.qml` |
| Notifications | `NotificationPresenter` (system tray messages); reminder logic in `RemindersAndPreferencesBackend.cpp`; `ReminderPopup.qml` |
| Debug console | `DebugConsole.h` / `.cpp` and `DebugConsole.qml`: filtered log, `gui.log`, state view and `fm` commands |
| Tests | `*Test.cpp`, plus `GoogleOperationTestFixture.h` (a scripted stand-in engine) |
| Self-tests and benchmarks | `Gui*SelfTest.*`, `Gui*Benchmark.*`, `GuiPaintObserver.h`, `GuiBenchmarkSupport.h`; compiled into the app and started by `main.cpp` flags |
| Probes | `ComposerProbe.cpp` (a small CTest), `HtmlProbe.cpp` (`fm-html-probe`, lays out a folder of saved message HTML) |
| `third_party/` | Vendored litehtml and its gumbo parser; see `VENDORED.md` there |
| `fonts/` | Bundled Geist fonts and their OFL license; see [fonts/README.md](fonts/README.md) |
| `app-icon/` | App icon as PNG, ICO, ICNS and the Windows `.rc`; `scripts/make_app_icons.py` regenerates the ICO and ICNS from the PNG |

## Tests

Run everything with `scripts/dev.py test` (or the `scripts/dev.sh` / `scripts/dev.ps1`
wrappers). It configures and builds the preset, then runs `ctest --preset dev`
(`--preset release` for an optimized build). See
[docs/build-development.md](../docs/build-development.md) for setup and presets.

- **Registration.** GUI tests live in `gui/CMakeLists.txt` under `BUILD_TESTING`: each is
  an `add_executable(gui-…-test …)` plus `add_test(NAME gui-…)`. They run offscreen
  (`QT_QPA_PLATFORM=offscreen`, software Quick backend; `FM_CONTROLS_TEST_ENV`).
- **Kinds.** Unit tests cover single classes (`CalendarLayoutTest`,
  `ConversationListModelTest`, `ReplyDocumentTest`, `IpcTrust*Test`). Backend and UI tests
  compile the real `MailBackend` sources (`FM_NAVIGATION_TEST_SOURCES`,
  `FM_AUTHORING_TEST_SOURCES`) against the scripted engine, or use a small stand-in
  backend. UI tests copy the production `.qml` files (`FM_GUI_SOURCE_DIR`) into a
  temporary module and drive them with synthetic input.
- **Whole-app checks** are registered in the root `CMakeLists.txt` and run the built app:
  `gui-smoke` (`--smoke`), `gui-live-empty` (`--self-test-live-empty`), `gui-integration`
  (`--self-test`, with a real engine and `fm` CLI on a disposable profile),
  `gui-attachments` (`--self-test --self-test-attachments`) and `gui-startup-recovery`.
- **Benchmarks** are not part of CTest. Run the app with one of
  `--benchmark-{search,navigation,resize,calendar}-output <report.json>` and `--profile`;
  some are driven by `scripts/verify_*.py`.

## Running the GUI during development

- `scripts/dev.py run -- <args>` builds and starts the app. `--profile` (or `FM_PROFILE`)
  picks the profile; without one it uses the platform data folder's `fuckMail/default`,
  or `fuckMail/demo-v1` with `--demo-profile` (`--smoke` and `--self-test` use a temporary
  profile). `--engine` (or `FM_ENGINE_PATH`) picks the engine binary; by default it is
  the `fm-engine` beside the app.
- `--demo-profile` opens synthetic demo content; `--live-profile` creates or opens a real,
  initially empty profile at `--profile`. `--fixture-clock` pins the clock to the demo
  data's date.
- `--screenshot <file.png>` saves the rendered window after data loads, plus
  `<file.png>.items.json` with named item geometry. `--screenshot-view`,
  `--screenshot-calendar-view`, `--screenshot-date`, `--screenshot-settings-page`,
  `--screenshot-width`/`-height` and `--screenshot-set`/`-invoke`/`-click` stage the view.
  Run `fuckmail-gui --help` for the full list.
- For a quick compile while editing UI, configure a separate build directory with
  `-DFM_GUI_ONLY=ON`. It builds the GUI and its own tests but not the engine, packaging
  or whole-app checks, so the result cannot run on its own.
