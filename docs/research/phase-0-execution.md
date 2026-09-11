# Phase 0 execution evidence

Recorded 2026-09-11. Implementation checkpoint on `codex/phase-0`, based on `14f4eca`; native implementation checkpoint `02d33ca` is pushed. This is a running evidence record, not a claim that Phase 0 or the Google-first MVP is complete.

The development host is macOS 15.7.7 on arm64, Apple Clang 17 / SDK 26.2, Rust/Cargo 1.88.0, Qt 6.11.2, CMake 3.31.6 and the pinned Ninja distribution. [Development setup](../build-development.md) owns exact commands and provisioning observations. Dependencies live in ignored `.tools`; user data lives outside the build/package directories.

| Packet | Observed evidence | Remaining gate |
| --- | --- | --- |
| P0-00 | Pinned Rust/Qt tools build on Mac, Windows and Ubuntu; Mac native tests/package pass; Linux ICU acquisition repaired | Latest Windows/Linux test/package rerun and physical desktop qualification |
| P0-01 | Fixture 1: two isolated accounts, 220 conversations / 286 messages; independent MIME, provider/capture, calendar and fake-assistance expectations | 1m/3m and 25 GB/100 GB workloads are later measured tiers |
| P0-03 | One schema 4 store, durable journals/checkpoints, staged verified blobs, interruption/replay and scoped FTS; measured [100k search experiment](search-feasibility-execution.md) | Package upgrade execution; real retention/encryption policy before live data; larger performance tiers |
| P0-10 | Actual engine/CLI/Qt shared state, private transport, independent owners, SQL cancellation, partial frames, process kill and exact receipt recovery | Final native other-OS execution and packaged upgrade/old-client behavior |
| P0-04 | Actual Qt mouse/input/category workflow, persisted Unicode draft and selection, account/search/settings restoration, calendar footer; Qt formatting export reaches the shared persisted MIME capture | Final native other-OS checks; physical Windows/Linux input and later fuller composer UX |
| P0-02 | [16 Google fake tests](google-feasibility-execution.md), 9 Google/Core cases, durable page/checkpoint/FTS commits and manual precedence; installed-app/Workspace prerequisites | Actual OS vault and live personal/Workspace auth longevity before live accounts |
| P0-05 | [Independent MIME corpus and capture tests](mime-feasibility-execution.md), exact originals/decoded content, persisted authored formatting/attachments and recoverable fake sending | Later received HTML presentation and real sending; neither is a synthetic-phase dependency |
| P0-06 | [Calendar/Core integration](calendar-core-execution.md): 8 store tests, 17 independent calendar tests, 299 upstream tests with a documented DST patch; completed batch, edit/RSVP receipts, ACL changes, shared zoned projection | Live Google operations and fuller calendar GUI/CLI editing are later slices |
| P0-07/09 | Snapshot-scoped read/star/category actions through both clients; manual > permitted assistance > provider label reconciliation; new members unaffected by old snapshots | Reviewed composite Done/Delayed UX and live provider reconciliation |
| P0-08 | [6 Core application tests](local-assistance-execution.md) plus 7 fake-boundary tests; durable source/policy/manual/provider rechecks, replay and no review/action changes | Real runner/locality/cancellation before enabling a model |
| P0-11 | Mac relocated archive passes bundled runtime/GUI/CLI/lifetime checks; Windows/Linux build results and identified fixes retained | Final native packages and actual old/new artifact handoff; physical OS vault/input and trusted delivery stay separately unverified |

`cargo test --workspace --locked --offline` passed on this Mac, including actual local-socket engine integration. The first full CTest run exposed two GUI failures: asynchronous request pipelining violated the one-request-per-connection boundary, and initial polling could miss an early external change. Both were corrected and the GUI's native integration test then passed. The subsequent combined `scripts/dev.sh test` run passed all 6 CTests in 60.82 seconds: Rust workspace, GUI smoke, GUI/CLI integration, IPC trust, composer probe and calendar layout. Mac package verification subsequently passed. Log: `build/verification/native-test.log` (generated, not committed).

No live Google account, model, IMAP/SMTP server, external calendar feed or real mailbox content was accessed. Tests use disposable synthetic profiles. File/process recovery tests do not prove physical power-loss durability. CI build/test results, when available, do not prove graphics, input or credentials on the founder's home PC or an unavailable Linux desktop.

Native rendered captures (synthetic data, 2880 × 1900): [mail](assets/phase-0/mail.png), [calendar](assets/phase-0/calendar.png), [settings](assets/phase-0/settings.png). These were inspected for clipping/overflow; they are an initial functional interpretation, not final design approval.

`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` passes on the Mac.


## Remaining available-host verification

Schema 3 closed the paginated fake Gmail/Core/FTS, persisted capture, source extraction, category GUI and engine interruption gaps. Schema 4 now integrates completed Google calendar batches/edit/RSVP acknowledgments and validated fake assistance into the same store. Their scoped reports record limits and independent regressions.

The current closure gates are the final combined Mac suite, latest Windows/Ubuntu native tests and packages, and actual old/new package upgrade/rollback-refusal execution. Physical other-host input/keyrings, live provider access, real model execution, trusted distribution and larger benchmark tiers stay separately unverified. Those are not silently counted as passing or confused with the synthetic foundation.

## Pushed native checkpoint

