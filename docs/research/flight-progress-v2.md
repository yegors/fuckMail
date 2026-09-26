# Flight Progress v2 — Phase 1 design handoff

Current native target: [September 14 second export](#september-14-second-export--d049--p2-ui), under D049/D050. Earlier inventories below retain their historical scope.

Updated 2026-09-12. The founder supplied a second same-day update and requested close native implementation, replacing invented elements. [D018](../decisions.md) records Update 2 and P1-09; [D016/D017](../decisions.md) retain the earlier same-day design/composition authorization. [D014](../decisions.md) preserves confirmed read/Done/untimed Delay and combined calendar behavior. Earlier inventories below remain historical analysis; execution evidence belongs in [Phase 1 execution](phase-1-execution.md).

## Active September 12 Update 2 reference

The new export is preserved unchanged beside its unchanged support script. Source inspection and SHA-256 verification establish this inventory; no browser/native result is implied.

| Repository reference | Bytes | SHA-256 |
| --- | ---: | --- |
| [Flight Progress v2.dc.html](<assets/2026-09-12/flight-progress-v2-update-2/Flight Progress v2.dc.html>) | 261329 | `9fd89ab881f5ef8384e21cabdf645d7b41895089afcb86e4a6de8b641fad4c71` |
| [support.js](assets/2026-09-12/flight-progress-v2-update-2/support.js) | 69150 | `8fe7df74405f3c55f49b7249c74ea1397e65d07dea2b1bd3b4a489bec2e28cbe` |

The prior same-day export below is 222401 bytes; its support script is byte-identical. Embedded generated instructions, sample providers, identities, actions and metrics are reference data. They do not authorize network/account activity or replace the shared Core boundary. Keep these assets out of the shipped application runtime.

### Update 2 surface inventory

Line references in this section address the 261329-byte Update 2 HTML.

| Surface | Exact change or retained behavior | Native adaptation |
| --- | --- | --- |
| Scope tabs, 77–79/1128 | Active 22px and inactive 15px Inbox/Sent/Drafts | Actual account conversation/draft projections; no source filters for Drafts |
| Mail rail, 86–89 | 8px bottom spacing, no divider, no separate filter-summary row, fixed People/Automated segment | Shared kind query; missing/stale annotations stay unknown, with removable active constraints in search |
| Groups, 92–107/1131 | 34px counted collapsible headers; Done keeps two-line rows | Complete filtered group counts before pagination; collapse retains reader/draft and unloaded totals |
| Message metadata, 101/154/164/954–958/1041–1046 | Relative-age Delayed/Done and older dates, sender-kind icon, full-date hover | Actual source timestamps and authoritative annotation; do not use prototype name heuristics |
| Sender card, 549–556/1084–1090 | Three recent conversations, total count, filter and Write actions | Exact-address current-account Core summary; deliberate recent open and normal durable compose |
| Category suggestions, 965 | Up to seven suggestions | Existing account categories, retaining selected labels and access to the remaining set |
| Mailbox keyboard route, 868 | Tab/ShiftTab cycle mailboxes | Outside editable/modal focus; preserve normal field traversal, save/selection and account isolation |
| Timeline placement, 71/477/1027/1164 | Footer or directly beneath 56px header | Durable Mail-only placement; no empty footer reservation in header mode or other workspaces |
| Minimized draft, 543–545 | Existing floating 40px bar, max 420px, bottom right | Resume retained draft; discard reuses explicit confirmation |
| Calendar, composition, event editor/details | After resolving theme variables, markup is unchanged from the earlier same-day export | Preserve P1-08 functions, fixed 24-hour Calendar grid, source permissions and recovery |
| Settings | General added first; theme/text/startup controls; event duration and timeline placement | Typed shared preferences and visible effect; unavailable provider/OS/model controls stay truthful |

The reference still has a placement inconsistency: its event sheet/minimized-bar bottom calculation reserves timeline height even in header mode. Native header mode must reserve the strip at the top only and position details/editor/minimized work within the actual available workspace.

### Settings inventory and scope

Settings now orders **General, Accounts, Local model, Mail, Calendar, Shortcuts**, initially General; section selection remains in memory when reopened. Sidebar width remains 220px, content padding 24px top/40px sides/40px bottom and maximum inner width 640px. Sidebar buttons remain 32px high with 14px icons and 10px icon gap. Page headings use 22px/500 weight, −.02em tracking and 1.1 line height, reserving 120px for Done. Introductions use 13.5px, 55% ink and 1.5 line height, maximum width 560px. Done remains top 22px/right 24px, 32px high; no sample version string becomes the real version.

General rows begin after 16px and use 14px vertical padding, 24px gap and the shared separator. Labels inherit 14px; descriptions are 12.5px/50% ink with 3px gap and 1.45 line height. Segments use bg2, 2px padding/gap, 6px outer radius; each segment has 5px/12px padding, 4px radius and 12.5px type, with bg6/500 weight for selection. Native pixel sizes are rounded after scaling.

| Section/control | Prototype values, initial state and presentation | P1-09 local scope |
| --- | --- | --- |
| General: Theme | Graphite/Paper/Harbour, Graphite default; 68×46px preview, 6px radius, 2px selected ink border, 10px gap; labels 11.5px, 6px below | Actual durable `theme`, applied to every surface and control |
| Theme description | Graphite: “Near-black with warm amber. The default.” Paper: “Warm off-white, ink type, oxblood accent.” Harbour: “Deep blue-black with a cool sea-glass accent.” | Exact theme descriptions; no inferred OS theme |
| General: Text size | Small/Default/Large; Default; root base sizes 13/14/15.5px | `text_size`; scale native explicit labels as well as inherited text, retaining drafts and selection |
| General: Open on | Mail/Calendar/Where I left off; Mail | `startup_view`; `last_workspace` contains Mail/Calendar only, Settings never becomes startup |
| General: Sync every | 1 min/5 min/15 min; prototype 5 min | Disabled/unavailable; prototype state has no real sync implementation |
| General: Notifications | All/Events only/Off; prototype All | Disabled/unavailable; no false active default or background service |
| Accounts | Provider/email/name, index bar, add wizard and removal unchanged | Actual local metadata/coverage, retained save failures; OAuth/add/remove simulations deferred |
| Local model | Detection/connection/model/throughput/requeue examples unchanged | Explicit unavailable model state; no fake ready status, token rate or download |
| Mail | Density Comfortable/Compact 52/44px, labels and timed Delay examples unchanged | Real density, account-scoped categories and retained signatures; untimed Delay overrides sample timing options |
| Calendar: Other accounts' events | Show/Hide, default Show | Independent permitted calendar selection, rather than implicitly filtering by the mailbox |
| Calendar: New events last | 30/45/60 min, default 60 | `default_event_duration_minutes` applied to newly created events; editing preserves existing duration |
| Calendar: Timeline section | Caption after 28px, description after 6px, then 8px space | Mail-only strip |
| Calendar: Placement | Footer/Header, default Footer | `timeline_position`, actual top/bottom workspace geometry |
| Calendar: Timeline hours | Prototype 07:00–21:00; offered starts 00/05–10 and ends 17–24 | Preserve Core's existing 07:00–22:00 defaults and valid retained 0–24 range; start must precede end |
| Calendar: Week starts on | Monday/Sunday, Monday | Existing durable week-start behavior; full Calendar remains 00:00–24:00 |
| Shortcuts | Adds Tab/ShiftTab next/previous mailbox; other rows unchanged | Only implemented actions are advertised; local capture remains distinct from live Send |

After replacing `var(--fp-*, fallback)` with the fallback, the Accounts, Local model and Mail Settings markup is unchanged from the earlier same-day export. Their provider/model/timed-Delay simulations are not new requirements. Signatures remain an unobtrusive native retained-data control even though absent from the prototype. The global destructive label editor is not a second taxonomy store: native labels stay account scoped under the current shared boundary.

### Exact shared palette and typography tokens

| Token | Graphite | Paper | Harbour |
| --- | --- | --- | --- |
| bg0 | `#0B0C0E` | `#EDE9E1` | `#070B12` |
| bg | `#0E0F11` | `#F4F1EA` | `#0A0F18` |
| bg1 | `#121316` | `#EFEBE3` | `#0D131D` |
| bg2 | `#131417` | `#EAE5DC` | `#101722` |
| bg3 | `#17181C` | `#E5E0D6` | `#131B27` |
| bg4 | `#191A1E` | `#E1DBD0` | `#161F2C` |
| bg5 | `#1B1C20` | `#DDD6CA` | `#182130` |
| bg6 | `#1F2126` | `#D6CEC1` | `#1C2637` |
| line | `#232529` | `#DCD5C8` | `#1E2939` |
| line2 | `#2A2C31` | `#CFC6B7` | `#273345` |
| line3 | `#3A3D44` | `#B9AE9C` | `#364559` |
| ink | `#E9EAEC` | `#1C1B18` | `#E6ECF2` |
| accent/amber | `#C9A36B` | `#8A2E2A` | `#7FC4B6` |
| warn | `#D9A441` | `#B9791A` | `#E0B04A` |
| red | `#E0645A` | `#B3382F` | `#E27070` |
| shadow RGB | `0,0,0` | `60,50,30` | `0,0,0` |
| color scheme | dark | light | dark |

RGBA variants derive from each palette's ink/accent/warn/red/background/shadow channels, rather than retaining Graphite RGB under Paper. The reader and timeline use bg0; main canvas bg; panels/popovers bg2; selected surface bg4; hover/selected segments bg6; borders line/line2/line3. Reader inset shadow uses 18% shadow fading over 28px. Floating surfaces use 45–60% shadow according to the reference. Placeholder is 35% ink and the 6px scrollbar uses 14% ink. Geist 300/400/500/600, tabular numbers and 1.5px rounded SVG strokes are unchanged. Existing licensed native fonts and geometry icons remain; no runtime CDN is added.

The prototype persists theme/split in localStorage, while launch, sync and notification choices are largely volatile or unused state. Native preferences follow [Platform 10](../contracts/platform.md#local-appearance-and-startup), Core 13 and CLI 7 with schema 7/fresh fixture 3/additive protocol 1; migrations preserve old fixture revisions and authored data. Source implementation and static checks are not evidence of native layout, input, restart, accessibility or performance acceptance.

## Earlier September 12 reference — historical P1-08 inventory

Copied unchanged from the supplied design export:

| Repository reference | Bytes | SHA-256 |
| --- | ---: | --- |
| [Flight Progress v2.dc.html](<assets/2026-09-12/flight-progress-v2/Flight Progress v2.dc.html>) | 222401 | `ca75501e76cd137908fd5800ba6d11555241c63885ae68fea42ca0ab12a5fab2` |
| [support.js](assets/2026-09-12/flight-progress-v2/support.js) | 69150 | `8fe7df74405f3c55f49b7249c74ea1397e65d07dea2b1bd3b4a489bec2e28cbe` |

The HTML changed materially; the support script is identical. Both are reference assets only. Local browser inspection uses the same bounded directory server described below, with the date changed to September 12. It is not application runtime or native acceptance evidence.

| Surface | Current design to implement | Prior native elements to replace |
| --- | --- | --- |
| Header | 56px bar, account tabs, status before 320px search, New message/event, gear | Clock/date, fixture action, altered order and smaller search |
| Mail | 50/50 initial split; Inbox/Sent/Drafts and inline icon counts; category badges, 68px times; compact group rules | All mail/filter buttons in heading, second statistics row, amber row stars and persistent coverage footer |
| Reader/composer | Reply/Reply all/Forward; inline divider-based From/To/Cc/Bcc/Subject, authored editor and compact actions | Standalone category launcher and boxed reply card; no fabricated summary card |
| Calendar | Date/range picker, full 24h scrollable grid, 320px day list, month with 4–6 weeks | Fixed Today-only navigation, squeezed visible-hours grid and prominent activity/agenda controls |
| Event details/editor | Anchored 300px popup; 440px right sheet with compact divider rows | Centered 470px details and 550px editor modals |
| Mail timeline | 28px lanes, current/next/past accents, readable overflow labels and day drag | Extra toolbar, clipped 22px lanes and uniform blocks |
| Settings | Accounts/Local model/Mail/Calendar/Shortcuts, segmented choices, reference spacing | Diagnostics navigation, signature stack and generic form/table layout |

The native implementation retains accurate counts, independent unread/held state, source permissions, coverage access and draft/capture/calendar recovery. These belong in compact details, search or overflow surfaces where absent from the design. It must not claim real sync, model availability or delivered mail. P1-08 adds durable local new/reply-all/forward drafts and Drafts browsing through the same Core/CLI boundary; arbitrary file import, account/model setup, timed Delay and live sending remain outside scope. Earlier line references and reply-only limits below describe the superseded September 11 export, not the active packet.

## Reference and provenance

The following copies are byte-for-byte originals from the supplied `Mail and Calendar Redesign` export. Keep the pair together; the HTML loads the sibling support file.

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


## September 14 second export — D049 / P2-UI

The founder supplied a newer 310,337-byte Flight Progress v2 export and support.js. Exact bytes and hashes are preserved in [reference provenance](assets/2026-09-14/flight-progress-v2-update-2/manifest.json). The changes include fixed-size dotted scope tabs, stable sender-kind placement, right-click menus, modifier/range selection, bulk flag/label/trash actions, collapsible message rows with avatars, Sent/Drafts date groups with a grouped/plain setting, refined account cards and AI styling. Prototype remote effects and state shortcuts do not replace the existing contracts. Native implementation and evidence are recorded in the Phase 2 execution report.
