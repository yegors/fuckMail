# Calendar: Google, subscriptions and recurrence

**Current phase scope under D009 (2026-09-11):** Google Calendar is first. Recurrence, time zones, invitations and relevant iCalendar representations remain early work. External iCalendar URL fetching/subscriptions and their old early experiments below are deferred until founder Google GUI/CLI dogfooding; CalDAV remains separately deferred.

Sources checked 2026-09-10; reorganized 2026-09-11. The original research below records source evidence and proposals. The subsequent [calendar library experiment](../../crates/fm-calendar/README.md) and [Core integration execution](calendar-core-execution.md) now provide synthetic implementation/test evidence. No live calendar account flow is claimed; the external sources below have not been revalidated by these local tests.

Build boundary: [Calendar contract](../contracts/calendar.md).

## Calendar interoperability

Google Calendar incremental synchronization uses sync tokens. An invalid token returns HTTP 410 and requires a new full synchronization. Keep collection scope consistent and commit the replacement cursor only after the required pages have been applied. [Calendar synchronization](https://developers.google.com/workspace/calendar/api/guides/sync)

CalDAV provides calendar access over WebDAV; iCalendar defines the event representation. WebDAV collection sync is a separate extension. CalDAV scheduling and email-delivered invitations add further protocols. Consequently, CRUD, synchronization, subscriptions, and invitation handling need separate capability tests. [CalDAV](https://www.rfc-editor.org/rfc/rfc4791), [iCalendar](https://www.rfc-editor.org/rfc/rfc5545), [collection sync](https://www.rfc-editor.org/rfc/rfc6578), [CalDAV scheduling](https://www.rfc-editor.org/rfc/rfc6638), [iMIP](https://www.rfc-editor.org/rfc/rfc6047)

Recurring series contain rules and exceptions. Google warns against updating every occurrence individually to change a series. Recommendation: preserve series identity and exceptions, expand only a bounded view window, and test occurrence/series edits independently. [Google recurring events](https://developers.google.com/workspace/calendar/api/guides/recurringevents)

Read-only URL subscriptions are a separate integration: periodically retrieve an iCalendar feed, cache a successfully parsed snapshot, and render it locally. They provide visibility, not an editable calendar or scheduling endpoint. D009 defers their implementation until after Google GUI/CLI dogfooding; editable CalDAV remains separately deferred. [iCalendar format](https://www.rfc-editor.org/rfc/rfc5545)

Calendar has per-project/per-user quotas and operational limits, including bursts of writes. Measure polling and recurrence edits with the actual project; avoid rewriting large sets of occurrences to represent a series edit. [Calendar usage limits](https://developers.google.com/workspace/calendar/api/guides/quota)

## Original experiment proposals

P0-05 (calendar): evaluate maintained iCalendar and recurrence libraries, packaging and licenses. Editable CalDAV library selection can wait until its later phase.

P0-06 (calendar): exercise Google recurring-event edits, organizer updates, attendee RSVP and permission changes early. External-feed experiments are deferred. Coordinate current mail invitation transport with Gmail; the IMAP/SMTP contract joins only in the later phase.

## Phase 0 scope and library feasibility

Checked 2026-09-11. OAuth is shared with the [Google investigation](gmail.md), while event permissions and recurrence remain Calendar's responsibility.

A candidate scope set is `calendar.events` for event reads/writes on accessible calendars, `calendar.calendarlist.readonly` for calendar discovery and `calendar.calendars.readonly` for calendar properties/time zones. Add `calendar.settings.readonly` only if the method inventory needs account settings. These are a proposal to verify per endpoint: owned-events-only is insufficient for writable shared calendars, and the broad `calendar` scope should not be requested merely for convenience. Calendar sharing administration is not implied. [Calendar scopes](https://developers.google.com/workspace/calendar/api/auth)

Evaluate `icalendar` for parsing/building RFC 5545 data, `rrule` for bounded recurrence expansion and `chrono-tz` for IANA time-zone rules. The libraries cover different responsibilities. In particular, an IANA zone table does not prove support for arbitrary embedded VTIMEZONE definitions, Windows-style identifiers or preservation of unknown feed properties. Test these before adopting a representation. Record the bundled time-zone database version and how app updates refresh it. [icalendar](https://docs.rs/icalendar/latest/icalendar/), [rrule](https://docs.rs/rrule/latest/rrule/), [chrono-tz](https://docs.rs/chrono-tz/latest/chrono_tz/)

The recurrence candidate documents bounded APIs and limits that do not apply equally to unchecked/iterator paths. Bound occurrence windows and work explicitly, including sparse rules; do not expand an unending series into the database. [rrule limitations](https://docs.rs/crate/rrule/latest/source/README.md)

If the Rust shortlist cannot preserve required recurrence/time-zone behavior, compare libical on exactly the failing corpus. It is a maintained C implementation of iCalendar protocols with MPL/LGPL licensing options; an engine-side wrapper would add native build and ownership obligations. This is a contained fallback, not a reason to move calendar logic into the Qt presentation layer. [libical project](https://github.com/libical/libical)

| Investigation | Inputs and output | Acceptance and fallback |
| --- | --- | --- |
| P0-05: Is the local calendar representation sufficient? | Synthetic RFC examples and Google/ICS-shaped fixtures for all-day/floating/zoned times, DST boundaries, monthly rules, RDATE/EXDATE, moved/canceled occurrences and custom VTIMEZONE. Output expected versus actual bounded occurrence sets, round trips, measured cost and candidate limitations. | Series identity, exceptions and original data survive; unsupported rules/zones remain explicit instead of silently changing event times. Investigate libical for material gaps. Local Google expansion and feed expansion must share occurrence semantics even if adapters differ. |
| P0-06A: Can Google edits and sync converge? | Mock paginated deltas including token 410, concurrent organizer edits, attendee RSVP, permission loss and occurrence/series/following edits. Output operation/checkpoint traces and an invitation-notification ownership table. | Recovery retains pending intent; edits preserve unaffected exceptions; exactly one responsible path requests attendee notifications. Later verify with authorized consumer/Workspace test calendars and controlled recipients; fake success cannot prove Google behavior. |
| Deferred after Google dogfooding (formerly P0-06B): Are subscriptions dependable offline? | Local HTTP feed fixtures for valid updates, removals, redirects, not-modified responses, unavailable/truncated feeds and changed time zones. Output snapshot/refresh coverage and occurrence comparisons. | Only a complete valid refresh replaces cached data; stale state is visible; private feed URLs stay out of diagnostics. Parser limitations or feed errors do not discard the last good calendar. |
| P0-06C: Can both UIs consume one event view? | The same synthetic occurrences for full calendar, today's strip and CLI, including overlaps, all-day events, other-account busy overlays and stale coverage. Output cross-interface comparisons without final styling. | Times, occurrence identity, permissions and freshness agree. An overlay may suppress a title for display but must not claim server permission it lacks. Final visibility defaults stay a design decision. |

Calendar work can begin with these fixtures before OAuth or final UI design. Actual shared-calendar permissions, Workspace policy, invitation notification behavior and sustained sync require later account access; public verification and editable CalDAV remain deferred.
