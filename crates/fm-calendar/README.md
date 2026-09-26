# fm-calendar

The Google Calendar event model for fuckMail. It turns cached Google event
resources into typed events, expands recurrence into bounded windows,
projects the rows that the calendar views and the `fm` CLI show, and builds
conditional write requests. It has no database: `fm-core` owns event sources,
sync cursors and pending writes, and `fm-engine` runs the HTTP calls.

## Main entry points

- **`normalize(account, calendar, calendar_zone, source)`** keeps the original
  Google JSON and distinguishes all-day dates from timed events with offsets.
- **`expand(event, exceptions, window, access)`** produces the occurrences of
  an event inside a `Window`, keyed by account, calendar, series and original
  start, including moved and cancelled exceptions.
- **`Window::from_dates`** and **`project_google_events_in_window`** project
  every source calendar into one display-zone range. Source zones still drive
  normalization and recurrence. Rows sort by display-local date, all-day before
  timed on the same date, then by instant and stable identity. Fields the
  calendar's `Access` does not permit (for example private or free/busy-only
  calendars) are removed before serialization.
- **`edits`**: `edit`, `rsvp` and `following` build ETag-guarded
  `GoogleWrite` requests. Google Calendar sends attendee notifications; this
  crate never creates a second invitation email. A "this and following" edit
  stays two requests (trim the series, create the new one) and asks the caller
  to reconcile exceptions.
- **`recurring`** validates the small recurrence subset the editor can write
  and splits a rule for "this and following" edits. Provider rules outside
  that subset are kept as they are, never approximated.
- **`sync::SyncBatch`** stages paged incremental sync. It rejects cursor loops
  and out-of-order pages, and exposes the replacement set and final sync token
  only once the batch is complete. An expired sync token (HTTP 410) restarts a
  full batch.
- **`google_api::GoogleCalendarApi`** issues single Calendar API requests
  (calendar list, events, free/busy, writes) through an `fm-google`
  `HttpTransport`. It never retries on its own; quota and retry policy belong
  to the caller.

## Limits

Query windows are at most 42 days and 1,000 occurrences. An event may carry
at most eight recurrence lines (8 KiB in total), and its source at most
256 KiB. Sub-daily recurrence (`SECONDLY`, `MINUTELY`, `HOURLY`), unknown time
zones and rules the recurrence library rejects return errors; a truncated
result never claims full coverage. Custom `VTIMEZONE` definitions and external
iCalendar feeds are not supported.

Time zones come from `chrono-tz`'s bundled IANA database (`TZDB_VERSION`).

## Vendored `rrule`

`vendor/rrule` is `rrule` 0.14.0 with one recurrence fix, described in
[FUCKMAIL-PATCH.md](vendor/rrule/FUCKMAIL-PATCH.md). Upstream shifts a
nonexistent local time (for example 02:30 on a spring-forward day) to a later
hour and counts it against `COUNT`. RFC 5545 requires such an occurrence to be
skipped without consuming the count, and a repeated wall time to use its first
occurrence. The patch changes only the candidate conversion in
`src/iter/utils.rs`, not the recurrence algorithm, and keeps the upstream MIT
license and source commit.

DATE-valued `RDATE`/`EXDATE` values are normalized to the same internal
carrier as the recurrence dates, so date exclusions match, while events still
expose plain dates.

The regression `non_existent_dst_wall_time_is_skipped_without_consuming_count`
in `tests/calendar.rs` protects the fix; keep it when moving back to an
upstream release. The vendored crate is excluded from the workspace. Run its
own tests with `TZ=UTC`, because one unrelated upstream test assumes the
process time zone is UTC:

```sh
TZ=UTC cargo test --manifest-path crates/fm-calendar/vendor/rrule/Cargo.toml --lib --locked
```

## Tests

```sh
cargo test -p fm-calendar
```

The integration tests cover Toronto's March and November DST transitions
(including nonexistent and repeated wall times), exclusive all-day end dates,
monthly rules, `RDATE`/`EXDATE`, moved and cancelled instances, account
scoping, private and free/busy field removal, ETag and actor checks,
invitation ownership, and paginated sync with token expiry.
