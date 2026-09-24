# September 23 prototype implementation

## D112 — seventh export (2026-09-24)

The founder supplied the seventh export, `fuckMail-7.html` (747,584 bytes, SHA-256 `32c362c08d929983d276aa44c2b6eda1f988558aed470b37d102820985072d29`), with designer notes on account progress, colours, labels and the Local LLM page. The export's bundled component and markup were unpacked. Each note was matched to the component's `pipeV`, `pipeCard` and `pipeTab` logic and its Accounts, hover-card, colour-picker, labels-modal and Local LLM markup. The export was served on `127.0.0.1` and opened in the in-app browser at 1440×900. The sixth export is not on this Mac, so this round follows the designer's list of changes rather than a byte diff.

### Engine support

- **Progress.** `assist_progress` returns cheap per-account counts from indexed COUNT queries. It has no snapshot validation, so it is safe to poll every three seconds:
  - `local` and `eligible` messages;
  - `done`: results saved for every enabled task;
  - `queued`: what the arrivals watcher still has ahead of it, respecting the history cutoff and pause, plus what a running Catch up has left;
  - `unprocessed`: the rest;
  - `seconds_per_message`: the engine's running average of real analysis times.

  The exact per-range scan now runs only when Catch up opens.
- **Pause.** A per-account model `paused` flag is saved in the assistance settings, and resuming requeues the account. Pausing an account's sync holds its model work in this engine session, and resuming sync releases the hold and requeues. Both stages therefore pause and resume together, as the design asks, and sync's own session-only pause is unchanged.
- **Colours.** Account colours are a palette index in `meta`. A new account gets the first unused colour, `accounts` reports `color`, and `account_update` accepts `color`. Storing them in the shared preferences was tried first. That bumped the preference revision whenever an account was added, which made 40 Core tests expose a real conflict for clients, so colours moved beside the account.
- **Test.** The sample test reports Ollama's load and prompt time (time to first token), output tokens, generation time, rate and total time, and the raw reply when it is not valid JSON. The OpenAI-compatible path reports tokens and total time only.
- **Prompt variables.** `{{labels}}` and `{{partial}}` may be left out of the user message; `{{messages}}`, `{{model}}` and `{{source_ids}}` stay required.
- **Calendar.** `accounts` gains `calendar_events`.

### Choices beyond the notes

- **No invented totals.**
  - Mail shows "N of M" from Google's listed inventory. While the list is still being counted it shows "M+" and "Counting mail…", with no percentage or estimate.
  - Google reports no event total, so Calendar shows its count. A first pass shimmers until it finishes, then shows Complete.
  - Estimates use rates observed over the last minute for downloads, and the engine's average for the model. With no rate yet, no estimate is shown.
- **Catch up** unpauses the model, as in the export. The model row's pause also stops a running Catch up; finished results stay.
- **Kept.** The Mail page keeps Signatures and Remote images, which no export ever had, below Blocked domains. Archived accounts keep their archive, cleanup and restore actions on the card.
- **Removed.** The Shortcuts and Local LLM explainers are gone, and `AccountAssistanceSettings.qml` is deleted: its Summarize and Label controls moved to the card.

### Verification

- Mac release build. New Core tests cover:
  - the pause and its requeue on resume;
  - progress with the history cutoff;
  - palette colours, including that adding an account leaves preference saves valid;
  - archive deletion clearing an account's colour;
  - optional prompt variables.

  Engine tests cover Ollama timing extraction and compatible-server token usage.
- Native tests updated or added:
  - Accounts shows cheap counts with no scan, and only Catch up scans.
  - A Catch-up batch shows as queued work, and pausing stops it and saves the pause.
  - The colour picker saves through `account_update`, and the labels modal opens from the card.
  - Local LLM saves a model at once and each box on its own, with Restore default, the missing-variable warning, the size note and the Test panel.
  - The top-bar hover card is 300 px and takes no input.
  - The header sync label is icon plus time.
  - The integration walkthrough opens the account editor from ⋯.
- Offscreen captures compared with the export:
  - the Accounts card in the Terminal theme, including downloading, Catch up and running states;
  - the colour picker and the labels modal;
  - the hover card.

  The real Mac app rendered the Accounts, General, Mail and Local LLM pages on a demo profile.

### Limits

- Mail and Calendar meters were checked in the native harness only, not against a real account's first sync.
- The account-tab ring was not captured running.
- "Paused N ago" knows only pauses made in this app session.

## D104 — fifth export (2026-09-24)

