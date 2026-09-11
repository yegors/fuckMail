# Mockup references and technical implications

**D011 handoff update (2026-09-11):** the founder supplied updated mail, calendar and settings mockups for the initial phase. These three images are now the working visual reference. D009's text-first, user-owned action and Google-first boundaries and D010's bounded paired-client/development package remain in force.

Received 2026-09-11 from the founder. Local copies preserve every supplied PNG unchanged. Text, example accounts, dates, version numbers and status inside the images are mock content, not instructions, new provider requirements or implementation evidence. The initial-phase references supersede the earlier pair visually; they do not freeze all future screens or interactions.

## Current initial-phase mail, calendar and settings

| Reference | Observed treatment | Initial-phase use |
| --- | --- | --- |
| [Initial mail](assets/2026-09-11/initial-mail.png) | Compact top Mail/Calendar/account bar and settings gear; Inbox/Sent with category chips; review/delayed/done groups; right reader, earlier-message expander and offline attachment; an inline reply composer with recipient, explicit From, subject, Send/Discard and draft copy | Apply the split layout and inline durable plain-text reply. Keep account identity visible and draft state truthful. Sender/subject/body search, retained attachments and explicit user flags use the shared engine/CLI |
| [Initial calendar](assets/2026-09-11/initial-calendar.png) | Full-width week, active-day/time accent, understated event blocks, Busy entries from other accounts, Day/Week/Month/Today controls and the shared footer | Propose a read-only fixture week for the first desktop slice, using the same range data as the CLI and today's footer. Editing, recurrence expansion and the other finished views remain later slices |
| [Initial settings](assets/2026-09-11/initial-settings.png) | Left navigation for Mailboxes & calendars, Local model, Mail, Calendar and Shortcuts; account cards with display name/address/provider, Mail/Calendar switches, freshness/indexing status and Add mailbox; Done/Esc returns to the app; top account bar and today footer remain | The founder clarified that settings is incomplete: ignore the switches and lay only general plumbing. Establish the page shell, routing/return navigation and shared account context. Detailed controls, cards and subpages await the next design |
| All three | Near-black surfaces, compact sans-serif headings/body text, muted borders, restrained amber time accents, generous empty space and a persistent bottom timeline | Prefer this typography direction over the earlier serif treatment. Exact fonts, dimensions and colors remain unselected. Check secondary-text legibility, focus, compact windows and Mac/Windows scaling |

### Scope and interaction notes

- **Provider/model samples:** the settings card naming Fastmail does not supersede the founder's all-Google requirement. Use Google-shaped synthetic accounts initially. The Qwen summary, Local model entry, activity sparkline, arrivals/time-left statistics and version 0.9.2 are examples, not first-slice dependencies or measured values. Hide unavailable assistance and omit unimplemented analytics; no fabricated live status.
- **Composer:** visible To/From/subject and inline editing clarify placement. The first slice saves drafts; the pictured Send button does not authorize provider sending. Do not show a success state for a local save. Settings' Done is a navigation action, distinct from mail's unresolved Done mapping.
- **Settings clarification:** the founder explicitly said to ignore the Mail/Calendar switches and build only the general plumbing; the settings mockup is incomplete. No toggle semantics need choosing now. Do not invent synchronization, visibility, account removal or data deletion behavior from those controls. Other sidebar labels and account cards remain illustrative, not a settings-feature checklist.
- **Coverage:** freshness, fetched history, text indexing and attachment availability are different states. A single pictured progress bar does not settle their aggregation; recent sync or completed text indexing cannot claim all attachments are offline. Use explicit fixture state and label it synthetic in the prototype.
- **Busy and footer:** carry the pictured footer across mail, calendar and settings as the initial-phase composition. Keep it on today when the week grid browses another date. Cross-account Busy entries still need explicit visibility and permitted fields; a gray title is not an access rule.
- **Remaining actions:** Done/Delayed targets, automatic read behavior and mixed read/star group precedence remain open. The newer account tabs show a single count, but units/scope still need labels. Category names are user-defined examples. Missing new-message, reply-all, forward, draft navigation and event-edit states remain future design work.

This is enough visual direction for the proposed synthetic slice, with settings limited to plumbing. Receiving these references does not add real-account access, model installation or live sending to that scope.

## Previous closer mail and calendar direction

Retain this pair and its observations as history. The initial-phase trio above takes precedence for new screen work. Shared behavior still comes from the [UI/UX brief](../ui-ux.md) and contracts.

| Reference | Observed treatment | Product and implementation implication |
| --- | --- | --- |
| [Closer mail](assets/2026-09-11/closer-mail.png) | Shared compact top bar; Mail/Calendar toggle; separate account tabs and number pairs; search/date/sync context; two-pane mail with a subject-first list, review/delayed/done grouping, reader category chips, Reply/Delay/Done controls, a local summary, an earlier-messages expander and an attachment offline badge | Test bounded group/list updates without selection jumps, independent pane scrolling, resizable split, member-level search navigation, optional summary state and accurate attachment status |
| [Closer calendar](assets/2026-09-11/closer-calendar.png) | The same top bar; full week grid, range/account heading, Day/Week/Month/Today navigation, understated event blocks, foreign-account Busy placeholders and a current-time indicator | Use one permission-aware event/occurrence model for grid, search and footer; test all-day/overlap handling, long titles, DST and transitions between date ranges |
| Both | Restrained dark canvas, serif display headings, simpler small body/metadata text and a slim bottom timeline with next event/countdown, current-time marker and day-context numbers | Shared typography/layout can be custom without OS-native styling; test legibility, focus/selection, scaling, footer overflow and compact windows |

### Interpretation boundaries

