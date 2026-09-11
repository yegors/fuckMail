# Mockup references and technical implications

Received 2026-09-11 from the founder. The latest pair is closer to the intended product, but still not final; final mockups will arrive closer to UI implementation. Local copies preserve every supplied PNG unchanged. Text, example accounts, dates and status inside the images are mock content, not behavior specifications or implementation evidence.

## Closer mail and calendar direction

Use these references before the earlier mood images. Their structure informs the working [UI/UX brief](../ui-ux.md); the founder has not frozen fonts, dimensions, interaction details or the stack.

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

The two closer originals were copied from the supplied clipboard attachments on 2026-09-11. SHA-256 was compared before/after copying; both copies match exactly. They have not been resized or edited.

| Repo copy | Supplied clipboard file | Bytes |
| --- | --- | --- |
| [closer-mail.png](assets/2026-09-11/closer-mail.png) | codex-clipboard-750d1dcd-38b6-435a-b285-aeed07e66a91.png | 271275 |
| [closer-calendar.png](assets/2026-09-11/closer-calendar.png) | codex-clipboard-49aab6e5-6e58-45c3-9046-a981e898a89b.png | 190427 |

Mail SHA-256: `DD47B852C7187CE287B217ECF854487D0BBB028AF5E532ED18439140071C44D0`.

Calendar SHA-256: `82AF922AC5A1308F1EAC4EEE3266919505E0746E4C4FABC710138F5668AD6CE7`.

See the [UI/UX brief](../ui-ux.md) for the focused final-design handoff gaps. Required behavior is owned by the contracts; the reference images remain evidence of direction.