The founder supplied the fifth export, `fuckMail (5).html` (713,629 bytes, SHA-256 `b5cfd8b3…c0722b2ef`), which includes the fourth, with designer notes. Its bundled component and markup were unpacked and diffed against the fourth export. The export was served on `127.0.0.1` and inspected in the in-app browser at 1400×860 for the reader, summary states and Calendar Month. Sizes, colours and rules come from the export's markup and the notes. The changes are in the [decision](../decisions.md) and [Mail UI73](../contracts/mail-ui.md).

### Choices beyond the notes

- **HTML reader boundary.** The founder asked for the link card, image blocking and Blocked domains UI without their logic, which another session is building. The card, the Settings list and the reader hook-up call that session's content service (`backend.htmlContent`) for matching, blocking, counts and hosts; nothing here fetches, blocks or proxies. Without the service, the card offers only Copy link and the Settings field is disabled.
- **Real failures only.** The prototype fails every third summary to demonstrate the red state. Here the red state appears only when the engine reports a failure; a timeout reads "{model} stopped responding", other errors show the engine's message.
- **Done needs a watched run.** "✓ Done" shows only when a summary arrives while the card was showing progress, not when a saved summary loads on open.
- **First frame of HTML messages.** The HTML view loads asynchronously. The HTML session now answers held results synchronously and remembers laid-out heights, so a revisited message has its final height on the first frame; a first visit appears a frame or more later. The navigation-cache test now checks the first frame on the text reader; the HTML reader's first paint is that session's to test.

### Test repairs

The founder asked that tests stop failing routinely. Each failure was traced to either a real bug or an outdated check:

- **Real bugs fixed.** A queued Send's draft check was pruned by navigation and never answered (send-undo). A view save meeting a newer CLI save raised an error and stopped closing; it now merges quietly (integration). Recipient errors had no visible place after the top error bar's removal. Collapsing a list section could scroll its header out of view, and saved list offsets drifted with the list's origin.
- **Checks updated to current behaviour.** The attachments dialog's "OK" is now "Reload". The Forward button is hidden while a reply is open, so the composition test collapses the reply first. A restored editor taller than its scroll area is checked by its visible area. The Calendar header timeline no longer needs blocks. Google draft and Send harnesses answer the new `google_sent` read. Selection receipts carry categories, and its section toggle passes the header. The sender card opens All mail. An unreadable search raises no app-wide error. The local-model harness includes the header Summarize icon. The integration walkthrough checks polling before a merged close instead of expecting a failed close. The selection test reads the visible body when the HTML view and its text fallback share a name.

### Verification

- Windows 11, Release build (`build/d100`), on the shared working tree that also holds the HTML reader and diagnostics sessions' in-progress changes. The full native suite passes **36/36** in 432.65 s, including `rust-test-build`, `rust-workspace` and `gui-startup-recovery`.
- The previous full run failed 2/36. `gui-startup-recovery` built with `-p fm-engine`, whose feature set differs from the `--workspace` prebuild, so it recompiled fm-ipc and fm-engine inside its 90 s limit on every run; its command now uses `--workspace --test sessions` and passes in about 10 s. The other failure was the HTML session's own `gui-html-message-body`, which passes on the current tree.
- The Send-controls harness change, made after that run on the HTML session's report of a Debug-only failure, was rerun alone and passes.
- Visual checks compared offscreen fixture frames with the served export: Month's today tint and ring, dimmed past days and events, the now line with a two-event window and "+21 more"; the reader's sender pill, avatar, attachments caption and text-reader links; Settings → Blocked domains; and the local-model harness's progress, "✓ Done" and failure frames. The comparison led to four fixes: text-reader links were blue and are now ink with an underline; `*.example.com` uses the export's smaller monospace size; green now appears at once and only the return fades; and the failure line no longer repeats the engine's "Try again".
- Repository hygiene passes. Strict Clippy (`--workspace --all-targets -D warnings`) found three D101 issues, now fixed: a non-minimal boolean in Done-month collapsing, a literal-bool assert, and the CLI's `mail list` variant size, allowed because the command is parsed once. It then passes for fm-core and fm-cli, but still fails in fm-engine's local-model files, which another session is changing (too many arguments; `map` over `inspect`). This packet's Rust files are formatted; other sessions' files show formatting differences.

### Limits

- Windows only. macOS and Linux were not run.
- The link card, image placeholders and Blocked domains depend on the HTML reader's service and have no end-to-end test here.
- A message's first HTML visit appears a frame or more after the reader; prefetching it during reader preparation is not done here.
- The Month now line and event window are checked visually, not by a native test.

