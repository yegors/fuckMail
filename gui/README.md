# Native GUI feasibility slice

P0-00/P0-01/P0-04/P0-06, Mail UI and Calendar draft revision 5, synthetic fixture revision 1. C++20, Qt Quick/QML 6.11.2. The current design follows the September 11 initial mail/calendar/settings trio. It has no Node.js toolchain, live provider connection, model, received HTML renderer or database access.

`MailBackend` is the asynchronous private-IPC adapter. Its bounded queue sends exactly one request at a time, matching the engine session contract. QML receives bounded query results and invokes the same account-scoped engine commands used by the CLI. `IpcTrust` checks owner-private discovery metadata before reading its handshake token and checks the connected local server identity. This is an OS-user boundary, not protection from arbitrary hostile code already running as that same user.

The mail workspace has separate accounts, paged conversation results, sender/text search, explicit member-snapshot read/star/category actions, expandable readable messages, engine-verified retained attachment opening and a durable human reply. The signature is a separate account preview; the MIME builder appends it once when building outgoing source. The persisted GUI draft remains plain text. `ComposerProbe` separately exercises Qt's native formatting/selection/undo representation for the MIME feasibility boundary. Its optional `--export-dir fixtures` writes the actual Qt HTML and exact plain-text pair consumed by shared draft/MIME round-trip tests.

Account switching restores search, folder/category, selected conversation, pane sizes and scroll position. The category editor displays None/Some/All membership and applies or clears only the current conversation member snapshot; filter chips remain separate. Navigation waits for any pending draft save. A failed draft save keeps the editor and prevents a silent close. Settings retains the workspace underneath and only provides the general shell/navigation; the incomplete mockup switches are intentionally absent.

The selected account's fixture week and today's footer use the same projected calendar occurrence identities. Presentation splits midnight-spanning events, clips all-day exclusive intervals and places overlapping events in separate columns/lanes. It does not infer Busy access to another account or write calendar changes. Dates and labels explicitly identify the synthetic fixture date.

## Verification

Locally compiled on macOS arm64 against Qt 6.11.2. These executable checks passed:

- `gui-ipc-trust`: owner/mode checks, symlink rejection and connected local process identity (macOS requires the local-socket-capable execution environment).
- `gui-composer-probe`: authored Unicode, selection, bold/italic, undo/redo, HTML round-trip; no persisted draft format change.
- `gui-calendar-layout`: overlapping/touching events, overnight clipping, unchanged occurrence identity, today's clipping and exclusive all-day spans/lanes.

`--self-test` is the real GUI/engine/independent-CLI integration test. It creates a disposable private profile and checks bounded paging across refresh, Unicode text input, keyboard cursor/selection preservation across real mouse category actions and CLI-driven refresh, both flags, category assignment/removal readback, CLI draft recovery, account/search/selection restoration, settings state and calendar/footer isolation. This test passed on 2026-09-11 against the real Rust engine and separate CLI on macOS arm64. It initially exposed and then verified fixes for request pipelining and the first-status-poll race. Three native Metal-rendered screenshots were captured and inspected at 2880 × 1900 in `build/visual-evidence/{mail,calendar,settings}.png`; snippet overflow, short-event labels and calendar header alignment were corrected from those captures. This verifies synthetic input events, not every OS input method or accessibility workflow. Windows/Linux execution remains separate evidence.

For a GUI-only development compile while working on other Rust packages:

```sh
cmake -S . -B build/gui-only -G Ninja -DCMAKE_PREFIX_PATH="$FM_QT_ROOT" -DFM_GUI_ONLY=ON -DBUILD_TESTING=ON
cmake --build build/gui-only
```

Use the normal `dev` or `release` preset for the full application and integration checks; the GUI-only option excludes the engine and packaging. `--engine` / `FM_ENGINE_PATH` selects the executable for a development build. `--profile` / `FM_PROFILE` selects a profile. With neither override, the GUI and CLI use the same platform-local `fuckMail/demo-v1` path.

`--smoke` without an explicit profile creates disposable fixture data. `--screenshot file.png --screenshot-view mail|calendar|settings` captures a real rendered workspace after data arrives. Startup retries are bounded to 30 seconds; smoke and self-test watchdogs allow 40 and 60 seconds respectively. Closing the GUI disconnects its session, allowing the engine to exit when the last client leaves; the GUI does not kill an engine still used by another CLI session. A temporary test profile is removed only after engine discovery disappears, with a bounded wait.
