# Calendar, feeds and reminders

State: Draft · Revision: 8 · Updated: 2026-09-11 · Implementation: Phase 0 synthetic experiment verified; Phase 1 changes planned, not implemented; whole area remains Draft.

Revision 8 applies D014 and the [Phase 1 plan](../plans/phase-1.md): a combined calendar with full permitted details, day/week/month/agenda and event details, a browsable timeline only in Mail, and bounded nonrecurring create/edit plus self-RSVP through both clients. Consumers: Core, Mail UI, CLI, Platform and Quality. Current runtime remains schema 4, protocol 1 and fixture 1; the existing public calendar API is still read-only and account-scoped.

Revision 7 records the integrated Phase 0 experiment. Completed fake Google batches, retained cancellations and ETag-scoped edit/RSVP acknowledgements now commit through Core. Account/calendar permissions and time zones reach the same GUI/footer/CLI projection, and ACL changes advance the polling revision atomically. Consumers: Core, Gmail, Mail UI, CLI and Platform. Live sending and full interactive event editors remain later gates.

Revision 6 records D012’s implemented experimental slice. Google-shaped source events now feed the shared read-only CLI/week/footer projection. Independent recurrence, permission, edit-plan and sync-cursor tests live in [fm-calendar](../../crates/fm-calendar/README.md), including a documented upstream DST correction. Live Google editing, reminders and provider-store orchestration remain incomplete. Consumers: Core, GUI, CLI and Gmail.

Revision 5 applies D011: the latest week reference and shared today footer across mail/calendar/settings guide the initial slice. A fixture week is proposed before full calendar editing; pictured settings switches have no specified behavior. D009's Google-first/deferred-feed scope remains. Affected consumers: Core, Mail UI, Platform, CLI and Quality.

Owner: F — Calendar. Accepted scope and proposed behavior are separated below; concrete APIs, libraries and outstanding choices await the relevant feasibility work. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Google Calendar first, later read-only feeds; event/occurrence semantics, all views/search, edits/invitations, current/next status and reminder acknowledgements |
| Consumes | Core event/version store and account identity, provider changes and mail invitations, Platform notification/lifecycle facilities |
| Produces | Bounded combined occurrence/search views and browsable mail timeline with account/calendar identities, date/time-zone context and coverage; permission-aware actions, conference/location fields and occurrence reminder intents |
| Independent start | Calendar/ICS fixtures and mocked local API, including recurrence exceptions, time zones and simultaneous events |

Related: [core](core-storage.md), [Gmail](gmail.md), [IMAP/SMTP](imap-smtp.md), [platform](platform.md), [calendar research](../research/calendar.md).

Revision 3 records the closer nonfinal week/strip direction and its handoff gaps. Affected consumers/partners: Mail UI (footer placement and shared shell), Core (bounded permission-aware occurrence views), Platform (time/focus/lifecycle) and CLI (same permitted calendar fields and operations). No default overlay selection, final layout or new API is accepted.

## Phase 1 boundary — planned

The full calendar and mail timeline combine connected accounts, showing full event details wherever the account's permissions allow them. Mailbox selection remains separate mail navigation. Every event keeps its account, calendar and occurrence identity; a write explicitly chooses that owning account/calendar. Provider-limited free/busy or private events remain restricted before data reaches any view, tooltip, search result or CLI response. Combined display does not grant editing rights or flatten independent account identities.

Reuse the existing normalized recurrence/permission projection and tested overlap/day-clipping logic. Add day/week/month/agenda and event details, including conference URL and physical location as separate fields. The mail-only timeline browses previous/next days and has a Today action; its date is independent of full-calendar navigation. Keep a visible date/time-zone context and truthful incomplete/empty state. Persist bounded calendar visibility/navigation and view preferences through Core's typed boundary.

The first write slice creates and edits nonrecurring events and changes the signed-in attendee's own RSVP using a deterministic local fake acknowledgement. P1-00 fixes the supported field set, validation, calendar selection, operation/capability shape and minimal editor interaction. Reuse the existing edit/RSVP ETag checks and Core journal; the create path must add the equivalent identity, permission, replay and acknowledgement handling. A GUI action and its CLI equivalent produce the same durable event/operation result. Unknown or stale outcomes remain visible and cannot trigger duplicate invitations. Live Google writes and recurring one-instance/series/following edits are later gates; Phase 1 still reads and preserves recurrence correctly.

The current query limit is a half-open range of at most 42 days, 1,000 occurrences and 5,000 retained sources. Define aggregate bounds and scope-aware continuation behavior for the combined view before exposing it. Test permission changes, duplicate provider IDs across accounts, DST, all-day/overnight spans, overlapping lanes and a displayed month/week that excludes today. Assert matching identity/details in all views and CLI, explicit failures for unsupported writes, restart/replay and stale-revision handling. No live account, model, reminders service or external subscription is required for this local slice.

## Calendar

The first milestone implements Google Calendar only. External iCalendar URL subscriptions and their refresh/cache/UI/CLI gates below are explicitly deferred until Google GUI/CLI dogfooding. Google recurrence strings, time zones and invitation semantics may still use iCalendar representations; that necessary parsing is not the deferred subscription feature.

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
| iCalendar URLs — after Google dogfooding | Add/remove/show/hide read-only feeds, refresh and cache locally, render recurrence/time zones; no editing or RSVP through the feed |

Read-only subscriptions connect directly to the subscribed URLs, with no intermediary service. Preserve the last valid feed if refresh fails, show freshness/error state, and reconcile removals only after a successful complete refresh. Treat private subscription URLs as secrets in logs and diagnostics.