## D101 — fourth export (2026-09-24)

The founder supplied the fourth export, `fuckMail (4).html` (682,446 bytes, SHA-256 `9b2d25be…76d59cf`), with designer notes. Its bundled component and markup were unpacked and diffed against the third export; `support.js` is unchanged. The optional Claude Design import was not needed. The export was served on `127.0.0.1` and inspected in the in-app browser at 1400×860 for the search dropdown, Inbox sections and Calendar Day/Week/Month. Exact sizes, colours and rules come from the export's markup. The changes are listed in the [decision](../decisions.md) and in [Search 7](../contracts/search.md), [Core56](../contracts/core-storage.md), [Mail UI71](../contracts/mail-ui.md) and [CLI43](../contracts/cli.md).

### Choices beyond the notes

- **Core reads the query.** The prototype parses search text in the page. Here Core parses it once for GUI and CLI and returns its reading, so chips always show what actually ran. A removed chip rebuilds the text from that reading. While the next result is pending, the last reading and count stay on screen so chips do not blink with each key, and chips cannot be removed until the new reading arrives.
- **Sent matches one recipient.** For received Gmail messages Core stores only the first `To` address, so `from:` in Sent matches that address, not names or other recipients. Drafts match every recipient name and address.
- **Old filters.** A saved view's sender, status and attachment filters become query text on restore, so nothing filters the list unseen. Date filters stay for Sent's day chips and are dropped elsewhere.
- **Bare words no longer match senders.** The notes define bare words as subject or body. The native navigation test's sender-name search now uses `from:`.
- **Band cap.** The prototype grows the header band for every all-day event. Here it stops at four rows; a busier day shows three chips and "+N all day", which opens the existing list.
- **Day view's week.** Day view presents one day, so the backend lays out the anchor week's all-day events from the same cached six-week read to size the band. If only the day's own read is available, the band follows that day.
- **Ghost without a writable calendar.** The ghost is hidden when no calendar accepts new events, since a click would do nothing.
- **Months in the local zone.** Done months use this computer's zone, the same zone as row times; Core counts them with the zone the GUI sends.
- **Unreadable queries stay local.** A query Core refuses, such as `is:snoozed` or a fifth `from:`, shows Core's message in the dropdown and keeps the current rows. It no longer raises the app-wide error banner.
- **Sender card.** The prototype has no sender card. Its filter action now puts `from:<address>` in the search box and opens All mail, the scope the card's count covers, replacing the query instead of narrowing it.

### Verification

