# Phase 0 calendar persistence execution

P0-06 / Core integration, 2026-09-11. Consumes Calendar contract revision **6**, Core schema **4**, and synthetic fixture revision **1**. This extends the [calendar library experiment](../../crates/fm-calendar/README.md) into the existing Core database and shared calendar query. No Google credentials, HTTP transport, event editor, reminder service or external calendar feeds are involved.

## Observed tests

On this macOS arm64 host with Rust 1.88.0:

```sh
cargo test -p fm-core --lib calendar_store::tests --locked
cargo test -p fm-calendar --locked
cargo clippy -p fm-core -p fm-calendar --all-targets --locked -- -D warnings
```

All **8 Core calendar integration tests passed**; the final test runtime was **13.13 seconds** while other work used the host. All **17 calendar library tests** also passed (0.01 seconds); Clippy passed for both crates. Tests use disposable local profiles. [Implementation](../../crates/fm-core/src/calendar_store.rs), [tests](../../crates/fm-core/src/calendar_store_tests.rs), and the [historical migration](https://github.com/yegors/fuckMail/blob/3168800/crates/fm-core/src/migration-004-calendar.sql) are the executable evidence from that checkpoint.

| Boundary | Verified behavior |
| --- | --- |
| Complete batches | Partial pages cannot publish through the completed-batch API. Token expiry discards unpublished batch work while prior source/cursor remain. A completed two-page replacement updates the cursor and source visibility together. |
| Transaction recovery | A controlled failure on the second event INSERT rolls back the first event, visibility changes, cursor and batch receipt. Reopening and retrying succeeds. Exact replay returns its earlier receipt; different content cannot consume the same revision. |
| Source retention | Full-scan absence hides an event only when the replacement finishes; its source is retained. A sparse cancellation preserves the previous series/original-start identity for recurrence, while a separate observation field preserves the exact received tombstone. A cancelled moved instance does not reappear from its master. |
| Account/calendar identity | Identical event IDs in different calendars/accounts remain separate. Shared Core `calendar` queries group source by actual calendar and apply that calendar's zone and access before returning rows. |
| Display-zone boundaries and ordering | UTC and Tokyo sources share the advertised Toronto date window, including midnight boundaries in both directions. UTC recurrence keeps its source zone. All-day dates do not shift. Rows sort by displayed date, all-day before timed on that date, then actual timed instant and stable identity. |
| Permissions | Owner, private Reader, FreeBusy and None projections are tested. Restricted title/location/description fields do not enter shared JSON. ACL or time-zone changes atomically advance the global revision used by client polling; identical configuration does not invent a change. |
| Human edits | A prepared edit is durable before fake acknowledgement and does not change displayed source. Acknowledgement verifies account/calendar/event and recurrence identity, current ETag/access, intended fields and a new ETag. Exact acknowledgement replay is stable after reopen. Revoked permissions or a newer organizer version leave the authored pending intent and current source intact. |
| RSVP | The actor comes from the configured account and must be the event's self attendee. Only that response changes. A controlled acknowledgement SQL failure leaves source/cursor/receipt unchanged; reopen and replay then commit once. |

There is one source table, `calendar_sources`. Added tables hold sync scope/cursor, visibility/exact observation metadata, batch receipts and prepared write receipts. They do not create another calendar cache. Migrating existing profiles retains the fixture's primary-calendar Reader/Toronto defaults; newly seeded profiles initialize the same metadata. The shared week/footer/CLI query still uses the same bounded occurrence library.

The independent midnight regression failed before correction: `2026-09-12T02:00Z`, which belongs to Toronto September 11 at 22:00, was excluded because the UTC source calendar supplied the range boundary. Core now constructs one `Window::from_dates` from its advertised view zone and passes it unchanged to `project_google_events_in_window` for every calendar. Source zones still normalize recurrence. The prior `project_google_events` function remains a compatible single-zone wrapper. Ordering now compares parsed instants, so `09:00+02:00` correctly precedes `08:00-04:00` on the same displayed day.

## Limits and remaining work

These are deterministic provider-shaped writes and acknowledgements inside Core. The tests do not establish actual Google ETag semantics, permission changes, attendee notifications, transport cancellation or ambiguous HTTP acceptance. Google Calendar remains the sole notification owner in the prepared write plan; no invitation email is emitted here. The methods are Rust-internal integration entry points, not new GUI/CLI event-editing commands.

Completed batches accept at most 10,000 source records, with 256 KiB per normalized source. Shared projection accepts at most 5,000 retained sources, 1,000 resulting occurrences and a 42-day range. Unsupported data fails explicitly. These are feasibility bounds, not a large-calendar performance or complete-history claim. Only completed batches are durable; an interrupted partial batch restarts from the prior committed cursor. A changed query fingerprint cannot reuse that cursor and requires an explicit full-resync workflow.

Full replacement hides absent records while retaining their source; garbage collection is not implemented. Multi-request following-occurrence editing remains the existing non-atomic library plan and still needs durable multi-step orchestration. Live account testing, event editor interaction, reminders, calendar selection/overlay defaults and host-specific desktop behavior retain their separate gates. SQL failure and close/reopen tests are not hardware power-loss evidence.
