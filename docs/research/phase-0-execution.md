# Phase 0 execution evidence

Recorded 2026-09-11. Implementation checkpoint on `codex/phase-0`, based on `14f4eca`; native implementation checkpoint `02d33ca` is pushed. This is a running evidence record, not a claim that Phase 0 or the Google-first MVP is complete.

The development host is macOS 15.7.7 on arm64, Apple Clang 17 / SDK 26.2, Rust/Cargo 1.88.0, Qt 6.11.2, CMake 3.31.6 and the pinned Ninja distribution. [Development setup](../build-development.md) owns exact commands and provisioning observations. Dependencies live in ignored `.tools`; user data lives outside the build/package directories.

| Packet | Observed evidence | Remaining gate |
| --- | --- | --- |
| P0-00 | Rust workspace builds; Qt Quick native surface builds; repeatable bootstrap/doctor succeeds on Mac | Native Windows/Linux CI and physical desktop qualification |
| P0-01 | Fixture 1: two isolated accounts, 220 conversations / 286 messages, mixed flags/categories, long thread, retained/missing/partial attachments, read-only calendar sources; five independently checked MIME source files | Broader interruption/provider-operation/assistance integration corpus and larger scale tiers |
| P0-03 | Durable mutation/replay tests, scoped search correctness, staged/verified blobs and safe attachment exports; measured [100k search experiment](search-feasibility-execution.md) | Remaining interruption/checkpoint and real retention/encryption decisions; larger performance tiers |
| P0-10 | Real Rust engine/CLI tests pass: paired state, exact retry, draft restart, final-owner shutdown, simultaneous launches, stale endpoint, private handshake/version/account errors. Qt integration walkthrough passes on Mac | Packaged and native other-OS runs; expanded disconnect/cancellation and upgrade evidence |
| P0-04 | Qt integration passes actual Unicode editor input, CLI draft readback, account/search/selection restoration, read/star independence, settings return, bounded pages and week/today isolation. Separate Qt document probe passes formatting/undo/redo | Complete package/native input checks; persisted draft-to-capture integration; later fuller composer UX |
| P0-02 | [15 Google fake tests](google-feasibility-execution.md), source-backed installed-app/Workspace prerequisites | Durable Core provider checkpoint integration; no live account/auth longevity evidence |
| P0-05 | [9 MIME tests and retained-content tests](mime-feasibility-execution.md), exact author/signature/attachment round trips into a local Gmail capture | Persisted Core draft/capture path and remaining interruption integration |
| P0-06 | [17 independent calendar tests](../../crates/fm-calendar/README.md) plus 299 upstream tests with a documented DST patch; one Core projection supplies GUI week/footer and CLI | Live Google edits/RSVP and full provider-store orchestration are later; no remote calendar exercised |
| P0-07/09 | Explicit snapshot-scoped read/star/category actions persist, replay and remain independent; newer incoming member is not silently affected | Full provider reconciliation/manual override interaction |
| P0-08 | [7 fake-boundary tests](../../crates/fm-assist/README.md): off/absent, invalid/unmapped/stale output, source coverage/draft exclusion and owner cancellation | Durable Core application transaction and real runner evidence before enabling a model |
| P0-11 | Mac archive relocates successfully; packaged GUI/CLI, bundled Qt loading and final-owner shutdown pass with SDK environment cleared | Native CI artifacts, remaining upgrade checks and physical other-host rows |

`cargo test --workspace --locked --offline` passed on this Mac, including actual local-socket engine integration. The first full CTest run exposed two GUI failures: asynchronous request pipelining violated the one-request-per-connection boundary, and initial polling could miss an early external change. Both were corrected and the GUI's native integration test then passed. The subsequent combined `scripts/dev.sh test` run passed all 6 CTests in 60.82 seconds: Rust workspace, GUI smoke, GUI/CLI integration, IPC trust, composer probe and calendar layout. Mac package verification subsequently passed. Log: `build/verification/native-test.log` (generated, not committed).

No live Google account, model, IMAP/SMTP server, external calendar feed or real mailbox content was accessed. Tests use disposable synthetic profiles. File/process recovery tests do not prove physical power-loss durability. CI build/test results, when available, do not prove graphics, input or credentials on the founder's home PC or an unavailable Linux desktop.

Native rendered captures (synthetic data, 2880 × 1900): [mail](assets/phase-0/mail.png), [calendar](assets/phase-0/calendar.png), [settings](assets/phase-0/settings.png). These were inspected for clipping/overflow; they are an initial functional interpretation, not final design approval.

`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` passes on the Mac.


