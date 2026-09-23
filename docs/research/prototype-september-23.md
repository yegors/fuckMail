# September 23 prototype implementation

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