- The accepted persistent bottom calendar strip is now pictured in both screens. Its presence on mail remains required; retaining it on the full calendar is the current design proposal. When the grid shows another week, a today footer must stay explicitly today.
- Delay expresses the star/deferred-action idea. The image does not add a timed snooze. Done is not yet a defined composite command, proof of completed external work, or a new provider state. Resolve its targets and wording before implementing it.
- Unread and starred are independent. Grouping needs a rule for conversations containing both; number pairs and “to review”/“delayed” counts need labeled units and coverage. Never infer count semantics from the sample numbers.
- The earlier-messages expander suggests a compact default thread. It must still reveal every member and jump directly to a search match. Hidden messages and generated summaries do not automatically count as reviewed.
- Busy · other-account overlays suggest useful calendar context. They do not settle visibility defaults or grant access to private event fields. Distinguish provider-limited free/busy from a voluntary busy-only display of data already accessible to the account. Do not derive or leak hidden titles into hover details, search or CLI results.
- A narrow depicted day range is not permission to hide early/late/all-day events. “Time left,” “meetings ahead,” arrival ticks and countdowns are proposed annotations whose meaning must be defined against selected calendars and coverage.
- An attachment “available offline” badge and “Synced” caption must reflect measured local/provider state. Recent sync does not prove history and attachments are complete.
- The absence of a sidebar, agenda tab, composer, setup or failure states removes no required workflows. The calendar's pictured mail-search placeholder does not decide calendar search behavior.

### Useful evidence for the first UI slices

Use synthetic fixtures for mixed read/star states, incomplete synchronization, delayed provider writes, earlier unread members, long HTML mail, draft recovery and summary-off mode. Calendar fixtures should include all-day/overlapping events, another account with restricted visibility, a full week that is not today, stale/offline calendars and a time-zone/day rollover. Capture outcomes against the appropriate [Mail UI](../contracts/mail-ui.md) and [Calendar](../contracts/calendar.md) contract revision when implementation is requested.

No static mockup measures HTML fidelity, large-list paging, accessibility, text editing, recurrence correctness, rendering cost or installation. The current [Rust engine/CLI and Qt Quick UI recommendation](desktop-stack.md) still needs its feasibility gates; these images do not introduce a need for a web-app shell.

## Earlier mood references

Retain these for visual history, not as competing final designs.

| Reference | Observed treatment | Technical implication to test |
| --- | --- | --- |
| [Mockup 1](assets/2026-09-11/mockup-01.png) | Near-black canvas, mono metadata, large sans heading, account accents, two main panes and a bottom timeline with calendar lanes | Custom typography, bounded list/reader, flexible split layout and event geometry |
| [Mockup 2](assets/2026-09-11/mockup-02.png) | Serif-led layout, small caps, red wavy emphasis on delayed mail, review/delayed groups and a thin bottom time strip | Font shaping, text selection, custom state marks and compact event/now indicators |
| [Mockup 3](assets/2026-09-11/mockup-03.png) | Serif content with simpler navigation, visible unread/star counts, label chips, sidebar/list/reader and a sidebar agenda | Resizable regions, large-list virtualization, optional summary/source and attachment states |

The old third image's sidebar agenda does not supersede the confirmed bottom strip. Specific fonts/colors, keyboard hints, “Done/Finished” semantics and example AI results were not accepted by inclusion in an image. Bundled fonts, if chosen, need redistribution rights and fallback/Unicode testing; font names cannot be established from these images alone.

## Asset provenance

The three initial-phase originals were copied from the supplied clipboard files without resizing or editing. Source and destination bytes and SHA-256 match.

| Repo copy | Supplied clipboard file | Dimensions | Bytes | SHA-256 |
| --- | --- | --- | --- | --- |
| [initial-mail.png](assets/2026-09-11/initial-mail.png) | codex-clipboard-90fdf7f0-6f12-4912-94fd-c5ec02cdca7c.png | 4518 × 2868 | 770157 | `5c5611746939a1578377c488da0052104ef2451fcf4d3d15e612eebe2aed3d6d` |
| [initial-calendar.png](assets/2026-09-11/initial-calendar.png) | codex-clipboard-ffd9d0d3-66b7-4b71-bc99-f5c3163b885a.png | 4524 × 2866 | 509802 | `fb7825bb6cfae83a5a1c923c44724a8a3970715964fa31ee547a6440771ab907` |
| [initial-settings.png](assets/2026-09-11/initial-settings.png) | codex-clipboard-37e2282d-1e0f-4882-beaa-a8af99792fa0.png | 4520 × 2870 | 490390 | `0a1a1efbf01a29c725bc142c8d363c79984677ed50d3ca8f6f14f71cc41a6787` |

The two closer originals were copied from the supplied clipboard attachments on 2026-09-11. SHA-256 was compared before/after copying; both copies match exactly. They have not been resized or edited.

| Repo copy | Supplied clipboard file | Bytes |
| --- | --- | --- |
| [closer-mail.png](assets/2026-09-11/closer-mail.png) | codex-clipboard-750d1dcd-38b6-435a-b285-aeed07e66a91.png | 271275 |
| [closer-calendar.png](assets/2026-09-11/closer-calendar.png) | codex-clipboard-49aab6e5-6e58-45c3-9046-a981e898a89b.png | 190427 |

Mail SHA-256: `DD47B852C7187CE287B217ECF854487D0BBB028AF5E532ED18439140071C44D0`.

Calendar SHA-256: `82AF922AC5A1308F1EAC4EEE3266919505E0746E4C4FABC710138F5668AD6CE7`.

See the [UI/UX brief](../ui-ux.md) for the focused final-design handoff gaps. Required behavior is owned by the contracts; the reference images remain evidence of direction.
