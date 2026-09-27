# Quality, performance and release acceptance

## D161 — subject-only summaries (Quality88)

Local assistance28. Required checks:
- a subject-only email prepares with its subject and empty text;
- "(no subject)" or a blank subject with no text fails with `assistance_limit`;
- a subject-only email is pending in the queue while a blank one is not, and `eligible` drops only by the blank one.

## D160 — reading text and forward layout (Quality87)

Mail UI111, Core82 and CLI59. Required checks:

- **Conversion:** div boundaries, inline spacing, entities and byte-order marks; per-level quote marks with blank lines at the outer level; lists, tables and `pre`; nesting 5,000 deep without stack exhaustion.
- **Source reply:** `message_source` returns `body_text` and truncates all three parts under pressure.
- **Previews** drop quote marks.
- **Text-mode rows:**
  - an Apple-style whole-quoted forward yields label, headers, text, attribution, quote 1 and quote 2;
  - a Gmail-style forward keeps the note apart and joins a wrapped attribution;
  - prose mentioning a forward stays text;
  - a nested forward gets its own label.
- **Native reader:** rows read top to bottom, quotes indent 16 px per level, second-level quotes draw two bars, and the caption is capitalised.
- **Composer:** the forward quote of the HTML-only fixture keeps the heading and each paragraph on their own lines.

## D159 — prototype, full-length list and large-message checks (Quality86)

Mail UI110, Core81 and Gmail41; schema 40, live0/demo3 and protocol1. Required checks:

- **`gui-mail-gpu`** (Windows and macOS, real scene graph):
  - a sender photo fills its circle at 18–36 px;
  - after the first switch, Inbox → Sent → Drafts draws its first frame within 300 ms.
- **`gui-mail-selection`:**
  - The paged list keeps its length within two rows as a page arrives, and within four after a jump to the end, with no "Loading…".
  - Placeholder rows cover unloaded space.
  - The Spam/Trash menu closes on outside presses and folder changes.
  - "Inbox zero" draws while the last To review row leaves.
  - A reply's footer is 24 px inside the view.
  - Selection status is empty after success.
  - Tab progress reports its share, and the hover card uses whole numbers.
  - Settled undownloadable mail reads "Complete", with the count's tooltip naming it.
- **Rust:**
  - A 41 MiB source and a padding-escaped source both download whole, with the raw cap and 10-minute deadline set.
  - In-place decoding matches ordinary base64url across chunk boundaries and rejects bad input and over-bound length.
  - A request deadline outlasts the transport default and is bounded.
  - A 41 MiB message and a 60-attachment message publish with their attachments.
  - Migration 040 requeues only undeleted too-large and attachment-limit rows.
- Engine session tests compare the store epoch, not moving refresh generations.

