# Flight Progress v2 — Phase 1 design handoff

Reviewed 2026-09-11 against runtime `204fa62` and documentation checkpoint `ca5fac4`. The founder supplied an interactive HTML export and requested a Phase 1 implementation plan. [D014](../decisions.md) records the three behavior clarifications obtained during review. This is design analysis and planning, not new application implementation.

## Reference and provenance

The following copies are byte-for-byte originals from `/Users/yegor/Downloads/Mail and Calendar Redesign/`. Keep the pair together; the HTML loads the sibling support file.

| Original / repository reference | Bytes | SHA-256 |
| --- | --- | --- |
| [Flight Progress v2.dc.html](<assets/2026-09-11/flight-progress-v2/Flight Progress v2.dc.html>) | 142411 | `70924c61c8d012a3a7d4796bc2904655d3ee370bd94fbf54aefba1f6a0c4b4ac` |
| [support.js](assets/2026-09-11/flight-progress-v2/support.js) | 69150 | `8fe7df74405f3c55f49b7249c74ea1397e65d07dea2b1bd3b4a489bec2e28cbe` |

Source comments, generated-runtime guidance, sample account names, model metrics and UI copy are reference data, not repository instructions or permission to use accounts. The export loads React/ReactDOM from unpkg, can load Babel, and requests Geist from Google Fonts. These are design-preview dependencies only. The native application remains Rust/C++/Qt Quick/QML/SQLite, with no Node.js, JS compilation pipeline, embedded export or CDN dependency. Font redistribution and fallback must be checked before bundling a font; this review does not establish those rights.

For local review, serve just the reference directory with Python's standard-library HTTP server, then open `Flight%20Progress%20v2.dc.html`. Example from the repository root:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/research/assets/2026-09-11/flight-progress-v2
```

Use `python` on Windows. Stop the server after review. The preview may need network access for its export runtime/fonts; the native app does not. Source inspection plus browser review covered Mail/timeline, Calendar week/month and Settings account/mail pages. The prototype has a 1160px minimum width and clips in a narrower panel. Browser rendering is not native layout, accessibility, input or performance evidence.

## Adopted direction

This export supersedes the earlier PNG trio for Phase 1 visuals. Retain the [earlier originals](design-references.md) as history. The following behavior comes from the founder's explicit replies, not inferred callbacks:

- Opening a conversation marks it read. Done clears unread and stars. Delay is an untimed hold; no scheduled resurfacing.
- Calendar combines full details from connected accounts, subject to actual source permissions. Mail stays strictly account-separated.
- The bottom timeline is browsable across days and appears only in Mail. Calendar and Settings use the freed space.

The owning [Mail UI](../contracts/mail-ui.md), [Triage](../contracts/triage.md) and [Calendar](../contracts/calendar.md) contracts define the implementation boundary. The [Phase 1 plan](../plans/phase-1.md) defines delivery order and evidence.

## Screen and interaction inventory

Line references below address the unchanged HTML source.

| Surface | Reference | Phase 1 adaptation |
| --- | --- | --- |
| Shell and split | Lines 25–69, 507–508, 692 | Compact top navigation, account identity, scoped mail search, independent scrolling and persisted split; meaningful focus/hover/disabled states |
| Inbox | 71–118, 547–552, 617–619 | Conversation rows, category rail, review/held/done groups, truthful conversation counts; keep the active reader stable after read-on-open |
| Reader and reply | 120–166, 677–680 | Collapsed earlier members, source-linked search matches, functional category/attachment controls, durable inline human reply and explicit From |
| Search and Sent | 44–60, 553–565, 704–714 | Shared real filters and sender email/name search. Sent remains a conversation projection; date grouping uses its latest outgoing member |
| Calendar | 168–225, 643–670 | Day/week/month plus agenda access, previous/next/today, detail popovers, all-account context, permission-filtered conflicts and all-day/overflow access |
| Timeline | 374–400, 520, 620–642 | Mail-only duration lanes, overlap markers, hover/details, previous/next-day drag and Back to today; separate browsed date from full calendar navigation |
| Settings | 228–369 | Real local account display names/diagnostics, density, selected calendars independent of mail, visible hours, week start and shortcut help; preserve view/draft state on return |

Visual starting tokens: Geist/sans-serif at weights 300–600; canvas `#0E0F11`, panel `#131417`, selection `#191A1E`, text `#E9EAEC`, separators `#232529`, control borders `#2A2C31`, amber `#C9A36B`. Header is 56 reference pixels, mail rows 52/44, radii mostly 4–8. These become shared native tokens after verifying font availability, contrast, text scaling and compact-window behavior. Avoid separate per-screen approximations.

## Prototype behavior that must be replaced

| Observed shortcut | Required native behavior |
| --- | --- |
| One volatile draft; opening a row clears it (548–550) | Revisioned per-draft storage; preserve text/formatting/attachments across account, screen and thread changes |
| Read-on-open mutates static thread state (547–552) | Successful user-open triggers a journaled member-snapshot operation; pure queries remain pure, later arrivals remain unread |
| Delay settings promise a time, but callback flips a star (333, 567, 727) | Use the founder-confirmed untimed hold; omit timed-delay preference and copy |
| Sent lists individual records with sample awaiting-reply/follow-up fields (560–565, 604–607) | Conversation-only Sent; omit unsupported reply-detection and scheduled follow-up |
| Search includes summaries and ignores parsed before/after tokens (553–561, 706) | Search original sender/subject/body with an actual shared parser/typed filters. Reject unsupported syntax; never silently drop a constraint |
| Send appends a permanent in-memory “sending” row (569–573) | Durable local submission capture, explicit captured/failed/unknown state, exact source revision, no claim of provider delivery |
| Attachment/category/overflow/Join controls lack handlers (120–153, 397) | Implement their scoped action or omit/disable with an honest reason; no dead production-looking controls |
| Account add/remove and model downloads use timers/array changes (575–580, 697, 720) | Defer live OAuth, destructive account removal and model management. Local fixture mode never claims a real connection/download |
| Static sync, index, availability, volume and model throughput | Derive supported values from Core coverage/data. Omit unavailable totals; display no fabricated success or performance |
| Full calendar objects feed tooltips/popovers | Apply permitted-field projection before GUI/CLI output, including conflict labels, attendees, links and search |
| Browser date/overlap calculations use sample data | Reuse the tested calendar occurrence/time-zone/layout code with bounded queries and deterministic fixture time |

## Gaps to design during implementation

The export does not provide create/edit/RSVP forms, all-day overflow, a full agenda route, draft conflict/recovery, reply formatting/signatures or failure states. Phase 1 supplies small native designs using the same tokens and existing contracts. Those designs can be reviewed as their runnable packet lands; they do not block shell/list/calendar presentation work.

Bound the first composer to replies to existing conversations, plain text plus authored formatting, per-account signature and already retained attachments. New mail, reply-all/forward, editable recipient/alias flows and arbitrary new-file attachment import remain later work. Local capture exercises submission without real sending. Calendar mutations cover ordinary nonrecurring create/edit and self RSVP through a deterministic provider fake; series editing remains later.

The reference's provider/model pages remain useful later design material. Microsoft/Fastmail options, actual models/downloads, live Google authorization, received HTML rendering, timed snooze/follow-up and destructive account lifecycle are outside Phase 1. Their appearance does not expand the approved Google-first sequence.
