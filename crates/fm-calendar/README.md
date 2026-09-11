# Google Calendar feasibility boundary

P0-06, initially consumed Calendar contract draft revision 5, independent fixture revision 1; the integrated behavior is recorded in current Calendar revision 7. This crate has no database, network connection, feed reader or email sender. Core remains the single persistence owner. The current GUI uses its small read-only stored fixture week; richer calendar semantics have separate executable feasibility evidence. Core's shared week/footer/CLI projection uses `project_google_events_in_window` with its retained synthetic Google source collection, without introducing another store. The real GUI/CLI integration test passed against that single store on macOS.

`normalize(account, calendar, calendar_zone, source)` preserves Google source and distinguishes dates from offset-bearing timed events. `expand(event, exceptions, window, access)` produces a bounded occurrence view keyed by account, calendar, series and original start, including moved/cancelled exceptions. The same projected result is intended for the GUI, CLI and footer; permission filtering happens before serialization.

Core uses `Window::from_dates` and `project_google_events_in_window` to project every source calendar into one advertised display-zone range; source zones still control normalization and recurrence. The original `project_google_events` remains a compatible single-zone wrapper. Shared row ordering uses display-local date, all-day before timed on that date, then actual instant and stable identity. The [Core integration evidence](../../docs/research/calendar-core-execution.md) includes a regression for UTC/Tokyo midnight boundaries, recurrence, stable all-day dates and differing-offset timestamp ordering.

`edit`, `rsvp` and `following` create provider write plans with ETag guards. Google Calendar is the only attendee-notification owner; no second invitation email is created. Following edits remain two operations and explicitly require exception reconciliation. The caller must provide reviewed trim/new-series bodies and durable orchestration; this is not a claim to have completed live recurrence editing.

`SyncBatch` stages consistently scoped pages, rejects cursor loops/out-of-order pages, and exposes a replacement batch and final sync token only at completion. A simulated HTTP 410 restarts a full batch. The schema4 Core integration tests now verify retained visible data and pending user intent until the new complete batch commits, plus atomic source/cursor publication and replay.

Validated on 2026-09-11, macOS arm64, Rust 1.88.0:

```sh
cargo test -p fm-calendar --locked
cargo clippy -p fm-calendar --all-targets --locked -- -D warnings
```

Seventeen integration tests passed (0.01 seconds test runtime); Clippy passed. Independent expected values cover Toronto's March/November DST transitions (including nonexistent and repeated wall times), exclusive all-day end dates, monthly rules, RDATE/EXDATE, moved/cancelled instances, moved instances entering a different window, overlaps, account scoping, private/freebusy field removal, private-master exceptions, ETag/actor checks, invitation ownership and paginated/token-expiry state.

Versions: a [documented local patch](vendor/rrule/FUCKMAIL-PATCH.md) of `rrule` 0.14.0, `chrono-tz` 0.10.4 (bundled IANA 2025b), `chrono` 0.4.45. A test exposed that DATE-valued EXDATE did not exclude our recurrence's UTC-midnight date carrier. The adapter now normalizes DATE RDATE/EXDATE to the same internal carrier while preserving the original source and exposing dates, never timed all-day values.

An independent spring-gap fixture also exposed upstream `rrule` shifting a nonexistent 02:30 occurrence to 03:30 and consuming COUNT. The vendored correction returns no candidate for nonexistent times and selects the first occurrence of repeated times, as required by RFC 5545. It changes the candidate conversion, not the recurrence algorithm. The upstream MIT license and exact source commit are retained. Two upstream expectations explicitly changed for that correction; the full 299-test upstream unit suite passed with `TZ=UTC` (an unrelated upstream local-zone test assumes UTC):

```sh
TZ=UTC cargo test --manifest-path crates/fm-calendar/vendor/rrule/Cargo.toml --lib --locked
```

Limits are explicit: 42-day query windows, 1,000 occurrences/exceptions, eight recurrence lines / 8 KiB recurrence source, 256 KiB event source. Unsupported sub-daily recurrence, unknown zones and library validation limits return errors; no truncated result claims full coverage. No custom VTIMEZONE/feed compatibility is claimed. Provider-expanded instances remain a future adapter option for unsupported rules. The [Core execution report](../../docs/research/calendar-core-execution.md) records tested durable fake-provider batch/write recovery. Windows/Linux desktop execution, actual Google permissions/notification behavior, timezone database update policy, and live transport recovery retain their separate evidence requirements.

Primary sources checked 2026-09-11: [Google recurring events](https://developers.google.com/workspace/calendar/api/guides/recurringevents), [Google incremental synchronization](https://developers.google.com/workspace/calendar/api/guides/sync), [event resources](https://developers.google.com/workspace/calendar/api/v3/reference/events), [rrule's bounded API](https://docs.rs/rrule/0.14.0/rrule/struct.RRuleSet.html), [chrono-tz](https://docs.rs/chrono-tz/0.10.4/chrono_tz/), [RFC 5545 recurrence rules](https://www.rfc-editor.org/rfc/rfc5545#section-3.3.10).
