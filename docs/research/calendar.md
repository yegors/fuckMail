# Calendar: Google, subscriptions and recurrence

Sources checked 2026-09-10; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Calendar contract](../contracts/calendar.md).

## Calendar interoperability

Google Calendar incremental synchronization uses sync tokens. An invalid token returns HTTP 410 and requires a new full synchronization. Keep collection scope consistent and commit the replacement cursor only after the required pages have been applied. [Calendar synchronization](https://developers.google.com/workspace/calendar/api/guides/sync)

CalDAV provides calendar access over WebDAV; iCalendar defines the event representation. WebDAV collection sync is a separate extension. CalDAV scheduling and email-delivered invitations add further protocols. Consequently, CRUD, synchronization, subscriptions, and invitation handling need separate capability tests. [CalDAV](https://www.rfc-editor.org/rfc/rfc4791), [iCalendar](https://www.rfc-editor.org/rfc/rfc5545), [collection sync](https://www.rfc-editor.org/rfc/rfc6578), [CalDAV scheduling](https://www.rfc-editor.org/rfc/rfc6638), [iMIP](https://www.rfc-editor.org/rfc/rfc6047)

Recurring series contain rules and exceptions. Google warns against updating every occurrence individually to change a series. Recommendation: preserve series identity and exceptions, expand only a bounded view window, and test occurrence/series edits independently. [Google recurring events](https://developers.google.com/workspace/calendar/api/guides/recurringevents)

Read-only URL subscriptions are a separate, simpler integration: periodically retrieve the iCalendar feed, cache a successfully parsed snapshot, and render it locally. They provide visibility, not an editable calendar or scheduling endpoint. The founder explicitly requires these in MVP and has deferred editable CalDAV. [iCalendar format](https://www.rfc-editor.org/rfc/rfc5545)

Calendar has per-project/per-user quotas and operational limits, including bursts of writes. Measure polling and recurrence edits with the actual project; avoid rewriting large sets of occurrences to represent a series edit. [Calendar usage limits](https://developers.google.com/workspace/calendar/api/guides/quota)

## Next experiments

P0-05 (calendar): evaluate maintained iCalendar and recurrence libraries, packaging and licenses. Editable CalDAV library selection can wait until its later phase.

P0-06 (calendar): exercise recurring-event edits, organizer updates, attendee RSVP, permission changes and read-only feeds early. Design the full calendar fixtures before promising a daily driver. Mail invitation transport is coordinated with the Gmail and IMAP/SMTP contracts.
