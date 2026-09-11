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

1. Commit a paginated fake Gmail import/history update, normalized messages, FTS changes and provider checkpoint through Core; prove restart and replay ordering.
2. Extend the persisted draft/capture experiment to authored formatting and a retained attachment, then test crash/unknown outcomes and schema migration. The plain Unicode capture/reopen test now passes.
3. Assert the incoming MIME manifest’s visible-text and unsafe-text exclusions through the exact Core extraction path.
4. Add explicit GUI assignment of configured categories and a CLI readback; filtering alone is not assignment.
5. Consume permitted fake assistance through a Core transaction that rechecks source/policy/manual/provider stamps. Independent validator tests are insufficient.
6. Apply completed calendar sync/edit/RSVP fake outcomes to the one store; incomplete batches must retain the previous view and cursor.
7. Test real engine termination, lost-response replay, queued disconnect/cancellation and slow-client isolation beyond graceful restart.
8. Relocated Mac package verification is complete; finish remaining upgrade checks and actual native CI jobs. Physical other-host, live-provider and real-runner evidence remains separately unverified.

This list preserves the full Phase 0 scope; it does not add legacy protocols, browser-accurate HTML, a finished CLI, signing or larger benchmark tiers to the first foundation gate.


## Pushed native checkpoint

`02d33ca` includes the native implementation and the verified offscreen-plugin deployment fix. Final `scripts/dev.sh test` passed **6/6 CTests in 58.95 seconds**. The package was copied to a path with spaces, then exercised with system-only PATH and SDK/Qt/engine overrides cleared. Packaged CLI help/search, GUI smoke/integration, bundled Qt dependency loading and final-client shutdown passed.

Generated archive: `dist/fuckmail-darwin-arm64.tar.gz` (about 53 MiB), SHA-256 `80d5cb9defb71c9ab9f08220c5265b3542b5a0fd2fbe98b1641f09748f5e122b`. Its manifest records `8b8230f` plus the two packaging changes subsequently committed as `02d33ca`; it does not mislabel that pre-commit archive as a clean release build. Exact logs are in ignored `build/verification/native-test-final.log`, `native-package.log` and `package-runtime.log`.

[Native CI run for 02d33ca](https://github.com/yegors/fuckMail/actions/runs/34645947282) started for macOS, Windows and Ubuntu. At this recording it is in progress, not a passing result. The HTTPS Git token lacked workflow scope; the authorized GitHub connector created the workflow, that commit was merged locally, and the remaining push succeeded. No credentials were exposed or changed.