Checked together with D104 on the same tree: see [its verification](#verification). The Search, Core and CLI changes pass in `rust-workspace`; the native search, navigation, Calendar controls and selection tests pass in the 36/36 suite. During implementation, offscreen native frames were compared with the served export for the search dropdown, Inbox sections, the top bar and Calendar Day/Week/Month.

### Limits

- Windows only. macOS and Linux were not run.
- `from:` in Sent matches only the first `To` address.
- The old date and status filter popover is gone; dates remain only as Sent's day chips and Calendar's mail counts.
- At most four all-day rows show in the header band.

## D099 — GUI memory (2026-09-24)

The founder asked for lower GUI memory without making the app slower. The audit ran the optimized Windows build on disposable demo profiles and broke memory down by region. Most private memory was the Windows heap holding QML objects, not textures or images. Per-object probes found the largest avoidable costs:

- Settings, with all its pages, was built at startup and kept alive while hidden.
- Every line icon owned a `Canvas` and its texture, about 53 KB each, even when the image was identical.
- Every `FmButton` built the styled Basic ToolButton plus a `Row`, icon and hint slots: 53.5 KB each. The new button costs 37.2 KB with a text label.
- Reader caches kept each message's raw source text, which only the source dialog shows.
- Each date field built a hidden month grid, and the attachment viewer built its media player before any video was opened.

The changes are listed in the [decision](../decisions.md) and in [Mail UI70](../contracts/mail-ui.md).

### Measurements

Windows 11 25H2, x86_64, Qt 6.11.2, Direct3D 11, 1.25 device pixel ratio, optimized build. Each view opened on a fresh demo profile with the fixture clock and was measured after 20 seconds. Three runs per view; medians shown. The baseline is the D098 tree without these changes.

| View | Working set before → after | Private bytes before → after | Resident heap before → after |
| --- | --- | --- | --- |
| Mail | 269.1 → 220.6 MB | 284.3 → 200.8 MB | 165.7 → 121.6 MB |
| Calendar | 209.2 → 164.7 MB | 250.0 → 191.8 MB | 134.2 → 93.6 MB |
| Settings | 200.6 → 162.2 MB | 243.1 → 169.2 MB | 127.5 → 92.5 MB |

Settings is now built on first open, so the Settings row includes it and the page it opens on; other pages are built when first visited and then kept. Mail and Calendar no longer carry Settings until it is opened.

### Startup and Settings

A scratch probe, added only to otherwise identical builds, timed startup to the first frame, the first Settings open and each page's first visit on a fresh demo profile (Direct3D, three runs each). Frame times run from the request to the first swapped frame.

| Measure | Before | After |
| --- | --- | --- |
| Startup to first frame, median | 1,057 ms | 901 ms |
| First Settings open | 9–12 ms | 23–33 ms |
| First visit: Accounts | 7–13 ms | 23–31 ms |
| First visit: Local model | 20–27 ms | 15–28 ms |
| First visit: Mail | 14–18 ms | 15–25 ms |
| First visit: Calendar | 5–9 ms | 9–14 ms |
| First visit: Shortcuts | 6–12 ms | 20–35 ms |
| First visit: Diagnostics | 7–12 ms | 9–11 ms |

Later opens and visits are unchanged. Building the whole Settings view on first open took about 100 ms, so each page is now built when first shown. A background build started by hovering the Settings button was tried first. Qt advanced it only while frames were drawn, so it often had not finished by the click.

### Interaction speed

Both builds used the same engine and CLI. Each route has 100 measured native inputs; p50 / p95 shown. The benchmarks need their window to keep focus. The founder's desktop was in use, so several Direct3D runs lost activation and were discarded. A scratch copy of the verifier that changes only the platform (offscreen) and renderer (software) measured the same routes. Software rendering is slower than Direct3D, so only the comparison matters; it puts both builds' Calendar view switch over the 100 ms budget, which applies to the Direct3D build.

Calendar, Direct3D (`verify_interaction.py --mode calendar`), one of three complete pairs:

| Route | Before | After |
| --- | --- | --- |
| Day/Week/Month switch | 25.5 / 46.8 ms | 25.4 / 49.4 ms |
| Month day click | 11.7 / 23.9 ms | 9.6 / 19.6 ms |
| Week next/previous | 6.5 / 7.8 ms | 6.4 / 7.8 ms |
| Month next/previous | 7.4 / 9.7 ms | 6.6 / 10.3 ms |

In all three pairs every median was equal or faster, and the view-switch p95 was 2.6–5.7 ms higher (63.4 → 66.1 and 61.5 → 67.2 ms in the other two). Variation between runs on this desktop was larger: the unchanged baseline's week-step median ranged from 6.5 to 16.4 ms. Two off-screen pairs, run in both orders, did not repeat the view-switch difference:

| Route | Before (two runs) | After (two runs) |
| --- | --- | --- |
| Day/Week/Month switch | 95.0 / 110.6, 90.5 / 113.7 ms | 98.1 / 107.0, 93.5 / 106.3 ms |
| Month day click | 52.5 / 67.9, 54.2 / 69.2 ms | 50.4 / 69.3, 50.4 / 72.7 ms |
| Week next/previous | 7.0 / 8.5, 7.7 / 8.7 ms | 7.7 / 8.7, 7.3 / 11.1 ms |
| Month next/previous | 11.3 / 15.8, 13.0 / 15.6 ms | 12.4 / 15.8, 9.9 / 12.9 ms |

Mail navigation, off-screen (`--mode navigation`), the only complete pair of final builds:

| Route | Before | After |
| --- | --- | --- |
| Mailbox click | 68.5 / 115.5 ms | 65.4 / 76.9 ms |
| Mailbox Tab | 66.5 / 78.3 ms | 65.9 / 74.4 ms |
| Mailbox Shift+Tab | 67.9 / 114.4 ms | 64.1 / 72.7 ms |
| Conversation click | 23.7 / 64.9 ms | 23.4 / 46.1 ms |

In a second pair the new build measured the three mailbox routes before it stopped at a conversation click (below): medians 67.0/69.1/68.3 ms against 71.0/67.3/70.7 ms, p95 75.2/104.9/116.8 ms against 128.9/86.4/135.6 ms. One Direct3D navigation run of the new build completed (p95 126.0/138.7/138.3/65.7 ms); its baseline lost activation, so it is not compared.

The first implementation drew icons through an image provider. Off-screen, its mailbox-switch medians were 5–35 ms slower than the baseline in two pairs, because each icon cost more to create than a Canvas and requested its image twice. The native icon item replaced it.

Two of three off-screen runs of the new builds, one of them with the image-provider icons, stopped at a conversation click that opened a different conversation, 088 instead of 004. Opening the previous conversation marks it read, and the list refresh that follows reorders rows. The benchmark starts the next click 25 ms after the correct frame without waiting for that refresh, and the new builds paint the reader sooner. Five baseline runs did not stop. D099 does not change the list model, pinning or row clicks; the race is recorded for separate investigation.

### Visual parity

- Icons: all 56 icon names at eight sizes were drawn by the old Canvas and by `FmIconItem` at 1, 1.25, 1.5 and 2 device pixel ratio, with an opaque and a translucent colour. Fractional positions were checked in dark and light. At 1, 1.5 and 2, and at every fractional position, the images are identical. At 1.25, 4 of 456 cells differ by at most 2 of 255 on up to 13 pixels: the Google mark at 11, 13 and 15 px and `more` at 14 px.
- Buttons: 30 button configurations (text, icon, leading and trailing icon, shortcut hint, left alignment, narrow widths) match the old button pixel for pixel at 1, 1.25, 1.5 and 2 device pixel ratio, rechecked with the final icon item. Fractional heights were checked at 75 heights with the earlier image icon.
- App frames: 14 fixture-clock captures (Mail, reply, reply footer, four Calendar views, seven Settings pages) were compared. On this day two captures of the same build differed by 1 of 255 on about a thousand pixels, so frames were compared ignoring 1-level differences; at that tolerance repeated captures match. The seven Settings pages are identical. Each Calendar view differs in 1 pixel and Mail in 2 pixels, by 2 of 255. Both reply captures differ in 11 pixels, at most 31 of 255 at the tip of the Attach paperclip. The paperclip difference also appeared with an image-based icon and was not reproduced outside the app; it is not visible at normal size.

### Verification

- The release GUI suite passes 28 of 34. The same six tests fail with the same messages on a build of the same tree without this change:
  - `gui-integration`: search timeout at stage 81.
  - `gui-attachments`: newer-draft conflict at stage 98.
  - `gui-send-undo`: one request only after the real six-second window.
  - `gui-mail-selection`: the checked label submenu toggles the wrong way.
  - Both Google draft authoring and snapshot tests: an unexpected operation method.
- `gui-mail-selection` stops before its source-dialog and account-settings checks. A scratch copy that only logs the known label failure passes the rest of the test on both builds, including the source dialog that now reads raw text on demand.
- Timing probes built 30 list rows or 30 buttons and painted the first frame, alternating old and new versions. The native icon and new button build and paint faster than the Canvas icon and old button (for example, rows: 1.6 → 1.1 ms to build, 8.3 → 6.1 ms including the frame, off-screen). An earlier image-based icon built slower than Canvas and requested every image twice, so it was replaced.

### Limits

- Measured on Windows only. macOS and Linux were not run.
- On Direct3D 11 a video tile keeps its poster frame after its decoder is destroyed. This was checked only on Windows; the software renderer used by native tests never shows video.
- The first Settings open and each page's first visit now include that page's build time, up to about 25 ms more than before; see the table above.

## D094 — Flight Progress v2 detail review

The Claude Design import required `/design-login`, which this session could not complete. The review therefore used the local third export `fuckMail (3).html`: 666,474 bytes, the same file as D092. The export was served on `127.0.0.1` and captured with headless Chrome over CDP at the native window size. Mail, thread, composer, Calendar day/week/month, event details, event editor and each Settings page were compared with offscreen native frames. The native frames came from disposable demo profiles with a fixture clock. Comparison was by side-by-side crops, not a pixel diff. Captures and scripts stay in session scratch storage, not Git.

Findings fixed:

- **Type:** Qt pixel sizes are integers, so 12.5/11.5/13.5/10.5px text rendered at the wrong size. Fonts now use point sizes from a measured points-per-pixel ratio.
- **Line boxes:** Qt fixed line heights put the extra space below the glyphs, while CSS centres them. A shared top/bottom padding correction now centres each line. Line heights round like Qt so geometry stays whole.
- **Thread header:** the summary listed the user and used commas only. It now follows the export: sender, then Cc, "and" before the last name. Own avatars use the account's name. The latest received message uses relative/weekday/date time.
- **Sync label:** it now reads "Syncing…" / "Synced N ago" / "Retry sync · N ago".
- **Calendar:** segmented Day/Week/Month control and date navigator with chevron. Day headers and all-day chips fixed. The rule sits 16px below the headers in day view with no all-day row, otherwise 10px. Hour labels 10.5px; triangle now-marker. Event blocks use a square account patch with time, title and detail on one row. Month cells draw their own grid lines, with hover and "+ N more".
- **Event details:** range line (amber when current), place/video/people rows, conflict ring and relative start.
- **Event editor:** same-day end, Repeat after Ends, Notes before Remind, compact Remind and Show as, and 32px footer buttons.
- **Settings:** capitalised label table, count after name, reference Shortcuts layout with hint and Reset all link. Monospaced prompt editors, Google account marker, date grouping labels, and version footer.
- **Composer and Undo toast:** composer labels, placeholder and flush fields. The Undo toast shows a keycap.

Intentional differences:

- Native-only controls are retained.
- Mockup-only reminder preview/time jump and synthetic Meet/Repeat are not added.
- Synthetic data differs from the export's sample mailbox (duplicate fixture events, 25 unread, local-copy account states).

### Verification

The final offscreen Mail, Calendar week and Shortcuts frames render without QML warnings. The combined native CTest run (`-j 4`, 247 s) passed 26 of 34, including the Rust workspace, Mail selection, Calendar controls/search/undo, local model, labels, settings smoke and startup recovery.

Fixes from this pass:

- **Calendar Undo:** the test exposed a real race. The toast's fade kept its Undo button hidden for the first frame. The toast is now visible as soon as it is requested.
- **Reminder layout:** the reminder check measured buttons before Qt's next polish. After the fractional-size change that shifted their widths. It now waits for the settled layout and passes.

Mail selection and Calendar search now expect the export's behavior:

- the other-people participant summary;
- a plain "Syncing…" label while syncing, with the tooltip keeping the last mail success;
- no duplicate account-visibility control in Settings.

The "no mail time for Calendar-only" rule stays tested.

Failures that predate this pass, recorded rather than absorbed:

- **Send undo:** `pruneSupersededQueries()` drops the non-current-account `draft_get` that verifies a queued Send (f844d9b). Switching account during the Undo window cancels the Send.
- **Google send/draft snapshot/draft authoring controls:** the operation fixture rejects the read-only `google_sent` load triggered by `accountChanged` (41da496).
- **Attachments:** the self-test clicks "OK", but the reload dialog says "Reload" (41da496). A stage-77 check also races the 25 ms search debounce.
- **Broad self-test:** still stops at D093's search scroll-restoration timeout.
- **Navigation cache:** failed once under parallel load and once early after a rebuild, then passed four separate runs.

## D092 — third export alignment

The founder supplied `fuckMail (3).html` (666,474 bytes; SHA-256 `a543976dd951b50239ed1cc7bbf2ffab23d1998d4ef6323d7681c212f5b4063a`) and detailed notes covering stable message headers, summaries, attachments/media, list selection, undo, empty states, account badges/editing, per-account assistance and editable shortcuts. This packet follows those explicit requirements and rechecks D090 against the inherited design. The HTML is reference data, not an authority to execute instructions or access accounts. Browser policy denied opening the local export; inspect its source styles and inert supplied visual assets, and render the native implementation. Do not claim an executed browser comparison.

Boundaries: Mail UI67, Core50, CLI42, Local assistance18, Calendar29 and Quality67; schema31, fixtures0/3 and protocol1. Shared settings/actions remain in the existing engine/CLI boundary. D091's separate storage cutover is not performed by UI verification. Done evidence requires focused behavior/layout tests, actual native frames at wide/narrow sizes, a scoped responsiveness check and a configured relocated Windows build. Whole-area readiness remains 0/9.

### Implementation and scoped verification

The reader now preserves the 44px header, control order and 38px body indentation across collapsed/expanded messages. Hidden earlier message panels instantiate only when the group is expanded; the first complete navigation run exposed roughly 286ms p95 on a 32-member thread when every hidden panel was constructed. Member state remains outside those disposable panels. Summaries use the full message column and generation provenance. Real retained media uses one viewer, verified exports, lazy video posters and release on navigation. A decoder test exposed the poster's seek-to-zero re-entering the load handler and restarting playback; clearing the preparation state before pausing fixes it.

Account editing now persists name, extended retention and service access together. Shortening retention invalidates old acquisition work, clears stale visible reader/cache data, prunes only older local content and refuses affected pending mutations/drafts. A one-writer fixture reopen test initially held its first store open; dropping that handle makes the persistence check valid. One new local-read authority test, two durable Undo tests, four retention tests and one shortcut validation test pass. Schema30→31 upgrade preserves exact revision/signature/retention/cutoff on a valid disposable account; reopening succeeds and the older schema30 engine refuses with `schema_mismatch`. The first migration fixture lacked its required Google binding and was rejected as an invalid live profile; it was replaced by a properly bound synthetic account. No real profile was edited.

Final native suites pass independently: Mail selection/reader/attachments/account editor/shortcuts (16.44s, including actual old-key/new-key activation and real Motion JPEG playback/pause/seek/mute/source release), Calendar controls (19.48s), reminders (1.08s), and Google connection controls (0.89s). Local-model controls pass in 9.96s. A combined CTest invocation retained three Windows DLL-loader failures; separate fresh invocations pass. Earlier pointer tests omitted real mouse movement and a stale tooltip intercepted clicks; the test now sends the native move/press/release sequence and waits for actual controls. Native checks use disposable synthetic mail and never send mail or invitations.

The earlier navigation oracle incorrectly required a displayed HTML alternative to equal its plain alternative. It now compares the current source-version prepared HTML document to actual `QQuickTextDocument` text and retains independent plain-text checks for plain mail. The first corrected software-rendered run measured all 400 exact account/message frames, with click/Tab/Shift+Tab/conversation p95 of 113.8521/112.5383/122.1833/285.9588ms, failing the unchanged 100ms target. Failed direct build-tree launches lacked deployed Qt libraries; their task-owned processes were stopped and subsequent runs use the self-contained package. Failed benchmark JSON/logs remain in ignored `build/d092-evidence`; other failed attempts are recorded above and in the task transcript. No partial or failed run counts as a pass.

The lazy-member change reduces software-rendered long-thread p95 to 62.8085ms in the next complete run; account click/Tab/Shift+Tab become 109.7354/85.2836/95.22ms. That run still fails one target. Subsequent review removes an unnecessary empty-list publication immediately before cached account publication, and replaces deferred source callbacks with an object-owned timer so destroying a reader cannot leave a callback against its deleted QML context. Once shown, hidden message panels retain their per-message format state until conversation navigation. The native navigation/cache suite passes (16.40s), including source identity, rapid paging, stale callbacks and first-frame geometry. Its idle-frame helper now calls `QQuickWindow::update()` to request actual rendering; `requestUpdate()` alone could stop producing frames after idle animations ceased.

Configured relocated empty-live startup and actual Mail, Calendar week, Mail settings and Calendar settings screenshot modes pass. Inspected frames include all three themes, narrow reader/account layouts, actual media cards/viewer, account editor, shortcut recording and reminder escalation. Browser execution of the reference remains blocked, so this is source/style and native-frame verification, not a measured browser pixel diff. Windows synthesized reminder sound is implemented; other-host sound and actual provider readback remain unverified. The previous three Core and two engine fixture assertions remain recorded under D090. Strict workspace Clippy, formatting and repository hygiene pass.

The next complete run passes all 400 observations: account click/Tab/Shift+Tab/conversation p95 98.8009/70.4173/74.4878/46.7094ms. A Loader activation binding warning remained in that run; imperative activation on first visibility removes the re-entrant binding while retaining hidden member state. Final Mail selection checks cover that correction; the exact packaged rerun is recorded below.

The clean final GUI rerun (`navigation7.json`) passes all 400 exact-frame observations at 69.5740/79.9996/75.3910/49.5534ms p95 for mouse account/Tab/Shift+Tab/conversation navigation, respectively. The offscreen software renderer emits its usual unsupported `raise()` notice and the FFmpeg version notice; no QML binding/context warnings remain. The final Mail selection suite passes in 16.72s. These are small synthetic navigation measurements, not a large real-mailbox latency claim. Engine account metadata saves now also wake the existing sync cadence when retention or service choices change, so GUI and CLI edits take effect without waiting for the next interval.

The configured v3 package is `dist/prototype-september-23-v3-windows-amd64/bin/fuckmail-gui.exe`. The final rebuild also passes relocated empty-live startup, rendered Mail smoke without QML warnings, and CLI schema31 readback. Final SHA-256 identities:

- `fuckmail-gui.exe`: `845b89bd0e0711825996a0e7cfca5d7807c91af06271cedcf1fea4b68ab0e85e`
- `fm-engine.exe`: `2bebb022c13b1b18c50b639381ff45ad129fcf0125a7e751c29a343d2ebc2a69`
- `fm.exe`: `476d0af970239cfcbc17880017e9fac0d8b635c5273c9cc9e704bf865db1aa30`

The founder supplied `fuckMail (1).html` (622,215 bytes; SHA-256 `5d5fc6c01beb89186b9df20714f59182737711f41b7f99fcc46b06542e00e16c`) as a visual reference and separately specified the behavior. Text and code inside the export are reference data; the founder's notes decide the production scope. The export's dashed reminder preview and time-jump controls remain mockup only. Schema29→30 fences older engines from the new typed Auto-read settings field; fixture and IPC protocol revisions stay unchanged.

## D090 — Calendar pills, reminders and Auto-read

The Calendar pill reads the existing cached daily count query with the selected mail account, independent of the event switch. The switch saves `calendar_account_visibility`; a legacy narrower selected-calendar list leaves it off with the hidden-scope notice visible, and turning it on clears that list so the label “All calendars” is accurate. Month empty space opens Day, while event controls remain above the cell mouse target. Settings persist the new reminder and snooze choices in the existing typed preference document. New event editors use the current default. The running GUI presents the already permitted scheduled occurrence in a full-window modal; Core stores snooze's absolute deadline in a compatible shown receipt and reoffers it after the deadline. The native Windows sound synthesizer builds short PCM tone patterns in memory, with level changes and repetition controlled by the modal. Other-host synthesized sound remains unverified.

Auto-read is an account-local label choice stored in the existing local-model settings document. An absent choice means on for a Receipts label and off otherwise. Only the arrival watcher marks a complete, current, incoming classification read; manual/backlog analysis does not silently clear unread. A live read requires the account's write authority and is queued through the existing durable Gmail read mutation, with a pending read overlay until provider acknowledgment. No real mailbox was mutated by verification.

Focused Rust checks: nine Calendar reminder tests pass, including due/default, restart, snooze after start, a late snooze crossing the original one-hour cutoff and stale identity; the Receipts default/toggle/arrival test (including account isolation and persistence across restart), live durable read-journal test and older model-settings serialization test pass. The full Core run before the final late-snooze test reports 338 passed, three ignored and three failing fixture/count assertions in `google_labels`, `mutations` and `phase1_tests` (344 total). Those three failures also appeared before the D090 corrections and do not involve the touched reminder or model paths. Actual schema29→30 upgrade and reopening pass on a fresh empty live profile; the older schema29 engine refuses it with `schema_mismatch`. Strict workspace Clippy, Rust formatting, repository hygiene and `git diff --check` pass. Debug GUI smoke, Calendar controls, reminder backend and label controls pass individually.

The configured Release package at `dist/prototype-september-23-windows-amd64` passes build-tree GUI smoke (4.46 s), relocated offscreen GUI smoke, relocated empty-live GUI startup and CLI schema30 readback. The offscreen empty-live test reports no seeded accounts, conversations or events. A hidden-window smoke attempt timed out because that harness waits for a painted conversation; it passes with `QT_QPA_PLATFORM=offscreen` and `QT_QUICK_BACKEND=software`, matching CTest. No actual account or provider mutation was run. Binary SHA-256: GUI `E9D59836F39064598D98CEBE5ADBDE38D3E158C65821BFF160681126FBBCA122`; engine `FA7D9B4E3D86DF94105F70517D9F33CB1356BC6FDA4F1A63BA056E5C83C71B39`; CLI `C7BF43A177CF8D6E60EB77D61C9B8AC11E44DE0B1CC19E327621117EAF91CE0B`. Source baseline is `3168800` plus the uncommitted D089/D090 working tree. Other-host synthesized sounds, a live-account Calendar readback and broad GUI self-test acceptance remain open.

The whole engine test run reports 164 passed and two failed assertions in Gmail inventory continuation/recent-first coverage fixtures (`google_sync::mail::tests`). D089/D090 changes in that module are confined to Calendar refresh and arrival-classification admission, but the broader engine gate remains failed. The 13 focused Calendar worker tests pass. Preserve these failures when integrating; neither a passing scoped packet nor a package smoke closes whole Phase 2 acceptance.

Two actual packaged offscreen frames were inspected from a disposable synthetic profile: `build/d090-release/calendar-month.png` shows the count pills in the top-right of past month cells, no future pills, and the labelled All calendars switch; `calendar-settings.png` shows the revised intro plus default 15-minute reminder and 5-minute snooze segments without the duplicate account row. These PNGs remain ignored local evidence. The sound patterns and fullscreen reminder have native logic/QML checks but no other-host or actual-account visual playback evidence.