`02d33ca` includes the native implementation and the verified offscreen-plugin deployment fix. Final `scripts/dev.sh test` passed **6/6 CTests in 58.95 seconds**. The package was copied to a path with spaces, then exercised with system-only PATH and SDK/Qt/engine overrides cleared. Packaged CLI help/search, GUI smoke/integration, bundled Qt dependency loading and final-client shutdown passed.

Generated archive: `dist/fuckmail-darwin-arm64.tar.gz` (about 53 MiB), SHA-256 `80d5cb9defb71c9ab9f08220c5265b3542b5a0fd2fbe98b1641f09748f5e122b`. Its manifest records `8b8230f` plus the two packaging changes subsequently committed as `02d33ca`; it does not mislabel that pre-commit archive as a clean release build. Exact logs are in ignored `build/verification/native-test-final.log`, `native-package.log` and `package-runtime.log`.

[Native CI run for 02d33ca](https://github.com/yegors/fuckMail/actions/runs/34645947282) started for macOS, Windows and Ubuntu. Its Mac job passed build, native tests, packaging and artifact retention. Windows compiled Rust and Qt successfully but test compilation exceeded the 180-second execution watchdog, and its GUI smoke/integration checks failed without useful logs. Ubuntu bootstrap failed because Qt’s separate ICU archive was absent. Commit `17f3699` fixes and locally reproduces the ICU loader issue; the next checkpoint separates test compilation and enables useful Windows GUI diagnostics. Neither failed native job is counted as passing. The HTTPS Git token lacked workflow scope; the authorized GitHub connector created the workflow, that commit was merged locally, and the remaining push succeeded. No credentials were exposed or changed.


## Schema 3 integration checkpoint

`cargo test -p fm-core` passed 27 unit tests, 2 coverage tests, 7 mutation tests and 5 search tests (one larger benchmark remains explicitly ignored). This includes 7 actual Google-to-Core integration cases and 9 persisted MIME capture/recovery/migration cases. [Google evidence](google-feasibility-execution.md) and [MIME evidence](mime-feasibility-execution.md) record the exact assertions and import bounds.

Root's `cargo test -p fm-engine --locked --offline` passed 6 actual socket/process integration tests plus a session cancellation test. The new process test owns and forcibly terminates only its own engine child after another client observes a saved draft; the original sender never consumes its response. Reopening and replaying recovers the recorded response, and replay after a newer edit cannot overwrite that edit. A partial-frame owner cannot stall an independent client, and disconnect releases its lifetime. A session test verifies queued-request cancellation; Core tests interrupt an active SQLite read and verify the next owner is unaffected. The first parallel kill test hit its shorter 10-second fixture-startup timeout; the isolated run passed, and aligning the test with the CLI's 15-second startup bound passed the full suite. This is not a power-loss claim.

The full Mac `scripts/dev.sh test` run passed **7/7 CTests in 93.39 seconds**: separate Rust test compilation, complete Rust workspace execution, GUI smoke, GUI/CLI integration, IPC trust, composer probe and calendar layout. Log: `build/verification/schema3-native-test.log`. GUI integration includes actual QML category-button mouse input, independent CLI readback/removal, and preserving authored Unicode and selection across updates. The native Cocoa app also opens on this Mac; a desktop screenshot/launch alone does not prove every physical input behavior.

Incoming text now shares `fm_mail::extract_incoming_text` between fixture seed and fake provider import. All five independent MIME expectations check visible text, unsafe-text exclusions and exact retained sources through Core. Authored Qt formatting remains a document-export feasibility path through persisted drafts/MIME capture; the interactive GUI composer remains plain text. No model or live sending was enabled.


## Schema 4 integration checkpoint

The final Mac `scripts/dev.sh test` run passed **7/7 CTests in 138.29 seconds**, including the complete Rust workspace (125.28 seconds), GUI/CLI integration, private IPC, composer and calendar layout. Log: `build/verification/schema4-final-native-test.log`. Strict workspace Clippy passes after moving test modules to the end of their Rust files; that cleanup changes no runtime behavior.

Calendar sources, complete batch checkpoints, edit/RSVP acknowledgments and permitted fake assistance now persist through Core. The [calendar report](calendar-core-execution.md) includes independently reproduced fixes for cross-calendar day boundaries and chronological sorting of different UTC offsets. The [assistance report](local-assistance-execution.md) records source/policy/manual/provider checks in the application transaction. A locked fake vault rejects both cached and expired token use without refreshing or falling back to plaintext; real OS vault adapters remain unimplemented and must be verified before live accounts.

The native Cocoa walkthrough used an explicitly disposable profile: actual mouse input saved a Unicode draft, category application was read back independently by the CLI, calendar/settings/account/search navigation preserved state, and closing the final owner removed engine discovery. The final package upgrade script will exercise this behavior across actual old/new binaries; its result is still pending at this checkpoint.

[CI for schema 3](https://github.com/yegors/fuckMail/actions/runs/34648653748) passed on Mac. Ubuntu passed Rust and four Qt checks but lost reply selection when a category popup took focus. Windows completed cold compilation but found a private discovery-file owner mismatch and a fixture-startup timeout. The next source checkpoint sets the explicit Windows process-user owner at file creation, adds an actual Windows trust test, aligns client startup bounds to 30 seconds, uses system fonts for offscreen Windows tests and preserves Linux reply selection. These fixes retain the owner/ACL validation; their native rerun remains required.