Not measured: peak memory for a real large download. The estimate and its arithmetic are in [Gmail research](../research/gmail.md#large-messages-2026-09-27).

## D158 — selective background refresh (Quality85)

Core80/Mail UI109/CLI58, unchanged schema and live0/demo3 fixtures. Verify real store writes advance the affected generations, repeated status reads remain stable, and unknown writes, missed row hooks, rollback and reopen invalidate conservatively. Native IPC checks must verify independent requests, reader publication, draft delivery refresh, unsaved text, deferred list invalidation, epoch changes and legacy-engine fallback, with no background read-on-open or send mutation.

Record request-count evidence and native/engine checks in [storage research](../research/search-storage.md). Fewer requests demonstrate less background work; they do not establish a percentage improvement in click latency or eliminate reader refreshes from unrelated mail within the same area.

Verified: 839 Rust tests passed (9 ignored), Clippy, targeted native refresh/selection/undo/remote/review checks, and the real-engine draft/attachment walkthrough. Account-only polling requested one projection. Full GUI-suite/layout attempts hit concurrent paging-length and reply-bottom assertions; no full-suite pass is claimed for this slice.

## D157 — GUI source sharing and image memory (Quality84)

Mail UI108; schema, live0/demo3 and protocol1 unchanged. Verify repeated cached preparations share the exact source allocation and make no extra source request, identical reader replies stay cached, same-revision metadata refreshes and older revisions cannot replace it. Preview tests must cover physical-pixel sizing, stable card decode size during resizing, aspect/orientation, video poster handoff and playback/download controls.

Windows Release evidence: native mail-selection suite and real-engine attachment walkthrough passed with software and D3D11 rendering; targeted reader/attachment controls and the walkthrough passed at 200% scaling. The initial full-suite 200% attempt failed an avatar pixel check before reaching attachment controls; it is not reported as a full high-DPI pass. The existing native folder-switch timing gate passed, with warm first-frame observations of 38–88 ms (four observations, not a percentile benchmark).

Three paired synthetic thumbnail runs at 125% scaling averaged 12.32 MiB less resident memory for sixteen images, with readiness 71–73 ms versus 84–140 ms. A generated viewer resize completed two loads instead of 21 and retained about 5 MiB less, with a lower sampled peak. Cache checks share a 396,000-byte source string across repeated preparations. See [storage research](../research/search-storage.md) for fixtures, Qt behavior and limits. These establish scoped savings, not a reduction of the full live-session excess, an all-workload latency guarantee or the combined idle-memory budget.

## D156 — progress reuse, verified reads and GUI audit (Quality83)

Core79/Gmail40, live0/demo3 and protocol1. Required checks: unchanged progress avoids mailbox scans; calendar/global revisions do not invalidate it; message/provider/model/account/policy/bound changes refresh it; transactions and rollback cannot retain transient counts. Blob reads retain exact size/hash, missing/corrupt-file and regular-file protections; staged replay and native attachment flows preserve their results.

Windows evidence: full Rust workspace 836 passed, 9 ignored; final progress slice four passed. Initial/cached/unrelated-write progress calls executed 27,090/68/68 SQLite steps on demo3. Strict Clippy, formatting, hygiene and whitespace checks passed; release engine/CLI and native D3D11 empty-profile/mail/attachment checks passed. Details, the intermediate failed test attempts and GUI memory measurements are recorded in [storage research](../research/search-storage.md). Active-sync recount cost and long-session GUI allocation ownership remain open. These results do not establish the combined idle-memory budget, other platforms or whole-feature readiness.

## D154 — HTML senders (Quality82)

- `fm-core` checks `html_senders` validation (lowercase, distinct per account, at most 2,000, required) and that a stored `message_format` is dropped on read and refused on save.
- `gui-mail-selection` opens a message with HTML as plain text, checks the badge names the sender, switches it to HTML (saving that account and address) and back to plain text through the menu (forgetting it).

## D152 — offline extraction and bounded copies (Quality81)

Required evidence: a synced attachment adds only its original durable MIME; exact bytes survive reopen and cache removal; staged imports resume without decoded blobs; malformed source/part references fail; schema37 upgrades to38 without losing authored state; conversion retains standalone copies when the original is unavailable; shared authored hashes survive cleanup; received attachments in drafts survive message purge and account transfer; export eviction respects recent paths and user downloads. Exercise existing send, draft, invitation, retention, archive and native attachment flows. Measure first and repeated exports plus durable bytes saved using generated content in a release build. Record results and their platform/fixture limits in [storage research](../research/search-storage.md).

Core77/Gmail39, fixtures0/3 and protocol1. Windows verification: Rust workspace 831 passed, 9 ignored; the final attachment regression slice passed after strengthening cache-loss coverage. Strict all-target Clippy, formatting, repository hygiene and diff whitespace checks passed. The native synthetic attachment walkthrough and independent CLI inspection passed. Release measurements for generated 1/5/20 MiB attachments: first export 13.536/40.724/146.507 ms; repeated mean 0.926/0.941/1.114 ms, saving the full decoded size in permanent storage. See [storage research](../research/search-storage.md) for fixture limits and the earlier startup-status race. No live provider or other-platform claim; whole-feature readiness is unchanged.

## D151 — reply after discard, posters, account cards (Quality80)

**Native tests (`gui-mail-selection`)**
- Reply, type, Discard, then Reply again reopens the same reply. It stays open for 1.5 s of repeated draft refreshes, then typing revives it with no new draft, and a second Discard closes it. The old composer fails this.
- A card below the reader loads once the long message above it folds, with no scrolling. The old on-screen check fails this.
- A video card, after its decoder is released, still paints the video's first frame.
- The viewer's Right arrow from one video plays the next.
- Five sync refreshes of the account list keep the same account card, its labels on screen, and read no labels again.
- Reply, Forward and Delay stay visible while replying.
- Reply with words written, then Forward:
  - opens a forward with those words above the forwarded message;
  - discards the reply once the forward is saved;
  - never empties or rebuilds the thread;
  - marks Forward as the filled button.
- Discarding the forward returns to Reply first. Discarding an empty new message returns to the thread it was started from.

## D150 — deletions read ahead, reply and attachment checks (Quality79)

**Rust tests**
- A bulk move to Trash and delete for good, replayed across three history pages, reads the deletions once and asks nothing about the deleted mail after the first page. The deleted mail leaves every view, and a label change in the same history still applies. With the shortcut disabled the test fails.
- If the deletions read fails, the same replay asks about each message and ends in the same state.

**Native tests**
- `gui-mail-selection`:
  - Reply in a thread longer than the reader, with the thread growing while it opens, ends at the bottom with the whole reply showing.
  - The attachments header folds the cards away and back.
- `gui-send-undo`: the send line runs along the bottom of the reply, clear of its header.

## D149 — sending, previews, month toggles and sync catch-up (Quality78)

**Rust tests**
- Replies to Gmail message IDs with `=` build and keep their exact identifiers; an ID with a space is still refused.
- A counted upload keeps its `Content-Length` and bytes and reports every byte.
- A held send reports its size through `google_send_progress` and stops once it has an outcome.
- Older mail keeps downloading while a history scan is behind. Without the turn-taking the test fails.

**Native tests**
- `gui-mail-selection`: with 34 of 36 months folded, a clicked month header stays within 1 px as it opens, while its refresh waits 2.6 s, when the refresh lands, and through six quick toggles whose refreshes land late.
- `gui-send-undo`: upload progress shows while Gmail receives the message, then polling stops once Gmail accepts.
- `gui-conversation-list-model`: hiding a section rebinds only the row after it.

**Measurement**
- `inbox_months_ten_years_60k` (ignored; `FM_YEARS_REPORT`): 60,000 generated conversations over 120 months, release build, warm cache, no concurrent writer. The uncached first Inbox page takes 28.5 ms with no months folded, 38.4 ms with 60 folded and 38.0 ms with 119 folded. Cached repeats take about 0.1 ms.

## D107 — list details and HTML fidelity checks (Quality77)

**Rust tests**
- The schema34 upgrade gains the people tables and restarts the link pass.
- "+N" leaves out own, plus-tagged, dotted-Gmail, shown-sender and no-reply mailboxes.
- `message_format` is one validated preference.
- Previews drop bare, bracketed and angled links, and read HTML source as text.
- The sanitiser keeps a quote when it is the only content, and finds a digest part and a text/html part outside the HTML body.

**Native tests**
- `gui-html-view`:
  - fixed-width cells shrink into a full-width table;
  - 12 levels of nested tables lay out in under 3 s;
  - a relative superscript stays on its line;
  - CSS tables and attribute styles survive a breakpoint;
  - a 100% tall table keeps its content height;
  - long runs and preformatted lines wrap without scaling, keeping their text.
- `gui-mail-selection`: "+N", count order and hiding.

**Real-mailbox acceptance (founder-authorized, read-only)**
- Probe every cached HTML message with `fm-html-probe` at 720 px. Each must render with painted text, and scaling is recorded.
- Record parse, layout and paint times.
- Compare widths and heights against Chromium at the same width. Differences must be explained.
- Scan `mail list --json` for markup or web addresses in previews and for "+N" noise.
- Capture the real list offscreen without opening an unread conversation.
- Real content stays out of Git and is deleted after the run.

## D106 — console tail and reminder regression checks (Quality76)

Verify default tailing, actual wheel input stopping follow mode, the same visible event/pixel offset across repeated appends and 1,000-row front eviction, explicit resume, resize, keyboard navigation, filter composition, pause/copy, focus restoration and warning-free QML. Inspect native renders in Graphite, Paper, Harbour and Terminal. Preserve D103 disk rotation, engine import, redaction and profile admission checks. Exercise a Google-shaped long calendar/occurrence identity through poll/show/snooze/resume/dismiss and reject malformed/oversized identities. Consumers: Core58/Calendar31/Mail UI75. Record host-specific results and failed attempts in [the execution record](../research/local-assistance-execution.md#d106--console-viewer-and-reminder-identity-2026-09-24).

## D105 — HTML reader checks (Quality75)

**Sanitiser tests**
- A hostile corpus is neutralised: scripts, handlers, script-bearing URLs, CSS `url()`/`@import`/`expression`, forms, SVG, `<base>`, meta refresh and MSO conditionals.
- Attribute and CSS values are validated.
- Nesting and size limits hold. Quoted history is removed except in forwards.
- Plain-only mail has no HTML part.
- Plain-text links trim trailing sentence punctuation.
- Block patterns follow the export's rules.

**Core tests**
- The scoped view resolves `cid:` and `data:` images, and reports cached images on reopen.
- The image decoder accepts PNG, JPEG, GIF and WebP within bounds and refuses other bytes, including SVG.
- The image cache is kept per account under its limit.
- Remote preferences are validated.
- Link hosts are counted across accounts, outside the current conversation, and follow their message on delete.
- A schema33 profile gains the link index once.

**Fetch path, against a loopback HTTPS origin and an authenticating loopback HTTPS proxy**
- Only public https targets are allowed.
- Only the fixed headers are sent.
- Oversize images and missing resources fail.
- Redirects to http, redirect loops and redirects to a blocked host are refused without contacting the blocked host.
- `http://` is never requested, and cancellation holds.
- In proxy mode the origin, which has no DNS record, is reached only through the proxy's CONNECT.
- A wrong password and an unreachable proxy both fail, and no request reaches the origin directly.

**Engine session test on the demo fixture**
- `message_html` is sanitised and carries no source URLs.
- `cached_only` returns `not_cached`.
- A blocked pattern returns `blocked` with `by`, and an unnormalised pattern is refused.
- Link summaries and pattern counts match.
- The cache limit is 256 MiB.

**CLI:** the new commands need their scope before storage is touched.

**Native `gui-html-view` (45 checks)**
- Link rules, layout and placeholders.
- Declared sizes never reflow; an undeclared image reflows once; blocked images are never drawn and their pixels are released.
- Hover and click, selection and copy.
- Restyling links keeps the height while the view lays out again, and keeps the text.
- Scale-to-fit at 900 px and the 0.5 floor.
- Content laid out before has its height on its first frame.
- A long document keeps at most 4 tiles, and scrolling paints each new position.
- Failure fallback, tile release, and no Qt warnings.

**Native `gui-html-message-body` (43 checks)**
- The full body flow: inline images only without asking; Load images after the bar has laid out; blocked links; hover; cancellation on leave.
- The proxy-unavailable bar with Retry, and Always from.
- A held message shown again uses the cached result and has its final height on its first frame, with no new image requests.
- Text fallback, the settings panel, and no QML warnings.
- Disabling the height memory makes the three first-frame checks fail.

**Rules**
- No automated test fetches from the real internet, uses a real mailbox or contacts a real proxy.
- Plan budgets, fuzzing, the litehtml differential check, other hosts and the founder's fidelity review remain required before H1's go decision.

Consumers: Core57, Mail UI74, CLI44 and Platform19. [Execution record](../research/html-mail.md#d105--html-reader-implementation-2026-09-24).

## D104 — fifth-export and native suite checks (Quality74)

The native suite must not fail routinely: a test that checks superseded behaviour is updated to the current contract, not left failing. The local-model test drives the header Summarize icon, the neutral progress box, a contained failure and a successful retry. The selection test covers the sender pill, avatar and recipient-name contact cards, the omitted To-self line, and the text reader when the HTML view is unavailable. The navigation-cache test checks the prepared text reader, recipients and saved summary on the first painted frame; the HTML view's own first paint belongs to the HTML reader's tests. The integration walkthrough saves a conflicting view from the CLI, confirms status polling continues, then closes and reads back both the GUI's pending list position and the CLI-only field. The send-undo test keeps a queued Send's draft check across navigation. Visual checks compare offscreen native frames with the served export for the reader, summary states, Month and Week. Consumers: Mail UI73. [Execution record](../research/prototype-september-23.md#d104--fifth-export-2026-09-24).

## D103 — diagnostic log checks (Quality73)

Verify per-profile private log placement after engine admission, redacted/allowlisted metadata, severity and subsystem filtering, model start/completion/failure fields, UTF-8/Unicode summary length, sync state and page/pass counters, 2.5 MiB current/previous segments for each process, engine-to-console import, 1,000-entry view eviction, and stable scroll position while new events arrive. GUI and engine together budget about 10 MiB of retained diagnostic files. Focused Windows checks are recorded in [the D103 execution record](../research/local-assistance-execution.md#d103--persistent-scoped-diagnostics-2026-09-24); current full GUI/package and other-host checks remain separate.

## D101 — search grammar and fourth-export checks (Quality72)

Core tests cover the grammar parser; bare words against subject and text but not the sender; `from:` on senders, a bare address, and Sent recipients; `label:`, `is:` and `has:` agreeing with the typed filters they stand for; conflicts, limits and malformed operators; `search_terms`; Done month counts in a client zone, including a month boundary that differs between UTC and that zone; independent month collapse; `search_senders` ranking and recipients; and the two new view keys. Draft search keeps its own cases. The native suite must add no failures. Its search test drives the real dropdown: operators, completion, people after `from:`, chip reading and removal, an unreadable query that raises no app-wide error, Enter with Recent, removing a recent search, and right-edge placement across a resize. The navigation self-test runs the combined grammar against the fixture engine with two refused queries that keep the last results. The Calendar controls test checks one band height in Day, Week and Month, Day's sidebar counts, the hover ghost over empty, busy and last slots, and a Month week number opening its week. Visual checks compare offscreen native frames with the served export. Consumers: Search7, Core56, Mail UI71 and CLI43. [Execution record](../research/prototype-september-23.md#d101--fourth-export-2026-09-24).

## D099 — GUI memory checks (Quality71)

Memory is measured on the optimized build with three fresh demo profiles per view (Mail, Calendar, Settings), the fixture clock and a 20-second settle. Record working set, private bytes and resident Windows heap. A memory change must also show no visual change: compare icons and buttons with their former rendering at 1, 1.25, 1.5 and 2 device pixel ratio, and the 14 deterministic app captures. The Mail navigation and Calendar benchmarks must not regress against the previous build on the same host, and the native suite must add no failures. UI that is now built on first use is timed against the previous build: startup to first frame, the first Settings open and each page's first visit. These must stay within the existing 100 ms interaction budget. Consumers: Mail UI70. [Execution record](../research/prototype-september-23.md#d099--gui-memory-2026-09-24).

## D098 — native Calendar view benchmark (Quality70)

`fuckmail-gui --benchmark-calendar-output <file>` measures a disposable synthetic profile. `scripts/verify_interaction.py --mode calendar --bin <dir> --report <file>` owns the profile, engine and verification. There are four routes: Day/Week/Month segment cycle, Month date click into Day, Week next/previous and Month next/previous. Each route gets one warmup and 100 measured native mouse inputs. A sample runs from input to the first swapped frame whose view, date, exact-range published rows and visible event delegates match. The verifier checks every painted event identity against `calendar_sources`. The report also counts frames painted before the correct one, the visible jank. The cached calendar view switch budget stays p95 <= 100 ms per route, and the verifier's exit code follows it. Measure optimized builds; the debug build is for development only. Consumers: Mail UI69 and Core55. [Execution record](../research/phase-2-calendar.md#d098--calendar-view-performance-2026-09-23).

## D095 — legacy schema removal checks (Quality69)

Verify that runtime and active tests reference only `fresh-schema.sql`, that the source tree contains no pre-reset SQL migration files, and that schema32 creation and old-version refusal still pass. Run the Rust workspace, strict lint, formatting and repository hygiene. Consumers: Core52; fixture and protocol revisions unchanged.

## D093 — fresh SQLite reset verification (Quality68)

Verify schema32 creation in one transaction for empty live and synthetic profiles, 60 `STRICT` tables, the expanded retention constraint, `foreign_key_check`, FTS integrity/coverage and refusal of versions 1–31 without rewriting their schema. Reopen after a missing first transaction, finished Gmail page and published Calendar run; compare exact replay and retain failed/uncertain payloads. Exercise the category index and FTS-first rare-term plan with generated two-account data. Run the combined Rust workspace, strict lint, native GUI/CLI and relocated package from the shared D092 tree before handing off a fresh-start binary. Test only disposable profiles; the founder will move the current profile and re-authorize accounts. Generated-corpus figures are diagnostic and do not close the 100k/1m/3m, other-host, power-loss or real-mailbox gates. Consumers: Core51/Search6/Gmail26/Calendar30; schema32, fixtures0/3, protocol1 and readiness0/9.

## D092 — v3 visual and behavior checks (Quality67)

Verify actual header geometry/order, narrow wrapping, per-message HTML/collapse, summary provenance/progress, verified attachment export and real media decoding/playback/seek/mute/release. Check serial single/bulk Undo with immutable receipts/stale revision rejection, actual old/new shortcut activation, recording conflicts/fixed keys, and account retention/access through shared commands. Render wide/narrow/themed native screens and scheduled reminder states. Verify actual schema30→31 preservation/reopen/older-engine refusal and a configured relocated Windows package. Preserve all failed attempts and baseline failures; no executed browser pixel-comparison claim follows when local reference navigation is denied.

The navigation oracle checks displayed HTML against its current prepared source-version document, and plain mail against retained fixture text. HTML and plain alternatives may differ. Keep the existing 100-observation routes and 100ms p95 target; report renderer, incomplete routes and failures explicitly. Evidence belongs in [the scoped implementation record](../research/prototype-september-23.md). Whole-area readiness stays 0/9.

## D090 — updated Calendar/Auto-read acceptance (Quality66)

Verify per-account nonfuture count pills across day/week/month, click-through and event/cell separation; persisted All calendars scope; new-event reminder defaults and existing-event preservation; due/no-historical-burst, snooze/restart/elapsed/dismiss, keyboard and sound escalation/mute; and Receipts default plus disabled/other-label/account isolation. Run focused Core and native checks, an actual schema29→30 upgrade/old-engine refusal and a configured Windows package before calling this an installable candidate. Preserve the older D089 Calendar catch-up evidence separately. Other-host sound/visual and real-account readback are distinct gates. Fixtures0/3, protocol1 and readiness0/9 unchanged.

Windows candidate evidence: nine focused reminder tests, account-local Receipts Auto-read and durable live read-journal checks pass; the Calendar, reminder and label native controls pass; build-tree and relocated offscreen GUI smoke, empty-live startup, schema29→30 upgrade/old-engine refusal, strict lint and repository hygiene pass. The full Core run retains three fixture/count assertion failures (338 passed, three ignored), and the full engine run retains two Gmail inventory/coverage assertion failures (164 passed). No live-account readback or other-host synthesized-sound verification has been performed. [Exact binary and execution record](../research/prototype-september-23.md).

## D089 — Calendar catch-up regression (Quality65)

Verify a synthetic remote event is fetched and visible after a new Calendar worker starts against retained tokens; manual Sync must fetch another remote delta before the normal cadence, including through the native header for a Calendar-only grant. A failed forced pass must preserve retry intent. Verify restricted local selection is disclosed and Show all restores the combined view without a provider mutation. Retain the reported log only in ignored local storage. Record live-profile diagnostics as anonymized metadata, not mailbox contents or account identities. Relocated startup checks pass, while the broad packaged Mail self-test and actual event readback remain open; no full Calendar readiness claim follows from synthetic tests. Consumers: engine/Calendar27/Mail UI65; schema29, fixtures0/3/protocol1 unchanged.

## D088 — clean threaded messages (Quality64)

Verify plain and HTML reply-header removal, nested quotations, unquoted inline answers, exact source retention and unchanged attachment/composition paths. Exercise expand/collapse/re-expand on earlier, sent and newest messages using production QML and keyboard, across themes and narrow layouts. Re-run reader stability and package checks; keep failures and source/binary identities in ignored evidence. Core48/Gmail25 trim newly acquired reading/search copies while preserving exact source blobs; existing archive rows are not rewritten. Schema29/fixtures0,3/protocol1 and readiness0/9 remain unchanged.

Executed D088 evidence: 16 mail/MIME tests, ten Google store tests, sixteen synthetic live-preparation tests and strict lint pass. Six native suites pass in 19.94 s; final chevron/control screenshots and isolated Cocoa reader frames pass. Cocoa selection harness limitations and earlier broad scroll-restoration failure remain recorded separately. See the [execution report](../research/prototype-september-22.md#d088--clean-thread-contributions-and-reversible-expansion).

## D087 — navigation and reader stability (Quality63)

Verify pathological nested email tables at initial and final widths, sustained Page Up/Down and arrows with delayed replies, final target and bounded event-loop work, plus actual reader frames for late geometry changes. Retain failing baseline, timings, source/binary identities and prior evidence. Executed: six rebuilt native suites pass in 19.42 s; the final scene-synchronization reader suite passes offscreen and with macOS Cocoa. The latter preserves all 89 observed target frames, handles a 102-key burst in 18 ms with one foreground reader request, and reaches its exact final painted target in 163 ms (including a synthetic 25 ms source delay). Cold/next/warm reader samples are 79/18/11 ms; these are three scoped observations, not a p95 or real-mailbox performance claim. Signed configured Mac relocation, empty-live startup/CLI and shutdown checks pass. No broad or other-host acceptance claim. Consumers: native GUI and verification; shared Core/CLI/schema29/fixtures0,3/protocol1 unchanged.


## D086 — mail list and account backlog

D086 requires missing-only/date/account/source selection and preserved current-summary/manual/read-state tests, real engine batch progress/cancellation/session fairness and CLI checks, native sender-column geometry, single-cursor and paging/range navigation, settings controls and a configured Mac build. Keep failed attempts and rendered captures in ignored local evidence. Final evidence: five rebuilt native suites, 17 packaged engine/CLI checks, strict lint and signed Mac relocation/startup pass. The broad GUI self-test is not passed: its post-arrival scroll-restoration stage still times out. See the [execution report](../research/prototype-september-22.md). Source inspection of the supplied HTML is reference evidence; browser policy prevented opening its file URL. No whole-area readiness or other-host package claim follows.

Consumers: Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62. Schema29, fixtures0/3 and protocol1 unchanged.


## D084 — reader crash and presentation (Quality61)

Use the actual local crash stack to identify the native fault without exporting mailbox memory. Reproduce the error-response path in a synthetic Debug native test, then verify the fix, delayed-source final presentation, repeated navigation and stale-result rejection. Prefetch must not submit read flags. Check that cached HTML/metadata/summaries survive read receipts without reader reconstruction, mailbox tabs have no horizontal track and content scrollbar behavior remains intact. Record exact binaries, failures and source/host limits; query timings alone do not establish perceived reader speed.

## D083 — Calendar mail-count verification (Quality60)

Verify combined/scoped message counts, zero dates, exclusive bounds, midnight and both DST transitions, provider-date precedence, removed-account exclusion and no store writes. Native checks must fence held replies across date/account/hide changes, suppress unchanged revisions, validate returned rows and exercise the real day/week/month QML in all themes. Run the shared query through the built CLI/engine and visually inspect integrated Calendar captures. Preserve exact source/binary identities and failed attempts in ignored storage. Consumers: Core46/Calendar26/Mail UI59/CLI40; no schema/fixture migration, provider action, broad performance, package or other-host acceptance follows.

## D082 — debug-console verification (Quality59)

Use native production-QML input to verify backtick/shifted tilde, editable/search exclusion, Escape/focus restoration, paused-view capture, full clipboard reports and theme layout. Exercise synthetic rate-limit codes/delays, changed sync counters, model-job errors, repeated-state suppression, explicit metadata projections/redaction, threaded Qt capture, 1,000-entry/2,048-character limits and clear. Compile the integrated GUI and check startup with a disposable profile and an identified existing engine; preserve failed attempts and source/binary identities in ignored storage. No real mailbox actions, broad benchmark, package or other-host acceptance follows. Consumers: Mail UI58/native diagnostics; schema29/fixtures0/3/protocol1 unchanged.

## D081 — mail responsiveness

Quality58 requires native signature focus/selection preservation during account count refresh; immediate navigation while another read receipt is pending; correct ordered live pages during insert/delete/flag changes; automatic prefetch and explicit error retry; persistent wider scrollbar controls; and measured synthetic thread/list paint latency. Preserve baseline and failed runs. The scoped performance request supersedes previous deferrals for these paths only.

The [D081 execution record](../research/prototype-september-15.md#d081--mail-responsiveness-and-scrolling-2026-09-22) records six passing final native suites, focused Core paging/lint, improved 20,000-message query timings, inspected scrollbar geometry and a configured relocated Windows candidate. Three full-Core failures reproduce on untouched HEAD. The older paint oracle cannot handle the current segmented reader; rendered latency remains an open gate, with failed attempts retained. Query timings are not a substitute for that gate.

## D080 — local summary reliability (Quality57)

Use synthetic mail only to compare the installed Gemma4, Qwen variants and Muse Glimmer with identities, cold/model-switch and repeated warm timings retained. Exercise the real engine and shared validator; report completion, sender-kind/label quality and factual misses separately. Preserve failed baselines. Verify old saved templates inherit direct answers, explicit overrides survive, whitespace and multilingual punctuation do not cause false rejection, and malformed output, invented/duplicate source IDs, invented labels, tools, stale results and cancellation retain their guards. Verify at most one correction, no intermediate commit, and the shared time/request bounds. Run native assistance controls and a configured relocated package. This scoped inference measurement does not reopen unrelated broad performance work or establish arbitrary-mail accuracy. Local assistance15, schema29, fixtures0/3 and protocol1; readiness remains 0/9.

## D079 — final interaction review (Quality56)

Verify exact tile geometry across Saving, Saved and long failure states in production QML. Verify generated EXIF portrait dimensions and refusal to preview arbitrary non-image bytes; retain multi-chunk save/atomic failure/navigation checks. Assert one plain production Sending status, keep dispatch/progress and newer-edit protections, and rerun cached-summary/observed-job motion checks. Inspect native cards/viewer captures, then verify a configured relocated empty-profile Windows package. This is focused interaction validation, not a whole-app latency or real-provider acceptance claim.

Mail UI56/Quality56; Core44/Gmail24/CLI39/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged; readiness remains 0/9.

## D077–D078 — send, navigation and attachment verification (Quality55)

Verify accepted/current/newer/in-flight/reopened Drafts projections in Core; native startup admission, immediate dispatch/progress, accepted composer cleanup and preservation of newer work. Keep old delayed cancellation/race coverage explicitly isolated from the production zero-delay default. Cached summaries must not animate after navigation or late readback; retain observed-job transition coverage.

Use generated local image/binary attachments in actual production QML. Verify thumbnails, modal/actual-size controls and reader shortcut fencing. Save multi-chunk bytes across navigation, verify atomic replacement, reject corrupt digest and nonlocal destination, preserving previous destination content on failure. Inspect native captures and verify a configured relocated package using an empty profile. No actual mailbox send or broad/performance run is required.

Core44/Gmail24/Mail UI55/CLI39/Quality55/schema29; Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9; D068 stays parked.

## D076 — summary motion verification (Quality53)

Use held synthetic start, poll and committed-result replies to measure production QML geometry. Queued/running/readback states must retain initial height, button width and email position. Sample the result expansion to prove monotonic intermediate heights instead of a single jump. Check retained text/geometry during regeneration, menu-only cancellation, error/retry stability and no duplicate external feedback. Retain lifecycle race, source, sent-mail exclusion, all-theme/text-size and real reader regressions. Verify a configured relocated Windows package with an empty profile; no actual inference/mailbox action is required.

Mail UI53/Quality53; Core43/Gmail23/CLI38/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness stays 0/9; D068 remains parked.

## D075 — date/menu verification (Quality52)

Verify actual production QML date formatting with a fixed local weekday/time, equivalent instants expressed in different offsets and local midnight rollover. Assert both timestamp lines share a right edge, the menu has a ten-pixel gap and centers against the entire timestamp block, earlier/latest menus align with the reader edge, and the star sits beside the short time. Exercise all themes and narrow large text, inspect native captures and verify the configured relocated Windows package with a fresh empty profile.

Mail UI52/Quality52; Core43/Gmail23/CLI38/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness stays 0/9; D068 remains parked.

## D074 — reader projection verification (Quality51)

Verify nested plain/HTML/Gmail/Outlook quotations, preservation of interleaved current answers, independent reveal/collapse, conservative ordinary-prose handling, safe links, compact empty-paragraph handling and deep-nesting fallback. Exercise real QML controls and verify sender contacts, right-edge star geometry, long model-name clipping and narrow large-text layout in Graphite/Paper/Terminal. Re-run authoring round-trip/undo tests to ensure received-only spacing does not change outgoing content. Build and check a configured relocated Windows package using a fresh empty profile. Generated fixtures contain no user email or credentials; actual mailbox writes and performance/broad workspace work are not part of this packet.

Mail UI51/Quality51; Core43/Gmail23/CLI38/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D073 — filing, reader contacts and draft verification (Quality50)

Require an actual loopback HTTP wire check for empty submission framing, scoped query and filing/readback regressions, native selected-row removal and immutable bulk targets. Verify decoded draft equivalence and body/header/attachment/reply-routing differences, legacy proof refresh, restart, local-text preservation and unchanged conflict protection. Exercise real QML hover/click on participants, avatars, folded You, senders and recipients; verify empty header behavior, sender-icon geometry, received metadata suppression, latest full time and narrow/large-text layout. Check Graphite/Paper/Terminal frames and local-assistance controls, strict scoped lint and a configured relocated Windows package. Preserve failed attempts in ignored evidence. Actual-account diagnostics remain metadata-only; generated checks do not establish real Google filing or other-host acceptance.

Consumers: Core43/Gmail23/Mail UI50/CLI38/Quality50; schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D072 — account and model feedback (Quality49)

Verify archive deletion, active/stale/cleanup refusal, exact replay, reconnect identity, foreign keys and other-account/shared-file preservation in isolated profiles. Check setup defaults/choices, durable acquisition queries, old-item exclusion without stored bytes and restart. Exercise native model activation/refresh, archive confirmation, restoration status and setup save-before-sync. Measure expanded request size with escaped/multibyte bodies. Prove schema28 upgrade, old-engine refusal and schema29 reopen with the preserved binary; verify a configured relocated package. Preserve failed attempts. No actual-account mutation or real-mail model inference during automated checks. Full prototype parity and other-host acceptance remain separate.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D070–D071 / P2-UI3 (Quality48): verify typed visibility saves/replay/restart and an actual schema28 old-engine round trip; native Show/Hide and mailbox switching must preserve individual selection and reminders while rejecting stale account results. Verify three-day timeline clipping, cache bounds, zone invalidation, actual drag/arrival and selected-panel accessibility, plus theme captures and the configured relocated package. Keep original failed attempts. Consumers: Core41/Calendar25/Mail UI48/CLI36; Gmail21/schema28/fixtures0,3/protocol1 unchanged. Source/native review does not establish rendered-reference or other-host acceptance.

D069 / P2-UI3 (Quality46): verify the actual native search controls apply/toggle the existing attachment, delayed and date filters; Today/Yesterday boundaries must be consecutive UTC days, and Earlier must not silently retain a lower bound. Preserve search words, account scope, keyboard Escape behavior and Calendar query separation. Inspect current Graphite/Paper/Terminal captures plus narrow large text. D068's unfinished provider work is preserved outside the active build; no schema29, provider-cleanup or rendered-reference acceptance is claimed. Core40/Gmail21/Calendar23/Mail UI46/CLI35/schema28, fixtures0/3 and protocol1.

D067 / P2-UI3 outgoing presentation. D067 acceptance covers frozen MIME content after later draft edits/restart, signature/routing preservation, account isolation, page bounds and uncertainty, exact acquired-ID suppression including later deletion, byte corruption/cancellation refusal, native pending Sent/reader/reply presentation, Undo, selection/mutation guards, Send recovery regression and configured relocation. Only scoped isolated checks count. Full prototype, remaining provider draft/account-disposition/interaction gaps and other-host qualification stay open. Consumers: Core40/Gmail21/Mail UI45/CLI35/Quality45, schema28, fixtures0/3 and protocol1.

D066 / P2-UI3 (revision 44) adds six-second Calendar deletion Undo before the existing conditional Calendar command. The native queue preserves the confirmed account, event/recurring scope and revision, temporarily hides only matching Calendar/timeline/search rows, and restores current projections on Undo. Close/disconnect cancels unsubmitted deletions; invoked requests retain existing durable recovery. Schema28/fixtures0,3/protocol1 unchanged. Scoped native Calendar Undo, editor/layout, search, occurrence, reminder and existing Send/draft Undo checks pass, along with configured relocated package151. The full prototype goal remains open; see D066 and the prototype execution record.

D065 / P2-UI3 (Quality43) implements durable draft-discard Undo with exact revision/lifecycle checks, captured authored buffers and unfinished recipient input, shared CLI/GUI recovery and grouped six-second toast Undo. Schema28 adds the snapshot journal and recipient-input storage; fixtures0/3 and protocol1 stay unchanged. Capture/upload continues verifying retained bytes before use. Scoped Windows Core, engine, CLI and native checks, actual schema27→28 migration/refusal/reopen, strict lint and configured relocated package146 pass. The full prototype goal and provider-cleanup/Calendar-Undo/pending-Sent gaps remain open. See D065 and the prototype execution record.

D064 / P2-UI3 adds the prototype's six-second GUI Send Undo interval and shared action toast. The client saves first, binds the pending action to the original account/lifecycle/authored revision, and starts no Send command during the interval. Undo, GUI close or disconnection cancels pending actions and preserves drafts. Navigation may continue; a late result cannot replace a different editor. Dispatch rereads the same draft/current Google revision and uses the unchanged exact Core Send protocol; it never automatically resubmits an uncertain result. CLI33 exposes an optional six-second pre-request cancellation interval. Mail UI42/Quality42, Core38/Gmail19/schema27, fixtures0/3 and protocol1. Scoped Windows native/CLI checks, explicit Ctrl+C cancellation, strict lint and configured relocated package135 pass. The GUI test uses the actual MailView, closes/reopens its composer, waits the real six seconds, and verifies lifecycle, disconnect, late-read and close cancellation. The previous package115 is preserved. Draft-discard, Calendar-delete Undo and full pending-Sent presentation remain required for prototype parity.

D063 / P2-UI3 adds explicit From-account transfer with source revision and Google revision checks, verified retained content, destination ownership and durable exact-request replay. A move preserves the original local history and waits for the destination Google draft readback before removing the unchanged original Google draft. Unknown deletion outcomes only reconcile the same request; explicit retry starts a new deletion attempt only after a known rejection. Account lifecycle changes fence cleanup. The composer and CLI share transfer/status/retry; source provider IDs never become destination IDs. Schema 27 adds the bounded transfer journal and preserves existing receipt bytes. Core 38/Gmail 19/Mail UI 41/CLI 32/Quality 41, fixtures 0/3 and protocol 1. Scoped Windows Core/provider/native/CLI, actual migration/refusal/reopen, strict lint and relocated package115 checks pass. Full prototype alignment and other-host acceptance remain open; package94/schema26 is preserved in ignored relocation94.

D062 / P2-UI3 adds serial multi-file composer input and bounded cached-recipient suggestions with account attribution. Preserve exact attachment retry receipts, captured draft ownership, unsaved edits and selected routing. Suggestions are read-only until chosen; schema 26, fixtures 0/3 and protocol 1 are unchanged. The D062 decision and [execution record](../research/prototype-september-15.md) own the boundary and pending verification.

D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

D060 / P2-UI3 requires provider-aware loopback/discovery checks, exact compatible HTTP messages and schema, complete model-matched results, cancellation/error bounds, preserved schema-24 data and native switching with stale-discovery rejection. Actual LM Studio/vLLM installations and weight identity are not established by protocol fixtures. See Local assistance 12.

P2-UI3 reader checks cover retained-byte verification, account/source mismatch rejection, read-state preservation, Unicode and escaped JSON preview bounds, native HTML/plain/source controls and passive received formatting. Keep failed harness/build attempts in ignored evidence. Source inspection and native captures cannot establish rendered parity with the browser-blocked supplied prototype.

D059 / P2-UI3 requires focused received/sent/mixed-thread and shared preference tests, native selection/keyboard/deletion, recurrence/date/form/confirmation checks, native visual inspection and a separate configured Windows candidate. Preserve exact prototype bytes and keep generated captures/logs ignored. Browser reference rendering is blocked; source comparison alone does not establish pixel-perfect acceptance.

D058 / P2-LLM2D limits all assistance to received mail. Reader queries expose the actual sent flag; sent targets fail before inference and historical sent results remain stored but hidden. Combined failures display once; null/empty tool calls are accepted as no tools, and truncation has its own error. Schema 24 preserves prompt/account choices while expanding the former default response allowance to 4,096 tokens. See Local assistance 11 and its scoped execution evidence.

D057 / P2-LLM2C simplifies Settings to editable System/User content with normal paragraph breaks. Preserve stored message order and all undisplayed request fields on edits. Schema 23 and inference are unchanged. Native content round-trip/preservation and relocated Windows package checks pass; see the local assistance execution report.

D056 / P2-LLM2B exposes the complete editable Ollama JSON request: system/user messages, all five variables, full response schema and generation options. No hidden envelope or schema is injected. Schema 23 archives and wraps prior prompt text, preserving model/account choices. Application output/action validation remains independent of editable instructions. Consumers: Local assistance 9/Core 32/Mail UI 34/CLI 26/Quality 34; fixtures 0/3/protocol 1 unchanged. See [the current request contract](local-ai.md) and [D056 evidence](../research/local-assistance-execution.md#d056-complete-request-visibility-2026-09-15): 10 validator, four Core and 14 Google-mutation tests, strict lint, native editor/lifecycle, 15 engine/CLI checks, actual upgrade/refusal and relocated configured package pass. The installed-model sample has eight valid outputs but only seven expected label matches.

Historical evidence at the September 14 computer handoff: `618e8a0` passes the repeated actual-MailView/model regressions and normal CTest03 **22/22 in 214.33 seconds**, with 369 source inputs and binaries unchanged. Package03 was interrupted without a terminal result; final package and unchanged performance checks remain required on the next host. Earlier Package02/navigation/search/resize results apply only to their recorded source. Preserve the [single handoff archive](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-final/manifest.json) and [execution details](../research/phase-2-execution.md#september-14-computer-handoff); this evidence update changes no contract or budget.

D054 requires one native vault attempt across concurrent denied workers, explicit retry/reconnect recovery, account isolation, cancellation ownership, truthful completed-history timestamps across failures/restart, immediate manual wake under shared owner/quotas, native header activation and a configured Mac package. No actual token or mailbox dump enters Git.

Updated 2026-09-14. Budgets below are acceptance targets; scoped measurements are linked separately. Google-first required behavior includes fast sender/subject/body search, responsive user-owned daily triage, conversation-only mail, full offline messages/attachments, reliable Google calendar views and optional local classification/summaries on Windows/macOS/Linux. The Windows development host is recorded; real mailbox message counts and local models remain unmeasured.

State: Draft · Revision: 66 · Updated: 2026-09-23 · Implementation: D090 scoped Core and native controls pass; actual schema29→30 upgrade/refusal and configured package remain open. Calendar28/Mail UI66/Local assistance17; whole-area readiness0/9 unchanged. D089 broad packaged Mail navigation failure remains recorded separately.

D055 / P2-LLM2: one shared model and visible template (`{{labels}}`, `{{messages}}`, `{{partial}}`), combined per-message output, default-on prospective summaries/automatic single-label classification, manual precedence, and summary list previews. Schema 22 fences settings and assisted delivery; fixtures 0/3/protocol 1 unchanged. This supersedes conflicting D051–D053 delivery defaults below; historical evidence remains scoped to its original version. See the D055 decision and Local assistance 8. Windows verification passes 9 validator, 3 Core and 14 Google-mutation tests, scoped strict lint, 14 engine/CLI checks, two scoped native tests, eight installed-model cases, actual schema upgrade/refusal and relocated package checks. See [D055 evidence and limits](../research/local-assistance-execution.md#d055-unified-message-analysis-2026-09-15); this is not full product or other-host acceptance.


D049/D050 P2-UI requires immutable bulk targets and account fences, selection without read-on-open, partial failure visibility, native context/label/Trash/date controls, visible splash timing, late-linked label projection/reopen repair, default Google quotas and shared-owner equality, plus a configured relocated Mac package. Historical failures remain in ignored evidence; whole Phase 2 gates remain open.

D048 gives the expanded Rust workspace 1,800 seconds of aggregate execution time after both Windows jobs exhaust 600 seconds while tests continue progressing. The separate compile allowance stays 900 seconds; the CI job allowance becomes 60 minutes to include cold compilation, tests and package checks. Every test assertion, individual operation/native watchdog and application performance target remains unchanged. Calendar control waits report their exact call site without extending their 1,500-ms deadline. Consumers: CMake/Actions configuration and development contributors; runtime contracts, schema and fixtures are unchanged. Earlier failed attempts retain their original limits.

D047 CI fixture corrections reuse private-profile creation before testing verification-marker refusal, and observe a frame after query completion before asserting replacement group delegates. Existing refusal, count/row, reader/draft, paging and timeout assertions remain unchanged; [execution evidence](../research/phase-2-execution.md#d047--p2-ci--september-14-hosted-verification-repairs) retains failing attempts and scoped reruns. This changes no acceptance budget or runtime contract.

D046 evidence storage: keep authored results, failures, source/binary scope, reproducible test code/fixtures and supplied design references in Git. Preserve bulk logs, traces, screenshots, manifests and run archives in ignored local or CI artifact storage, with location and checksum in the owning report. Earlier immutable evidence remains byte-preserved in the verified snapshot and historical commits, including local-only Mac evidence. Consumers: documentation contributors and native CI via `scripts/check_repo_hygiene.py`; reject generated output paths, raw evidence under `docs/`, files over 2 MiB and curated documentation assets totaling over 10 MiB. Quality 27 changes storage/CI only; historical runtime Quality 26 results, all product contracts, schema 20 and fixtures 0/3/protocol 1 are unchanged.

D045 search dismissal: the search text field handles Escape while retaining its query. Deliberate mouse/Tab/back-Tab focus, typing and Find may open the contextual popup; restoration of focus after another popup or window must not reopen a dismissed search. Keep Mail and Calendar query/filter state independent. Consumers: Mail UI 28/Quality 26; Calendar 21 and other shared boundaries unchanged. Verify against actual Main input and popup-focus restoration.

D043/D044 live UI repair: refresh the Calendar catalog when its published generation changes and allow an already-open new-event editor to receive its first writable target without losing typing. Preserve an established target across reorder/withdrawal. Recurring UI authority uses an observed policy epoch, independent of ordinary source bookkeeping revisions: membership, effective permission, write/create eligibility and time-zone transitions advance it, including revoke/restore cycles. Retain lifecycle, exact projected occurrence, scope/action, fresh engine proof and conditional write fences. Timed/all-day/timeline event blocks support native accessible activation; dropdown options expose their visible names. Consumers: Calendar 21/Mail UI 27/Quality 25; Core 27/Gmail 13/Platform 15/CLI 21/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged. Focused regressions and real UI outcomes are recorded in the Phase 2 execution report; whole-area acceptance remains open.

Mac regressions retain the existing callback-destruction check and add cancellation before a queued socket handshake. Native composition waits for the actual startup overlay to disappear before clicking. The Settings appearance check first observes the earlier Calendar settings in the normal status projection; it still forbids an appearance-only Calendar reload. These are lifecycle/test-precondition corrections, not relaxed assertions or paint deadlines.

D040/P2-01C acceptance adds email-first Add/reconnect/restore through the existing signed loopback fixture in actual GUI/CLI processes; invalid email and absent/invalid app-registration failures before browser/provider work; client-ID binding after package relocation; existing legacy configuration compatibility; and relocated package setup availability. Retain state/PKCE/identity/grant/cancellation/race regressions and unchanged native/performance targets. Real personal/Workspace consent, native-vault/disk/backup checks, complete offline coverage, sustained refresh and current other-host qualification remain separate gates. Do not treat a generated account or configured setup response as actual Google acceptance. Consumers: Platform 15/Gmail 13/Mail UI 26/CLI 21/Quality 24 with Core 27/Calendar 20/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged.

D039 acceptance covers cached Calendar search text fields, partial grants/privacy, selected visibility, recurrence/exception and multi-day identity, bounds, no mutation/provider work, CLI routing and native query/selection/stale-response behavior. Rebuild affected targets and rerun focused tests, then package the final source and measure the existing navigation/search/resize targets. Earlier full workspace/native/package observations retain their exact source limits; do not relabel them as rerun on a later source. Consumers: Core 27/Calendar 20/Mail UI 25/CLI 20/Quality 23; schema 20, Platform 14/Gmail 12/Search 5, fixtures 0/3 and protocol 1 unchanged.

The large-mailbox paint oracle continues to require every visible row to belong to the current model. Retain the failing actual-MailView regression for a 100-to-201-row pinned structural refresh: Qt's retired index-minus-one delegates must be invisible and disabled, while current delegates retain exact role data, canonical index/order, nonoverlapping geometry and the surviving selected object's identity. Exercise repeated structural changes and unchanged data-only notifications. Do not filter stale delegates out of the benchmark to obtain a pass. This is a regression check for the existing D038 stable-list behavior, with unchanged input-to-paint budgets.

Revision 22 adds the final feedback-build gates under D038: ordinary empty live startup with explicit demo separation; actual Settings add/remove/restore controls; effective catalog/event/free-busy grants in both HTTP and projections; recurring create/series/occurrence/following workflows with unchanged old receipts and explicit recovery after a partial split; running-client notification lifetime, permission fences and reschedule/reopen deduplication; updated prototype controls and startup; bounded cache eviction and stable reader updates; schema-19 upgrade/refusal/reopen; paired native/CLI workflows; and relocated Windows packaging with Qt Widgets deployment. The founder resumes the unchanged navigation/search/resize targets. Historical failures remain; new correctness or compilation results never imply a performance pass. Keep one final execution record, source identity and relevant failed attempts. Current real-account comparison, complete-history and sustained OAuth evidence remain distinct from generated-provider tests and the feedback package.
Revision 21 adds D037/P2-06C gates: exact moved/DST/all-day occurrence identity and bounded lookup; private/locked/special master and instance restrictions; held source/permission/lifecycle races; original master/sibling/metadata preservation; sparse cancelled exception retention through pending publication/full refresh/reopen; malformed/duplicate/byte/exception limits blocking affected series; unchanged two-seat quota/memory ownership with concurrent lookups; exact v1/v2 journal preservation and v3 late response/read recovery; actual schema-18 upgrade/old-engine refusal; paired native/CLI generated-provider workflows and final source binding. Retain failures and notices. D031 still defers performance. Consumers: Core 25/Calendar 18/Mail UI 23/CLI 18/schema 19; Gmail 12/Platform 13/Search 5 unchanged. Readiness stays 0/9.

Revision 20 adds D036/P2-01B gates: direct Core active/removed guards; replay and cross-kind conflicts; held local save/copy through remove-restore; provider claim/publication versus definitive late outcomes; serialized refresh/store/cleanup including cancellation and restart; failed-restore credential truth; two/last-account navigation and unsaved native buffers; archive read/search/export/receipt inspection with exact retained bytes, unchanged flags/journals and zero HTTP; actual native/CLI loopback addition/cancellation/partial grants/same-subject restore/wrong-subject refusal; no automatic import or outbound work; default/isolated startup refusal and final-owner drain; actual schema-17 upgrade/old-engine refusal; and final workspace/native/provider regressions with source binding. Generated credentials only. D031 still defers performance, and recurrence/reminders, current packages/CI and protected real-account acceptance remain open. Consumers: Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20/schema 18; Search 5, fixtures 0/3/protocol 1 remain unchanged.

Revision 19 adds D035 invitation gates: exact guest add/remove/all-remove/no-op semantics, preserved metadata and incomplete/asynchronous refusal; organizer/self permissions; retained labels/conferences/attachments; exact sendUpdates and quota admission; ETag/connection/permission races; lost response, read convergence and late outcomes without fabricated notification evidence; restart/no duplicate mutation; actual schema-16 upgrade/refusal with old receipts intact; and paired native/CLI generated-provider checks. Core 23/Calendar 16/Mail UI 21/CLI 16/schema 17 define this packet. D031 still defers performance runs; complete recurrence/reminders, D034 account workflows, current packages/CI and actual accounts remain required.

Revision 18 records D033's B3B functional gates: schema-15 upgrade/refusal and receipt preservation; complete copy routing/text/file identity and unsupported conversion refusals; no upload on adoption, correct first-edit and immediate-Send target; stale/cross-account source/confirmation fences; distinct delete admission and journal replay, changed target, known failure/lost response/cancellation/restart; and native/CLI receipt/source inspection. Exact known-target unresolved work blocks conflicts; similarity never establishes ownership. Generated providers supply independent write/read and MIME oracles. D031 defers benchmark execution, not the existing latency/resize requirements. Current packages/CI, complete Calendar and protected real-account admission remain separate. Consumers: Core 22/Gmail 11/Mail UI 20/CLI 15/schema 16; fixtures 0/3/protocol 1 unchanged.

Revision 17 requires D032 migration/old-engine refusal and exact old receipt preservation; two-account/multipage inventory, duplicate/replaced IDs, cancellation/restart/expired tokens, generation/permission changes, malformed/oversized items and 404; original RAW, routing, HTML, binary/CID retention; read-only refusal at every mutation layer; B1/B2 identity races without invented ACKs; bounded memory/writer work and cached queries; actual paired CLI/provider and native offline reopen/source export. Retain both accepted and failed prior evidence. Core 21/Gmail 10/Mail UI 19/CLI 14/schema 15 define the new runtime boundary, fixtures 0/3/protocol 1 unchanged. D031 continues to defer performance remeasurement; 100/200-ms navigation/search targets and final resize/package/live gates remain open and unchanged. No new latency result or whole-feature acceptance is implied.

Revision 16 aligns only the aggregate native `gui-integration` CTest watchdog with the existing SDK-free package allowance: 180 seconds. Its main walkthrough retains 120 seconds, each restart child retains 45 seconds, every independent fresh GUI/CLI process and lifecycle assertion remains, and input-to-paint/search targets stay 100/200 ms p95. D030 follows two 150-second aggregate failures with measured progress through late startup scenarios; it is a harness allowance, not a correction or proof of application speed. Retain both failed runs, final-source measurement and the primary walkthrough's narrow timing margin. Consumers: CMake/CTest, native and package verification, Core 20/Gmail 9/Mail UI 18/CLI 13; runtime source, schema 14, fixtures 0/3 and protocol 1 are unchanged by this adjustment.

Revision 15 requires D029 mixed-journal/schema compatibility, exact strict MIME in all four composition modes, simultaneous GUI/CLI and draft-sync/newer-edit races, quota-before-memory and cached-query progress, and cancellation/late acceptance/restart without automatic duplicate submission. Pair actual CLI/provider and native Send/status/recovery acceptance, preserving frozen recipients, body and attachments. Repeat final-source navigation under unchanged budgets. This consumes Core 20/Gmail 9/Mail UI 18/CLI 13/schema 14 and does not establish real delivery, complete remote Drafts or remaining Calendar/live gates.

Include recipient commit/validation and content/navigation changes during a held native save, proving no latent Send after failure or later autosave. Prove that a same-revision accepted draft cannot be sent again with a different request, while a strictly newer saved revision requires a fresh explicit direct submission and a unique RFC Message-ID. Pending/unknown attempts remain blocking. Default-off generated provider acceptance uses a separate explicit Send opt-in, bounded retained submissions and actual Sent metadata/RAW readback; it cannot use network or native credentials.

Exercise local auxiliary-capacity refusal separately from a lost IPC response: only the never-created fresh invocation is immediately known unsent. After a real lost pre-journal response and newer edits/observation, two independently prepared original proofs must both fail without a journal or claim. Only an exact absent-row, strictly newer monotonic-revision proof may release that original uncertainty on explicit Resume; generic stale/not-found errors and existing unknown rows must remain blocked. Preserve newer editor content through every recovery path.

Revision 14 requires Google draft migration/legacy receipt isolation, off-writer exact authored MIME including recipient-free drafts, strict unchanged send validation, upload/readback identity and retained-byte checks, changed ID/hash/missing/unknown conflicts, account/generation/newer-local-revision races and restart without automatic duplicate creation. Verify actual shared quota admission before queued MIME loading/encoding, independent cached-query progress during held draft work and cancellation permits/late durable receipts through owner shutdown. Paired CLI/native status and explicit conflict actions must preserve author text and prototype geometry. Compare the final integrated navigation source against the unchanged 100-ms p95 and exact paint oracles; fixtures 0/3 and protocol 1 stay unchanged. This packet consumes Core 19/Gmail 8/Mail UI 17/CLI 12/schema 13 and does not itself verify real send, complete remote Drafts or live accounts.

Include an invalid oldest draft followed by valid newer work, proving one deterministic preparation failure cannot starve the account and an edited revision can progress. Verify that explicit replacement after an unknown update requires a freshly observed existing target, preserves the original unknown receipt, and refuses unknown creation or a missing unknown target. Historical request replay must not regress the current native draft status.

Revision 13 defines P2-05A2 attachment acceptance against Core 18/Mail UI 16/CLI 11/schema 12. Require migration preservation, import→save→synthetic capture→restart byte equality, exact replay after source deletion/change, stale/account/limit/corruption refusal and unchanged received attachment filters/counts with nullable local imports. Test first-save source/identity races and capture replay against later edits. Actual engine cached queries must progress during held preparation, and cancellation must retain its bounded worker permit until that worker completes. Native/CLI checks must preserve selected account/draft and unsaved text across file-picker/import outcomes. Existing latency targets, fixtures 0/3 and protocol 1 stay unchanged; a functional attachment test does not replace navigation, resize, package or live-provider acceptance.

P2-05A2 navigation diagnostics add at most 64 arm events and 64 delivered-frame events per input, with separate dropped counts. Record each arm's generation, input-relative timestamp and changed snapshot field names; each frame retains synchronization, rendering, swap and queued GUI-callback timestamps plus acceptance/rejection reason. Render-thread work remains fixed-size copies and constant-time clock reads; diagnostic allocation stays on the GUI thread. Keep every existing snapshot field, generation/model/visibility check, workload, timer and the 100 ms p95 budget. Instrumentation overhead remains inside the measured run; this additive diagnostic scope is not a performance pass. Consumers: the native navigation benchmark and shared developer-only paint observer.

For a stalled mouse input, retain its exact target geometry/visibility/enabled state, scene/global coordinates, move/press/release acceptance and target lifetime, plus account/draft/deferred-save state before and after dispatch. A private account-selection entry counter and last-entry guard state distinguish delivery from backend deferral; they expose no product control or protocol field. Keep the original input coordinates/event sequence and watchdog, with no retry or extra settling delay. This is bounded diagnostic evidence and does not establish that an uncompleted navigation route passed.

The same packet corrects observed list-refresh churn with granular native row updates. Require actual model signal/persistent-identity checks for unchanged rows, data changes, insertion, removal, moves and scope resets, plus native retained-delegate/viewport checks during read-on-open and summary refresh. Existing full navigation coverage must still prove groups, pinning, paging, filters and account separation. Keep the failed first schema-12 benchmark and diagnostic-only rerun; neither is evidence for a later product correction. Repeat the unchanged concurrent-acquisition benchmark only after the correction and functional checks are integrated.

Account-tab acceptance also retains a target control and its geometry through a genuinely changed account payload from independent CLI work. A press held across another account refresh must remain pressed on that same control; releasing at its original center must select the intended account and preserve the originating draft. Keep original source/read-state assertions and normal account restoration. This guards tab identity during background updates without adding retries or settling delays to the performance workload.

Revision 12 scopes D026/P2-08A to native navigation during real Gmail acquisition workers with simulated HTTP/vault services. The first two-account tier stays below 10,000 conversations per account and keeps recent target conversations stable while older unique conversations arrive. Reuse the unchanged GUI input/model/paint and SQLite identity/body oracles, normal polling and deliberate read-on-open. Require 100 correct paints for each of mailbox click, Tab, Shift+Tab and conversation click, each <=100 ms p95. Independently verify MIME/text/FTS retention and actual successful durable publication during every route, plus GUI/CLI owner lifetimes. Record all raw samples, p99/max, renderer/DPR, source/binary hashes, fixture definition and supplied simulated quota values. A passing mail-only tier does not establish concurrent Calendar, large-mailbox/search/resize, physical input, packages or real Google acceptance. Consumers: Platform 12, native measurement tools, engine workers, Core 17, Gmail 7, Calendar 15, Mail UI 15 and CLI 10; schema 11/fixtures 0 and 3/protocol 1 unchanged.

The [P2-08A Windows development measurement](../research/phase-2-execution.md#p2-08a-native-navigation-during-gmail-acquisition-under-d026) passes all four routes at p95 **84.59 / 82.38 / 91.30 / 77.03 ms**, 100 correct paints each, while both accounts durably acquire messages during every route. Independent MIME/text/FTS checks cover 1,290 messages and 141 part receipts; GUI/CLI ownership and final cancellation pass. The maximum reader sample remains **213.99 ms**; repeated snapshot re-arming is observed, but its cause is unproven. Graphite/native D3D11/DPR 1.25 and exact source/binary hashes scope this pass. Calendar concurrency, larger corpora/search, resize/themes, packages, physical input and live providers remain separate. Earlier failed verification attempts and the Qt shutdown warning are retained.

Revision 11 prepares D020's real-account acceptance. One personal Gmail and one Workspace user must independently pass identity/consent/vault, restart refresh, provider changes, offline/recovery and mail/calendar interoperability; additional intended Workspace policies get separate rows. Cloud/admin availability is founder-reported, not live evidence. Record actual granted scopes, project quotas/billing state, effective poll/cooldown behavior and later refresh beyond the seven-day Testing boundary under the applicable configuration. Physical Windows data protection/vault qualification precedes its pilot; other physical hosts remain separately gated.

Keep cached navigation <=100 ms p95 and visible search <=200 ms p95, with at least 100 observations per measured route and exact account/source/frame checks during concurrent mail/calendar acquisition. Preserve P1-10 resize/three-theme controls and the independent ingestion oracle. Record actual inventory, retained MIME, attachment/index coverage and errors; estimates, partial import and screenshots are not complete-offline evidence. P2-08 pilot admission can pass before full-history backfill; complete coverage is a separate closeout gate.

The September 13 schema-9 Windows development integration passes **10/10 CTests in 292.33 seconds**, including Rust workspace, native empty live profile, full Settings/mail/composition/Calendar walkthrough and actual restart/readback. The native walkthrough takes 147.84 seconds against its unchanged 150-second allowance; this functional pass is not a navigation-latency measurement. Its shutdown warning remains recorded. D023 adds shared Calendar preparation and off-writer page/chunk work with scoped synthetic checks. Seven 250-event pages of 16,426,391 bytes produce 119 ordinary chunks at most 985,575 bytes/15 items: debug writer stage p95/max is 52.9193/56.8742 ms, and consolidation p95/max is 26.5253/36.9004 ms. These SQL measurements establish neither UI latency, worst-case oversized-item latency nor physical allocation/RSS. [Calendar evidence](../research/phase-2-calendar.md) records the workload and artifacts. [Phase 2 execution](../research/phase-2-execution.md) preserves failures and exact tested scope. Native concurrent latency/resize, live personal/Workspace comparisons and final packages/CI remain required. Whole-area readiness remains Draft.

D023's integrated schema-10 Windows Debug run passes **10/10 CTests in 282.36 seconds**, including 353 executed Rust workspace tests and native GUI/CLI/restart checks. The main walkthrough passes in 139.83 seconds under the unchanged 150-second watchdog. All 262 captured source/tool/fixture hashes match after the run; three native executable hashes are retained. The shutdown warning reports four items still being created at engine destruction and remains unresolved. A separate 306-test integration run and strict four-crate lint pass. These functional runs verify no input-latency, resize, RSS or speedup target; current release/performance and live-account gates remain open. [D023 logs and scope](../research/phase-2-execution.md#d023-integrated-verification).

D024/D025's P2-07A schema-11 source has 376 passing Rust workspace tests (five ignored) and strict four-crate/all-target lint. The first full Windows run passed 11/12 CTests; after Calendar/label control corrections, the final native rerun passes all ten GUI/lifecycle checks plus their build fixture, 11/11 in 179.12 seconds. Rust is unchanged from its passing run, and all 271 final source/tool/fixture hashes match. The 143.11-second native walkthrough remains below its unchanged 150-second watchdog. The focused Calendar/label controls and real auxiliary-IPC tests pass with no QML binding-loop warning; three items are still reported during engine destruction. These are functional passes, not a single full 12/12 run or latency/resize/live-provider acceptance. [P2-07A evidence and failed attempts](../research/phase-2-execution.md#p2-07a-native-google-integration-under-d024d025).

Use synthetic/fault-injected quota, cursor expiry, lost response, concurrent edit, disk-full and malformed-source cases first. Real send/attendee tests require approved test recipients/calendars and independent Google web/CLI/GUI comparison; no deliberate exhaustion of the founder's limits. Unknown sends cannot auto-retry, and calendar notifications have one owner. Keep real payload oracles in the protected profile, sanitized counts/results in Git, and all native CI credential-free. Require native Mac/Windows/Ubuntu checks and SDK-free final packages, plus actual host vault/lifecycle tests for live use. No single login/send or planning document verifies a whole feature area. Consumers: Core 17, Gmail 7, Calendar 15, Mail UI 15, CLI 10, Platform 11 and existing Search/Triage. [Phase 2](../plans/phase-2.md) owns the staged matrix and gate order.

Revision 10 applies D019's reported interaction/rendering defects. Measure actual mailbox click and Tab/Shift+Tab plus conversation-click input to the first correct usable content paint, with account/row/member identity checks, against the existing 100 ms p95 cached-navigation target. Record at least 100 warm samples per route on fresh fixture 3 and the retained 100k seed copied into disposable profiles; separate query, model and render time when practical. Include ordinary polling and read-on-open effects, and retain independent CLI/draft/account correctness checks. Record resize frame intervals/tail stalls and frame captures at native Windows scaling for borders, divider and vertical lines; inspect all three themes, regular/compact sizes and enabled/disabled button contrast against the prototype. Preserve the existing search/ingestion workload and budgets. Consumers: Mail UI 13, Core 13, CLI 7, Calendar 11, Platform 10 and native measurement tools. No run or improvement is claimed from this specification.

The final Windows package at `25b8b1e` passes 900 navigation identity checks, with every 100-observation route below 100 ms p95 on fresh and large synthetic profiles. Packaged mailbox p95 is 81.58–91.62 ms and reader p95 78.32–91.73 ms; some p99 observations exceed 100 ms and remain in the reports. All four concurrent-search classes pass at p95 64.54/74.75/108.18/164.58 ms during 5,000 independently verified arrivals. Release03 passes 9/9 tests in 158.15 seconds and SDK-free relocated workflows/restart pass. Three-theme D3D11 resize traces at DPR 1.25 retain aligned, constant-color sampled rules and nonzero calendar scroll; nine package control captures are reviewed. Mail offer-to-swap p95 is 9.82–10.49 ms; Calendar is 7.40–7.87 ms, with a maximum observed swap interval of 25.79 ms retained. These finite programmatic traces establish neither physical scanout nor other-host GPU behavior. No resize budget was added. Final P1-10 CI now passes 9/9 plus relocated packages on all three hosts; whole-product readiness stays 0/9.

Revision 9 adds D018/P1-09 evidence for all three native palettes, text sizes, saved initial/last workspace, both Mail timeline positions, new-event duration, full counted collapsible groups, kind filters and exact-account sender cards. Require actual native controls and independent CLI/restart readback, regular/compact visual review, schema 4/5/6 preservation into 7 and package verification. Fresh fixture 3 adds explicit source-bound annotations without reseeding existing profiles. Consumers: Core 13, Mail UI 12, Calendar 11, CLI 7, Search 5, Triage 6, Platform 10 and native tooling. The unchanged ingestion workload offers 25 sources every 100 ms; cooperative staging remains bounded at 32 resident items and each completed page commits atomically. Actual commit timestamps establish overlap. All four 100-observation native classes must meet the existing 200 ms p95 budget. Earlier failures and passes retain their source-specific scope.

P1-09 [native05](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-native-05.log) passes the updated Windows controls, idle CLI-to-GUI convergence, all four composition modes/source-conflict recovery, Calendar edits and all saved startup choices in actual fresh processes. The [dev02 concurrent-ingestion measurement](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-dev-02.json) still fails: p95 is 67.27/118.10/106.50/200.81 ms, while all 5,350 actual sources and commit-overlap oracles pass. [Corrected Settings captures](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/visual-02/manifest.json) cover D3D11 at the existing 125% Windows scale. These do not establish final package, other-host, physical-input or whole-product acceptance. Schema6 preservation has an executed Core regression; no historical schema6 package exists, so its package-to-package upgrade remains unverified separately from the required preserved schema4/5 runs.

The subsequent [complete Windows release run](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-full-release-01.log) passes 9/9 CTests in 196.98 seconds, including all 163 nonignored Rust tests and full native GUI/restart. Real responses verify explicit query priority over unsent automatic reads, preserved manual-read/write barriers and polling progress after four bypasses. The [dev03 native concurrent measurement](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-dev-03.json) passes all four unchanged 200 ms p95 targets at **67.74/73.01/98.64/168.36 ms**, with 100 target/reset pairs per class and independent checks for 5,150 actual sources plus overlapping commits. This is recorded-host development-binary evidence; earlier failures remain preserved.

Clean source `8743162` subsequently passes [Windows package relocation](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-package-01.json), SDK-free full native GUI checks and actual preserved [schema4→7](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-upgrade-schema4-01.json)/[schema5→7](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-upgrade-schema5-01.json) upgrades with exact source/authorship/receipt retention and old-engine refusal. Its [packaged native concurrent measurement](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-package-01.json) passes at **70.18/76.96/110.34/189.57 ms p95**, 100 target/reset pairs per class, **5,100 independently verified arrivals / 204 atomic pages**, unchanged seed and system-only PATH. All classes overlap actual commits.

P1-09 final-source native CI at `791254d` passes **9/9 CTests on each host**: macOS 15 ARM64 in **224.91 seconds**, Ubuntu 24.04 x64 in **383.68 seconds** and Windows Server 2022 x64 in **967.53 seconds**. All three SDK-free relocated CLI/GUI development-package checks and package/log retention steps pass. The manifests identify clean `791254d`, a documentation/evidence checkpoint on runtime `8743162`. CI programmatic/offscreen checks do not establish physical input, other-host GPU behavior or P1-10's navigation/resize measurements. Mac packaging retains codesign diagnostics despite passing runtime checks; trusted signing/distribution remains unverified. Subsequent P1-10 acceptance closes Phase 1 on available hosts; whole-product readiness remains 0/9.

Revision 8 adds P1-08/D017's new/reply-all/forward/Drafts acceptance to the September 12 native design replacement. Core 12/Mail UI 11/CLI 6 use schema 6, fixture 2 and additive protocol 1. Require actual native controls plus independent CLI/MIME readback, original recipient/source retention across arrivals, explicit account scope, incomplete recipient retention, optimistic conflict/discard behavior and fresh-process draft restoration. Compare native captures with the updated reference at regular and compact sizes. Earlier schema-5 native/package/performance results remain scoped to their recorded source. The P1-07 arrival/count/selection, edited calendar/RSVP restart and painted search with independently verified concurrent source ingestion remain required; budgets are unchanged. Consumers: Core, Mail UI, Calendar, CLI, Search, Platform and packaging/test tooling.

Revision 7 preserves the subsequent [clean-package GUI measurement](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/search-gui-100k-package.json): combined-filter p95 **221.80 ms** fails the unchanged 200 ms target despite the earlier passing run. The correction shortens debounce and drops obsolete unsent queries; it requires a new measured result. The benchmark reports input-to-model and model-to-paint components alongside the complete boundary and returns nonzero for a measured budget failure, after normal shutdown. No warmup/reset work is included as a measured target. Full Windows release/relocation and schema 4→5 checks pass at `1132c44`; final timing/native control/CI acceptance remains recorded separately. Consumers: Search, Mail UI, Core, Platform and native test tooling.

The [final clean-package rerun](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/search-gui-100k-package-final.json) at `c34b82e` verifies the correction: rare-subject/body/grouped/combined p95 **49.13/56.11/85.03/135.53 ms**, 100 actual target/reset pairs per class, native D3D11 and the same 200 ms boundary. Complete native control/restart and relocated package checks also pass. Final evidence and limits retain the earlier failed trials and separate final-source CI/physical/full-product gates.

Revision 6 records passing warm 100k Core and native Windows input-to-paint measurements without changing either budget. Core grouped/combined/concurrent-query p95 is 28.04/82.94/87.53 ms; queue-to-response p95 is 98.70 ms. Native D3D11 rare-subject/rare-body/grouped/combined p95 is 94.32/95.77/126.07/174.26 ms, with 100 actual target-model replacements and independently confirmed reset paints per class. See the search evidence and [native report](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/search-gui-100k-measured.json). These are this generated workload on the recorded host, not cold-start, million-message, physical-input or live-provider evidence. Consumers: Core, Search, Mail UI, Platform and native test tooling. The expanded aggregate Rust suite watchdog is 600 seconds after Windows CI exhausted its old 180-second allowance; application performance targets and individual operation watchdogs are unchanged.

Revision 5 keeps the existing budgets and adds the app-consumed grouped first-page/filter queries, source-ingestion contention, native formatted document and combined calendar checks to the active Phase 1 execution record. The initial 100k/100-observation grouped and filtered p95 values (253 ms and 648 ms) exceed the proposed 100 ms Core budget; concurrent query/queue-response p95 was 601/602 ms. These are failed measurements under correction, not readiness. Native compilation and a passing startup/layout test do not prove the complete phase or the input-to-painted-result budget. Affected consumers: Core, Search, Mail UI, Calendar, CLI and Platform.

Revision 3 applies D009: separate first usable, Google dogfood and later-protocol evidence. Best-effort HTML comes later; full browser fidelity and legacy protocols do not gate first use. Affected consumers: all workstreams. Current Google-first readiness denominator is nine.

Consumes versioned builds, synthetic fixtures, hardware/runner metadata and workstream results. Produces reproducible measurements and pass/fail or explicitly renegotiated gates. Start independently with fixture definitions; measured experiment results now live in Phase 0 evidence and the search report. Proposed product budgets are distinct from those measurements.

Home Windows evidence, 2026-09-11: the planning checkpoint `5bed257` (runtime unchanged from `204fa62`, fixture 1/schema 4/protocol 1) passed 8/8 dev CTests in 69.13 seconds and 8/8 release CTests in 95.63 seconds, including test compilation but excluding the initial application build. A scoped developer-tool correction then enabled packaging with project-local Rust. Relocated GUI/CLI, final-owner shutdown and native D3D11/RTX 5090 rendering passed. The handoff record preserves the failed packaging attempt, correction, package hash and remaining manual checks. These are development-host checks, not performance-budget or whole-feature release passes; readiness remains 0/9.

Revision 2 connects Phase 0 to reusable fixtures and evidence gates. Affected consumers: every workstream, especially Core/CLI/Platform for the first shared slice. No fixtures, harness or measured results exist yet.

## Fixture and verification foundation

Current gates are scoped to the Google-first product (nine areas); IMAP/SMTP and external iCalendar subscriptions do not count until the later protocol phase. First usability requires a real GUI plus useful partial CLI, readable text including HTML-only inputs, human drafts and basic mail/calendar behavior. Expand CLI/GUI parity with each Google workflow. The founder must dogfood usable Google clients before legacy implementation begins; document actual scenarios and failures, not merely an installed build. Best-effort HTML is later work and does not block initial use or select the toolkit.

Create these artifacts during P0-01 and their consuming experiments; this table describes inputs and expected evidence, not tests that already exist.

| Fixture family | Owner and scenarios | Verification purpose |
| --- | --- | --- |
| Identity and triage | A: separate accounts, colliding provider IDs, mixed unread/star states, later thread replies, archived/folder mail, partial coverage | Correct account scope, snapshot actions and totals independent of loaded rows/filters |
| Source mail and drafts | C/E: varied MIME/encodings/quoting, inline resources, malformed inputs, large parts and authored draft/signature/attachment cases | Preserve original bytes, extracted fields and attachments; round-trip human content and control reader resources |
| Sync and durable operations | A/B/C/F: paginated deltas, expired cursors, partial moves, stale revisions, crash points, acceptance-unknown send and Sent-copy failure | No skipped checkpoint, lost authored work or automatic duplicate send; provider outcomes remain distinct |
| Protocol and lifecycle | A/I/G: version mismatch, concurrent clients, locked credentials, disconnected/slow consumers and final-client exit | Same semantics from Rust CLI and Qt adapter; bounded memory, one writer and explicit recovery |
| Calendar | F: independently expected occurrences, time-zone changes, exceptions, permissions, overlaps/all-day, RSVP and stale feeds | One correct event view for full calendar, strip and CLI; no title/field access beyond actual permission |
| Assistance | H/A: off/unavailable, invalid/abstaining/late output, changed mappings/manual overrides, new thread members | Manual use unaffected; only permitted filing; faithful source/coverage handling and cancellation |
| Scale and packaging | D/G: seeded varied corpus plus clean installation/profile fixtures | Repeatable retrieval/ingestion measurements and actual GUI/CLI/renderer behavior on required targets |

Version small fixture manifests and expected outcomes, record provenance/licenses and keep private content and generated bulk data out of source control. Shared boundary tests use the same fixture revision in Rust and C++; calendar expected results must not be generated solely by the recurrence library under test. A provider mock demonstrates our response to its input, not the real provider's behavior.

When code exists, layer fast domain/fixture tests, shared-protocol and crash/recovery integration tests, local protocol sinks, GUI interaction/rendering checks and clean-install/live-provider checks. Run the relevant layer for each slice and publish exact successful commands with the implementation. Start CI with headless/build checks on available supported runners; actual OS/desktop/keyring tests remain separate evidence. No speculative test scripts or CI configuration are added by this plan.

A result records build/dependency versions, fixture revision/seed, OS/architecture/hardware/display session, executed command, expected/observed outcome and pass/fail/blocked status. Only executed checks can pass. Blocked platform or account rows remain visible while independent work proceeds. Use the existing budgets below, not a new per-plan definition of readiness.

## Representative workload

Use synthetic mail and non-sensitive fixtures for repeatable development. Add founder-owned data only through a deliberately scoped local evaluation. Do not put private messages, attachments, addresses or credentials into the repository or benchmark reports.

The supplied `du -h` snapshot reports about 25 GB for the existing eM Client local store, with major directory totals around 2.9 GB, 7.6 GB and 15 GB. This is user-supplied disk-usage evidence, not a verified account inventory, full server size, or message count. No eM Client database was inspected.

Proposed scale tiers: 100,000 messages for rapid iteration, 1 million across several accounts as the primary large-corpus test, and 3 million as a stress tier. Include a roughly 25 GB offline-content case anchored to the current store and a 100 GB growth case. These are synthetic capacity scenarios, not claims about the founder's message count. Include both many short messages and attachment-heavy mailboxes.

Record message count, unique text bytes, binary bytes, average and tail body size, number of accounts/folders/labels, thread lengths, languages, calendar series, local retention coverage and index size. Repeated identical text alone is not a representative search corpus.

## Candidate budgets

Proposed reference: a high-end computer with 32 GB RAM and an NVMe SSD; record exact hardware before benchmarking. Report Windows, macOS, Ubuntu, Arch and Omarchy results separately; all required platform/configuration gates block MVP. Pin versions, architectures and display sessions rather than claiming every Linux configuration.

| Interaction | Proposed target | Measurement boundary |
| --- | --- | --- |
| Warm local search | p95 <= 100 ms | Core query to first 50 conversation results, including grouping, matching-message snippets and filters |
| Search as seen by user | p95 <= 200 ms | Final input event to painted first results, including debounce and any cross-thread/process communication |
| Cold local search | p95 <= 500 ms | First query after a documented cold-start/cache protocol |
| Cached mailbox switch/conversation open | p95 <= 100 ms | User action to usable visible content for normal-sized conversations |
| Cached calendar view switch | p95 <= 100 ms | User action to visible events after bounded recurrence expansion |
| Triage action feedback | p95 <= 100 ms | Read/star/category click to locally durable acceptance and visible pending state; provider confirmation is measured separately |
| App launch into cached inbox | p95 <= 2 s | Process launch to usable cached list; excludes first import and auth |
| Scrolling | 60 Hz target with no sustained visible stalls | Frame timing while backfill/indexing runs; inspect tail frames |
| Idle resource use, local assistance off | <= 300 MB combined resident memory, < 1% average CPU | All app/rendering processes over a quiet 5-minute interval while open; no model inference or polling |

Long conversations and large/complex messages get bounded asynchronous rendering and progress rather than blocking the whole UI. Fetch and render conversation members incrementally without exposing a separate message mode. The memory budget is a starting hypothesis and must include any HTML-renderer/runtime subprocesses, even with a native shell. Measure native list/calendar interaction separately from HTML reader cost. After GUI close with no independent CLI session, verify that no GUI/engine/renderer process, sync request or reminder remains active. Also test an explicitly invoked CLI command/watch without the GUI and engine exit when that final client ends. Record the peak during import separately; set its release budget after phase 0 measurements.

Measure local assistance separately in disabled, cold-model and warm-model states. Report app plus runner RAM, GPU/VRAM or unified memory, CPU/GPU utilization, model disk size, input/output size, latency, throughput and incremental load relative to the runner's baseline. Do not claim the 300 MB off-mode budget includes model weights. Keep interactive search/scrolling/triage targets during active inference; throttle or pause background jobs to protect them. Set model-specific inference budgets after measurement, and verify cancellation stops the owning GUI/CLI session's runner job without terminating a shared service.

Do not impose a fictional whole-mailbox import time independent of provider quotas and bandwidth. Measure time to useful recent conversations, messages/text and attachment bytes per minute, quota consumption, disk amplification, projected completion and resume behavior. Include disk space for full attachment retention and temporary download/maintenance overhead. Define the recent-mail target after a real authorization and provider experiment.

## Evaluation method

Run representative queries with and without simultaneous synchronization, insertion, deletion, index maintenance and optional local inference. Include common subject/body terms with huge match sets, rare terms, sender names/addresses, partial sender input, current-account/category filters, long conversations and selected languages. Measure grouping and matching-message hydration, not only raw full-text query time. Report p50/p95/p99, repetitions, fixture version, build mode, hardware, cache state, and storage growth. A process restart alone does not prove the OS page cache was cold.

Use at least 100 observations per interaction class where practical. Save reproducible commands and reports when the harness exists. If a target fails, investigate query plans, result hydration, locks, indexing batches and UI work before replacing a library.

## Search usefulness

Collect 10–20 retrieval tasks covering the founder's confirmed needs: sender display name, sender email address, subject keyword and body keyword. Include a few exact identifiers and multi-message conversations. Attachment-content search is not part of the required MVP benchmark.

For known-item tasks, propose the desired conversation appearing in the first 10 relevance results in at least 90% of cases, with the actual matching message revealed on opening. Exact identifiers and explicitly matching filters should never be missed once their fields are indexed. Evaluate quoted-reply duplication and required languages separately; typo handling is a future scope decision. Show incomplete coverage whenever the underlying data has not been acquired or indexed.

## Reliability under load

Acceptance scenarios include interruption during initial import, expired provider cursors, remote deletions during backfill, concurrent edits from another device, low disk space, index rebuild during reading, lost submission acknowledgements, revoked calendar permissions, and daylight-saving transitions. Verify that unsent drafts survive restart and that stale index entries never expose removed-account data.

Add classification during manual/provider edits, renamed category destinations, interrupted IMAP folder moves in the later protocol phase, stale summaries after new replies, unavailable runners, cancellation and account removal during inference. Preserve reading position, manual corrections and unread/star counts. A model result must never silently revert a newer user choice or complete a held action.

## Triage and local-model usefulness

During the proposed daily-driver trial, evaluate whether the founder can review new mail, complete action holds and reach zero unread/zero starred within each account's agreed scope. Record remaining holds and why they remain without automatically clearing them. Verify that archived/category-folder unread mail counts, filtered-out work is still discoverable, and partial synchronization or pending writes cannot produce an unqualified all-clear. The daily zero goal is a workflow outcome, not a benchmark guarantee that incoming mail stops or every action finishes that day.

Build a labeled synthetic/consented-local fixture set with the configured categories, ambiguous mail, human replies from automated-looking addresses, newsletters, receipts, multiple languages and new messages within old conversations. Measure per-category precision/recall, abstention, invalid-output rate, manual-correction rate and incorrect filing separately. Model-reported confidence alone is insufficient to enable auto-filing. Choose and record acceptance thresholds before enabling automatic classification in the trial; validate Ollama and vLLM adapters against their actual models and supported platforms.

Evaluate summaries for factual fidelity, attribution to the included messages, preservation of key requests/dates, appropriate length, and explicit partial/stale coverage. Test text that contains instructions without allowing it to control application behavior. Original-text search must work identically with summaries disabled, missing or incorrect. Evaluate calendar search on organizer/attendee/location/title examples and known occurrence coverage separately from mail search.


## Required Google-first release evidence

Mail: separate account identity, correct conversation grouping, fast scoped search, human-only plain-text/HTML composition and signatures, full offline messages/attachments, reliable Gmail API retrieval/submission, durable drafts and uncertain-send recovery. Add accurate Unread/Starred filters and account totals, explicit action holds, manual/provider category persistence and view restoration. Filing, generated summaries and filter changes must not create a false daily all-clear.

Calendar: all four views, search/coverage, compact all-day rows, concurrent/current-next events, create/edit, recurrence exceptions/time zones, invitations/RSVP, shared permissions and running-app reminders. External read-only subscriptions have a later protocol gate. Verify Join opens the meeting link and acknowledges only that occurrence's reminder. Test conflicting edits and changes made in Google Calendar separately from ordinary CRUD.

Local assistance: disabled/unconfigured/manual operation, Suggest and configured Auto-apply, independent summaries, both runner adapters on supported local setups, correction precedence, unknown/invalid/stale output, source-linked partial/stale summaries, explicit historical batches, unavailable-runner fallback, no cloud execution, no composer access and verified cancellation on close. Set category quality acceptance thresholds from labeled fixtures before enabling automatic filing in the founder trial.

Platforms: prebuilt GUI/engine/CLI installation on Windows, macOS, Ubuntu, Arch and Omarchy, responsive interaction during import, recovery from interrupted work/low disk/expired cursors, and no GUI-owned work/reminders after close. An independent explicit CLI session follows its documented lifetime. Numeric targets are documented in [performance](quality.md); core scope is in [product scope](../product-scope.md).

Smooth installation, trusted signing where applicable, a maintained update route and dependency/license review are now MVP work. Broad public-launch/store work remains deferred; OAuth verification and any model redistribution requirements must be resolved for the actual distribution scope before release. Tasks, contacts, widget dashboards and broad assistant integrations remain outside MVP. Optional local classification/summaries do not authorize AI email writing.

## CLI and installation gates

For the first usable slice, test its useful partial CLI alongside the GUI; grow to complete applicable Google workflow coverage before the pre-legacy dogfood gate. Test JSON/schema compatibility, pagination/streams, exit codes, explicit noninteractive authentication errors, file/stdin payloads, operation IDs and partial outcomes. Compare GUI and CLI effects against the same core fixtures. Verify concurrent clients, cancellation, protocol mismatches and uncertain-send recovery without opening a GUI or requiring a display server.

Measure cold CLI command startup, warm-engine query latency, stream backpressure and engine memory separately from rendering; set numeric CLI budgets after the first prototype. Keep existing interaction budgets during concurrent CLI work and indexing.

Run installation, first launch, terminal command discovery, update, recovery and uninstall on clean target systems without developer tools or a model. Test actual Linux desktop/keyring/portal/sandbox behavior and a CLI-only environment; a container-only build test is insufficient. Record package sizes and first-run steps rather than promising tiny or dependency-free downloads.

## Open before claiming readiness

Agree the actual reference devices, account/provider inventory, corpus and language mix, inference model/platform matrix, quality thresholds and peak-import budget. Numeric budgets above remain proposals. An unrun test is not a pass. Record failed targets and any accepted revisions in [decisions](../decisions.md), and link reproducible evidence here when it exists.


Revision 4 records D012: retain native command/test results, synthetic corpus definitions, search percentile/cache context and explicit unverified platform/live-provider gates. All workstreams consume this evidence rule. No small experiment promotes a complete feature to Verified.

## B3B Windows functional checkpoint — 2026-09-13

The [D033 execution record](../research/phase-2-execution.md#p2-05b3b-explicit-remote-draft-actions-under-d033) and immutable 337-artifact archive record 512 workspace tests/five ignored, 146 isolated engine unit tests plus three process tests, paired native/CLI draft actions, the 104-source remote Drafts regression, existing draft/Send regression and actual schema-15→16 upgrade/refusal/reopen. Native CTests pass 17/17 in 192.37 seconds, with 139.167-second integration under D030 and unchanged primary/child watchdogs. Seven delegate notices remain visible. All 331 source inputs, 207 Rust inputs and 20 selected executable identities are rechecked. These are functional test durations; no new latency, resize, physical-input, package, live-account or warning-free acceptance follows. D031 preserves the failed B2 performance result and defers remeasurement.


### P2-01B functional evidence under D036

The [immutable Windows archive](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json) binds **467 artifacts**, **356 runtime inputs**, **230 Rust inputs** and **22 executable identities**. Final workspace03 passes **547 tests, five ignored** with strict Clippy/format checks; isolated engine02 passes **156 unit + four process**, and feature CLI02 **five unit + 15 process**. Paired CLI04/native06 lifecycle runs, actual schema-17→18 upgrade/refusal/reopen, existing mail/Calendar regressions and final native CTest02 **18/18** pass. Native CTest time is **196.13 seconds**; integration **138.250 seconds** uses D030's unchanged 180-second aggregate allowance. Seven delegate notices plus one teardown notice, all failed attempts and the corrected test fixture are retained. This establishes scoped Windows functional behavior with generated providers/in-memory credentials, not actual OAuth grants/native credential storage, current packages, performance, physical input or complete Phase 2 acceptance. D031 still defers measurements and readiness remains **0/9**.

### P2-06C functional evidence under D037 — 2026-09-14

The [immutable Windows archive](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-06c-interim/logs-manifest.json) binds **343 artifacts**, **371 runtime inputs**, **240 Rust inputs** and **23 executable identities**. Workspace01 passes **574 tests, five ignored**, strict lint and format; isolated engine passes **161 unit + five process** and CLI **five unit + 16 process**. Paired occurrence CLI02/native01, all existing mail/Calendar/account regressions and actual schema-18→19 upgrade02/refusal/reopen pass. Native CTest01 passes **19/19** in **200.02 seconds**, including **139.370-second** integration under D030's unchanged 180-second aggregate allowance. The primary 120-second and child 45-second watchdogs are unchanged. Six delegate notices and one teardown notice, earlier failed attempts and narrower historical source scopes remain retained. Exact source identity, metadata preservation and generated recovery behavior are verified within this scope. D031 still defers navigation/resize measurement; no real-provider, native-vault, current-package, physical-input, warning-free or whole-area acceptance follows. Remaining Calendar/partial-grant work and readiness **0/9** stay explicit.

## D052 / P2-LLM consumer boundary

Revision 30 consumes [Local assistance 6](local-ai.md): shared discovery/settings/jobs/results/correction commands, schema-21 storage and manual precedence, bounded owned Ollama work, native/CLI parity and scoped acceptance. Scoped Mac implementation checks pass: eight shared assistance tests, focused Core plus 17 schema checks, strict lint, native Settings/reader harness, synthetic engine/typed CLI including label acceptance, actual schema-20→21/refusal/reopen, local Ollama samples/cancellation and the configured relocated package. The seven-case model trial yields 7/7 sender kinds and 5/7 exact label sets; the two false-positive label sets remain recorded. See the [execution report](../research/local-assistance-execution.md#d052--p2-llm-mac-implementation-2026-09-14) for exact source/package scope, failed attempts and remaining gates. Fixtures 0/3 and protocol 1 remain unchanged. Earlier evidence retains its original revisions; whole readiness stays 0/9.


## D053 summary presentation and control repair

The reader follows Update 2’s bounded Summary card below thread metadata: 6-pixel radius, subtle assistance-colored outline, 2-pixel left accent, 14/18/15-pixel padding, compact uppercase heading and right-aligned model attribution. A message summary appears beside that message. Source/provenance and re-summarize controls are secondary; long output remains scrollable without displacing all original mail. All model output remains escaped/plain text. Empty, busy, failed, stale and partial states are distinct.

Summary enablement alone exposes both thread and individual-message actions, including retained sent messages. Classification/correction targets only received messages. Requests carry current account/conversation and generation identity; late start replies are canceled, old polls cannot overwrite a newer job, start/cancel admission is immediate, and disconnect ends the local busy state. Source navigation reuses the reader’s exact member expansion/scroll mechanism. Settings preserves edits on refresh/conflict, offers explicit saved-settings reload, ignores obsolete discovery/sample responses and displays readable sample results. Existing session ownership and on-demand summary semantics remain unchanged.

Verify these with native controls and rendered frames, focused engine/CLI tests and the actual local runner; retain original failures. This packet does not expand automatic inference or change schema 21, fixtures 0/3 or protocol 1.

D053 final scoped evidence: [native09 and configured Package03](../research/local-assistance-execution.md#d053-summary-ui-and-lifecycle-repair-2026-09-14) pass on the combined D053/D054 runtime, with Release engine/CLI and real synthetic Ollama summary checks. Three theme renders were inspected; cancellation, pending/obsolete responses, independent summary controls/settings and exact source targeting are covered. Preserve all preceding failures and package identities. No whole-area or other-host acceptance is inferred.

### D061 Windows checkpoint

The [prototype execution record](../research/prototype-september-15.md#d061-label-management-final-scoped-checks-and-windows-candidate) records passing local label lifecycle/membership/provider/session tests, actual schema-25-to-26 migration and refusal, native controls/captures, strict scoped lint, 17 synthetic local-provider integration checks and the configured relocated Windows candidate. Raw failed attempts and package identities remain in ignored verification storage. Full prototype coverage, rendered reference comparison, actual provider/model quality, performance and other-host qualification remain open.