Mail invitations and calendar events must converge on the same provider-backed event state. Avoid separately emailing an invitation when the Google Calendar operation already handles attendee notifications. Shared-calendar use does not automatically require an ACL administration UI. The combined calendar shows full permitted details across connected accounts; running-app notification preferences remain a later design choice.

Keep conference URL and physical location as separate fields. Present a clear Join action in event details/reminders using provider conference data or a recognized link from the feed; no meeting-service integration is needed. Joining/dismissing acknowledges the local reminder only, never an RSVP or a claim that the meeting was attended. Long descriptions and attendee lists expand on demand. Show current/next time and other concurrent events without mandatory modal takeover, flashing titles, repeated audio, or ambiguous close-versus-dismiss semantics. Reminder state is scoped to the account, event and occurrence, and reconciles when the meeting changes.

## Mail timeline and full calendar

The calendar timeline remains at the bottom of every Mail screen, including search, reading and composition. Under D014 it appears only in Mail, and permits day browsing with an explicit Today action. Calendar and Settings use their own available space without this strip. Retain the dedicated day/week/month/agenda calendar. See the [visual design brief](../ui-ux.md).

Calendar owns event content, occurrence identity, current/next computation, coverage and reminder/Join behavior. Mail UI owns the timeline's placement and resizing with the mail shell. Supply a bounded combined day view with explicit account/calendar identities and date/time-zone context. The timeline and full calendar use the same event state but independent viewed dates. Both use synthetic fixtures before a provider is available.

The Flight Progress v2 horizontal timeline makes event duration and overlap readable through lanes, date navigation and a current-time marker for today. It supersedes the earlier footer across all views. Full-calendar navigation preserves the mail timeline's selected day. Example times, mail statistics and static progress are not runtime facts; compute them from the relevant shared state or leave them out.

Preserve overlaps and all-day overflow, distinguish empty from stale/unloaded, and offer event details, Join where available and a route to the full calendar. Connected accounts contribute full permitted detail even when another mailbox is selected. Respect provider free/busy and private-field restrictions in the data supplied to the grid, timeline, tooltips, search and CLI. A gap in displayed events is not guaranteed free time.

## Calendar consistency

Use one local event store for day/week/month/agenda and reminders. Keep Google series identity, exceptions, attendee state and event versions together. Map a mail invitation to its calendar event and reconcile provider updates before sending any additional scheduling email. Prevent duplicate notifications when synchronization replays changes.

Calendar subscriptions are read-only snapshots; a failed or malformed refresh does not erase the last valid events. Resolve feed removals only from a valid complete refresh, and display stale status. Do not route feed edits or RSVP to Google or SMTP as if the feed granted those capabilities.

Schedule reminders from the locally reconciled event state and invalidate them when events move, cancel or disappear. Closing the GUI stops its synchronization/reminder session; explicit independent CLI calendar/sync commands may run without the GUI. On reopening, reconcile before scheduling upcoming reminders; do not replay a burst of obsolete alarms. Scheduling horizon and running-app notification preferences remain design choices. Test daylight-saving changes, all-day boundaries, organizer/attendee updates and edits from another device as distinct acceptance cases.

Serve agenda, current/next status and calendar search from the same bounded event/occurrence views. Keep all simultaneous or overlapping events and selected calendar identities visible. Compact all-day rows expose hidden counts. Join opens the conference URL and acknowledges only the local occurrence reminder; physical location, RSVP and attendance remain distinct. Honor acknowledgement through refresh and reconcile it against rescheduled occurrences.

## Acceptance gates

- Exercise Google calendar views/queries, editing/recurrence, RSVP and permissions through the headless CLI and GUI, preserving the same event/operation state. Add feed parity in the later protocol phase.
- Feed the mail timeline and full calendar from the same combined occurrence state. Check mailbox/view switches, compact windows, day rollover, no events, overlapping/all-day events and stale/offline coverage; the timeline remains on Mail screens, is absent from Calendar/Settings and cannot imply that unloaded calendars are empty.
- Day/week/month/agenda, cached search and current/next status agree on the same event/occurrence identity. Show true overlaps, all-day hidden counts, selected calendar identities and search coverage. The pictured Day/Week/Month controls do not remove agenda or event search.
- Use Flight Progress v2 for the Phase 1 visual slice. Browse timeline days independently of a full-calendar week that excludes today, then return with Today. Exercise full permitted cross-account details, restricted free/busy events, early/late events outside the example span, simultaneous/all-day overflow and permission changes. Restricted details must not leak through alternative views or CLI output.
- Test one-occurrence, series and following-occurrence edits, exceptions, DST/all-day boundaries, organizer changes, RSVP, cancellation and remote concurrent edits without silent rule simplification or duplicate invitations.
- Recover expired Google sync tokens and changed permissions. In the later subscription phase, keep a valid cached feed after a failed refresh and remove feed events only after a complete valid refresh. Feed URLs stay out of logs; feeds grant no edit/RSVP capability.
- Join opens the conference URL while preserving physical location and RSVP; acknowledgement survives refresh for that occurrence and reconciles on rescheduling. GUI close stops its reminders; reopen reconciles before scheduling upcoming alerts. CLI-only calendar operations share event state and do not create a hidden reminder service.

## Open before affected behavior ships

P1-00 resolves combined-query bounds, selected visibility and continuation behavior, timeline ranges/overflow, date/time-zone presentation, agenda/detail entry and the bounded create/edit/RSVP interaction. Dragging the timeline changes its displayed day; it does not imply event rescheduling. Recurrence edit controls, reminders, live invitation handling and remote conflict recovery remain their later affected gates. Combined full permitted details and the mail-only browsable timeline are settled by D014. No CalDAV editor or shared-calendar ACL administration is required for MVP.