## Remaining local work before the full phase can close

The schema 3 checkpoint closes the earlier local gaps for paginated fake Gmail/Core/FTS commits, persisted formatting/attachment captures, exact MIME extraction, explicit GUI category assignment and engine kill/replay/cancellation. The evidence below and linked tests identify their limits. Remaining work:

1. Consume permitted fake assistance through a Core transaction that rechecks source/policy/manual/provider stamps. Independent validator tests are insufficient.
2. Apply completed calendar sync/edit/RSVP fake outcomes to the one store; incomplete batches must retain the previous view and cursor.
3. Finish actual native Windows/Linux tests and packaged runtime checks, then refresh the Mac package for the final Phase 0 source. Physical other-host, live-provider and real-runner evidence remains separately unverified.

This list preserves the full Phase 0 scope; it does not add legacy protocols, browser-accurate HTML, a finished CLI, signing or larger benchmark tiers to the first foundation gate.


## Pushed native checkpoint

`02d33ca` includes the native implementation and the verified offscreen-plugin deployment fix. Final `scripts/dev.sh test` passed **6/6 CTests in 58.95 seconds**. The package was copied to a path with spaces, then exercised with system-only PATH and SDK/Qt/engine overrides cleared. Packaged CLI help/search, GUI smoke/integration, bundled Qt dependency loading and final-client shutdown passed.

Generated archive: `dist/fuckmail-darwin-arm64.tar.gz` (about 53 MiB), SHA-256 `80d5cb9defb71c9ab9f08220c5265b3542b5a0fd2fbe98b1641f09748f5e122b`. Its manifest records `8b8230f` plus the two packaging changes subsequently committed as `02d33ca`; it does not mislabel that pre-commit archive as a clean release build. Exact logs are in ignored `build/verification/native-test-final.log`, `native-package.log` and `package-runtime.log`.

[Native CI run for 02d33ca](https://github.com/yegors/fuckMail/actions/runs/34645947282) started for macOS, Windows and Ubuntu. Its Mac job passed build, native tests, packaging and artifact retention. Windows compiled Rust and Qt successfully but test compilation exceeded the 180-second execution watchdog, and its GUI smoke/integration checks failed without useful logs. Ubuntu bootstrap failed because Qt’s separate ICU archive was absent. Commit `17f3699` fixes and locally reproduces the ICU loader issue; the next checkpoint separates test compilation and enables useful Windows GUI diagnostics. Neither failed native job is counted as passing. The HTTPS Git token lacked workflow scope; the authorized GitHub connector created the workflow, that commit was merged locally, and the remaining push succeeded. No credentials were exposed or changed.


## Schema 3 integration checkpoint

`cargo test -p fm-core` passed 27 unit tests, 2 coverage tests, 7 mutation tests and 5 search tests (one larger benchmark remains explicitly ignored). This includes 7 actual Google-to-Core integration cases and 9 persisted MIME capture/recovery/migration cases. [Google evidence](google-feasibility-execution.md) and [MIME evidence](mime-feasibility-execution.md) record the exact assertions and import bounds.

Root's `cargo test -p fm-engine --locked --offline` passed 6 actual socket/process integration tests plus a session cancellation test. The new process test owns and forcibly terminates only its own engine child after another client observes a saved draft; the original sender never consumes its response. Reopening and replaying recovers the recorded response, and replay after a newer edit cannot overwrite that edit. A partial-frame owner cannot stall an independent client, and disconnect releases its lifetime. A session test verifies queued-request cancellation; Core tests interrupt an active SQLite read and verify the next owner is unaffected. The first parallel kill test hit its shorter 10-second fixture-startup timeout; the isolated run passed, and aligning the test with the CLI's 15-second startup bound passed the full suite. This is not a power-loss claim.

The full Mac `scripts/dev.sh test` run passed **7/7 CTests in 93.39 seconds**: separate Rust test compilation, complete Rust workspace execution, GUI smoke, GUI/CLI integration, IPC trust, composer probe and calendar layout. Log: `build/verification/schema3-native-test.log`. GUI integration includes actual QML category-button mouse input, independent CLI readback/removal, and preserving authored Unicode and selection across updates. The native Cocoa app also opens on this Mac; a desktop screenshot/launch alone does not prove every physical input behavior.

Incoming text now shares `fm_mail::extract_incoming_text` between fixture seed and fake provider import. All five independent MIME expectations check visible text, unsafe-text exclusions and exact retained sources through Core. Authored Qt formatting remains a document-export feasibility path through persisted drafts/MIME capture; the interactive GUI composer remains plain text. No model or live sending was enabled.
