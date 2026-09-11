# Calendar, feeds and reminders

State: Draft · Revision: 2 · Updated: 2026-09-11 · Implementation: not started.

Owner: F — Calendar. Accepted scope and proposed behavior are separated below; concrete APIs, libraries and outstanding choices await the relevant feasibility work. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Google Calendar and read-only feeds, event/occurrence semantics, all views/search, edits/invitations, current/next status and reminder acknowledgements |
| Consumes | Core event/version store and account identity, provider changes and mail invitations, Platform notification/lifecycle facilities |
| Produces | Bounded today-strip and occurrence/search views with selected calendar identities, date/time-zone context and coverage; permission-aware actions, conference/location fields and occurrence reminder intents |
| Independent start | Calendar/ICS fixtures and mocked local API, including recurrence exceptions, time zones and simultaneous events |

Related: [core](core-storage.md), [Gmail](gmail.md), [IMAP/SMTP](imap-smtp.md), [platform](platform.md), [calendar research](../research/calendar.md).

## Calendar

Calendar correctness is a dedicated MVP gate because unreliable Google integration is a primary reason for building the product. Keep Google calendar operations native to the Google API and event permissions explicit. Shared calendars must show events and permit edits only to the extent the account is allowed.

| Workflow | Proposed acceptance detail implementing the confirmed requirement |
| --- | --- |
| Views | Day/week/month/agenda use the same event state and preserve calendar identity; calendar visibility is selectable; compact expandable all-day rows show hidden counts; simultaneous/overlapping events remain visible |
| Find an event | Search cached title, organizer, attendees, location and description within selected calendar visibility; show search coverage and navigate to the matching occurrence |
| Create/edit | Timed/all-day events, correct target calendar, time zone, title/location/description and attendees; deletion/cancellation handled correctly |
| Recurrence | Preserve series and exceptions; support common rules and editing one occurrence, the series, or following occurrences; unusual rules must not be silently simplified |
| Invitations | Organize, update and cancel meetings; attendee accept/tentative/decline; changed times and cancellations propagate without duplicate invitations |
| Shared calendars | Honor read-only, write and limited-visibility access; recover cleanly if permissions change |
| Reminders | Local notifications while running, with clear Soon/Now timing and explicit dismiss-for-this-occurrence; Join opens the conference URL and acknowledges that reminder; respect notification preferences, avoid duplicates, and stop after app close |
| iCalendar URLs | Add/remove/show/hide read-only feeds, refresh and cache locally, render recurrence/time zones; no editing or RSVP through the feed |

Read-only subscriptions connect directly to the subscribed URLs, with no intermediary service. Preserve the last valid feed if refresh fails, show freshness/error state, and reconcile removals only after a successful complete refresh. Treat private subscription URLs as secrets in logs and diagnostics.

Mail invitations and calendar events must converge on the same provider-backed event state. Avoid separately emailing an invitation when the Google Calendar operation already handles attendee notifications. Shared-calendar use does not automatically require an ACL administration UI. Calendar overlay defaults and which running-app notifications are enabled remain design choices.

Keep conference URL and physical location as separate fields. Present a clear Join action in event details/reminders using provider conference data or a recognized link from the feed; no meeting-service integration is needed. Joining/dismissing acknowledges the local reminder only, never an RSVP or a claim that the meeting was attended. Long descriptions and attendee lists expand on demand. Show current/next time and other concurrent events without mandatory modal takeover, flashing titles, repeated audio, or ambiguous close-versus-dismiss semantics. Reminder state is scoped to the account, event and occurrence, and reconciles when the meeting changes.

## Persistent today strip and full calendar

Today's calendar is a required strip at the bottom of every mail screen, including search, reading and composition. This fixes the earlier optional placement/visibility proposal. Retain the dedicated day/week/month/agenda calendar. See the [visual design brief](../ui-ux.md) for design treatments and open choices.

Calendar owns event content, occurrence identity, current/next computation, coverage and reminder/Join behavior. Mail UI owns the strip's placement and resizing with the mail shell. Supply a bounded day view with explicit selected calendar identities and date/time-zone context; keep the strip and full calendar on the same state, including date rollover and event changes. Both streams can use the same synthetic event fixture before a provider is available.

The proposed horizontal timeline makes elapsed/remaining time and event durations readable. Preserve overlaps and all-day overflow, distinguish empty from stale/unloaded, and offer event details, Join where available and a route to the full calendar. Which calendars appear while switching mailboxes is still open; the strip itself remains visible.

## Calendar consistency

Use one local event store for day/week/month/agenda and reminders. Keep Google series identity, exceptions, attendee state and event versions together. Map a mail invitation to its calendar event and reconcile provider updates before sending any additional scheduling email. Prevent duplicate notifications when synchronization replays changes.

Calendar subscriptions are read-only snapshots; a failed or malformed refresh does not erase the last valid events. Resolve feed removals only from a valid complete refresh, and display stale status. Do not route feed edits or RSVP to Google or SMTP as if the feed granted those capabilities.

Schedule reminders from the locally reconciled event state and invalidate them when events move, cancel or disappear. Closing the GUI stops its synchronization/reminder session; explicit independent CLI calendar/sync commands may run without the GUI. On reopening, reconcile before scheduling upcoming reminders; do not replay a burst of obsolete alarms. Scheduling horizon and cross-account overlay defaults remain design choices. Test daylight-saving changes, all-day boundaries, organizer/attendee updates and edits from another device as distinct acceptance cases.

Serve agenda, current/next status and calendar search from the same bounded event/occurrence views. Keep all simultaneous or overlapping events and selected calendar identities visible. Compact all-day rows expose hidden counts. Join opens the conference URL and acknowledges only the local occurrence reminder; physical location, RSVP and attendance remain distinct. Honor acknowledgement through refresh and reconcile it against rescheduled occurrences.

## Acceptance gates

- Exercise applicable calendar views/queries, editing/recurrence, RSVP, feeds and permissions through the headless CLI as well as the GUI, preserving the same event/operation state.
- Feed the persistent bottom strip and full calendar from the same occurrence state. Check mailbox/view switches, compact windows, day rollover, no events, overlapping/all-day events and stale/offline coverage; the strip must not disappear from mail screens or imply that unloaded calendars are empty.
- Day/week/month/agenda, cached search and current/next status agree on the same event/occurrence identity. Show true overlaps, all-day hidden counts, selected calendar identities and search coverage.
- Test one-occurrence, series and following-occurrence edits, exceptions, DST/all-day boundaries, organizer changes, RSVP, cancellation and remote concurrent edits without silent rule simplification or duplicate invitations.
- Recover expired sync tokens and changed permissions; keep a valid cached feed after a failed refresh and remove feed events only after a complete valid refresh. Feed URLs stay out of logs; feeds grant no edit/RSVP capability.
- Join opens the conference URL while preserving physical location and RSVP; acknowledgement survives refresh for that occurrence and reconciles on rescheduling. GUI close stops its reminders; reopen reconciles before scheduling upcoming alerts. CLI-only calendar operations share event state and do not create a hidden reminder service.

## Open before affected behavior ships

Resolve which calendars populate the strip on mailbox changes, timeline range/overflow, recurrence edit controls, reminder horizon and running-app alert preferences. Bottom-strip placement and presence on all mail screens are already settled. Define invitation handling across accounts and event conflict rules. No CalDAV editor or shared-calendar ACL administration is required for MVP. Calendar correctness is an early integration gate, not work postponed until the mail UI is polished.
