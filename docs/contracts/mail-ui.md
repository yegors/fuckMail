# Mail UI, reader and human composer

## D211 — one-line subject in the composer (Mail UI153)

- The subject field folds what it is given the way Core folds a received subject (Core98): each run of whitespace holding a line break, tab or other control character, or U+2028/U+2029, becomes one space, or nothing at either end. Typed spaces stay. The field then shows the folded text, even when it matches what is already saved.
- So a pasted line break, which the field draws as a space, no longer makes every save fail with "Subject must be one line of at most 8192 bytes".

Consumers: `MailBackend::setDraftSubject`, `ReplyComposer`; Core98; gui-composer-input.


## D210 — sender warnings, verified image loading, attachment safety (Mail UI152)

- **Sender warning.** An expanded received message whose `sender_check.warnings` is not empty shows a 13 px `warning` icon left of TEXT/HTML (`messageSenderWarning_<id>`), red (`Theme.error`) when one is `dmarc_fail`, else `Theme.warn`. Its tooltip: "Check before you trust this message", one "• " line per warning, then "Gmail’s checks: SPF … · DKIM … · DMARC …" when Gmail checked. Sent mail never shows it. `MessageSource` exposes `senderCheck`, `senderVerified` and `senderWarnings`, from the parsed source or a live Gmail message.
- **Always load images.** `HtmlMessageBody.senderVerified` is true only when Gmail verified the sender and the message is not in Spam. An allowed sender's images load automatically only then, including once the verdict arrives after the HTML. Otherwise the image row reads "N images hidden: they load automatically only when Gmail verifies the sender and it isn’t Spam". Load images still works for the one message.
- **Unsubscribe by email.** Clicking Unsubscribe for a `mailto` method opens a confirmation (`unsubscribeConfirmation_<id>`): "Send an unsubscribe email?", naming the address and saying Gmail confirmed it belongs to the sender. Send sends; Cancel sends nothing.
- **Attachments.** Image thumbnails and the viewer show only the engine's PNG preview, never the file itself. A video card decodes nothing: it shows a play badge (`attachmentPlay_<id>`) and plays only in the viewer once opened; the poster frame and duration are gone. Opening a file whose type runs code, installs software, changes settings or opens in a browser (apps, installers, disk images, scripts, shortcuts, `.terminal`, `.fileloc`/`.inetloc`/`.webloc`, `.jar`, HTML and SVG, and the Windows and Linux equivalents) shows a toast instead: "fuckMail doesn't open .x files: they can run programs or show fake sign-in pages. Save it and open it yourself only if you trust it.", with Save to Downloads. Saved copies are marked as downloaded from the internet.

Consumers: `MailBackend` (`requestAttachmentPreview(account, id, video)`, `openAttachment`, toast action `save_attachment`), `AttachmentTile`, `AttachmentViewer`, `MessageNotices`, `HtmlMessageBody`, `MessageSource`; Core97, Gmail45; gui-mail-selection, gui-html-message-body.


## D208 — quoted history on request (Mail UI151)

- Each expanded received message whose reader left quoted history out ends with a 28 × 14 px "···" button: 4 px corners, 7% ink fill (14% hovered or while showing), the `more` icon at 55% ink. Tooltip and accessible name: "Show quoted text" / "Hide quoted text". It is absent when nothing was left out.
- **Left out** means, in HTML mode, the engine's `quote_removed`; in text mode, a non-empty `body_full` in the source reply.
- **Showing it.** HTML mode reloads `message_html` with `quotes: true` (its own GUI cache entry and image ids). Text mode lays out `body_full` as a forward's text is laid out: the sender's text, `attribution` rows and `quote` rows per level (D160's styles). A failed HTML load falls back to that text.
- **What changed in the quote.** While shown, quoted lines that the earlier messages in the conversation do not contain (answers written inside the quote, or edits to it) are highlighted. `ReceivedBody::quoteAdditions` compares words (lower case, letters and digits) in order against the earlier messages' texts, newest first, so re-wrapping does not count. A line of 6 words or more found anywhere is original. A shorter one must continue within 3 words of where the previous line matched, or of where an earlier message starts. Bounds: 20,000 original words, 2,000 quoted lines. A message with no earlier message here is not diffed.
  - Text mode: each added line is its own `quote` row with `added: true`, at full ink on a 10% accent tint, its own level's bar at 70% accent.
  - HTML mode: the added lines are tinted on the paper at 22% accent through the search-highlight path. A search's matches take their place while a search is shown.
- The choice belongs to the open message view and is not saved.

Consumers: `MailView.qml`, `HtmlMessageBody`, `HtmlContent`, `ReceivedBody::historySections` and `quoteAdditions`; HtmlMessageBodyTest, ReplyDocumentTest; Core96.

## D207 — mail each way in address suggestions (Mail UI150)

- Each To, Cc and Bcc suggestion row shows how many messages went to the address and came from it (`sent`, `received` from Core95), between the address and any other-account name: a 9 px up arrow then the count, a down arrow then the count, in the subtle colour with tabular figures. Counts of 1,000 and more read `1.2k` or `12k`. A direction with none is left out; a row with neither, and the raw "Use address" row, show no counts. The row's accessible name adds "12 sent, 40 received".
- Rows arrive in the engine's order, busiest first. Narrowing while a lookup is pending keeps that order.

Consumers: `RecipientField`; ComposerInputTest.

## D206 — menus open where clicked; the workspace switch moves (Mail UI149)

- **Menus.** `FmMenu` keeps itself inside the window only once it is visible (`onVisibleChanged`, and on size changes while visible). Before a popup shows, Qt reads its `x` and `y` as 0, so D205's clamp in `aboutToShow` put every context menu at the left edge.
- **Mail/Calendar switch.** The switch stays instant. Then the ink is one shape behind both tabs (`workspaceInk`): its leading edge travels in 180 ms and its trailing edge in 280 ms (eased out), so it stretches on the way and settles on the open tab. It finds its tab through the Repeater's `count` (`itemAt()` notifies nothing, which left launch without ink) and is placed without travel the first time. The tab it lands on pops from 90% with an overshoot (320 ms), and its label colour fades over 170 ms. The view arrives 28 px from the side its tab sits on (Calendar from the right, Mail from the left), fading up from 40%, over 220 ms (`workspaceTravel` from ±1 to 0). Settings, and reduced motion, are cuts.
- **Startup.** The splash shows the animated wordmark only; the amber cursor after it is gone.

Consumers: `FmMenu`, `Main`, `StartupOverlay`; MailSelectionTest, GUI integration self-test.

## D205 — motion (Mail UI148)

- **Timing.** `Theme` owns one scale: `pressMs` 90, `quickMs` 120, `baseMs` 170, `glideMs` 220, `emphasisMs` 420; `enterEasing` OutCubic for arrivals, `exitEasing` InCubic for departures (shorter), `moveEasing` InOutCubic for travel. Every one-shot transition's duration passes through `Theme.ms()`, which returns 0 under reduced motion. Looping progress indicators keep their own pace. Durations already in this contract are unchanged. State always changes first; no action, navigation or input waits for motion, and the toast accepts a click from its first frame.
- **Reduced motion.** `Theme.reducedMotion` is true for the `motion` preference `reduced`, or for `system` when `systemMotion.reduced` is (see Platform 24). It makes every transition a cut and hides the Undo countdown, the Inbox clear moment, the count roll, the calendar slide and Sync's turning arrows.
- **Folding moves.** The fold that takes a reviewed row out of To review (D144: 170 ms ease-in height, opacity 0 at 70%, refresh after 190 ms) now also runs when Done, Delay, Delay's undo or filing (archive, trash) takes the open conversation out of its Inbox section, from the reader's buttons or as a one-row selection action (E, D), and when `review_leave_seconds` moves it. The row carries `leave_kind` (`done`, `delay`, `back`, `filed`, `reviewed`) and drifts 18 px toward where it goes as it folds: left for `done` and `filed`, right for `delay`, not at all otherwise. The list refresh follows the fold rather than racing it; the last row of a section that stays still turns into its empty line in place. Multi-row selections move at once as before.
- **Popups.** `FmMenu`, `FmComboBox`'s list, `FmDateField`'s picker, the Spam/Trash menu, all-labels, capture history, Calendar's date picker and options, summary sources and actions, sender-kind correction, Calendar's event list and activity, and the account colour picker fade in from 97% scale over 120 ms (origin at the corner or edge they open from). `FmDialog` rises from 96% over 170 ms with its dim fading alongside. Tooltips fade in over 120 ms. All of them close at once, so nothing lingers over the next action. The search popups stay instant: live search starts when they open. While a popup eases in, `opened` is still false, so choices, keys and shortcut fences that mean "on screen" read `visible` (the row menu's targets, the label submenu's checks, Enter in confirmations, the message source text, the sign-in, signature and account dialogs' reply guards, the workspaces' `popupOpen`), and a popup sets itself up in `aboutToShow`, not `opened` (Google sign-in's form reset, Calendar activity's load). Menus clamp to the window before they show. The Spam/Trash button's guard against reopening counts from when the menu starts to close.
- **Toast.** It rises 8 px into place while fading in (170 ms) and sinks while fading out (120 ms). An Undo toast draws a 2 px accent line along its foot that runs out evenly over the time left to undo (`actionToast.remaining_ms`, new, beside `duration`).
- **Buttons and switches.** A prominent `FmButton` sinks to 97% and darkens while held. `FmButton.iconSpinning` turns the icon, finishing its turn when it stops; the header Sync button uses it while `syncing` and the app is in front. The pill `FmSwitch` knob stretches 5 px mid-travel (170 ms, InOutCubic) and lands square; the square switch's mark pops in with a small overshoot.
- **Counts.** An account tab's To review count, changed from one non-zero value to another, rolls 7 px in from the side it moved from (down from above when it falls) over 170 ms.
- **Inbox clear.** When the clear reader first shows after the inbox was seen with mail waiting in this account, the check grows from 40% with an overshoot and a 25° turn (420 ms) while a ring spreads to 2.6× and fades (560 ms after 120 ms), and the title, line and next event rise 8 px and fade in from 80, 150 and 220 ms. Launch, an account switch and an inbox already clear show it still. Once per clearing.
- **Calendar.** Moving to another day, week or month in the same view slides the header band, time grid and month grid 18 px in from the side the dates come from while they fade up from 30%, over 220 ms. A view change is a cut.
- **Superseded.** D076's 220 ms summary height animation no longer applies (the summary card has no height animation); D119's console enter is the 200 ms slide with no fade.

Consumers: `Theme`, `Main`, `MailView`, `MailBackend` (`foldOutMoved`, `leaveReview`, `runMailSelection`, `actionToast`), `ActionToast`, `FmButton`, `FmSwitch`, `FmMenu`, `FmDialog`, `FmToolTip`, `FmComboBox`, `FmDateField`, `SearchPopup`, `CalendarSearchPopup`, `CalendarView`, `SummaryCard`, `MessageAssistance`, `CalendarEventList`, `CalendarOperations`, `AccountColorPicker`, `SenderCard`, `LinkCard`, `SettingsView`; MailSelectionTest. Platform24, Core94.

## D204 — read on screen; leaving To review (Mail UI147)

- **Arrivals.** A message new to the open thread's visit, unread and not sent, makes `MailView` call `reviewOpenConversation()` (read-on-open again) once the reader is visible and the application is active. Messages already seen in the visit, including ones marked unread by hand, never trigger it.
- **Leaving To review.** `review_leave_seconds` (0, 5, 10 default, 30) after a deliberate opening, a pinned, read open conversation is released (`releasePin`) and the list reloads, so it shows in its own section while open. 0 keeps the pin until another opening. Settings → Mail: `reviewLeavePreference`.

Consumers: `MailView`, `MailBackend` (`reviewOpenConversation`, `leaveReview`), `SettingsView`; MailSelectionTest, GUI integration self-test.

## D203 — opening at the first unread message (Mail UI146)

- A thread first shown with unread mail scrolls its first unread message's header to just below the reader's top (`revealFirstUnread`, through the same settle-and-place timer as Show match), once per visit and again if a fuller copy moves it, until the user scrolls. A restored `readerY`, an open reply, or no unread mail leaves the position alone.

Consumers: `MailView` (`followThread`, `revealMember`); MailSelectionTest.

## D202 — paste attaches screenshots and files (Mail UI145)

- Paste in the composer body attaches copied local files, and an image copied without text as `Screenshot YYYY-MM-DD at HH.MM.SS.png` (then ` (2)`, …), written under the profile's `cache/pasted` and kept a week. Anything else, including an image with text, pastes as plain text. A paste during a running import joins its queue; a paused or uncertain import is resolved first. The editor menu shows "Paste as attachment" when that applies.

Consumers: `ReplyDocument` (`pastePlain`, `clipboardAttachable`, `imagePasted`, `filesPasted`), `MailBackend::attachPastedImage`/`attachPastedFiles`, `TextMenu.pasteAvailable`; ComposerInputTest.

## D201 — reader heading and TLDR tag (Mail UI144)

- **Reader heading.** The subject stands alone: no `#label` after it and no `categoryAssignmentButton`. Labels are changed from the actions menu's Labels submenu (`contextLabels`, `applyLabel_<id>`). This replaces the label in the D198 reader heading below.
- **TLDR tag.** The tag's capitals are centred on the first line's capitals (Qt sets the 1.6-line text at the top of its line box).

Consumers: `MailView` reader heading, `SummaryCard`; LocalModelTest, GUI integration self-test.

## D198 — the Terminal design (Mail UI143)

Replaces these earlier rules: the reader subject size (21px), the To review/Delayed headers with count chips (Mail UI130), list shading, the thread fold ("N earlier messages") with its opening rule and folded-thread rule, odd-message tones and hover washes, the 44px message header, and the neutral participant discs.

- **Fonts.** Geist for the interface; the bundled Geist Mono is `fixedFontFamily()` (sources, code, search operators, key chips).
- **Shell.** A 48px top bar, then a 24px shortcut bar (`keyHints`) above the bottom edge. The shortcut bar shows the view's keys from `ShortcutRegistry` and is hidden when `key_hints` is false.
- **List header.** Lowercase `inbox` (amber count), `sent`, `drafts` (dim count) and the ▾ Spam/Trash menu. On the right, `scopeStat` (`count` and `label` properties, e.g. "6 to review") and `scopeStatHeld` ("6 delayed") give way first when the pane narrows. Below them is a 30px rail of `#label` toggles (`railLabel_<id>`, `railDay_<key>`) and the people/automated toggles.
- **Section headers.** Every header is a 32px row: an 11px uppercase title (full ink for To review and Delayed), its count (`mailGroupCount_<key>`, amber for To review), and a hairline to the chevron.
- **Rows.** The grid is 24px avatar (7px from the top), sender column (24% of the row, at least 150px; name 13px, `+N`, a 10px kind glyph, address 11px), subject over preview, `#label` (11px, lowercase) and a 76px time column. Columns are 16px apart, with 16px side padding. The subject line carries "remote", "(N)" for the thread and the paperclip count. `rowSeparator_<id>` is a 1px ink-7% line inset 16px, transparent on a group's first row. There is no shading. Sent and Drafts rows keep the arrow or pencil, "To …" and the preview, plus `mailSendState_<id>` while a send is in progress.
- **Times.** Rows, messages and contact cards use one format. Under a minute is "just now", then "N min ago" and "N hr(s) ago" within 24 hours, then "Yday HH:mm" for yesterday, the weekday and time earlier this week (weeks start on Monday), and "d MMM" (with the year when it differs) before that. The full date stays in the tooltip.
- **Reader heading.** The subject is 16px on a 1.25 line, with the thread's label after it as a dim lowercase `#label` (`categoryAssignmentButton`) that opens the label chooser. The participants are 20px avatars coloured by name, 3px apart, with no overlap or ring. Heading rows are 12px apart.
- **Thread.** Every message is listed in date order. A visit opens the messages since your last reply, counted from the first still to review, or the latest alone when none are; the rest start closed. Messages arriving later open as before (several at once all open; the previous latest keeps its state). `messageRule_<id>` draws a hairline above every message but the first.
- **Message header.** One 40px row, open or closed (`messageHeading_<id>`): a click anywhere on it outside its controls toggles the message, and Space does too when focused.
  - `toggleMessage_<id>` is a caret in the 24px gutter, turned when open.
  - Then the 18px avatar, name and kind.
  - The middle shows the preview when closed and the selectable address (its own width only) when open.
  - When open, the actions (HTML/TEXT, summarize, more) come next, and the time (`messageShortDate_<id>`, 11px) always ends the row.
  - Hover is ink at 2.5%.
  - The cc line is indented 28px.
- **Your sent reply.** While a reply still shows as sent from here, a sent message that arrived during the visit stays hidden until that reply retires, so the reply is never shown twice.
- **Summary box.** It has no border, an AI 7% fill, 8px corners and 12/16 padding. The TLDR tag (`summaryActions`) leads the first line and opens sources, details and Cancel. Then come the conversation's `#label` in AI 70% and the text at 12.5px with a 1.6 line. Wrapped lines return to the left edge. The tooltip reads "Summary · model · time". A fresh summary turns green with a check (`summaryDone`). Busy shows the stage and elapsed seconds over a 2px bar.

Consumers: `MailView.qml`, `Main.qml`, `KeyHints.qml`, `SummaryCard.qml`, `MessageAssistance.qml`, `AccountHoverCard.qml`; MailSelectionTest, ReplyStabilityTest, LocalModelTest.

## D199 — whole-thread summary in the reader (Mail UI143)

- For a thread of at least two messages with summaries on, `summarizeConversation` (32×32, AI glyph, line2 border; AI 45% border and 12% fill while shown or busy) follows Delay/Done after a separator.
- The first click sends `assist_start {task:"thread", conversation_id}`. Later clicks show or hide the result for the session, and a stale result is rewritten when shown again.
- `threadSummary` sits under the actions in the summary box style, titled "Thread summary", with a close ×. While busy it reads "Reading N messages…" and then "Summarizing the thread…". Its error shows only while it is open.
- `MailAssistance.start("thread")` tracks the job as the conversation's thread work; message summaries and classification are not shown as busy for it.
- "Classify thread" classifies the latest received message.

Consumers: `MailView.qml`, `MailAssistance.qml`; MailSelectionTest, LocalModelTest. See [Local assistance32](local-ai.md#d199--whole-thread-summary-on-request-local-assistance32).

## D197 — coming back to the app syncs at once (Mail UI142)

- When the application becomes active again after losing focus (not its first activation at launch), `google_sync_refresh` is sent quietly for every account whose sync this window owns. Accounts not owned, waiting on keychain access, or mid start/refresh/pause are skipped. At most one wake per 10 s, counted from the last wake that sent a refresh.

Consumers: `MailBackend::applicationActivated`; CalendarSearchTest.

## D196 — address suggestions without the wait (Mail UI141)

- `recipient_suggestions` passes queued automatic reads, like an explicit query. The To/Cc/Bcc entry asks 50 ms after the last key; until the answer comes, the rows already shown narrow to those whose name or address contains the text.

Consumers: `MailBackend::sendNext`, `RecipientField`; MailSelectionTest, ComposerInputTest.

## D195 — a forward opens in To (Mail UI140)

- Opening the composer focuses the To entry for a forward with no To recipient, and the body otherwise.

Consumers: `ReplyComposer.focusEditor`, `RecipientField.focusEntry`; GUI integration self-test.

## D192 — mail actions do not wait (Mail UI139)

- **Order.** The engine connection still sends one request at a time. An explicit query (`conversations`, `conversation`, `view_get`, `drafts`, `sender_summary`) or a mail change (`flag`, `set_category`, `mail_filing`, `mail_undo`) passes queued automatic reads (polling, including calendar catalog and window reads started by polling); an automatic read runs after four bypasses. Any other manual read or write is a barrier, and manual requests keep their order.
- **Shown at once.** Sending a read or star change (`flag`: read on open, Read, Unread, Delay, Done) shows it on the matching rows, the kept row and the open reader at once. Rows change section when the reply lands. A refusal reloads the stored state.
- **Queued, not dropped.** A single-conversation action or a selection action taken while another mail change saves runs when it finishes. A single-conversation action runs only while that conversation is still open; a selection action only in the same account. A disconnect clears waiting actions.

Consumers: `MailBackend` (`sendNext`, `showFlagChange`, `finishMailMutation`, `mutateConversation`, `applyMailSelection`, mail undo); MailSelectionTest.

## D191 — waiting for a busy engine; folders never show another's rows (Mail UI138)

- A request to a connected engine fails as unanswered after 60 s (120 s for starting or refreshing sync). A slow reply is a busy store, not a lost engine.
- After a disconnect the app keeps trying to reach or start the engine for up to two minutes, so an engine that is still closing does not leave it disconnected.
- Switching folder shows that folder's saved rows at once when it has them; otherwise the list is empty until its rows arrive. Rows of another folder are never drawn as this one's.

Consumers: `MailBackend` (request watchdog, `attemptConnection`, `setFolder`, `listScope`), `ConversationListModel::scope`; MailNavigationCacheTest.

## D190 — save status apart from preferences (Mail UI137)

- `preferencesStatus` and `preferencesPending` notify through `preferencesStatusChanged`. `preferencesChanged` is emitted when preference values change, not when a save only finishes or fails, so a save re-evaluates no preference bindings.

Consumers: `MailBackend`, `SettingsView`, `AccountEditor`, `GoogleConnectionDialog`, `SignatureDialog`.

## D189 — a last row turns into its empty line (Mail UI136)

- A row folding out of To review (read, then another opened) that is the last in a section staying shown carries `leaves_empty` in the list model (`MailBackend::listItems`). It keeps its row height; its content fades out and the section's empty line (`EmptyLine`, the same one `SectionEmpty` shows) fades in over 170 ms. The empty line item then replaces it, so nothing below moves in any frame. Other folding rows still collapse.
- A sender name in its contact pill is shown whole whenever its advance fits the pill; only a name that does not fit is elided.

Consumers: `MailBackend`, `MailView` (`SectionEmpty`, `EmptyLine`, grouped rows, `SenderPill`); MailSelectionTest.

## D188 — instant Mail/Calendar switch (Mail UI135)

- The Mail timeline's header and footer slots are inside the Mail page of the workspace stack, so switching workspaces never changes the stack's size. Calendar and Settings still show no timeline and reserve no space.
- The open workspace is remembered with `MailBackend::rememberWorkspace` ("mail" or "calendar"), merged into the quiet navigation save (800 ms, flushed at shutdown). It emits no `preferencesChanged`. `startup_view: last` reads it as before.

Consumers: `Main.qml`, `MailBackend`; GuiSelfTest startup workspace check, the native Calendar benchmark.

## D186 — copy the reader's header text (Mail UI134)

- `readerSubject`, an open message's `messageSenderName_<id>` and `messageSenderAddress_<id>`, and `summaryBody` are read-only, mouse-selectable text (`CopyText` in `MailView`). Each copies with the platform copy key and has the app's text menu (Copy) on right-click, with an I-beam cursor.
- A click focuses them, as the message body does. Read-only focus leaves single-key shortcuts on.
- The sender name stays inside its contact pill. A click with nothing selected opens the contact card, and it is selectable only while the message is open, so a folded message expands from its whole row. A long name is elided to its space.
- The subject is rich text with a fixed 26px line (`line-height` with `-qt-line-height-type: fixed`). A search tints matches with spans, as before.

Consumers: `MailView` (`CopyText`, `SenderPill`, reader heading and member header), `SummaryCard`; MailSelectionTest.

## D185 — Reply after a forward; a message's own Reply (Mail UI133)

- **Thread actions.** Reply, Reply all and Forward under a thread, and their keys, answer the newest message that isn't a forward you sent: a message of yours whose subject starts with "Fwd:" or "Fw:" (any case). A thread of nothing else answers its newest message.
- **A message's "…" menu** answers that message (`MailBackend::beginDraftFrom`):
  - `contextReply` and `contextForward` reply to or forward it.
  - `contextReplyAll` shows when it has someone on To or Cc besides you and its sender. For your own message, only Cc counts, since Reply already goes to its To.
  - The thread's own "…" menu shows Reply all when the thread's Reply all button does. A list row's menu has none.
- **Switching** between Reply and Forward keeps the message the draft answers.
- **Headers.** A message's header request (`requestMessageSource`) is kept through a reload of the same account's thread and accepted only for its own message and version. It is no longer dropped or cancelled when a draft opens.

Consumers: `MailBackend` (`threadReplySource`, `beginDraftFrom`, `requestMessageSource`), `MailView` (`menuMember`, `replyAllFor`, `composeFrom`); MailSelectionTest.

## D184 — a smaller reader subject (Mail UI132)

- The reader subject (`readerSubject`) is 21px, weight 500, letter-spacing −0.02em, on a fixed line of 21 × 1.25 (`titleLine`, 26px at the default size), in Inbox, Sent and every other folder. Its box stays whole lines of its laid-out text. This replaces 28px on 1.15 lines.

Consumers: `MailView` reader heading; MailSelectionTest.

## D183 — forwards keep their files and show at once (Mail UI131)

- A forward opens with the message's files as its attachment chips; `removeDraftAttachment_<id>` removes one. Send sends what is left.
- The sent forward stays in its thread from Send until Gmail's copy takes its place, as Mail UI114 meant.
- Switching between Reply and Forward moves the words written unless the draft holds files the writer added. A forward's starting files (its source message's) don't count; they stay with the retired forward, and a new forward starts with them again.

Consumers: `MailBackend` (`draftAddedFiles`, outgoing previews), `ReplyComposer` attachment chips; Core86; MailSelectionTest.

## D182 — section headers, starred contacts (Mail UI130)

- **To review and Delayed headers** replace Mail UI117's band:
  - The row is 40px tall with 24px side padding, no background and no rules. It scrolls with the list; nothing is pinned. A click still folds or opens the section.
  - Left to right: the 12px icon (exclamation circle for To review, clock for Delayed), 7px, the label at 13px weight 500 in full ink and normal case, then the count chip 2px further on, space, and the chevron.
  - The count chip (`mailGroupCount_<key>`) is at least 22×20 with 6px side padding and 5px corners, filled with ink at 8% and no border. Its text is 12px, weight 600, full ink, tabular figures.
  - Date groups are unchanged.
- **Starred contacts.** A contact card for anyone but yourself has a 28×28 star at the end of its name row (`senderCardStar`, 4px corners, `bg4` on hover, 14px icon).
  - Unstarred, it is an outline at 40% ink with tooltip "Star this contact". Starred, it is filled amber with "Starred — click to remove".
  - A click adds or removes `{account, email}` (the card's account and lowercase address) in `preferences.starred_senders`.
  - Mail from a starred contact shows the filled amber star in place of the sender-kind glyph on list rows. In the reader, the star (`messageSenderStar_<id>`, tooltip "Starred contact") sits before the kind-correction button, which stays.

Consumers: `MailView` (`GroupHeader`, `starredSenders`, rows, reader header), `SenderCard`, `FmIconItem` (`contactStar`, `contactStarFilled`); Core85; MailSelectionTest.

## D181 — one-line mail text stays on one line (Mail UI129)

- Subjects, senders and summaries shown on one line (list rows, the reader's subject, sender card and recent-mail rows, capture and source lists) read line breaks and runs of spaces as one space (`Theme.oneLine`). List row subject, sender and preview are also capped at one line, so a row never grows past its height.

Consumers: `Theme`, `MailView`, `SenderCard`, `MessageSource`, `ReplyComposer`; MailSelectionTest (`oneLineRows`).

## D180 — format bar and lists (Mail UI128)

- **Format bar.** Under the body of every editable draft: Bold, Italic, Underline, Bulleted list, Numbered list, Undo, Redo. It is always shown, so selecting text moves nothing. A style is shown on (`selected`) where the selection or caret has it (`ReplyDocument.formats`). Tooltips name the shortcuts.
- **Styles.** Bold, Italic and Underline (⌘B, ⌘I, ⌘U, or the bar and the editor's menu) apply to the selection. With nothing selected they apply to what is typed next at the caret, in the same undo step as the typing; moving the caret elsewhere drops the choice.
- **Lists.** Bulleted (⌘⇧8) and Numbered (⌘⇧7) turn the selected paragraphs, or the caret's, into list items, or back into paragraphs when all already are. A list of the same kind just above is continued. Each is one undo step. Return adds the next item; Return on an empty item, or Backspace at an item's start, ends that item's list. Lists are one level.
- **Saved and sent.** The HTML keeps `ul`, `ol` and `li`. The plain text puts "• " or "1. " before each item, as received lists read in plain text, and a reload compares the plain text the same way.

Consumers: `ReplyDocument` (`format`, `formats`, `plainText`), `ReplyComposer`, `FmIconItem` (`bulletList`, `numberedList`); ComposerInputTest.

## D178 — sync resumes after a reconnect (Mail UI127)

- When a live connection drops, the accounts whose sync this window owned are resumed on the next connection, with the same checks as at launch. Sync the user paused, and accounts that need signing in again, are not started.

Consumers: `MailBackend` (disconnect, `resumeStartupGoogleSync`); MailSelectionTest.

## D177 — the reply stays whole in view (Mail UI126)

- **Size.** The reply form's body grows with its text up to its cap (400 px for replies), and never past the height that keeps the whole form, title to footer, within the reader with 12 px above and 24 below. Past that it scrolls inside. The rest of the form is measured once each layout settles.
- **Following.** While the keyboard is in the form, the reader scrolls just enough to show the form's top and footer whenever the form grows with its text or attachments, the reader changes size or the thread above lays out. Writing follows the form while any of it shows. Other changes follow only while it was whole in view. Scrolling away by any means stops following until the form shows whole again.
- **Done sections** refresh when the date changes in the zone Core counts them in, not on a change of the calendar's display zone or the UTC date.

Consumers: `ReplyComposer` (`availableHeight`), `MailView` reply following; ReplyStabilityTest, CalendarMailActivityTest.

## D176 — long message text, search result heading, Show match (Mail UI125)

- **Long message text.** The reader's text rows draw all of their text, whatever their length. Qt's TextEdit draws only the part in view once its source passes 10,000 characters, and a paragraph taller than the view then stays blank when scrolled into. `MailBackend.drawWholeText` turns that off for each body row when its text is set.
- **Subject.** The reader's subject box is its text's laid-out height in whole title lines. A search tint arriving after the subject laid out no longer adds lines of space below it.
- **Show match** shows when a matching message's text holds a match, or the thread has more than one message. It expands the first matching message and scrolls its body's first match to a third of the way down the reader: the HTML view's first tinted range (`FmHtmlView.highlightRect`) or the first match in its text rows. Without a match in the body it scrolls to that message, as before. Opening a search result still brings the matching message into view, not the match inside it.

Consumers: `MailBackend` (`drawWholeText`), `FmHtmlView` (`highlightRect`), `MailView` reader heading, body rows and match reveal; MailSelectionTest (`longBodyPaints`, also with `--gpu`; `searchReader`).

## D175 — every Inbox list item is one row high (Mail UI124)

- **Items.** The Inbox list holds, in section order: a header item per section (`section:<key>`), an empty item for an open section with no mail (`empty:<key>`, "Inbox zero. Enjoy it.", "All reviewed. Nice work." or "Nothing waiting on later."), then the rows of each open section. Rows of a folded section are not items, the kept (open) conversation included. The backend builds rows and items in one model update; the model's `itemType` role (`header`, `empty`, `row`) picks each item's delegate, and only rows build a row's content.
- **One row high.** Every item is exactly the row height (52 px comfortable, 44 compact, scaled with text size). A header sits at the foot of its item, so the room above a header is the row height less the header's (18 px above a Done header in comfortable, 10 in compact). The list's top margin hides that room above the first header, which starts the list. Inbox rows draw no headings of their own. Sent and Drafts keep their day headings in rows.
- **Length.** While older mail is still to load, headers show down to the section whose rows come next: the section the loaded rows end in, when it has more, else the next open section with mail. Folded and empty sections before it show at once. Everything after, headers included, is `unloadedItems`, and the space below the list is that many rows high. The list's length is always its items plus `unloadedItems`, in rows, so the scrollbar is exact through scrolling, paging, folds, new mail and density changes.
- **Folds.** A folded or opened header keeps its item and its place; only the rows after it change. When rows above the view fold away, the rows on screen stay where they are. A fold at the list's end leaves the view at the new end.
- **Kept rows** go back into their own section, by section order and after the rows of that section that were above them. A section's rows are never split.
- **New lists.** A new list for the same account (another folder or search) starts at its top, and the saved list position follows it, as it does whenever the list moves. Switching accounts restores that account's own saved position.
- **Search.** A search starts with every section open, and its list requests leave out `open_months`. A section folded during a search stays folded until the search text changes; it is not saved to the mailbox view. Clearing the search restores the mailbox's saved folds. Switching accounts starts the next account with its own folds.
- **Plain-text links** end before trailing sentence punctuation (`. , ; : ! ? ' "`) and before a closing `)` or `]` the link did not open.

This replaces Mail UI123's fold hosts and Mail UI122's header items.

Consumers: `ConversationListModel` (`itemType`, `indexOf`), `MailBackend` (`listItems`, `inboxSections`, `unloadedItems`, `listIndex`, kept-row placement, search folds), `MailView` list delegates, sticky band, keyboard positioning and scroll range, `ReceivedBody`; MailSelectionTest (`listGeometry`, also with `--gpu`), MailNavigationCacheTest, GuiMailNavigationSelfTest, ReplyDocumentTest.

## D172 — local mail first; replying finishes Delayed mail (Mail UI121)

- **Launch.** The first mail view and list load before any Google sync starts. Sync starts after the first list has shown, or 3 s after connecting. A sync start or refresh may take up to 2 minutes (a keychain prompt waiting for the user) before the connection counts as unresponsive; other requests keep 15 s. Connection trouble shows in the account bar: the tab's red dot, and the header Sync button's Retry with its keychain tip.
- **Reply to Delayed mail.** Settings → Mail → "Replying finishes Delayed mail" (On by default, `reply_marks_delayed_done`). A reply or reply-all sent in a thread with a starred message marks that thread Done once Gmail accepts it; on screen, with the Done Undo toast. A send undone, cancelled, refused or abandoned leaves the thread Delayed.

Consumers: `MailBackend` startup sync and send completion, `SettingsView`; GoogleSendControlsTest, ReplyStabilityTest.

## D171 — reconnecting and closing without the engine (Mail UI120)

- **Reconnect.** When a live connection drops (a request past 15 s, an out-of-order reply or the engine exiting) and the window is not closing, the engine is started again after 1 s, at most three times in five minutes. After that the empty mail list reads "Your local mail isn’t connected.", shows the error and offers Reconnect (`mailReconnect`).
- **Closing.** Without an engine, closing always quits. The unsaved view and settings are let go. An unsaved reply asks once: the engine reconnects and the error reads "Your reply isn't saved yet. Wait for the local engine to reconnect, or close again to quit without it." A second close quits. Reconnecting resets the question.

Consumers: `MailBackend` (connection, `shutdown`), `MailView`; MailSelectionTest.

## Spelling (Mail UI119)

- **Where.** The composer body only (not Subject, which is a single-line field without a text document, nor recipients). Checked text excludes lines starting with `>` and everything from the `---------- Forwarded message` line on, plus tokens that contain a digit, `@`, `/`, `\`, `_`, `#`, `=` or a dot followed by a letter (links, addresses, domains, paths), all-capital words and single letters.
- **Checker.** The system's: NSSpellChecker on macOS, the Windows Spell Checking API in the user's default language (else en-US) on Windows, none on Linux. macOS calls run on one serial background queue; typing never waits. Words count as correct until their answer arrives; answers are cached per word for the editor's life. Headless runs (offscreen, minimal) use no system checker.
- **Underline.** A 1.6px dotted rule in `Theme.error` under each misspelled word, drawn over the editor from `ReplyDocument.misspellings` (layout-only marks, never document formats: the saved plain text and HTML are unchanged). The word the caret is still at the end of is not underlined until the caret leaves it or focus goes. Hidden for read-only (archived) drafts.
- **Menu.** Right-click on an underlined word puts up to six suggestions first in the editor's usual menu (medium weight), or a disabled "No guesses found", then "Ignore spelling" (this session) and "Learn spelling" (the user's system dictionary), then the usual rows after a rule. Suggestions wait at most 200 ms. Choosing one replaces the word as one undoable edit that keeps its formatting, moves the caret after it and autosaves like typing.
- **Correction (macOS).** When a space, Return or `. , ; : ! ? ) ] } " ”` follows a word and System Settings' "Correct spelling automatically" is on, the system's correction replaces the word as its own undo step, within 1.5 s and only if the word is unchanged. ⌘Z straight after restores the typed word and keeps the caret after the separator; that word is not corrected again at that spot. Windows does not correct automatically.

Consumers: `ReplyComposer`, `TextMenu` (`leading` rows), `ReplyDocument`, `SpellHighlighter`/`SpellChecker`; SpellCheckTest, ComposerInputTest.

## D169 — switching mailboxes and opening threads (Mail UI118)

- **Switching.** After the session's first list, each other active account's saved view (not Drafts or Spam) is read ahead into the mail view cache. A switch to a cached account shows its rows in the same frame, and its first thread when its view has none open; showing is not reading. A switch to an account with nothing cached reports `loading` until its first list arrives or other navigation takes over. It never shows "Inbox clear" or "Nothing open" meanwhile. The reader stays blank while a chosen thread loads.
- **Opening a thread.** Unread messages open, or the latest alone when none are unread; all earlier messages fold under "N earlier messages". The layout follows the thread until its revision reaches its list row's, or until the user unfolds or folds anything. Then new messages keep the one being read open (Mail UI113), and several arriving at once all open. A remembered opened message or "expanded" view key no longer unfolds a thread.

Consumers: `MailView`, `MailBackend` account switching and view cache; `AccountProgress`; MailboxSwitchTest, ReplyStabilityTest.

## D168 — account menu, Done by week and month, bands and quiet scrollbars (Mail UI117)

- **Account card.** Only ⋯ in the header, tooltip "Sync, pause, edit or remove". Connected: Sync now / Syncing… / Resume and sync now (unpauses first), Pause syncing / Resume syncing, rule, Edit account, rule, Remove account in red. Disconnected: Reconnect, rule, Edit account, rule, Remove account. The Labels and Signature rows are one button each: pointer cursor, 3% ink wash over 150 ms, "Edit ›" as a cue.
- **Done sections.** `done:week` ("This Week"), `done:month` ("This Month"), then `done:YYYY-MM` ("August", "December 2025") and `done:undated`. The week starts on `mail_week_start`. Week and month fold through `collapsedSections`; older months start folded and open through `openSections`. The list sends `done_week_start`, `today` and `open_months`, rebuilds sections when the date or the setting changes, and builds no rows for folded sections. Hovering a folded header prefetches its rows.
- **Settings → Mail.** "Week starts on", a seven-day dropdown after "Show drafts in the corner".
- **Section headers.** Replaced by Mail UI130: To review and Delayed are plain 40px rows with a count chip, not pinned. Done and date groups keep the lighter rule.
- **Scrollbars.** The conversation list and thread reader bars are hidden at rest and when content fits. Real pointer movement over the view (not Qt's per-frame hover repeats), user scrolling, hover or drag shows the bar within 90 ms; it fades out over 260 ms after 0.9 s at rest. The 14px gutter stays reserved and still takes hover, clicks and drags. This replaces Mail UI61's always-visible bars for those two views.

Consumers: `AccountCard`, `SettingsView`, `MailView`, `FmScrollBar`, `MailBackend`, `RemoteMailBackend`; Core84.

## D167 — stale reads retried, dangling view keys dropped (Mail UI116)

- A mail action refused with `stale_revision` reloads the conversation. When its members equal the action's snapshot and only the revision moved, the action is sent once more with the new revision and a new request ID; otherwise the refusal shows as before.
- A `view_save` refused as `not_found` with `details.view_key` drops that key from the saved and pending view and saves again.

Consumers: `MailBackend` (`requestMailAction`, `saveView`); Core84.

## D166 — account menu (Mail UI115)

- The Settings → Accounts "…" menu is non-modal and does not dim the window, like the app's other menus. Escape or a click outside closes it.

Consumers: `AccountCard`.

## D165 — forwards stay in their thread (Mail UI114)

- A forward's pending preview joins its source thread like a reply's, and the thread keeps the forward once Gmail's copy arrives. Mail UI113's Send behaviour applies to forwards too.

Consumers: `MailBackend` outgoing previews, `MailView`; Gmail43.

## D164 — a reply that stays put (Mail UI113)

- **Folded threads.** When messages are added after the newest one (a sent reply, or new mail), the message that was newest stays shown and keeps its open state. "N earlier messages" counts only the hidden ones and disappears when none are hidden. The pins last while the thread stays open.
- **Send.** The composer stays open with a sweeping 2 px hairline along its bottom while Send saves and waits for Gmail. It closes in the frame its sent message appears in the thread. That message fades in over 200 ms and settles 10 px up, then shows the upload line of Mail UI103. The reader never rests past the thread's end, and scrolls just enough to show the sent message whole. Scrolling takes over. Send does not re-apply the saved view.
- **Locks.** While a draft is locked, its body, subject and recipient inputs are read-only rather than disabled, so they keep keyboard focus and the cursor.

Consumers: `MailView`, `ReplyComposer`, `RecipientField`, `MailBackend` send queue; Quality (gui-reply-stability).

## D162 — checking downloaded mail (Mail UI112)

- While the inventory lists and `unchecked_messages` is above zero with mail kept locally, the mail stage is checking:
  - short status "Checking N%", where N is the share of kept mail already checked, at most 99;
  - hover status "Checking downloaded mail · N to go" and label "Checking";
  - the meter and the account tab's underline show the checked share instead of the counting sliver.
- Errors and pauses still take precedence.

Consumers: `AccountProgress`, `AccountCard`, the account tab and the mail-selection check; Core83, Quality89.

## D160 — forwards in text mode (Mail UI111)

- Text mode reads a message's `body_text` from its source reply when present, else the stored `text`.
- **A forward's rows,** from its marker line (Apple Mail's "Begin forwarded message:" or a "Forwarded message" rule) on:
  - `forward`: an 11 pt caption, "FORWARDED MESSAGE", at 50% ink. When text comes before it, a 1 px rule and 14 px of space sit above.
  - `headers`: its From/Date/To/Subject block, a 13 pt two-column table with bold labels at 72% ink.
  - The forwarded text, as ordinary reader text.
  - `attribution`: an "On … wrote:" line, 13 pt at 55% ink.
  - `quote`: quoted history, 14 pt at 55% ink, indented 16 px per `depth`. Each level has a 2 px bar at 16% ink, running on into the next row while that level continues.
- **Levels.** A forward that its client quoted whole starts from its own level. A nested forward repeats the caption and headers.
- **Other mail.** Reply history outside forwards stays hidden as before. A line that only mentions a forwarded message is ordinary text.

Consumers: `ReceivedBody`, `MailView.qml`, `MailBackend` and the reply-document, mail-selection and integration checks; Core82, Quality87.

## D159 — Flight Progress v2 (8), full-length list, quiet progress (Mail UI110)

- **Account tabs:**
  - 32 px blocks, 6 px apart, no pill.
  - An 18 × 18 letter tile with 4 px corners, tinted 24% when selected and 10% otherwise, then the name and unread count. Selected text is full ink at weight 500; others are about 50% ink, and hover brightens only the text.
  - A 2 px underline, inset 8 px, in the account colour marks the selected tab.
  - While the account syncs, the underline becomes a progress bar over a track: the account tint at 35% when selected, ink at 10% otherwise. The bar is at least 4%, amber while downloading and silver-blue while the local model works. Its width eases over 0.5 s, and its opacity pulses in 80 ms steps only while the app is active.
  - No ring. The red dot marks a disconnected account.
- **Hover card and Settings:** the hover card uses whole, grouped numbers; Settings → Accounts keeps the short form. Settings is a 32 × 28 button (4 px corners) in a surface tray (2 px padding, 6 px corners). It is transparent with a muted icon when idle and ink with a dark icon while Settings is open.
- **Composer:**
  - The title is Compose Email, Reply, Reply All or Forward: 20 pt, weight 500, after a 28 × 28 tile (5 px corners, gold at 12%) holding a 14 px gold edit, reply, reply-all or forward icon.
  - Opening a reply, reply-all or forward scrolls the thread down only until the box's footer is 24 px inside the view. It never scrolls up; a new message doesn't scroll.
- **Drafts:** draft rows have no Discard link; the list's right-click menu keeps Discard.
- **List length:**
  - The list is its full length from the first page. Unloaded rows are a spacer sized from the engine's counts (exact per Inbox section, estimated elsewhere) with placeholder rows over its visible part.
  - The next page prefetches within four screens of the loaded end. Scrolling past it loads through to that point, at most 2,000 rows per request.
  - There is no Load more footer, and no "Loading…" text in the list. A list error shows Try again.
- **Menus and tips:**
  - Label chips have no tooltip.
  - The Spam/Trash menu closes on any press outside it and on a mailbox or filter change; its own button toggles it.
  - Text fields, message bodies (plain and HTML) and the composer share the app's text menu: Undo, Redo, Cut, Copy, Paste, Select all, and the composer's formatting after Paste.
- **Status:**
  - Selection actions show no status line when they succeed; only a failure, or a stop after an account or session change, shows one.
  - When the last row leaves To review, the empty state appears in the same frame as the row's removal.
- **Mail progress:** it says "Complete" when the download has finished, even if Gmail sent a message malformed. The hover card says "Download complete" and the count's tooltip names the messages that can't be downloaded.

Consumers: `Main.qml`, `MailView.qml`, `ReplyComposer.qml`, `AccountProgress.qml`, `TextMenu.qml`, `MailBackend` and the native mail-selection, integration and GPU checks; Quality86.

## D158 — refresh changed projections (Mail UI109)

- Automatic status polling compares Core80's independent generations. Reload accounts, labels, the reader, drafts and capture history only when their dependencies changed. Keep explicit refresh and action recovery intact. Missing/incomplete generations use the previous full refresh; a different store epoch invalidates every area.
- A draft-only change refreshes the active/reply draft and outgoing previews without preparing the received reader. Preserve dirty-text, busy-state, revision, account and navigation guards; background queries never mark mail read. Draft/Sent lists and draft chips follow draft changes.
- Keep mail-list invalidations pending during search typing, loading and older-page browsing, delivering them when eligible even if the next poll has identical generations. Calendar and preference refreshes retain their own generations/revisions.

Consumers: `MailBackend`, status IPC and native mail-selection checks; Core80/CLI58/Quality85. This is area-level tracking; mail changes elsewhere may still refresh the open reader. Cache capacities and first-open behavior unchanged.

## D157 — shared reader data and display-sized attachment images (Mail UI108)

- Cached reader sources share their immutable maps/text with prepared readers, without a JSON round trip or cache replacement. Identical conversation refreshes keep the existing cache entry. Same-revision metadata changes still apply; older revisions, stale source versions, account/lifecycle and navigation guards remain unchanged.
- The current attachment grid's previews are at most 400 × 250 logical pixels. Request that size multiplied by screen density for image thumbnails and saved video posters. The requested size stays constant across ordinary card/window resizing, and cached images remain reusable.
- The viewer covers its physical display pixels, rounding each requested image dimension up to a multiple of 128. Asynchronous resizing retains the existing image until the replacement is ready. Aspect ratio, orientation, original bytes, downloads and video controls are unchanged.

Consumers: `MailBackend`, attachment tiles/viewer and native mail-selection checks; Quality84. Cache capacities and engine/CLI behavior unchanged.



## D155 — one list/reader divider for the whole app (Mail UI107)

- The Mail list/reader divider has one width for every account. It is applied once when Mail loads and saved to `window.json` (Platform 23) when a drag ends, never mid-drag.
- Switching accounts or restoring an account's view no longer sets the divider, so the panes do not relay out. Per-account view state keeps the list and reader scroll positions; a stored `splitWidth` is ignored (the engine still accepts it for older clients).
- Without a saved width (first run, tests, benchmarks) the list takes half the window.

## D154 — HTML or plain text per sender (Mail UI106)

- Mail shows as plain text unless its sender is chosen for HTML in that account (`html_senders`, Core78). Messages without an HTML part are always text.
- The HTML/TEXT badge and **Format** in the … menu change the choice for that message's sender, account and address, so all their mail follows, in every thread, including after a restart. Tips say "for all mail from <address>". Messages without a sender address show neither control.
- This replaces the single `message_format` choice from D107; other senders in the same thread keep their own format.

## D151 — reply after discard, switching reply and forward, video posters, account cards (Mail UI105)

- **Reply after a discard.** A discard closes the reply once, at that draft revision. Reply reopens the discarded reply, and it stays open through refreshes until it is discarded again. Typing or a file revives it.
- **Loading cards.** A card re-checks whether it is on screen on any layout change above it (heights, positions, the reader's size), not only on scrolling.
- **Video posters.**
  - The poster decoder runs until its first frame has been shown, then pauses.
  - The GUI keeps that frame as a JPEG of at most 960 px for this run, in a temporary folder under the profile's `cache/` that is removed on close. Then it releases the player.
  - A card that is reopened or scrolled back into view shows the kept poster and length at once. If no frame can be kept, the paused player stays.
- **Viewer.** The arrows load the item they move to, not the one they leave.
- **Settings → Accounts.** Cards come from a keyed model of all accounts, archived ones included. A sync refresh updates cards in place, and their labels are read once per card.
- **Writing under a thread.**
  - Reply, Reply all and Forward stay while a message is written. The kind being written is the filled button, and Reply is filled when nothing is written.
  - The kind being written brings its box into view. Another kind, while the box is open, turns the message into it. With the box closed, each kind starts its own draft.
  - The buttons stay enabled while a draft saves; the press waits for the save. A second press while that kind is opening does nothing.
  - Reply all shows whenever the thread's reply offers it, including while a forward is written.
  - A draft started for the thread on screen keeps the thread, its unfolded messages and scroll position while it loads.
  - The box fades in and moves 10 px up into place over 200 ms, including when its kind changes. The reader glides to it over 200 ms (OutCubic), then follows layout changes at once.
- **Switching kind.**
  - The words above the starting text (the account signature, or the forwarded message when there is none) go above the new draft's starting text, as plain text.
  - The old draft is discarded with no Undo once the new draft is saved. Drafts with files, with a send pending or accepted, or with words below the starting text are left, and their words above still come along.
- **Discarding a new message** opens the draft or thread it was started from, or leaves the reader empty. Undo reopens it.

Consumers: `gui-mail-selection`.

## D150 — reply placement, folding attachments (Mail UI104)

- The send line of Mail UI103 runs along the bottom of the reply, not under its header.
- **Attachments header.** "N ATTACHMENTS", its hairline and a chevron form one button. A click folds the cards away or shows them again, per message for the session. Folded cards load no previews.
- **Viewer.** A file named as a video (`mp4`, `webm`, `mov`, `m4v`, `mkv`, `avi`) with no type or `application/octet-stream` previews as a video. Clearing the player between items is not an error, and a good preview clears the last one.
- **Reply:**
  - A reply of this draft already on its way or sent makes Reply start a new draft.
  - Opening a reply shows it whole at the end of the thread. The reader holds it there while the thread lays out, until the height has held for 250 ms (3 s at most). Scrolling hands the reader back at once.
  - Saved reader positions don't move the reader while that runs. A reply taller than the reader shows from its top.

Consumers: Gmail38; `gui-mail-selection`, `gui-send-undo`, `gui-attachments`.

## D149 — send progress, previews and steady month toggles (Mail UI103)

- **A reply on its way to Gmail** shows a 2 px line under its header. It fills with `google_send_progress` (`MailBackend.sendProgress`, by request ID, polled every 150 ms while a send is in flight, whole percents only), and sweeps until the size is known. Once Gmail accepts, a check shows next to the time until Gmail's copy replaces the reply.
- The reader asks for a reply's sent copy only after the engine has recorded the send, so no error flashes.
- A sent reply's attachments preview from the files it was sent from.
- **Month toggles.** A clicked section header stays where it was on screen:
  - the toggle applies just after the click returns;
  - the header is restored before the next frame paints, after each change to the rows;
  - it holds until the refresh lands (at most 15 s) and lets go when the list is scrolled.
- After a structural diff, the list model rebinds only the rows whose data or predecessor changed.
- A conversation read already on its way no longer undoes a just-answered unsubscribe.

Consumers: Gmail37, Core76; `gui-mail-selection`, `gui-send-undo`, `gui-conversation-list-model`.

## D148 — instant filters, no unknown-kind mark (Mail UI102)

- **Sender kind mark.** The person or bot mark shows only for a known kind. Unknown shows nothing, both on list rows and in the reader's correction button. Only mail the model hasn't read yet has no kind.
- **Changing what the list shows.** People/Automated, labels, Sent day chips, folder tabs, Trash and the contact card's "All N with" update the list in place. Rows still shown keep their delegates. A search is still a new list, placed by the saved list position.
  - A view seen in the last few moments, and its counts, comes back from memory (`m_viewRows`, first pages by account, folder, search, filters, labels and collapsed months). The list then reconciles with Core.
  - Hovering one of those controls loads the view it would show, so the click is instant. A click before that answer arrives shows it as soon as it does.
  - Without either, adding People or Automated, choosing a first label, or dropping one of several labels narrows the rows on screen at once. Counts follow Core's answer.
  - With nothing to show yet, the rows still shown fade to 55% over 90 ms if the answer takes longer than 80 ms, then come back.
- **Saved list place.** When rows merged in above the visible ones move the list's origin, the saved offset follows what is shown.
- **Collapsed months** keep their rows per view, so a month never expands with another filter's rows.

Consumers: Local assistance27.

## D147 — no row tooltip, honest Empty trash (Mail UI101)

- Mail list rows have no hover tooltip. Subject, sender kind, time, labels and Delayed state are all on the row or in the reader.
- **Empty trash confirm.** The count is the larger of Gmail's Trash total and `stored`. When Gmail's Trash is already empty and copies remain here, it reads "Gmail’s Trash is already empty. N message(s) left here will be removed from this computer."

Consumers: Gmail36.

## D146 — quiet delivery, steady month toggles (Mail UI100)

- **Reader note** (`readerNote`): one line under the reader's buttons, always reserved while they show, so its text never moves the thread. It reads, in order:
  - "Read from the server · not on this machine";
  - "Read-only account archive";
  - "Cached copy · reconnect with Mail access to make changes";
  - "Google did not accept this change · check account access" (in the error colour) when `mailDelivery.state` is `failed`.
- Pending and unknown delivery states show nothing. The thread line and the message header show no status (`mailDeliveryStatus` and `messageDeliveryStatus` are gone).
- **Section headers** have no tooltip.
- **Loading texts.** `firstLoading` is true only while the first list read of an account's folder runs. The list's empty-state text is blank then; outside it the list shows "No matches" or "No conversations in this view", never "Searching…". The reader shows "Loading…" only then. "Inbox clear" and the other empty-reader lines stay up during refreshes.
- **Collapsing everything loaded.** With more mail to read and no rows loaded, the list shows the sections down to and including the first open one with mail (Mail UI124). Its rows arrive under its header, and Load more stays hidden until they do.

Consumers: none outside the GUI.

## D144 — list, thread, buttons, top bar, composer (Mail UI99)

- **List shading.** Every other row in a group has a 2.8% ink background (`groupPositions`), starting again at each group, including the whole list in Spam and Trash. The multi-selection, opened and pointer states take priority.
- **Leaving To review.**
  - When another conversation opens while a read row is kept in To review, the backend marks that row `leaving` where it is.
  - The delegate folds its height to 0 in 170 ms (ease-in), its opacity reaching 0 at 70% of the way.
  - After 190 ms the list refreshes and the row appears in its section. A second opening during the fold drops the first row at once, and the next refresh places it.
  - Leaving rows are skipped by Up/Down. Inbox only.
- **Thread.** A folded message's row adds a 3.5% ink wash on hover, fading over 150 ms, over its every-other tone.
- **Button hover.** `FmButton.hoverBrightness` (1.12 for prominent buttons, else 1.3; 1 turns it off) multiplies the label, icon and filled background colour channels while hovered, like CSS `brightness()`. The selected option of `FmSegmented` lifts too.
- **Top bar.**
  - The left starts with the Mail/Calendar switch.
  - Settings (`settingsButton`) is a 30×30 outlined icon button after the search box: radius 6, a `line2` border that turns `line3` with an ink icon on hover. While Settings is open it is solid ink with a canvas icon.
  - The export's 2 px account-colour strip along the top is left out: the founder asked for it removed.
- **Composer hint.** When the writer's own text matches `\b(attach(ed|ment|ments|ing)?|enclosed)\b` (case-insensitive) and the draft has no files or included attachments, the hint reads "Mentions an attachment — none added" in gold. It ranks after Add at least one recipient.
  - Text after "---------- Forwarded message ----------" and lines starting with `>` don't count.
- **Settings → Mail.** "Show drafts in the corner" (Show/Hide; `show_draft_chips`) replaces "Group Sent and Drafts by date". Hide removes the draft chips and the reading pane's extra bottom room. Sent and Drafts always group by date.
- **Search dropdown.** `to:` reads "Recipient name or address", with example `to:finance@example.com`.

Consumers: Core73, Calendar38.

## D137 — mail download progress (Mail UI98)

The Accounts card's Mail row (`AccountProgress.view`):
- **Counting** (inventory incomplete, no `inventory_total`):
  - the count shows local messages only;
  - the status reads "Counting · N found" from `inventory_counting` ("Counting…" before the first page, D138);
  - the bar shimmers;
  - the tooltip says what is local and how many Google has so far.
- **Counted:**
  - "local of total", where the total is `max(local, inventory_total)`;
  - the bar and percentage are local/total, capped at 99% until `inventory_complete`;
  - the time left comes from the one-minute download rate;
  - "Paused at" and "Stopped at" keep the percentage.
- **Finished:** "Complete", as before. The old "N of N+" and "Counting mail…" states are gone.
- **Account tab ring** (`view().ring`): the same share while downloading, and an 8% sliver only while counting (D139).

Consumers: Core72 (reads `google.sync`).

## D133 — the engine CLI in the console (Mail UI97)

- **`fm <args>`** in the console command line runs the bundled `fm` (CLI55) and prints its output as console lines, facility `fm`: stdout at info, stderr at err. Bare `fm` runs `fm --help`. Starting a command pauses live events (as `pause` does; the log file keeps recording) and says so, so the output stays in view until `resume` or tail -f.
- **Same engine.** The console runs `fm` from beside the app (else beside the engine) and adds `--profile` and `--engine` for this app's profile and engine, so `fm` talks to the engine the GUI already runs. It adds `--account` for the selected account, except for `accounts`, `status`, `help`, `google connect`, `google sync-status` and `google setup-status`, where it would change the command. Any of these the user types wins.
- **JSON.** With `--json`, stdout becomes one expandable line holding the reply (up to 1 MB; otherwise plain lines).
- **Lifetime.** One command at a time. Ctrl+C ends it (on Windows and Linux only when no text is selected; ⌘C still copies on macOS), as does `stop`. Closing the app ends it. The status bar shows "● fm running", and a failed start or non-zero exit is reported.
- **Full window, quick slide.** The console fills the whole window (replacing the earlier `max(360, 76%)` height). `~` slides it down from above in 200 ms (OutCubic) and back up in 160 ms (InCubic), with no fade. `~` follows the intended state, so a press mid-slide reverses it; a press while it slides up brings it back down. App shortcuts stay blocked for the whole slide.
- **Output scrolls into view.** Whenever a typed command or its replies (including `fm` output arriving later) reach the view, it jumps to the bottom and follows again, even if you had scrolled up.
- **Privacy.** `fm` output is the user's own mail data: it is never scrubbed and never written to `gui.log`. It stays in the in-memory view, so copy and export include it like any other visible line. A run keeps at most 5,000 lines of 4,000 characters.

## D131 — contact card text can be copied (Mail UI96)

- Recipient names no longer have an address tooltip; the contact card already shows the address.
- The card's name, address and status line are `CardText`: read-only `TextEdit`s (`senderCardName`, `senderCardEmail`, `senderCardStatus`) with `selectByMouse`, an I-beam cursor and `persistentSelection`. They never take keyboard focus. They wrap instead of eliding, the address anywhere, so the whole text can be selected.
- While any of them holds a selection (`holding`), the card stays open:
  - leaving it, or the name under the pointer, no longer closes it;
  - passing over another name doesn't switch to that contact;
  - it closes on Esc or a click outside, and opening another contact or closing clears the selection.
- A `Shortcut` on `StandardKey.Copy`, enabled only while the card is holding a selection, copies it.
- The card's conversation rows stay clickable, not selectable.

Consumers: none outside Mail UI.

## D130 — search highlights, labels by name (Mail UI95)

- **Search highlights.** MailView reads `searchTerms.phrases` (Search 11). `subjectPhrases` are the `any` and `subject` phrases; `bodyPhrases` are the `any` and `body` phrases.
  - `MailBackend.matchRanges(text, phrases)` returns `[[start, end], …]` in UTF-16 units. It uses the header-only `SearchMatch`, with Core's rules:
    - lower case;
    - Latin accents removed (Unicode decomposition, plus ł đ ø ħ ŧ ı ŀ ſ);
    - runs of whitespace treated as one space, and punctuation kept;
    - a phrase starting with a letter or digit must start a word.
    Overlapping ranges merge.
  - Each list row's subject and preview hold a `MatchMarks`: rounded 2 px boxes behind the Text, placed with `FontMetrics.advanceWidth` and clipped to what the Text shows.
  - The reader subject switches to rich text with a tinted span only while it has a match, keeping its fixed line height.
  - The text body's TextArea gets a `SearchHighlighter` (a `QSyntaxHighlighter` on its document, set by `MailBackend.markDocument`). It tints matches within each paragraph without changing the text, links or selection.
  - `FmHtmlView` gains `highlightPhrases`, `highlightColor` and `highlightCount`. It finds matches in its laid-out text and paints them under the selection. HtmlMessageBody passes them through as `matchPhrases`.
  - The tint is `Theme.matchTint` (the accent at 32%, 34% on light themes), or `Theme.matchPaper` (45%) on an HTML message's paper.
  - Clearing the search removes every tint. Summaries are not searched, but a match inside one shown in the preview is tinted too.
- **Labels by name.** The label rail and the "All labels" menu sort by name, ignoring case. With more than seven labels, the rail still shows the seven most used, in name order.

Consumers: Search 11 (reads `phrases`).

## D128 — To review transitions, invitation day (Mail UI94)

- **Kept row placement.** `MailBackend::keptRowIndex` puts the open conversation back into a reloaded Inbox inside its shown section: after the rows of that section that were above it, before the rest. Sections rank To review, Delayed, then Done. Other folders keep its old index. A row moved there from another section carries `pinned_from` (Core's group), and `publishConversations` adds `counted_section` (`held`, or `done:YYYY-MM` in the mail month zone).
- **Section counts.** MailView's `inboxSections` adds each `pinned` row to its `section` and removes it from its `counted_section`. The header count, the empty state ("All reviewed. Nice work.") and filtering all use these counts. The top "N to review" figure is unchanged.
- **Explicit actions release the row.** A completed `mail_filing`, or a `flag` carrying `starred` (Delay, Undelay, Done), for the open conversation calls `releasePin()`. The row moves at the reload that follows and stays open. Read-on-open (`flag` with `read` only) does not release it.
- **Navigation anchor.** `releasePin()` records `navigationAnchor() {id, before, after}`: the neighbours of the open row before it moved. In `moveSelection`, a single Up or Down from that row goes to `before` or `after` when it is still in the list. Opening another conversation clears the anchor. Nothing opens automatically.
- **Invitation day.** `CalendarView.showEventInDay` dates a row without a layout `date` using `MailBackend.calendarDayOf(start)`: the day in `calendar_display_time_zone`, or the `YYYY-MM-DD` itself for an all-day row. It falls back to today only when neither gives a day.

Consumers: Calendar37 (unchanged).

## D127 — invitation row, To review holds the open item, dimmer pictures (Mail UI93)

- **Invitation row** (`MessageNotices`), first in the notice group, above the mailing-list and images rows. Rules sit between the rows.
  - It shows for a received message whose `message_source` result has `has_invite`. `MailBackend.requestMessageInvite(id, source_version)` asks Core71's `message_invite` every time (no cache) and answers with `messageInviteReady(ticket, result, ok)`. It asks again 500 ms after `eventsChanged` and after the row's own answer finishes.
  - Contents:
    - a 13 px calendar icon in gold;
    - `inviteWhen(start, end)` in the display zone, "Tue 29 Sep · 15:00 – 16:00", ink, tabular numbers;
    - for an update, "was Fri 25 Sep 15:30", crossed out at 35% ink;
    - the status word at 11.5 px, 45% ink: "Going", "Maybe" or "Declined" once answered, nothing before;
    - Yes / Maybe / No: 22 px segments in a 1 px `line2` box; chosen Yes is solid ink with dark text, chosen Maybe/No `bg6`.
  - A REPLY hides the row. A CANCEL shows "Cancelled" and no answers. Answers show for an invitation to you, and while its event has not synced. Until the event has `can_rsvp` they are dimmed, and hovering them says why.
  - Yes/Maybe send `calendarWrite` with `kind: rsvp` for the linked event (its etag and recurring context). The answer shows at once and stays until the calendar has it; a failed write shows "Not sent" with the reason. In live profiles, No calls `queueCalendarDecline`, and `calendarEventDeclining` (now invokable) shows "Declined" during the Undo.
  - Clicking the time emits `openInvite(event, declined)`. MailView turns on `calendar_show_declined` for a declined event, then calls `showEventInCalendar`.
- **To review holds the open item.** When a list reload returns the open conversation outside To review and it was pinned from To review, it is shown at its pinned index with `display_group: review` and `pinned: true`, with fresh fields (read). Opening a different conversation reloads the list, so it moves then. A row the view no longer returns stays pinned as before. Filing clears the pin at once.
- **Sender pictures.** A found picture draws at 75% opacity. A photo has no disc behind it; a brand icon's white disc dims too.

Consumers: Core71, Calendar37.

## D126 — sender pictures, hover timing, Send while saving, replies (Mail UI92)

- **Sender pictures** (`SenderAvatar`):
  - Initials: both first letters when the second word is capitalised, otherwise the first; punctuation counts as a space.
  - Colours: `oklch(0.36 0.06 H)` disc and `oklch(0.9 0.05 H)` text, where H is the name's 31-multiplier hash mod 360.
  - `HtmlContent.senderPicture(email, kind)` returns `{state, url, source, label}` from Core70's `sender_avatar`.
    - At most four requests run at once, and arrivals repaint at most every 40 ms.
    - Answers are dropped when the engine connection, `sender_pictures`, `blocked_hosts` or `remote_proxy` change, or the image cache is cleared.
    - Failures are asked again after 60 s.
    - Nothing is asked in synthetic profiles or while `sender_pictures` is false.
  - A brand icon sits on a white disc: 18, 12, 16 and 24 px inside 28, 18, 22 and 36 px. A photo fills the circle (Shape `fillItem`; square in the software renderer).
  - Placement:
    - List rows other than Sent and Drafts: 28 px, 12 px before the sender column, with a tooltip of "Brand icon · domain", "Profile photo · address" or "No photo · initials".
    - Thread headers and folded rows: 18 px. Participant stack: 22 px. Both keep the neutral disc and your account's initials.
    - Contact card: 36 px on the name's hue.
  - Settings → Image loading → Sender pictures switches `sender_pictures`.
- **Hover timing.**
  - Contact card:
    - Opens 200 ms after the pointer enters a name.
    - While open, switches after 350 ms; entering the card cancels the switch.
    - Closes 220 ms after the pointer leaves.
    - A 12 px strip above the card counts as the card.
  - Link card: `hover(link, box, thread)` opens after 200 ms. Hovering the open link again keeps it; leaving cancels a pending open.
- **Send.** Autosave stays local.
  - `can_send` no longer drops while saving or uploading.
  - `send_now` is false only while a save runs, or while an upload runs with no accepted Gmail draft yet.
  - Send pressed during an upload re-checks every second for Gmail's acceptance, for up to 60 s. It then sends, or says "Gmail is still saving this draft. Press Send again in a moment."
- **Replies.**
  - A sent reply closes the form at once (`sentReplyDone`), and Reply begins a new reply draft.
  - The thread of an accepted send re-reads Gmail's copy, so the local preview goes once that copy arrives.
- **Drafts.**
  - Open and save do not re-enter while a draft operation runs.
  - A read-only draft clears its unsaved flag before continuing.
  - Dropping empty drafts skips read-only and adopted ones.
- **Day strip.** Each label starts with a 14 px account avatar at x 7 (no tooltip; 55% when past), and the label moves to x 26. Lanes allow 20 px more per label.

Consumers: Core70, Calendar36, Gmail33.

## D125 — console copy, text selection and inline details (Mail UI91)

- **Copy.** A `copy` button after export, and a `copy` command, put the filtered lines on the clipboard in the export format. The console notes "copied N lines to the clipboard".
- **Scrolling and selection.** No drag scrolling: the wheel, the scrollbar and Page Up/Down/Home/End scroll. Pressing and dragging selects text across lines, scrolling when dragged past an edge. Ctrl/⌘+C copies the selected text, one line per row. A press without a drag is a click. Typing while the lines have focus goes to the command line.
- **Rows.** Each row is one monospace line: time, level and facility in fixed columns, then the message and its `key=value` details inline in a muted colour. Wrapped text hangs under the message column. A line that fits shows everything and does not expand. A line cut off at the edge ends in "…", and a click shows all of it wrapped. A line with JSON keeps its `{…} N lines` hint and expands to the JSON block. The separate `└` details line is gone. `DebugConsole.plainLine` defines the row text that selection positions and `plainRange` use.

## D124 — acceptance fixes (Mail UI90)

- **Drafts count** counts every draft in Drafts (Core's draft total). Chips still show only drafts with something written.
- **Contact card**: the mail view's hover handler reports the pointer to the open card (`trackPointer`); nothing hovers on the popup overlay, so the name under the pointer keeps its hover and the card no longer closes and reopens.
- **Attachments** exclude images the message's HTML shows (Gmail32).
- **Unsubscribe**: when `message_unsubscribe` answers `unsubscribe_by_email`, the reader calls `google_unsubscribe_mail` with `fallback: true` as part of the same action and reports that result.

Consumers: Gmail32, Core69, Calendar35.

## D122/D123 — first-class drafts, header, label toggles, inline compose (Mail UI89)

- **Header.** Inbox and Drafts carry raised counts (Inbox: to review, 11.5 px DemiBold `inkAlpha(.7)`; Drafts: unsent written drafts, accent), hidden at zero. Spam and Trash sit behind `binMenuButton` (▾) after Drafts; its popup lists `spamScopeButton`/`trashScopeButton` with icon and Gmail count (`google_bin_counts`, read when it opens); in Spam or Trash the name shows beside ▾. `newMessageButton` (solid, 28 px, mail icon) follows the stats; `n` still opens it. The top-right New is removed in Mail and Calendar. Narrow panes wrap stats and New below the tabs.
- **Label rail.** `railLabel_*` toggles (`#` 14 px mono, accent when on and `inkAlpha(.28)` off; name 12.5 px; count 10.5 px mono; 2 px accent underline when on). Any number can be on (OR); none shows everything; a `label:` query decides instead. No All. Sent and Drafts show `railDay_*` toggles; Spam and Trash show none.
- **Inline compose.** A new message replaces the thread in the reader, at least max(420 px, 50% of the window), starting at From; the body is at least 96 px (140 px when tall) and grows. Footer (`composerFooter`): Send, `draftAttachButton`, one `composerHint` line and `discardReply`. No title row, minimise or top bin. Leaving (thread, folder, account, Settings, view) keeps it in Drafts; an empty new message is dropped without Undo. Esc on a new message with content opens `keepDialog` "Keep this message?" (`keepMessageDiscard`, `keepMessageEditing` "Keep editing (Esc)", `keepMessageSave` "Save to Drafts (↵)"); drafts opened from Drafts never ask. Discard confirms "Discard this draft?" — "It will be removed from Drafts here and in Gmail."
- **Recent with.** Under a new message with a recipient: "Recent with {first name}", up to four conversations with that address (`conversations` folder all), each subject · first line and Today/Yesterday/d MMM; "All mail with {name} (N)" searches the address.
- **Draft chips.** Bottom right above the day strip: up to four unsent drafts of the account (`draftChip_*`), then `draftChipsMore` "+N" to Drafts. 26 px rounded, near-transparent, accent reply or pencil icon, subject / first words / "Untitled"; tooltip account, "Reply to …" or "To …", start of text. Reply drafts open their thread with the inline reply; new drafts open Drafts with the composer. Hidden while composing; the reader adds 64 px bottom space while shown.
- **Drafts sync is silent.** The source/actions panels, Refresh and inventory status row are removed. The composer hint shows only real problems: "Couldn't save this draft to Gmail. It's kept here.", "Couldn't save this draft to Gmail: {reason}" (35 MB limit, invalid From, another address, invalid addresses/formatting/attachments) and "Gmail hasn't confirmed this draft yet. It's kept here."
- **Inbox clear.** `inboxClearTitle` "Inbox clear" (20 px) under a small accent check, "Nothing to review or delayed in {account}.", hairline, and `inboxClearNext` "NEXT · time · title ›" for the next event today, opening Calendar Day on today with that event's card.
- **Day strip.** The event card opens next to the clicked event; the more menu is removed ("Back to today" stays).

Consumers: Core68, Gmail31, Calendar34.

## D121 — to: search, stable progress bars (Mail UI88)

- **Search dropdown.** Narrow it down lists `subject:`, `body:`, `from:` ("Only the sender") and `to:` ("Only recipients (To or Cc)"), then "An address on its own finds mail from or to it." ("finds drafts to it" in Drafts).
  - Chips: bare addresses lead as "From or To" ("To" in Drafts), then "Subject or body", Subject, Body, From, To and the rest. Removing a chip rebuilds the text with bare addresses first.
  - People are offered after a trailing `from:` (Senders) or `to:` (Recipients); picking one keeps the operator.
- **Account progress.** A calendar whose first pass has finished stays Complete with a full bar; later delta passes, which run alongside mail, no longer reset or animate it. Bars animate only changes in progress while their stage is live, never a relayout.

## D120 — tenth export: day strip, folder tabs, Spam and Trash (Mail UI87)

- **Day strip.** Fallback range 0–24 (Core67 default). Per day: a full-height `Theme.ruleWidth` separator at x 24 (`accentAlpha(.45)` today, `inkAlpha(.2)` otherwise; it replaces the first hour rule) and a marker row at x 30, y 5, 5 px apart: date number 12 px DemiBold (accent today), `ddd` 10.5 px uppercase `inkAlpha(.5)` with .06 em spacing, then `·` (`inkAlpha(.25)`) and Today/Tomorrow/Yesterday (date colour, .85) when it applies. The marker exposes `label` (for example "11 Fri · Today"). Hour labels hide at index 0, at the last index, where they would overlap the marker, and within .42 h of now on today. The right-hand date label is removed; "Back to today" stays top-right away from today. The now line's time label hides when it would overlap the marker and a 7 px dot marks the line instead.
- **Folder tabs.** Inbox · Sent · Drafts at 20 px, then (live profiles) a 1×14 px `inkAlpha(.14)` divider and Spam, Trash at 14 px Medium, `inkAlpha(.32)` until selected or hovered. The chevron menu, its popup and the All mail entry are removed; `setFolder("all")` stays for search and the sender card. `s` cycles Inbox → Sent → Drafts → Inbox and from Spam/Trash returns to Inbox.
- **Spam/Trash list** (`binMode`). Spam rows come from `google_spam_list`, one per `thread_id` (`remote:<thread>`, `spam: true`, message count per page), paged by `next_page_token` through Load more; they carry no cloud mark. Trash uses the local folder. No section headings and no row buttons. Strip (`binStrip`, shown while rows exist): 14 px above, 24 px sides, "Auto-deleted after 30 days" 11.5 px `inkAlpha(.45)`, a rule, and "Empty spam"/"Empty trash" (`inkAlpha(.7)`, error colour on hover). Header stat: shield or bin icon, count, "in spam"/"in trash"; Spam counts Gmail's conversations from `google_bin_counts` once known, else the loaded rows; Trash counts the local folder. Tooltips: "Messages Google marked as spam · auto-deleted after 30 days" / "Deleted messages · auto-deleted after 30 days". The delayed stat hides. The rail shows only "All N" (selected). Empty: "No spam. Nice." / "Trash is empty." at 24/22 px, 13 px `inkAlpha(.5)`, or the listing error in the error colour. Switching to Trash clears category and filters like Drafts and Spam.
- **Reader.** Spam threads open through `google_remote_thread` in the normal reader. In Spam and Trash the Reply/Forward/Delay row is replaced by `binActionsRow`: "Move to Inbox" (inbox icon, prominent, tooltip "Not spam · move to Inbox and trust this sender") or "Restore" (restore icon, tooltip "Move back to Inbox", `fileConversation("restore")`), then "Delete permanently" (bin icon, error text, `withAlpha(error,.4)` border, `.08` hover fill).
- **Confirms** (`binConfirm`, destructive). Delete: "Delete permanently?" / "“{subject}” will be deleted for good. This can’t be undone." / Delete. Empty: "Empty spam?" or "Empty trash?" / "N message(s) will be deleted for good. This can’t be undone." (Gmail's message total when known, else the conversation count) / "Empty spam"/"Empty trash". `reconnect_required` opens "Reconnect to delete for good" whose Reconnect opens Settings → Accounts and starts that account's reconnect. `FmDialog` widens to its button row so Cancel and the action never wrap.
- **Backend.** `binCounts {account, folder, messages, conversations}` and `binBusy`; `notSpam()`, `deleteForever()`, `emptyBin()`; `binActionFinished(action, ok, result)`. Success removes the row, opens the next and toasts "Moved to Inbox · marked not spam", "Deleted permanently", "Spam emptied" or "Trash emptied"; Trash then refreshes counts. The restore toast reads "Restored to Inbox". `spamRequest`/`spamReply` and `SpamState`/`SpamList`/`SpamReader` are removed.
- **Notices row (fixes Mail UI85).** `NoticeButton` width is its label's implicit width plus icon and 16 px padding, fixed in the layout; the row text has zero preferred width and elides first. "Always load images from {sender}" may shrink to 120 px only after the text is gone. No negative margins.

Consumers: Core67, Gmail30, Calendar33, CLI51; `gui-mail-selection`, `gui-html-message-body`, `gui-integration`.

## D119 — developer console and loaded images (Mail UI86)

- **Console geometry.** A modal popup at the top of the window, full width, `max(360, 76% of height)` capped at the window, over a 40% shadow overlay. Enter: 180 ms fade plus 16 px slide down. Theme colours; system monospace (SF Mono on macOS; Cascadia Mono, JetBrains Mono, Consolas, DejaVu Sans Mono and so on elsewhere) at 12 px on a 19 px line.
- **Opening and closing.** ~ or ` toggles, except in editable fields other than the console's own grep and command fields, and except while a search popup is open. Esc, `exit`/`quit`, the close button and a press on the overlay close it and restore the previous focus. Workspace shortcuts are fenced while it is open.
- **Top bar (40 px).** Gold `~/fuckmail`, faint `$`, `console`; the `logs`/`state` tabs; on the right `tail -f` (green with a pulsing dot while following), pause/resume (gold while paused), clear, export, a rule and close.
- **Filter row (logs tab, 34 px).** `level ≤` and the eight syslog names; the chosen level and every more serious one read in their level colour, the chosen one filled. `fac` with all and the eight facilities. A 24 px bordered grep box. Filters live on `DebugConsole` (`level`, `facility`, `grep`) and reset the view to the latest matching line.
- **Lines.** Columns: 92 px time `HH:MM:SS.mmm`, 62 px level, 56 px facility, then the message, which wraps. The row tint and text colour follow the level:
  - emerg, alert and crit: a red badge with canvas-coloured text on a faint red row;
  - err: red text on a faint red row;
  - warning: gold text on a faint gold row;
  - notice: ink;
  - info: .78 ink;
  - debug: .45 ink.

  Typed commands echo as gold `› command` with no level or facility. A line with JSON ends with a faint `{…} N lines` hint. Clicking a line opens `└ key=value` details (two spaces between pairs), then, for JSON, an indented block with a faint fill and a left rule. In the block, keys are muted, strings green, numbers blue, true green, false red and null grey. A `copy` button copies the two-space pretty JSON and reads `copied` for 1.2 s. Text in the block is selectable, and pressing it does not collapse the line. One line is open at a time.
- **Tail.** While following, the view stays pinned to the end, including as rows expand and the window resizes. Wheel, drag, scrollbar and Page Up/Home stop following and mark the current count. The "↓ N new" button (gold, bottom right) counts matching lines since then. The button, End, `tail`, `tail -f` or scrolling back to the end resumes following.
- **Command line (34 px).** Gold `›` prompt and caret, placeholder "type help", ↑/↓ history. Commands: help, clear, `level <name>`, `fac <name|all>`, `grep <text>` (alone clears), tail, pause, resume, state, logs, `sync <account>` (address prefix or display name), export, exit/quit. Unknown input answers in red "{cmd}: command not found · type help". Echoes and replies show whatever the filters and are not written to `gui.log`.
- **State tab.** A read-only tree refreshed each second: app (version with build fingerprint, Qt, pid, uptime, theme, log lines), ui (view, account, thread, settings tab), accounts by display name (address, status, last sync, retention, auto summarize, auto label), llm (provider, endpoint, status, model, queue, average latency from `assist_settings_get`/`assist_progress`), network (image proxy, blocked domains, image cache) and db (path, size, schema, WAL pages). Top-level keys are gold, strings green, numbers blue, booleans green/red, null grey. Values are live only, never persisted or exported.
- **Status bar (22 px).** Left: `● tail` / `○ scrolled` / `‖ paused`, "N lines · M shown", the active filters. Right: last-hour err count (red above zero), warn count, `~ close`.
- **Backend (`DebugConsole`).** Levels are syslog 0–7. QtFatal → crit, QtCritical → err, QtWarning → warning, QtInfo → info and QtDebug → debug; ERROR/WARN/INFO/DEBUG events map likewise. Facility comes from the IPC method family, the named source (Qt categories `qt.*`, `qml` and `js` are ui) or the engine scope. The ring holds 5,000 lines and the `lines` model the latest 600 matching; a burst of more than 50 lines resets the model instead of inserting row by row. Pause drops new lines from the console while `gui.log` keeps them. Clear hides retained lines without touching the file. Export writes the matching lines to Downloads in `ISO8601 <n> level facility message {details}` form, with JSON underneath at four spaces, and notes the path in the console only. JSON payloads come only from explicit projections (`fields.json`, engine `category_ids`), scrubbed and bounded (depth 6, 64 keys, 50 items, 16 KiB).
- **Images row (amends D118).** It shows while images are hidden, failed through the proxy, or loaded (`HtmlMessageBody.imagesShown`).
  - Loaded: the icon is the plain picture; Load images becomes a green check with "{N} image(s) loaded" (`imageCount`); the text reads "Images loaded for this message only".
  - Allowed sender (`senderAllowed`): the text reads "Images from {sender} load automatically" and the button "Stop always loading from {sender}" (tooltip "Ask again before loading images from this sender").
  - Stop always loading calls `HtmlContent.disallowImages`; loaded images stay.
- **Message headers.** A folded message expands from a click anywhere on its header row, which shows the accent outline only while hovered. An open message collapses only from its chevron: clicking its header does nothing, so its address (now selectable text) can be copied. The outline also shows for keyboard focus, never after a mouse click.
- **Contact card hover.** Clicking a name or avatar opens the card at once. Hover opens it after 280 ms. While a card is open, hovering another contact switches only after 450 ms, and never while the pointer is moving down toward the card. The card stays open while hovered and closes 400 ms after the pointer leaves both it and its contact.

## D118 — ninth export: remote search, message notices, unsubscribe, retention badge (Mail UI85)

- **Search dropdown.** A last row, above a rule: cloud icon (pulsing while searching), "Search remote mailbox", a status line and a switch (`backend.remoteSearch`, session only; hidden for synthetic profiles and in Spam). The status line is hidden while off or empty, else "Searching {address}…", "N more result(s) from the server", "Nothing more on the server" or the failure (`backend.remoteSearchState`: busy, count, error, address).
- **Remote rows.** `conversations()` inserts the current query's remote rows (`remote: true`, id `remote:<thread>`) by `latest_at`; in the Inbox only among Done rows, with `section` `done:yyyy-MM`, skipped when that month is collapsed. Rows for another account, folder or query never show. A 13 px cloud sits before the subject with the tooltip "Not on this machine yet · fetched from the server". Remote rows are left out of `snapshotMailRows`.
- **Remote thread.** Opening `remote:<thread>` calls `google_remote_thread`; the newest message gets `reader_sections` as stored mail does. No draft read, no read-on-open, no mutations or reply/forward (backend guards), and the action row is hidden. The meta line reads "Read from the server · not on this machine". The selection is not remembered for restart. Attachments (`state: "remote"`) do not preview automatically; open, save and download go through `google_remote_attachment` (`attachmentFile`).
- **Notices group (`MessageNotices`).** Above the message text, 16 px before it: a rule, the rows that apply with a rule between, a rule. Rows are 36 px: 13 px icon, one elided line at .55 ink, 26 px text buttons that highlight on hover. Hidden when no row applies.
  - Mailing-list row, only for the latest message when it is received, `conversation.mailing_list` is present and its source has `unsubscribe`: "Mailing list · {month} this month, {year} this year". The button icon is an envelope for `one_click` and `mailto`, an external-link arrow for `link`; tooltips name the method. "Unsubscribe" → "Unsubscribing…" → done: a green check and "Unsubscribed from {sender} · it can take a few days to stop", or "Unsubscribe page opened in your browser" for a link; the button hides. A failure shows its reason in red and keeps the button. Backend: `unsubscribe({id, source_version, method})` → `unsubscribeReady(ticket, result, ok)`; success patches `mailing_list.unsubscribed` and opens a link's URL.
  - Hidden-images row, only in the HTML view with hidden remote images: "{N} image(s) hidden so {sender} can’t tell you opened this" with Load images (this message) and "Always load images from {sender}" (`remote_images_from`). A proxy failure reads "Proxy unavailable — images not loaded" with Retry. The separate image bar is removed from `HtmlMessageBody`.
- **Accounts.** After the address, an outlined 16 px badge "Keeps {period}" or "Keeps everything" from `mail_retention`, tooltip "Mail older than {period} is removed from this machine · still in Google" or "All mail is kept on this machine".

## D117 — one To review, one Delayed (Mail UI84)

- "To review" and "delayed" in the Inbox header, the account tab badge and hover card, and the all-clear state count only Inbox conversations (`inbox_review_count`, `inbox_held_count`).
- A conversation read or marked Done while open moves to its new section on the list refresh that follows the change. It stays pinned at its old place only when the view no longer returns it, such as under an unread filter. Sections therefore never repeat.

## D116 — Spam view (Mail UI83)

- The folder menu lists All mail, Trash and Spam; the folder button reads "Spam" while it is open.
- Spam replaces the list with a live Gmail listing: a note that spam is read live and not kept, then rows of sender (bold when unread), date, subject and snippet, and a status line (reading, empty or the error, with Try again) and Load more.
- Selecting a row reads that message live into the reader pane: subject, sender and date, then its plain text. Links and images are not loaded.
- Category chips, sender kinds, counts and single-key actions are off in Spam. Spam is never remembered as the folder to reopen. Backend: `folder` accepts `spam` without a Core query; `spamRequest(method, params)` and `spamReply(ticket, method, account, result, ok)` run the two methods as Google operations (60 s).

## D115 — the signature is ordinary composer text (Mail UI82)

The composer has no signature box and no Edit signature menu item. A new or reopened draft's body starts with two blank lines, then the account signature; the person edits or deletes it as normal text. Nothing is added at send. A body that is only the account signature is not authored: it does not open the reply box on a thread, show the collapsed draft, or ask before discarding. Consumers: Core63, CLI (unchanged grammar), Quality.

## Composer draft sync is silent unless something is wrong (Mail UI81)

The composer no longer shows Google draft sync progress ("Saved to Google.", "Uploading…", newer edits waiting) or a standing Check Google button. The status and its Check Google / Replace actions appear only when a draft needs the person: a conflict, an unknown or failed result, an upload that was not prepared, a content mismatch, feedback from a check, or a check in progress. Backend field: `googleDraft.attention`.

## D113 — signatures on the card and Image loading (Mail UI80)

- **Signature row.** It sits between Labels and the Local model and shows the signature's non-empty lines joined by " · ", or "None" (at .35). Edit, or Add when there is none, opens "Signature · {Account}".
- **Signature window.**
  - 560 px wide, with "Added to new messages and replies from {address}" and a close button.
  - A 150 px plain-text editor on the canvas colour, 14 pt, with the placeholder "No signature".
  - "Leave empty for none. Existing drafts keep theirs.", then Cancel and Save.
  - Save is active only with an edit, trims trailing whitespace, closes when saved and stays open with the error when refused. Esc or a click outside closes it.
- **Settings → Mail.** It no longer has Signatures. Its subtitle is "How the mail list looks, what stays blocked, and how images load."
- **Image loading.** A 15 pt title and a paragraph, then rows split by rules:
  - **Load through a proxy**, with its note and a switch. Turning it on with no saved host shows the fields without enabling anything.
  - **The proxy fields**, while it is on: Host, Username and Password in a 100 px label column.
    - The status line reads Testing, Reachable (with the time), the failure, "Enter the proxy host to test it", "That doesn’t look like a host name" (amber), "Not saved yet" or "Saved".
    - Test works on the saved proxy; Save needs a host. Forget removes a saved password.
  - **Image cache**: the size of 256 MB with Clear.

## D112 — seventh export: Accounts, account tabs, colours, labels modal and Local LLM (Mail UI79)

- **Accounts card.**
  - **Header:** the avatar in the account colour, which opens the colour picker. Name and address, then one status line:
    - Downloading · about X left, the later of mail and calendar;
    - Processing · about X left;
    - Up to date · N min ago;
    - Paused, or Paused N ago for a pause made in this session;
    - Disconnected, which opens reconnect when clicked.

    Then icon buttons for sync (spinning while syncing), pause or resume (which also holds the model) and ⋯ (Edit…, Remove). There is no Current badge.
  - **Rows:** a 100 px label, a meter, then controls. The count sits in a 136 px slot and the short status elides.
    - **Mail:** "48.2k of 61.8k", or "N of M+" while Google's list is still being counted. Its bar is amber while active and neutral when done. The short status is Complete, "81% · 8 min", Paused at N% or Stopped at N%. The row is hidden when mail sync is off.
    - **Calendar:** its event count, shimmering during a first pass. The row is hidden when calendar sync is off.
    - **Labels:** up to four names and "+N", with Edit ›, which opens the labels modal.
    - **Local model:** a silver-blue row. The meter shows done as a solid segment and queued as a pale one.
      - Short status: "29% · 3,323 queued · 3 h", "Off · 5.2k not processed", "No model" or Complete.
      - Controls: Set up (without a model); Catch up ▾, shown only when mail is unprocessed; the Summarize and Label chips; and the model's pause or resume.
  - **Catch up menu:** "Summarize & label older mail from…", then Last week, Last month, Last year and All N unprocessed, each with its count and estimate. A footnote says it runs in the background and skips processed mail.
  - Archived accounts keep their archive actions.
- **Account tabs.**
  - The badge is 18 px in the account colour, at .22 when active and .12 otherwise.
  - A 22 px progress ring shows amber while downloading and silver-blue while the model processes; its space is reserved so tabs don't shift. A red dot marks a disconnected account.
  - Names always show, followed by the review count.
  - A 300 px hover card appears after 250 ms and takes no input. It shows name, address, "N to review · N delayed" or "All reviewed", then the Mail and model rows, each with a bar, count and status.
  - The header sync button shows the icon and the time ("2 min", "just now", "Syncing" or "Retry"); its tooltip has the full text. "Syncing" covers new-mail checks, a requested refresh and the first sync, not a background download of older mail (`progress.mail_backfill`, D141).
- **Colours.** The picker is an 8×2 grid of 22 px swatches with the current one ringed. Calendar events use the account colour for their rule and corner badge; other accounts' events use it at half strength.
- **Labels modal.** "Labels · {Account}", 720 px wide, with the address and the existing label table and Sync with Google. Esc, the close button or a click outside closes it. Settings → Mail no longer has a per-account section.
- **Settings.**
  - Every section uses one 880 px column, and rows fill it; paragraphs stop at 640 px.
  - One-line subtitles:
    - General: "Appearance, text size and notifications for the whole app."
    - Accounts: "Connect accounts and see what each one is syncing and processing."
    - Local LLM: "Pick the on-device model that summarizes and labels your mail."
    - Mail: "How the mail list looks, and which domains stay blocked."
    - Calendar: "Defaults for new events, reminders and the calendar view."
    - Shortcuts: "Keyboard shortcuts. Click any row to change it."
  - The Accounts intro, the Shortcuts note and the Local LLM provider explainer are removed.
- **Local LLM.**
  - The page contains Provider, the endpoint with Detect, and the model. Choosing a model saves it at once.
  - **Test** is beside the model picker. Its result panel shows Running with elapsed time, then one of:
    - Passed: first token, total, tok/s and the returned JSON;
    - Failed: not valid JSON, with the raw reply;
    - Error: with the server's address.
  - **System prompt** and **User message** are separate boxes. The user message box shows `{{messages}}` and `{{labels}}` as chips above it, and an amber warning names a missing variable.
  - Each box has Restore default on the left, which loads the built-in text as a draft and is disabled at the default. Cancel and Save are on the right, active only with edits, beside a reserved "Unsaved changes" label. Save briefly shows "Saved".
  - A character and token count covers both boxes.

## D107 — sixth export list details and one HTML/Text choice (Mail UI76)

**List rows**
- After the sender name, a muted "+N" (Medium, tabular numbers) shows Core's `others`. It hides at zero. Hovering shows "Also on this thread:" with `other_names`, plus "and N more" beyond them.
- After the subject, the thread and attachment counts share one style: 11 px, muted, tabular numbers and no border.
  - The speech-bubble `thread` icon with the message count shows only above one message. Hover text: "N messages in this thread".
  - The paperclip with the attachment count. Hover text: "1 attachment" or "N attachments in this thread".
- A row's summary, complete or partial, replaces its preview line.
- Previews come from Core as readable text without web addresses (Core59).

**Reader**
- Plain text fills the message column, without a line-length limit.
- The HTML/Text header toggle and the … menu save `message_format`, so the choice applies to every message, including after a restart. Their tips say "in every message". Moving between messages keeps the choice.

**HTML drawing.** The vendored litehtml carries the local changes listed in `gui/third_party/litehtml/VENDORED.md`:
- measurement reuse;
- shrinking fixed-width columns;
- relative inline shift;
- percentage heights;
- media breakpoint restyling;
- height-only images.

The container adds two behaviours of its own:
- **Break points in long runs:** after `/ - ? & = | \`, and every 30 characters in a run over 60, without changing the text. Zero-width spaces are dropped.
- **Wrapping preformatted text:** `pre` wraps by default.

Consumers: Core59, Local assistance22, Quality77.

## D106 — developer console layout and tailing (Mail UI75)

The modal console fills the app window inside a centered 32 px margin. Five subsystem groups replace D103's separate low-level scopes: Mail (mail, drafts, sending and search), Calendar, Sync (sync, auth and accounts), Model, and App (settings, remaining IPC infrastructure and Qt). All shows every group. Severity and text filters compose with the scope. Aligned monospace time/level/event columns use the current theme, with warning/error markers and a quiet Model tint. Selecting a row opens its full timestamp, source, message and formatted, selectable metadata. Full unfiltered report copying and runtime state remain available.

Opening and changing filters start at the latest matching event. Wheel, scrollbar, touch and keyboard navigation stop following when reading older events. Reaching the end, End or Jump to latest resumes following. New events append to stable rows; front eviction preserves the first visible event identity and its pixel offset. If that event expires from the 1,000-entry history, the view stays at the oldest retained entry. Capture and bounded disk logging continue while the view is paused. Consumers: GUI DebugConsole, Core58/Calendar31 reminder queries and Quality76. Schema/fixtures/protocol unchanged by this packet.

## D105 — HTML reader, remote images and blocked domains (Mail UI74)

**HTML view.** In HTML mode a received message renders through `HtmlMessageBody`, which draws Core's sanitised HTML with `FmHtmlView` (`FuckMail.Html`, litehtml) on a light paper card with radius 6 in every theme.
- Parsing, layout and 512 px tile painting run on one worker thread. Tiles stay within one screen of the viewport, under a 32 MiB tile budget and a 24 MiB image budget across views.
- Layouts wider than the column scale down to 0.5×; beyond that they clip.
- The default font follows the app's text size. Selection, copy and link hover use the layout snapshot, never the worker.
- A report of `too_large`, no HTML part, layout failure or over 2 s of layout shows the text reader with the one-line notice from `failure`. Text and View source stay available.

**Frames.** Content, blocked-link and font changes lay out once, at the next polish.
- The view remembers the last 128 layout heights, keyed by content, width, font, blocked links and image states.
- `HtmlMessageBody` shows a result the GUI already holds (`htmlContent.cached()`) without waiting for the engine. A message shown again therefore has its final height on its first frame, and its tiles paint a frame or so later.
- Blocking or unblocking a link keeps the height and the old tiles until the new layout paints.
- `shown` is `ready`, or `loading` with a known height; the image bar and paper follow it. There is no fade.
- A message's first visit appears after the IPC round trip and the worker layout. It is not yet prepared with the text reader.

**Images.** `cid:` and `data:` images, and remote images already cached, show at once. Other remote images stay placeholders until the user asks.
- The bar reads "N images hidden to keep senders from tracking you · Load images · Always from <sender>". Load images requests only hidden or failed remote images. Always from saves `{account, email}` to `remote_images_from` and loads the images; that sender's later messages load theirs at once.
- The proxy-failure bar reads "Proxy unavailable — images not loaded · Retry". Nothing falls back to a direct connection.
- Blocked hosts and `http://` sources are never requested.
- Placeholders are vector-drawn: a dashed #c9ced6 rounded box (radius 6) with 135° #f4f5f7/#eef0f3 stripes, an icon, and alt text, "Image unavailable", "Blocked domain" or "Not loaded: http" in #8b919a 12px. A hairline box is used under 48×32, and a 24 px chip 96–240 px wide when the size is undeclared.
- Declared sizes never reflow. An undeclared image reflows once when it loads, and results arriving together cost one layout.

**Links.** Links to a blocked host are grey and struck through, and never open. `linkHovered(href, box)` and `linkExited()` drive the link card. `backend.htmlContent` provides:
- `linkSummary(host, conversation)`: messages in every account outside this conversation, with `complete`;
- `blockedBy(host)`, `linkBlocked(href)`, `block(input, source)` (returns an error string) and `unblock(pattern)`;
- `normalizePattern`, `hostOf` and `linkLabel`.

A pattern is an exact host, ignoring `www.`, or `*.domain` for the domain and every subdomain.

**Settings → Mail.** `RemoteContentSettings` holds:
- Use an HTTPS proxy (`remoteProxyEnabled`), with host[:port], username and password saved to the system keychain. The password field clears after saving, and Forget removes it.
- Save and Test, which reports the round trip or the failure.
- The image cache size (of 256 MB) with Clear.

The Blocked domains list (D104) uses the same service and `link_pattern_counts`. `htmlContent` sends each request on its own engine connection, runs at most 6 image requests at once, and cancels a message's image work when the message is left.

Consumers: Core57, CLI44 and Quality75; Search7, Calendar30, fixtures0/3 and protocol1 unchanged. [Execution record](../research/html-mail.md#d105--html-reader-implementation-2026-09-24).

## D104 — fifth export: reader layout, summary states and Calendar today (Mail UI73)

**Thread members.** Messages sit in 8px-radius blocks with 12px padding that bleed 12px past the column; odd-indexed messages take a 2.8% ink tone and there are no hairlines between messages. A collapsed row is 44px high, with hover 5% on toned rows and 2.2% on the others. The sender is a pill 24px high (radius 12, padding 0 8 0 3, shifted 3px left) holding an 18px avatar and the name; its hover fill is bg4 and hover or click opens the contact card. Collapsed rows use the same 18px avatar beside the name with a 7px gap. Details, summary, body and attachments start at the sender's left edge.

**Recipients.** Below the heading, pulled up 8px, one line per role: a lowercase key at 35% ink (at least 18px wide plus 6px), then comma-separated names at 55% ink that turn full ink on hover, show the address as a tooltip and open their contact card. A received message addressed only to the user omits the line.

**Spacing.** The summary box sits 10px below the recipients. The body starts 16px below a summary or its progress box, otherwise 10px below the recipients, otherwise 2px below the heading. Messages with attachments show "N ATTACHMENTS" (11px, uppercase, 40% ink, letter-spaced) with a hairline filling the rest of the row, 24px above and 10px above the tiles. The attachment viewer and count still cover the whole thread.

**Summary.** Summarize is a 26px AI-coloured icon in the header before `…`, visible only on received messages with summaries enabled and no summary shown; its tooltip names the local model. Resummarize is an AI-coloured item in the message's `…` menu when a summary exists. In progress: 3% ink fill, dashed 18% ink border, "Reading the message…" or "Writing the summary…" at 70% ink, elapsed seconds, and a progress bar (55% on 10% ink); the caption reads "Summarizing · {model} · on this machine · keep reading" and offers Cancel. A summary that arrives while the card watched appears at once in green (rgb 111,191,138 at 55% border and 8% fill) with "✓ Done" for 1.8 s, then fades to the AI colours over 0.6 s. A failure shows a red box with an alert icon, "Couldn't summarize — {model} stopped responding." for a timeout, otherwise the engine's message without a closing "Try again", and a red underlined Try again. The prototype's simulated every-third failure is not ported. The caption reads "{model} · {Month D YYYY HH:MM}".

**Links and blocked domains (UI only).** The HTML reader's content service owns link matching, the blocklist, counts, image fetching and the proxy (D100–D102). Hovering a link in either the HTML view or the text reader opens a 360px card below it (above near the window bottom): domain, "Not secure" for http links, "Blocked" or "Blocked by {pattern}", the full URL in monospace with the host emphasised, how often the domain was seen ("Still indexing this account" until counts are complete), and Copy link, Block domain or Unblock. It stays open while the pointer moves onto it and closes 200 ms after leaving. Text-reader links show their destination without the scheme or `www.`, in ink with an underline (Qt draws it in the text colour rather than the export's 35% ink), and a blocked link does not open. Settings → Mail lists Blocked domains for every account above the per-account settings: an add field (Enter to block, "Not a domain yet" while invalid, the service's error in warning colour), each pattern with where it was added and its hit count, × to unblock, and "Nothing blocked yet." when empty. The HTML reader's remote-content settings follow it.

**Calendar today.** Month marks today's cell with a 6% accent tint and a 1px 45% accent ring; past-day numbers show at 50% ink and past events at 50% opacity. Today's cell draws a now line (1px accent, 7px dot at its left end) between the last finished and next upcoming event. It shows a window around now: two events when the line is needed and there are more than two, otherwise three; "+N more" counts every event not shown. Week headers dim past day numbers. Sent mail counts use the same muted colour as received in Month, Week and Day's sidebar.

**Reader body.** A message in HTML format renders through the HTML reader's view when the service exists; if it fails, its notice shows above the text reader. A queued Send's draft check is kept across navigation. A view save that meets another interface's newer save reads that view, reapplies the unsaved keys and saves again without an error; closing waits for it instead of stopping. Recipient errors show under the composer's recipients. Collapsing a list section keeps its header where it was, and saved list positions are measured from the list's origin.

Consumers: Quality74; Search7, Core56, Calendar30, CLI43, schema33, fixtures0/3 and protocol1 unchanged. [Execution record](../research/prototype-september-23.md#d104--fifth-export-2026-09-24).

## D103 — scoped debug console (Mail UI72)

Every event line has a subsystem scope. The console filters by scope, severity and text without changing capture. App, IPC, Sync, Auth, Model, Mail, Calendar, Drafts, Sending, Accounts, Settings, Search, Qt and Other are separate choices. GUI IPC dispatch/completion entries carry the local request sequence so each operation can be followed. The view retains its position when new events arrive while the user is reading older lines; it follows the end only while already at the end. Changing a filter resets to the beginning. Clear view removes only the in-memory history, and the UI shows the active profile's diagnostics directory. Copy report includes unfiltered recent events and runtime state. GUI and engine log files rotate independently at 2.5 MiB, each keeping one previous segment. For a new profile the GUI waits for the engine's private-profile admission before creating its log directory. Consumers: native GUI and Quality73; protocol1/schema33 unchanged.

## D101 — fourth export: search, Inbox sections and Calendar band (Mail UI71)

**Top bar.** The view segment reads [gear | Mail | Calendar]: Settings is an icon-only 32px segment with the same active fill, tooltip "Settings — ,". The old right-hand gear and its divider are gone. The primary button reads New in both views, with an envelope icon in Mail and a calendar icon in Calendar; its tooltip names what it creates.

**Search.** The box holds the whole [Search 7](search.md#query-grammar-revision-7) query; its placeholder is "Search <account>" plus " sent" or " drafts". The dropdown is parented to the box, 440px wide and at most the window width minus 32px, hung from the box's right edge and never left of 16px. Pressing outside the box and dropdown, Esc, or Enter closes it; Enter and Esc also return focus to the workspace. Empty text shows Recent (clock, text, remove; up to six, newest first; a click runs it) and Narrow it down (`subject:`, `body:`, `from:` in monospace chips with a description and a muted example; a click appends the operator and keeps typing at the end). While typing it shows Core's reading as chips (Subject or body, Subject, Body, From or To, Label, Has, Status), each with remove, and the result count on the right, or Core's message for an unreadable query; that query keeps the current rows and raises no app-wide error. A last word of two or more letters completes to a matching operator. After a trailing `from:` it lists up to five people from `search_senders` (initials, name, address), requested 120 ms after typing stops; picking one fills in the address. Enter saves the text to the account's `recentSearches` view key. The filter popover and the old dropdown sections are removed. On restore, saved sender, status and attachment filters become query text; date filters outside Sent are dropped. The sender card's filter action sets `from:<address>` in All mail, the scope its count covers.

**Inbox list.** To review and Delayed always show while browsing, even when empty; an empty section shows one 40px line with an accent check ("All reviewed. Nice work." or "Inbox zero. Enjoy it." when Delayed is empty too; "Nothing waiting on later."). Searching or filtering hides empty sections and keeps the No matches state. Done splits into one section per month of each conversation's latest message in this computer's zone ("September"; the year only outside the current year; "Undated" last), newest first, with Core's full counts. Every section, empty or not, collapses from its header (tooltip "Show N conversations", "Expand" or "Collapse"), and the state is saved per section in `collapsedSections`. Rows have no state dot; the sender name and address start flush with the row padding, and the kind glyph follows the name, which elides before it. To review rows use semibold sender and subject in full ink, preview at 68% and address at 50%; Delayed rows regular at 85%; Done rows regular at 60%, with 50% previews and 40% addresses. Sent and Drafts keep their leading arrow and date grouping.

**Calendar.** Day, Week and Month share a header band 36px tall plus 28px per all-day row, sized by the busiest all-day day of the anchor's week (Core-side `calendarBandRows`, since Day view presents one day). At most four rows show; a day with more shows three chips and "+N all day". A Week column holds its date row (26px), mail pill and chips, with an accent rule under today. Day view's band holds only "All day · Title" chips resting on a neutral rule. Day view's sidebar starts with a row as tall as the band: "↓ N received" and "↑ N sent" with 18px numbers (sent in accent), 12px labels and a chevron; a click opens Mail. Without mail the row is an empty spacer with the same rule. Hovering an empty half hour in Day or Week shows a ghost (dashed faint border, 2px left rule, 3.5% fill, "+ HH:MM – HH:MM", at least 20px, clamped to 24:00). It hides over an event, when its range would overlap one, while dragging, while the editor is open, off the column, and without a writable calendar; it is decoration only. Month view adds a 56px week axis ("W38" in the hour-label style, the current week in accent) whose click opens that week, on today for the current week. Its weekday header shares the band height, with today's weekday in accent; cells draw only right and bottom rules. The event editor slides 24px in over 200 ms by position, with no scroll offset. Consumers: Core56, Search7, CLI43 and Quality72; Calendar30, schema33, fixtures0/3 and protocol1 unchanged. [Execution record](../research/prototype-september-23.md#d101--fourth-export-2026-09-24).

## D099 — GUI memory (Mail UI70)

`SettingsView` is created when Settings first opens and is kept for the rest of the session. Each page is built the first time it is shown and then kept; a page's own `Layout.maximumWidth` still applies. `FmDateField` creates its month grid only while the picker is open. `AttachmentViewer` creates `MediaPlayer`, `AudioOutput` and `VideoOutput` for the first video shown and destroys them when it closes. A video attachment tile decodes its poster without audio and destroys its decoder after the first frame; its duration is kept.

`FmIcon` wraps the native `FmIconItem` (`FuckMail.Icons`). It draws the former Canvas geometry at the window's exact device pixel ratio into a texture of `round(size × ratio)` pixels. Each window keeps one texture per name, colour and size and releases them when its scene graph is invalidated. A process must call `FmIconItem::registerType()` before loading `FmIcon.qml`. `FmButton` is a Templates ToolButton with the same public properties, padding and geometry; its icon row and shortcut hint load only when set, and its tooltip only while hovered.

`requestMessageSource(id, version, withRaw = false)`: cached and published reader sources omit `raw`. Only a `withRaw` request returns it, and that text is never cached. `MessageSource` fetches it for the source dialog or when the message menu opens, and clears it when both are closed. Consumers: Quality71; Core55 and CLI42 unchanged. [Execution record](../research/prototype-september-23.md#d099--gui-memory-2026-09-24).

## D098 — Calendar view performance (Mail UI69)

Day, Week and Month present events from one six-week engine read per month (Agenda keeps its own 42-day range). The GUI retains up to eight windows, keyed by range, selected calendars and display zone. Switching views, moving within a month or opening a Month day publishes from memory in the same event-loop turn. A window whose projection hit the occurrence budget is never sliced for a narrower view: the exact visible range is read instead, and neighbour prefetch is skipped. While Calendar is visible and no request is pending, the next and previous months load in the background. Forced reloads (Calendar generation, selection, zone or account changes) clear the cache but keep the current range's rows until the new read replaces them. A cache miss for another range clears rows that belong elsewhere. The loading line appears only after 400 ms.

Notify signals are split by what they change: `eventsChanged` (Calendar rows, view and date), `timelineChanged` (Mail timeline), `calendarSourcesChanged` (catalog), `calendarStatusChanged` (readiness and coverage), `todayChanged` (the date in the display zone, host zone or UTC), `clockChanged` (minute), and `diagnosticsChanged`. `todayDate`, `clockDay`, `nowHour` and `clockNow` read live values; only their notifications are rate-limited. Mail day labels use `clockDay`, so they re-evaluate at midnight rather than every minute. Hidden Day/Week grids, Agenda lists and date pickers instantiate no delegates. The Month grid keeps its last dates and rows while another view is shown and reuses its 42 cells. Tooltips (`FmToolTip.shown`), button icons, Month mail pills and "+ more" rows load only when used. Moving the Mail timeline reads only its three days. View, date and timeline navigation save quietly 800 ms after the last change, merged with any explicit preference save; shutdown waits for a pending navigation save only while connected. Automatic polling reloads preferences only when `status.preferences_revision` differs. Consumers: Core55 and Quality70; CLI42 unchanged. [Execution record](../research/phase-2-calendar.md#d098--calendar-view-performance-2026-09-23).

## D094 — Flight Progress v2 detail review (Mail UI68)

Text follows the prototype's fractional sizes (12.5, 11.5, 13.5 and 10.5px) through point sizes. Fixed line heights centre each line in its box as CSS does, so labels, headings and buttons keep the reference rhythm. The thread summary names the conversation's other people (sender, then Cc) with "and" before the last name; the user appears only on their own messages. The newest received message shows relative time today, then weekday/time within a week, then date. The header Sync shows "Syncing…" while running and "Synced N ago" after a mail success. "Retry sync · N ago" keeps the last mail success. A Calendar-only account shows no mail time (D089), and the tooltip keeps the exact last mail sync.

Calendar uses the Day/Week/Month segmented control and a date navigator with a trailing chevron. Event blocks, the details popover and the editor follow the export's order: when/relative time, place/video/people rows and conflict note; Ends, then Repeat, Notes, Remind and Show as. Reminder choices read "At the start", "5 min before" … "1 h before". The header's All calendars switch is the only account-visibility control; Settings → Calendar no longer repeats it. Mail date grouping reads Grouped / Plain list. Shortcuts use the reference description, section rules, keycaps, a live hint line and a Reset all link. The Labels table uses capital headings with the count after the name. The composer's From/To/Subject labels, "Name or address" placeholder and flush fields match the export. The Undo toast shows its shortcut keycap and accepts a click from the moment it appears; the fade no longer hides it for the first frame.

Native-only controls that the export lacks stay: Calendar "···" menu, provider/prompt editors, time zone, signature, local capture and Reset all. Mockup-only reminder preview/time jump and synthetic Meet/Repeat affordances stay out. Core51, Calendar30, CLI42, Quality68, schema32, fixtures0/3 and protocol1 are unchanged. Affected consumers: Calendar (view chrome only), Local assistance (Settings layout only), Quality (updated native expectations).

## D092 — third September 23 export (Mail UI67)

Implement the supplied v3 element order and dimensions: stable 44px message headers with 26px avatars/controls, inline sender address, short time with full hover date, HTML/Text, menu and collapse; detail-only opacity transitions and 38px indentation. Per-message summaries occupy the full column with AI color, provenance caption and an equally sized progress surface. Every message and the earlier-message group can collapse independently. Thread subject and participants wrap at narrow widths.

Thread list rows expose total attachment counts and fixed selected/multiple-selection backgrounds. Media uses uniform responsive cards, verified retained image/video bytes, visible-only video posters and one full-window viewer with navigation, real playback/seek/mute, Downloads export and source release on close/navigation. Other files use the existing verified native-open boundary. Single and bulk Done/Delay/read/label/Trash actions offer a six-second durable Undo; bulk restores the successfully reverted selection. Navigating to a different conversation dismisses that offer. Empty states distinguish an empty filtered view from all reviewed mail.

Account initials identify mailbox tabs and timed Calendar events. Accounts has plain names, Edit/Remove menus, one editor for name/retention/sync, truthful retention dates and same-account reconnect. Mail settings owns per-account Auto-summarize/Auto-label above Labels. Shortcuts records immediately in place, persists via shared preferences, transfers same-scope conflicts, supports None/Default/Reset all, rejects fixed keys and replaces old bindings. Text fields and popups retain their keys. D089/D090 Calendar catch-up, reminders and Auto-read remain included. Consumers: Core50/CLI42/Calendar29/Local assistance18/Quality67; schema31, fixtures0/3 and protocol1.

## D090 — September 23 prototype controls (Mail UI66)

The current-account Calendar mail pills, labelled All calendars switch, two Calendar reminder preferences and full-window reminder follow the founder's September 23 update. The new reminder is scheduled by the engine; dashed prototype preview/jump controls are excluded from production. Settings → Mail → Labels gains Label, Description and Auto-read columns. Each account/label has a checkbox with current-state help; Receipts starts on, every other label off. Consumers: Calendar28, Core49, Local assistance17, Quality66; schema30/fixtures0,3/protocol1 unchanged.

## D089 — Calendar sync and scope disclosure (Mail UI65)

The header Sync action is available for an account with mail or Calendar read permission. It requests a Calendar refresh as well as mail when granted, and includes Calendar work in its in-progress state. The displayed timestamp is explicitly the last successful **mail** sync, never evidence that Calendar is current. Calendar-only accounts show no mail timestamp. When a local Calendar selection or current-account mode hides permitted calendars, the Calendar page shows a scope notice and Show all control; the control clears the saved calendar selection, restores all-account visibility and combines the view. Preserve deliberately filtered views until the user invokes Show all. Consumers: engine/Calendar27/Quality65; schema29, fixtures0/3/protocol1 unchanged.

## D088 — clean threaded messages (Mail UI64)

Thread reading shows only each message’s new contribution, without quoted-history disclosure controls or Original Message/attribution headers. Preserve unquoted answers between quoted blocks. All message headers, including the newest, can expand and collapse with pointer or keyboard; keep an explicit toggle and a reversible earlier-message group. Exact original-source inspection remains separate from the normal reader. Core48/Gmail25 trim newly acquired reading/search copies while preserving exact source blobs; existing archive rows are not rewritten. Schema29/fixtures0,3/protocol1 and readiness0/9 remain unchanged.

## D087 — navigation and reader stability (Mail UI63)

Repeated keyboard input must retain its exact final target without unbounded reader work. Accumulate every key against the cursor, coalesce viewport/open work at a 16 ms frame interval, and cancel queued intent on account/query/mouse/new-message changes. Disable old-reader actions until its cursor target is selected. Received layout tables must not block the UI thread: linearize nested or malformed tables and projections exceeding 64 cells or 32 rows; preserve simple data tables, safe formatting/links and every cell’s text. Stored source and authored composition do not change. Cold navigation must prepare saved source, summary and body together, without provisional body or stale summary replacement. Adopting a prepared summary invalidates earlier controller result tickets. Reserve one status line so provider acknowledgment transitions cannot move the body; elided feedback exposes its full text on hover. Preserve editable focus, draft saves, read-on-open and account/source fences. Consumers: native GUI and verification; shared Core/CLI/schema29/fixtures0,3/protocol1 unchanged.


## D086 — mail list and account backlog

The September 22 prototype is the current visual target. Mail rows show sender name and address, subject/count above preview, category and trailing time, with summary icon only for an actual current summary. Use one mouse/keyboard cursor without a stale focused-row border; the open conversation and explicit multi-selection remain distinguishable. Arrow, Page Up/Down and Home/End navigation skips collapsed groups, reveals the target and supports Shift ranges. Typing and popup navigation keep priority. Accounts adds a second three-column statistics row (Messages / Summarized & labeled / Backlog) and a missing-only date-range menu. Show measured processed/total progress, per-message failures and Stop; preserve progress across page/account navigation, cancel on GUI session end. Display unavailable Google totals explicitly, never sample counts or invented duration estimates.

Retired reader components must not request a source under the newly selected account. Admit source requests only for an exact member/version in the current reader, retaining existing generation/lifecycle and source-integrity checks.

Consumers: Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62. Schema29, fixtures0/3 and protocol1 unchanged.


## D085 — scrollbar visibility (Mail UI61)

The conversation list and the thread reader keep a wide always-visible vertical scrollbar. Every other scrolling surface — dialogs, popups, settings, calendar, composer and previews — uses that same scrollbar only when its content overflows. Horizontal mailbox and label tracks stay off. Consumers: native mail list and composer checks. Schema29 and other boundaries unchanged.

## D084 — reader publication and navigation stability (Mail UI60)

Mailbox and label tabs have no horizontal scrollbar; retain horizontal wheel/trackpad and keyboard selection/reveal. Content scrollbars remain wide and visible. Prepare the latest retained message's source, metadata and saved assistance before showing a cold reader, and reuse that prepared view on return. Neighbor/hover prefetch is local, bounded and read-only. Only a deliberate open of a fresh matching snapshot may submit a read receipt. Source/summary loading must not replace an initial plain-text layout with a differently sized HTML layout. Preserve account/source/lifecycle/generation checks and explicit fallback when source data is unavailable. Never retain proxy JSON values after their owning temporary is destroyed.

## D083 — Calendar mail labels (Mail UI59)

Consume Core46 daily counts in the day/week/month date headings. Query only while these Calendar views are visible. Date/account/zone/connection changes invalidate old rows immediately; generation-fence replies and validate their exact scope, dates and nonnegative integer counts before publishing. Refresh changed store revisions at most once per two-second interval; unchanged revisions issue no query. Hide/close stops the observer. Counts follow Calendar's combined-account toggle and remain visibly unavailable on failure, with cached-only coverage and full direction/scope details in an accessible tooltip. The shared `CalendarMailCount` component uses existing fonts, colors and mail icon; reserve its row before month events. Consumers: Calendar26/Core46/CLI40/Quality60; schema29/fixtures0,3/protocol1 unchanged.

## D082 — session debug console (Mail UI58)

Backtick or tilde opens/closes the debug console in the app window; Escape closes and restores prior focus. Do not intercept text in editable fields, including message recipients/subject/body and search, or while either search popup is open. Opening diagnostics must not save, navigate, mark mail read or trigger provider work. Fence workspace shortcuts while open.

Keep up to 1,000 timestamped session events, at most 2,048 characters each, in memory while the console is closed. Include GUI Qt warnings/errors, IPC method/outcome/duration, long-operation activity, observed local-model job outcomes and changed Google sync/progress/error states. Report rate-limit codes and retry delays when supplied by the existing backend; do not invent provider events or cooldown duration. Suppress unchanged routine successful polls. Capture ends with the GUI; raw detached-engine stderr and unobserved between-poll transitions are outside this feed.

Expose selectable plain text, event severity/text filtering, a runtime-state view, pause/resume of the view, clear history and Copy report. Copy always includes current state and all retained severities regardless of filters or pause. State includes GUI/engine fingerprints, schema/fixture/store revisions, connection/queue/editor flags, aliased accounts/counts/coverage, sync and configured quota limits. Diagnostics use explicit metadata projections; no request/response payloads, mail bodies, search phrases, recipients, OAuth material or full profiles. Redact recognizable addresses, secrets, URLs, paths and quoted text from diagnostic strings before retention. New diagnostic fields require explicit projection. Consumers: native GUI/backend and native verification; engine/CLI/storage contracts and schema29/fixtures0/3/protocol1 are unchanged.

## D081 — responsive browsing and stable controls (Mail UI57)

Keep signature editors alive while account counts and sync status change; preserve focus, selection and unsaved text. Mailbox tabs show a quiet two-pixel activity line and the actual cached-message count during acquisition. Unknown inventory totals never become fabricated percentages; pause/error states stop the animation and remain accessible.

Opening another thread changes the reader immediately, independently of a prior thread's pending read receipt; only a fresh deliberate-open snapshot may mark its exact members read. Prioritize current reader work over automatic refreshes, retain bounded observed caches, and avoid rendering collapsed message bodies. Prefetch older mail several viewports ahead. Live paging resumes after the last ordered row across sync changes, deduplicates identities, retains the visible position, and stops automatic retries on errors. Strict snapshot cursors remain the default for existing callers. All scrollbars have a visible, wider hit target and track whenever their scrolling surface is shown.

Consumers: Core45/Mail UI57/Quality58. Gmail24/CLI39/schema29, fixtures0/3 and protocol1 unchanged. This request authorizes scoped navigation, scrolling and acquisition measurements; other Phase 2 acceptance gates stay open.

## D079 — stable attachment feedback and clear Send state (Mail UI56)

Reserve the attachment filename/status row for idle size, Saving, Saved and failure text. Status changes must not alter the tile height or shift subsequent email content. Elide long errors in place and expose their full text through hover/focus disclosure and accessibility; keep the retry/save control available. Modal saving feedback occupies its existing metadata line. Retain decoded images during asynchronous size changes, begin full-image loading as the viewer becomes visible, and honor raster orientation in preview dimensions and decoding. Close open attachment dialogs on identity change.

Production Send uses one Sending status with no obsolete delayed-Undo instruction or duplicated progress wording. Existing unknown-outcome recovery remains explicit. Consumers: Mail UI56/Quality56; Core44/Gmail24/CLI39/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged.

## D077–D078 — sending and attachments (Mail UI55)

Previously initialized, connected Google accounts resume acquisition once on GUI startup after preferences load. Respect saved service choices, removed accounts and explicit pause; new account setup still chooses retention before import. Paused sending offers Resume sync. Send immediately enters exact save/preflight and provider submission with no artificial Undo delay. A fixed overlay shows indeterminate Sending progress, including attachments, then confirmed Sent; no fabricated percentage or minimum duration. Unknown outcomes retain explicit read-only recovery and never auto-resend. Accepted unchanged drafts leave the editor and shared Drafts projection; newer edits and authored history survive.

Saved summaries appear immediately on navigation, including delayed cached-result reads. Only a job observed running in the current message may animate into its result.

Available raster attachments show a fixed-height thumbnail and open a shared-style modal with fit and actual-size viewing, scrolling, Escape/Close and Save. Every available attachment has a minimal Save button and native Save as picker. The backend uses the existing account-scoped verified attachment export and writes an atomic, length/digest-verified destination; a failed copy preserves an existing file. Freeze account/attachment identity for the picker and request. Progress and failures stay with the attachment. Unavailable content is explicit; unsupported images retain saving. No network images or automatic external application launch. Raster decoding is bounded to 64 million pixels; saving has no image-type restriction.

Consumers: Mail UI55/Quality55, Core44/Gmail24/CLI39/schema29. Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged; readiness remains 0/9.

## D076 — stable summarization surface (Mail UI53)

The initial Summarize control changes to Summarizing… with an indeterminate line inside the same 29-pixel-high surface. Reserve label/action width so queued/running state changes do not resize the button or move the email body. Remove the separate classification cancellation row for the combined job. Keep cancellation in the surface's actions menu. Do not fabricate completion percentages.

Once the committed result is available, fade in the summary and animate its required height over 220 ms with eased motion and clipped contents. Keep progress active between job completion and result readback; do not flash the idle button. Preserve existing text and height during regeneration. First failures appear inside the same card; retry retains the expanded card and error context until its outcome replaces them. Preserve source disclosure, bounded long-summary scrolling, sender correction, incoming-only eligibility and existing job/navigation guards.

Mail UI53/Quality53; Core43/Gmail23/CLI38/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness stays 0/9; D068 remains parked.

## D075 — reader date/menu (Mail UI52)

Display the latest received message's short time above its full local weekday/date and 24-hour time (`Friday 11 September 2026, 09:12`), with no visible timezone suffix. Both lines share their right edge. Center the 28-pixel message menu vertically beside the entire date block with a ten-pixel gap. Earlier expanded messages keep the same right-edge menu alignment. Place the star eight pixels before the short time, keeping it out of the date/menu gap. At narrow widths, wrap the full date and center the menu against the resulting block. Format the actual source instant in the machine's local timezone.

Mail UI52/Quality52; Core43/Gmail23/CLI38/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness stays 0/9; D068 remains parked.

## D074 — compact reader and quoted replies (Mail UI51)

Keep the delayed/star indicator at the right edge beside message actions, after the timestamp rather than in the stretchable sender area. CC recipients join the participant row with account-scoped contact cards; canonical email identity deduplicates names without merging distinct people by display name. Omit Reply-To metadata when it repeats the sender address. Constrain long selected-model names inside the dashed Summarize control, with elision and a full-name hover tooltip.

Received message bodies use a display-only projection. Explicit HTML blockquotes and Gmail/Yahoo/Proton quotation wrappers, Outlook reply boundaries, recognizable English reply-attribution/header blocks and plain `>` quotations become collapsed Show quoted reply controls. Each nested reply can be revealed independently; opening a parent leaves deeper levels collapsed. Preserve new inline answers outside quote blocks. Merge the Gmail attribution wrapper with its immediate blockquote and plain attribution with its quote prefix to avoid redundant disclosure levels. Expansion belongs to the current message source/format and resets on identity changes. Limit recursive detection to 32 levels; deeper content remains available as readable retained text when revealed. Ambiguous content stays visible.

Use compact received paragraph margins (zero above, seven logical pixels below), controlled line height, remove empty HTML spacer paragraphs and bound repeated line breaks. Plain paragraph gaps use the same display spacing. Keep ordinary formatting, clickable safe links and attachment actions. Historical links stay inside their quote instead of being repeated as separate buttons outside it. Received projection does not alter stored RAW, extracted text, search, model input or authored/composer spacing. Full retained source remains available through Message source.

Mail UI51/Quality51; Core43/Gmail23/CLI38/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D073 — filing and prototype thread details (Mail UI50)

Inbox uses real incoming Inbox membership. Successful Archive/Trash releases the selected row's presentation pin before list refresh, including context-menu and keyboard/bulk paths. Pending/unknown Google delivery remains contextual and does not stop the local filing result from updating the list.

Use the supplied September 15 prototype and current screenshot for the thread reader: label beside subject; overlapping participant avatars and individually contactable names; sender avatar/name/email at left; sender-kind correction immediately after the name; time/menu at right; full timestamp below the latest received time. All sender/participant/recipient names and avatars open the existing account-scoped contact card on hover or activation, including You in a folded sent row. Name hit areas follow the text, not the entire header. Expanded headers have no row hover or collapse action. A folded row unfolds from its remaining surface without turning its name into a fold control. Hide redundant received To-self metadata; keep sent To and other meaningful To/Cc/Bcc rows. Received summary actions show Summarize and the chosen model in the prototype's dashed control. Per-message summaries, incoming-only assistance and bottom reply actions remain.

The normal synced draft caption is Saved to Google. Raw byte differences alone no longer create a warning. A proven content difference explains that Google's saved draft differs and local text is safe. Keep unsaved/newer-local, conflict, unknown-outcome, Check Google and explicit replacement behavior.

Consumers: Core43/Gmail23/Mail UI50/CLI38/Quality50; schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D072 — account and model feedback (Mail UI49)

Removed account cards offer Delete archive with the common destructive confirmation. Name the account, explain permanent local deletion and next-startup unused-file cleanup, and state that Google data is unchanged. Cancel does nothing. Success removes the card and clears a selected archive; unsaved editor buffers keep their explicit disposition. Restoration clears obsolete removal status.

Google setup omits Meetings and offers Local retention: 1 month, 3 months, 1 year (default), All. Save it with the display name before sync; reconnect preserves the choice. Cards show the saved choice. Model selection stays visible during discovery. Remove the workspace-wide top error banner; retain startup and contextual reader, editor, account and Settings errors.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D070–D071 / P2-UI3 (Mail UI48): Calendar Settings starts with Other accounts’ events, Show/Hide; individual calendars and the display zone remain in collapsed details. Timeline dragging reveals the real adjacent day, settles after the existing threshold and retains cached events on arrival. Only the selected panel exposes focusable/clickable events; neighboring panels use their own date, hours, overlap shading and labels. All three panels honor visible accounts and deletion Undo. Consumers: Core41/Calendar25/CLI36/Quality48; Gmail21/schema28/fixtures0,3/protocol1 unchanged.

D069 / P2-UI3 (Mail UI46): the current objective is GUI/UX alignment. D068 remote draft cleanup is parked and schema28 remains active. Search uses the prototype's compact shortcut row, solid selected chips and scope-specific Sent date choices. Quick actions apply existing typed filters without altering search words; dates explicitly use the existing UTC filter boundary. Examples use actual cached sender/category metadata. Additional active filters remain visible and removable after switching scopes. Empty category sections are hidden; the popup scrolls and clips to the available viewport. Graphite, Paper and Terminal are the current visible themes. Core40/Gmail21/Calendar23/CLI35 and fixture/protocol revisions stay unchanged.

D067 / P2-UI3 outgoing presentation. Pending Send appears immediately in Sent and, for a reply, as a sent bubble in its source conversation. Inbox membership and canonical mutation/read-on-open IDs remain unchanged. The listing uses the frozen recipient/body preview and sending/status text; sent content receives no incoming assistance. The same request supports Undo in the interval, then Send status through its context menu. Pending rows support normal navigation/selection but regular filing/label/review actions reject temporary identities. Undo removes the exact preview. Durable pending/accepted content survives restart, and acquisition removes its duplicate. Reader text is bounded with explicit truncation, and retained outgoing attachment previews become openable through the acquired Sent copy. Consumers: Core40/Gmail21/Mail UI45/CLI35/Quality45, schema28, fixtures0/3 and protocol1.

D066 / P2-UI3 (revision 44) adds six-second Calendar deletion Undo before the existing conditional Calendar command. The native queue preserves the confirmed account, event/recurring scope and revision, temporarily hides only matching Calendar/timeline/search rows, and restores current projections on Undo. Close/disconnect cancels unsubmitted deletions; invoked requests retain existing durable recovery. Schema28/fixtures0,3/protocol1 unchanged. Scoped native Calendar Undo, editor/layout, search, occurrence, reminder and existing Send/draft Undo checks pass, along with configured relocated package151. The full prototype goal remains open; see D066 and the prototype execution record.

D065 / P2-UI3 (Mail UI43) implements durable draft-discard Undo with exact revision/lifecycle checks, captured authored buffers and unfinished recipient input, shared CLI/GUI recovery and grouped six-second toast Undo. Schema28 adds the snapshot journal and recipient-input storage; fixtures0/3 and protocol1 stay unchanged. Capture/upload continues verifying retained bytes before use. Scoped Windows Core, engine, CLI and native checks, actual schema27→28 migration/refusal/reopen, strict lint and configured relocated package146 pass. The full prototype goal and provider-cleanup/Calendar-Undo/pending-Sent gaps remain open. See D065 and the prototype execution record.

D064 / P2-UI3 adds the prototype's six-second GUI Send Undo interval and shared action toast. The client saves first, binds the pending action to the original account/lifecycle/authored revision, and starts no Send command during the interval. Undo, GUI close or disconnection cancels pending actions and preserves drafts. Navigation may continue; a late result cannot replace a different editor. Dispatch rereads the same draft/current Google revision and uses the unchanged exact Core Send protocol; it never automatically resubmits an uncertain result. CLI33 exposes an optional six-second pre-request cancellation interval. Mail UI42/Quality42, Core38/Gmail19/schema27, fixtures0/3 and protocol1. Scoped Windows native/CLI checks, explicit Ctrl+C cancellation, strict lint and configured relocated package135 pass. The GUI test uses the actual MailView, closes/reopens its composer, waits the real six seconds, and verifies lifecycle, disconnect, late-read and close cancellation. The previous package115 is preserved. Draft-discard, Calendar-delete Undo and full pending-Sent presentation remain required for prototype parity.

D063 / P2-UI3 adds explicit From-account transfer with source revision and Google revision checks, verified retained content, destination ownership and durable exact-request replay. A move preserves the original local history and waits for the destination Google draft readback before removing the unchanged original Google draft. Unknown deletion outcomes only reconcile the same request; explicit retry starts a new deletion attempt only after a known rejection. Account lifecycle changes fence cleanup. The composer and CLI share transfer/status/retry; source provider IDs never become destination IDs. Schema 27 adds the bounded transfer journal and preserves existing receipt bytes. Core 38/Gmail 19/Mail UI 41/CLI 32/Quality 41, fixtures 0/3 and protocol 1. Scoped Windows Core/provider/native/CLI, actual migration/refusal/reopen, strict lint and relocated package115 checks pass. Full prototype alignment and other-host acceptance remain open; package94/schema26 is preserved in ignored relocation94.

D062 / P2-UI3 adds serial multi-file composer input and bounded cached-recipient suggestions with account attribution. Preserve exact attachment retry receipts, captured draft ownership, unsaved edits and selected routing. Suggestions are read-only until chosen; schema 26, fixtures 0/3 and protocol 1 are unchanged. The D062 decision and [execution record](../research/prototype-september-15.md) own the boundary and pending verification.

D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

D060 / P2-UI3 adds the working Ollama/local OpenAI-compatible selector, provider-aware discovery and one model choice. Switching providers clears model/discovery state and preserves all prompt content and account policies. System/User fields remain plain text. The local server controls model execution; standard model IDs do not establish weight identity. See Local assistance 12.

P2-UI3 message headers show sender name/email, recipient metadata and a per-message menu. Format switches between the retained text and a passive best-effort HTML alternative. The native formatter reconstructs supported text/table/list/link tags and removes resource tags, arbitrary attributes and CSS; it never loads remote images, local files or scripts. Links use the existing explicit open action. View source shows selectable plain text with a Copy action; shortened previews say so. Source reads and format changes are read-only, follow exact account/message/source identity and cannot summarize sent mail. Historical archives retain these read controls. This component does not claim browser-grade received HTML fidelity.

D059 / P2-UI3 follows the September 15 supplied prototype throughout Mail, Settings and Calendar surfaces. Shared compact confirmations retain captured action identities and unsaved/recovery safeguards. Inbox maps to Core’s `received` scope; sent-only conversations stay in Sent. The settings palette choices are Graphite, Paper and Terminal. See the scoped coverage and verification record.

D058 / P2-LLM2D limits all assistance to received mail. Reader queries expose the actual sent flag; sent targets fail before inference and historical sent results remain stored but hidden. Combined failures display once; null/empty tool calls are accepted as no tools, and truncation has its own error. Schema 24 preserves prompt/account choices while expanding the former default response allowance to 4,096 tokens. See Local assistance 11 and its scoped execution evidence.

D057 / P2-LLM2C simplifies Settings to editable System/User content with normal paragraph breaks. Preserve stored message order and all undisplayed request fields on edits. Schema 23 and inference are unchanged. Native content round-trip/preservation and relocated Windows package checks pass; see the local assistance execution report.

D056 / P2-LLM2B exposes the complete editable Ollama JSON request: system/user messages, all five variables, full response schema and generation options. No hidden envelope or schema is injected. Schema 23 archives and wraps prior prompt text, preserving model/account choices. Application output/action validation remains independent of editable instructions. Consumers: Local assistance 9/Core 32/Mail UI 34/CLI 26/Quality 34; fixtures 0/3/protocol 1 unchanged. See [the current request contract](local-ai.md) and D056 evidence.

State: Draft · Revision: 66 · Updated: 2026-09-23 · Implementation: D090 Calendar/label native controls pass; configured package remains open. Calendar28/Quality66; schema30 and whole-area readiness0/9 unchanged. Broader post-arrival Mail navigation verification remains open.

D055 / P2-LLM2: one shared model and visible template (`{{labels}}`, `{{messages}}`, `{{partial}}`), combined per-message output, default-on prospective summaries/automatic single-label classification, manual precedence, and summary list previews. Schema 22 fences settings and assisted delivery; fixtures 0/3/protocol 1 unchanged. This supersedes conflicting D051–D053 delivery defaults below; historical evidence remains scoped to its original version. See the D055 decision and Local assistance 8.


D054 replaces the top Local copy status with account-scoped Sync and the last successful Gmail check time. Clicking starts/wakes sync using built-in budgets; progress, failure and never-synced states remain truthful. Disable when disconnected, synthetic, removed or mail permission is unavailable. Failed checks preserve the previous successful timestamp.

Revision 29: D049/P2-UI adopts the September 14 second export: stable 20-pixel Inbox/Sent/Drafts headings, reader spacing, context menus and modifier/range multi-selection. Inbox displays the existing non-Trash all-mail query, with actual imported category labels. All counts describe this same account scope. Selecting or right-clicking does not mark read; explicit opening retains read-on-open. Bulk state/category/filing commands retain exact member snapshots and account lifecycle, execute serially through the existing journal, and stop with an explicit partial result on failure. Later arrivals are never added, and provider acknowledgment remains distinct from local completion.

D045 search dismissal: the search text field handles Escape while retaining its query. Deliberate mouse/Tab/back-Tab focus, typing and Find may open the contextual popup; restoration of focus after another popup or window must not reopen a dismissed search. Keep Mail and Calendar query/filter state independent. Consumers: Mail UI 28/Quality 26; Calendar 21 and other shared boundaries unchanged. Verify against actual Main input and popup-focus restoration.

D043/D044 live UI repair: refresh the Calendar catalog when its published generation changes and allow an already-open new-event editor to receive its first writable target without losing typing. Preserve an established target across reorder/withdrawal. Recurring UI authority uses an observed policy epoch, independent of ordinary source bookkeeping revisions: membership, effective permission, write/create eligibility and time-zone transitions advance it, including revoke/restore cycles. Retain lifecycle, exact projected occurrence, scope/action, fresh engine proof and conditional write fences. Timed/all-day/timeline event blocks support native accessible activation; dropdown options expose their visible names. Consumers: Calendar 21/Mail UI 27/Quality 25; Core 27/Gmail 13/Platform 15/CLI 21/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged. Focused regressions and real UI outcomes are recorded in the Phase 2 execution report; whole-area acceptance remains open.

D040 changes Settings → Accounts → Connect Google account to a single Gmail/Workspace email field and Continue with Google. Enter submits a valid address; the app opens the system browser and shows progress/cancel/failure, then closes the dialog after verified success. Reconnect/restore prefill the saved email while preserving the selected stable account/lifecycle revision. Their same-subject checks allow provider-renamed email. Inspect app registration on dialog open; a missing/invalid build registration disables Continue and explains the build issue. No JSON picker or password input appears in the normal flow. Existing sync admission stays explicit. Consumers: Platform 15/Gmail 13/Mail UI 26/CLI 21/Quality 24 with Core 27/Calendar 20/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged.

D039 reuses the top search field contextually for Calendar with a separate query from Mail. A matching themed popup shows the current searched date range, coverage and bounded results; calendar visibility and the combined/account toggle constrain the shared query. Selecting a result navigates to its occurrence and opens details only after a fresh matching identity is available. Fence late responses across query/range/selection/account/lifecycle changes. Mail search behavior and timing targets remain unchanged. Consumers: Core 27/Calendar 20/Mail UI 25/CLI 20/Quality 23; schema 20, Platform 14/Gmail 12/Search 5, fixtures 0/3 and protocol 1 unchanged.

Stable conversation delegates must stop rendering and receiving input as soon as their model index is invalid. Qt may retain a removed delegate object until later destruction; object lifetime alone is not membership in the current list. Complete each structural layout before the next model batch, then publish the final row/previous-group roles once so retained delegates use current content. Structural refreshes preserve surviving selected delegates without painting or clicking retired rows; data-only updates keep their existing narrow notifications. This enforces D038's existing stable-list requirement and changes no shared command or schema.

Revision 24 applies D038 and the preserved September 14 reference. Normal launch starts an empty live database and exposes actual Settings account setup. The startup overlay ends after initial account/preferences hydration, including zero accounts, and releases immediately on a startup error; later mail queries never replay it. Keep the three themes and existing native geometry, with the reference's V workspace shortcut, compact composer controls, inline collapsed drafts, Calendar account/combined toggle and snapped range selection. Calendar scope/action resolution is captured before editing; supported common-rule creation, occurrence/series RSVP and following confirmations use the shared v4 boundary.

Revisited mail uses bounded observed account/view and conversation caches, with independent eight-MiB serialized-payload budgets plus native object overhead. Exact view filters, account lifecycle, source revision and selected draft guard reuse. Cached painting never marks messages reviewed or supplies independent mutation authority; current engine queries still refresh data and control read-on-open. A stable identified message model retains body delegates through read receipts and unchanged refreshes. Local authoring buffers are never cached or replaced by this path. Native reminders use the existing theme and permit explicit local Join/Dismiss without interrupting composition; stale permissions or occurrence changes remove their prior details. Consumers: Core 26/Calendar 19/CLI 19/Platform 14/Quality 22/schema 20; Gmail 12/Search 5 and fixtures 0/3/protocol 1 unchanged.
Revision 23 applies D037/P2-06C to the existing Calendar popup, editor, activity and Mail timeline. A recurring occurrence exposes resolution eligibility separately from edit capability; resolve its complete captured account/calendar/series/original identity before opening an editable form. Dedicated generations fence close/reopen, different selections, source changes and remove→restore. Never substitute the selected mailbox for the event account or treat a local expansion key/master ETag as an instance version. Use the existing editor for explicit one-occurrence edit/cancel, preserve authored fields on errors and explicit version refresh, and make cancellation scope visible. Activity preserves occurrence scope/original identity alongside truthful provider-response versus observed-state evidence. Series/following controls remain unavailable. Consumers: Core 25/Calendar 18/CLI 18/Quality 21/schema 19; unchanged Gmail 12/Platform 13/Search 5 and fixtures 0/3/protocol 1.

The generated-provider native account walkthrough `native-accounts-06` passes in 14.265 s, including saved-before-removal confirmation, retained unsaved-buffer disposition, archive reads and explicit same-subject Restore with no automatic acquisition. Final `gui-dev-ctest-02` passes 18/18 (17 native checks and one Rust build fixture); integration takes 138.25 s within the unchanged D030 aggregate 180 s limit. The final Send test fixture registers a real active navigation target and verifies unknown/archived-account refusal; runtime guards remain unchanged. Eight notices (seven delegate and one teardown) and earlier failures remain recorded. This is functional evidence, not warning-free or performance acceptance. Interim evidence: [P2-01B manifest](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json).

Revision 22 applies D036/P2-01B in existing Accounts settings and Mail/Calendar surfaces. Show explicit Remove, current credential-cleanup status/retry, Open archive and Restore. Active mailbox tabs omit archives. Archive browsing is an account-level read-only mode distinct from imported Google snapshot provenance, with cached source/attachment/receipt access and no read-on-open or mutation/provider continuations. Self-removal saves dirty work before a fresh revision-bound confirmation. Concurrent CLI removal retains an unsaved buffer under its original account/draft identity, fences late saves, and requires explicit disposition before discarding it; never attach it to a fallback account. Handle selected/last-account removal explicitly and preserve ordinary navigation invariants. Restoration requires the same verified subject, keeps retained data and does not start sync or Send. Consumers: Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20/schema 18; Search 5, fixtures 0/3/protocol 1 remain unchanged.

Revision 21 applies D035 to existing native Calendar editor/details and recovery controls. Permit ordinary organizer guest-address additions/removals only when shared capabilities allow them, preserving full source fields outside the form. Explicit save/cancel explains that Google may notify guests. Keep self-RSVP distinct from organizer roster edits. Display provider acceptance and observed-state reconciliation truthfully; neither claims notification delivery. Preserve account/source/ETag/navigation fences and authored fields on stale responses. Calendar roster limitations do not create a contacts application or redesign the prototype. Consumers: Core 23/Calendar 16/CLI 16/Quality 19/schema 17; Gmail 11/Platform 12/Search 5, fixtures 0/3/protocol 1 unchanged.

Revision 20 adds an Actions menu beside Saved sources in the existing Drafts preview. Create plain-text copy confirms primary From, retained recipients/files, omitted HTML/inline placement/thread linkage, no added signature and normal autosync. Edit Google draft requires Core's full-source fidelity assessment and a fresh source check; adoption alone does not upload. Delete Google draft permanently is separate from local Discard and identifies the exact account/source. Retained receipts expose definitive, refused and uncertain outcomes plus read-only Check Google recovery; absence never means sent or confirmed deleted. Confirmations and replies retain account/source/navigation stamps, while original source/export fences and three-theme geometry remain intact. Consumers: Core 22/Gmail 11/CLI 15/Quality 18/schema 16; fixtures 0/3/protocol 1 unchanged.

Revision 19 adds D032 cached remote snapshots to the existing account Drafts list and composer, with selectable text and clearly identified preview/source freshness. Preserve imported From and recipient classes; do not apply signature fallback or editor autosave. Gate subject/recipients/body/signature, formatting/paste/undo, attachment mutations, discard/capture and all Google write actions on persisted read-only provenance through the backend as well as controls. Keep existing local authoring/recovery behavior. A Drafts-only refresh/progress area uses existing category-control space and starts no work without an explicit owned sync session. Account/draft/source-revision floors reject late replies; navigation and reopen retain the correct source. Consumers: Core 21/Gmail 10/CLI 14/Quality 17/schema 15, fixtures 0/3/protocol 1 unchanged.

Source history and exact RAW/attachment exports use cached, revision-bound verified copies. Original HTML and CID relationships remain preserved even when only a bounded text preview can be shown. Opening source does not fetch external content or create provider writes. Three-theme layout, narrow reader geometry and existing button placement remain requirements; no performance pass is inferred from implementation.

Revision 18 connects the existing live Send action to D029: first save the editor, then freeze exact account/draft/local/Google revisions and submit through the shared engine. Show the selected operation outcome separately from editable text; preserve newer edits, account switching and exact historical receipts. Known prepared work can be explicitly resumed or abandoned at its current operation revision; uncertain work offers read-only inspection without automatic resend. Synthetic capture retains its existing behavior. Consumers: Core 20/Gmail 9/CLI 13/Quality 16/schema 14, fixtures 0/3/protocol 1; Calendar 15/Platform 12 unchanged.

Commit and validate recipient text before arming the Send save continuation. Any authored, routing, account or draft change before the save completes cancels that continuation and requires a fresh click; save failure clears it. After the operation is prepared, edits remain independent of its frozen content. Use the current Google revision watermark independently of each operation's revision when merging status. After an earlier version is accepted, explain that explicitly sending newer saved edits creates another email; never silently recreate its consumed Google draft.

Revision 17 keeps local autosave authoritative while owned Google sync coalesces durable draft uploads. Use the existing composer status/recovery surface for pending, acknowledged/readback, changed remote version, missing remote and unknown outcomes; add no per-draft Sync button or new primary layout. Late results match account/draft/operation and never replace newer text. Known conflict replacement is an explicit current-revision action that preserves both versions and explains the remaining concurrent-edit race; unknown outcomes offer read reconciliation without automatic upload. Existing Send stays unavailable until its separate frozen-submission path is verified. Consumers: Core 19/Gmail 8/CLI 12/Quality 14, schema 13/fixtures 0 and 3/protocol 1; Calendar 15/Platform 12 unchanged.

After checking an unknown update, offer explicit replacement only when the freshly observed draft ID equals that operation's retained update target. Unknown creation and a missing unknown target cannot be replaced. Capture the exact clean local/Google revisions and a new request ID at confirmation; dirty edits, account changes and later observations invalidate that confirmation. Historical operation replies must not overwrite current draft status. Show unsaved or newer saved edits as pending even when an earlier revision is synchronized; append any upload/readback fidelity warning so both facts remain visible.

Revision 16 makes the existing Attach control offer a local-file picker alongside retained conversation files. The shared engine imports and selects one file for an exact account/draft/revision; chips display its retained basename/size and keep existing removal behavior. Flush authored edits before submission and preserve them across errors, stale revisions and navigation. A late result cannot retarget another draft or overwrite newer local text. File reading/hashing and capture preparation use bounded engine workers, and long import work leaves the cached-query connection available. Keep the current prototype geometry, themes, source selection and readable controls. Consumers: Core 18, CLI 11, Quality 13, Gmail 7/MIME, Calendar 15 and Platform 12; schema 12/fixtures 0 and 3/protocol 1. This prerequisite exposes no real Send or provider draft upload.

P2-05A2's measured navigation correction keeps a stable native mail-list model: same-view updates notify only changed rows and structural differences, while new account/query results replace the prior scoped projection. Account switching immediately clears the old mail rows. The existing account-scoped query rows remain authoritative for selection, groups, filters, pinning and independent paint checks. Summary-only refreshes must not recreate row delegates. Mailbox tabs likewise retain their controls through account count/progress updates; a press/release spanning a refresh must keep the intended account. Preserve scroll policy and the current design while verifying retained delegates, correct neighboring group headers, paging and account separation. This is a native rendering correction within revision 16; it changes no engine/CLI method, schema or fixture.

Revision 15 integrates account-scoped cached capabilities, explicit sync ownership and truthful delivery/coverage into the existing native design. Retained reading and local drafts do not depend on a provider round-trip; delivery additionally requires grants, a connected account and owned synchronization. Unsupported real Send remains unavailable. Deliberate open uses the exact loaded member snapshot and surfaces pending/failure independently of local acceptance. Settings displays cached existing Google labels and binds only a user-selected destination to a category. Long Google actions use separate authenticated GUI-owned IPC connections, with bounded concurrency and cancellation on close; the serial cached-query connection remains free. Preserve all three themes, current controls and right-aligned reader actions. Consumers: Core 17, Gmail 7, Calendar 15, CLI 10, Platform 11 and Quality 11; schema 11, fixtures 0/3 and protocol 1. Native and performance acceptance must use final integrated source.

Revision 14 prepares D020's live Google workflows while preserving the current prototype and verified P1-10 layout/performance. Mailboxes stay separate; Calendar combines only permitted details. Existing Settings adds actual connect/reconnect/disconnect, provider progress and effective quota-aware cadence when the engine supports them. Display inventory/source/attachment/index coverage and paused/denied/uncertain states without exposing developer implementation detail or inventing completion. Cached navigation never waits for a provider round-trip.

Live deliberate opens preserve read-on-open snapshot semantics; passive import, restoration and CLI queries do not mark reviewed. Real flag/category/filing and draft/send operations use shared capabilities and durable outcomes. Add bounded local-file attachment import and native real-send/conflict recovery to the existing human composer, with explicit account/From/recipient identity. Remote draft conflicts preserve authored local content and known fetched alternatives, without promising atomic Google conditional writes before they are proved. Send is available only for a live capable account; old synthetic captures and pending fixture operations never become outgoing mail. Calendar notification effects remain clear before real scheduling actions, with one provider notification owner.

Retain all themes, compact controls, right-aligned reader actions, readable button states and independent scroll/draft selection. Native/CLI/live-source comparisons and measured navigation during concurrent mail/calendar sync are required before the pilot. Consumers: Core 14, Gmail 4, Calendar 12, CLI 8, Platform 11, Quality 11 and existing Search/Triage. This planning revision introduces no executable API or schema; see [Phase 2 plan](../plans/phase-2.md).

Revision 13 requires directly measured native cached mailbox/conversation navigation and stable resize rendering, plus the current prototype's right-aligned reader actions, readable enabled/disabled action buttons and correct compact geometry in all three themes. Use the existing Quality 100 ms p95 navigation target; a passing search budget is not a substitute. Preserve exact account/source selection, deliberate read-on-open, serial durable writes and authored fields while reducing unnecessary work. Never display another mailbox's rows as a fast substitute. Consumers: Core 13, CLI 7, Calendar 11, Search 5, Triage 6, Platform 10 and Quality 10; schema 7/fresh fixture 3/additive protocol 1 unchanged. Baseline and corrected input-to-painted-content/resize evidence remain required before this spike passes.

Windows package acceptance at `25b8b1e` records passing navigation/source checks, native controls/CLI/restart, readable enabled/disabled local capture and right-aligned reader actions at regular/compact sizes in all three themes. Compact actions wrap beneath labels; selected mailbox controls remain fully visible during Tab/Shift+Tab, with long account names elided and the full existing tooltip preserved. Delayed user opens retain the clicked row/index/group through a real mutation receipt, then hydrate the authoritative reader source; loading saved document text updates action availability without counting as a new authored edit. Final P1-10 native CI/package checks pass on three hosts; physical input/scanout remain separately unverified.

Revision 12 applies D018's September 12 Update 2 reference: shared Graphite/Paper/Harbour themes and text sizes, General-first Settings, typed startup/duration/timeline placement preferences, complete counted collapsible mail groups, explicit sender-kind filters and an account-scoped sender card. Source-hash-bound kind annotations and exact sender/count projections come from Core; the GUI does not infer a person or automation from a display name. Preserve durable composition, stale-result rejection, exact snapshot actions and capture recovery. Consumers: Core 13, CLI 7, Search 5, Triage 6, Calendar 11, Platform 10 and Quality 9; schema 7/fresh fixture 3/additive protocol 1. Source implementation is active; the new controls and projections require their own native/CLI/restart evidence.

Revision 11 applies D016's September 12 reference and D017's explicit confirmation of the P1-08 composition extension. New/reply/reply-all/forward share durable independent draft identities, explicit account/To/Cc/Bcc/subject, frozen source context and local MIME capture. Drafts lists actual saved drafts separately within the active account; it does not manufacture received messages or merge accounts. Existing replies migrate with authored fields, signatures, retained attachment identities and recovery receipts preserved. Account changes never retarget a draft's From identity. Preserve existing conflict handling, exact capture recovery and read-on-open snapshots. The header, list, inline composer, settings and calendar surfaces follow the updated reference; truthful coverage and recovery remain reachable through compact secondary controls. Consumers: Core 12, CLI 6, Search, Calendar, Triage, MIME, Platform and native acceptance tooling; schema 6/fixture 2/additive protocol 1. Alias editing, new-file import, live sending and model/account setup remain later. Implementation is active; earlier schema-5 acceptance does not verify this revision.

Revision 10 aligns native sender validation with Core 11's existing 256 UTF-8 byte/control-character boundary. Invalid sender/date input stays visible in the open filter form without changing active filters. Search coalesces input for 25 ms and cancels superseded unsent conversation queries; received results must still match the current account and generation. This changes neither query meaning nor the 200 ms input-to-painted-result target. Native filter, held-response/account-switch, explicit discard/capture inspection and independent timeline control coverage extend the acceptance harness; executed results were recorded in Phase 1 evidence, kept in Git history. Consumers: Core, CLI, Search, Calendar, Platform and native test tooling; schema 5/fixture 2/protocol 1 are unchanged.

Revision 9 records the active native implementation against Core 10/schema 5/fixture 2 and additive protocol 1. Deliberate conversation opening uses a successfully hydrated member snapshot; passive restoration remains a query. Done uses one read+unstar mutation, Delay changes only the hold, and a reviewed active row remains pinned until navigation/query changes. The composer edits authoritative plain text with a limited HTML alternative, per-draft signature snapshots, retained attachment IDs, explicit discard and local MIME capture. Formatting applies to selected text; paste inserts plain text. Conflicting authored saves retain local fields and require explicit confirmation before saving that version over the latest revision. Capture recovery queries unfinished journal entries before creating a new operation identity, including after restart. Capture history is labeled local and never claims delivery. Global typed preferences and revisioned account metadata are shared with CLI. Native integration/visual acceptance remained open in Phase 1 evidence at this revision. Consumers: Core, CLI, Calendar, Search, Triage, MIME and Platform.

Revision 8 applies D014 and the [Flight Progress v2 reference](../research/flight-progress-v2.md). Phase 1 adapts the native mail/calendar/settings interface, with confirmed read-on-open, Done clearing unread and stars, untimed Delay, and the browsable timeline only in Mail. Mail accounts stay separate; Calendar supplies combined permitted details. Durable drafts, conversation-only Sent and real local preferences replace prototype shortcuts. Consumers: Core, Triage, Search, Calendar, CLI, Platform and Local assistance.

Revision 7 records the next Phase 0 integration checkpoint. Explicit configured-category assignment now joins the shared snapshot actions, with actual QML mouse input and independent CLI readback. Qt-exported formatting and attachments round-trip through persisted Core/MIME capture; the interactive composer remains plain text. Consumers: Core, CLI, MIME, Triage and Platform.

Revision 6 records D012’s implemented experimental slice. The Qt reader, Unicode plain-text draft, shared account/search state, read-only week/footer and settings shell now run against the engine. The Qt formatting experiment and shared MIME builder are tested separately; no live sending is enabled. Consumers: Core, CLI, Calendar, Platform and MIME. See [GUI evidence](../../gui/README.md).

Revision 5 applies D011: the initial-phase trio guides the shell and inline reply; settings is plumbing only, with pictured switches ignored per founder clarification. D009's text-first, user-owned actions and paired-client boundaries remain. Affected consumers: Core, Calendar, Platform, Triage, Search and CLI.

Owner: E — Mail UI. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Top mailbox navigation, resizable mail split and header/footer placement of the Mail calendar strip; conversation/reading interactions, human drafts and signatures |
| Consumes | Core paged views and operation states; Search matching-message results; triage projections; Calendar's bounded date/occurrence view and actions; optional local summaries; Platform dialogs/renderer facilities |
| Produces | Explicit account/member-scoped user commands and authored draft content; visible loading, pending, coverage and failure states |
| Independent start | Mock local API and realistic paginated data; apply the founder's supplied design after review without provider/model network calls |

Related: [core](core-storage.md), [triage](triage.md), [search](search.md), [platform](platform.md), [HTML research](../research/html-mail.md).

Revision 3 records the closer nonfinal mail/calendar direction and the final-design handoff gates. Affected consumers/partners: Core and Triage (member flags, counts and actions), Search (collapsed matching members), Calendar (shared footer), Local assistance (summary states), Platform (renderer, focus/scaling) and CLI (the same command semantics). No shared API or accepted action mapping is introduced.

## Design constraint

The September 12 Update 2 [Flight Progress v2 HTML prototype](../research/flight-progress-v2.md) is the current Phase 1 reference, superseding the earlier same-day export, September 11 export and initial trio. Match its compact header, account tabs, contextual search, New message/event entry, mail actions/composer and settings geometry closely. Use shared palette and text-size tokens throughout. Remove the invented header clock/fixture action and prominent extra toolbars/statistics. Preserve accurate coverage, permissions and recovery through compact secondary surfaces. Build the native GUI and CLI together against the completed Phase 0 foundation. Settings includes functional local preferences and account display names; real account connection/removal and model installation remain later. Done clears unread and stars on its targeted existing members; Delay is an untimed star hold. Neither archives mail, and a model cannot issue either action.

The same opinionated layout, components and visual identity apply on Windows, macOS and Linux. The [visual design brief](../ui-ux.md) fixes mailbox selection at the top and a resizable split mail workspace. Its browsable calendar timeline occupies the selected footer or header position in every Mail scope, directly below the top bar in header mode. Calendar and Settings have no timeline and reserve no empty strip space. The HTML reference guides the compact shared top bar, left conversation list/right reader, typography and inline reply. Adapt its dimensions for usable native scaling and compact windows. Platform input and accessibility must work.

Propose a conversation list beside the reader/composer, with independent scrolling, usable minimum pane sizes and remembered split position. Keep the calendar strip visible across all mail scopes, search, reading and composition, including during scrolling/resizing. Mail UI owns placement and consumes Calendar's event projection; it must not create a second event/reminder store. Preserve the existing draft's account identity when switching mailboxes.

Stars must visibly mean deferred action, with proposed explanatory copy and honest hold age from [triage](triage.md). Read state and action completion remain distinct.

The list groups review, delayed and done mail; its reader uses category chips, Reply/Reply all/Forward and contextual Delay/Done controls with earlier members below. Phase 1 grouping is unread first, otherwise starred, otherwise done; a mixed conversation retains its hold marker even in review. These are projections of message flags, with no separate completion state. Done uses one shared snapshot operation to clear unread and stars on existing targeted members; new replies are excluded. Delay and the menu's Remove Delay change native star state without a timer. Explicit read/unread and Done remain available through the conversation menu/shortcuts. Single-letter shortcuts must not intercept editable text fields. Label list/group counts as conversations and underlying account totals as messages, with coverage and pending-state context. See [Triage](triage.md) for target and count rules.

A collapsed thread must preserve access to all members and reveal the exact matching search message. Read-on-open is confirmed. Proposed trigger: once a user-activated conversation loads successfully, submit the shared read operation against that loaded member snapshot, including its collapsed members. Prefetch, background refresh, automatic selection/restore, summaries and CLI read queries do not mark mail read; later-arriving members are excluded. This is the chosen interaction convention, not proof that every member was inspected. Keep the open conversation readable after its flags change. Optional summaries are absent in ordinary Phase 1 use; later enabled summaries show source, stale and failure state. Attachment availability and sync status reflect actual coverage.

Review, delayed and done headings use Core's complete `filtered_group_counts` before pagination, not counts of loaded delegates. Collapsing a list group hides its rows while retaining its true filtered count and the active reader/draft. Paging, query changes and new arrivals refresh the scoped totals without treating unloaded conversations as absent. Account badges keep their unfiltered account meaning. Show People/Automated controls and a neutral unknown state from Core's authoritative conversation kind: newest incoming member, or newest member for an outgoing-only conversation. Missing or stale source-hash annotations stay unknown. Kind is independent of which source member matched a keyword, does not change flags and is not a live classifier promise.

The sender card uses `sender_summary` for the displayed exact email within the current account, with a full conversation count and at most three recent conversations. It counts every conversation the person is on as sender or on To or Cc (`exact_person`, D140), and its "All N with …" link searches the bare address, which matches the same roles. It never combines matching display names or crosses accounts. Loading the card is read-only; clicking a recent conversation uses deliberate open, and its filter action applies `exact_sender` through the same shared query path. Exact sender combines with the other source predicates on the same member. Show removable active sender/kind chips, preserve them when applying the More filters form and clear them explicitly. Drafts excludes all received-source filters. The prototype's name-based bot heuristic and static sender-card examples are reference data only.

## Mail workflows

Phase 1 includes New message, Reply, Reply all and Forward with explicit From, editable To/Cc/Bcc and subject, authored plain text/formatting, per-draft signatures, retained attachments, revisioned save/discard and local MIME capture. Replies/forwards appear inline with their received source; a new draft needs no fabricated source conversation. Keep every draft tied to its original account and independent draft ID through conversation/account/settings navigation and restart. Draft creation retries reuse their operation identity. Account selection never moves or retargets an existing draft; alias management and new-file import remain deferred.

Freeze the selected reply/forward source member, source content identity and derived routing/reference context when the draft is created, including the first save of a compatible legacy reply. Later arrivals must not change recipients, source quotes or threading headers behind an authored draft. Reply all excludes the account's own address and duplicate recipients. An unavailable source or invalid recipient produces visible feedback, not fabricated routing. Existing conversation-keyed replies remain compatible with their durable draft identities and migrate without changing authored body/HTML, signature, attachment identities or journal receipts.

Recipient editing uses Core's shared bounded routing rules: at most 100 total To/Cc/Bcc entries; email at most 320 UTF-8 bytes; display name at most 4096 UTF-8 bytes; subject at most 8192 UTF-8 bytes; complete routing at most 64 KiB. Mailbox syntax and control-character validation apply before accepting an entry; invalid/incomplete input remains visible for correction and prevents navigation or capture from silently dropping it. Local validation and Core errors must agree. A save in flight preserves later edits to all authored fields, including recipients/subject. Captured or pending work is never presented as delivered; live sending remains later.

Minimise/Collapse hides the inline composer while retaining its authored fields; explicit Reply or reopening the same Drafts entry restores it. A saved draft may expand on its first load unless that draft's restored presentation records it as collapsed. A stale save retains body, HTML, signature, attachments and edited recipients/subject. Confirmed **Save my version** fetches the latest draft and verifies its frozen draft/conversation identity, mode, source member and source hash before adopting its revision. A mismatch retains every local field, the original revision and conflict, cancels close, and explains that reloading replaces the editor; it performs no save. Matching context allows retained fields through the ordinary optimistic save, and another racing edit can conflict again. Every source-backed save includes its loaded source member so Core can reject a different frozen source. **Reload saved draft** replaces local fields only after a successful read and cancels previously queued navigation/capture. Discard is explicit, durable and revision protected. Transient retry alone never resolves an authored conflict by overwriting the other version.

All draft modes use the existing single Core MIME capture journal. Before minting a capture operation ID, query `capture_history` for the current durable draft with `unresolved_only: true` and a bounded first record; conversation-wide history is insufficient when several independent drafts share that conversation. Core filters prepared, in-flight and acceptance-unknown entries before pagination. Recovery reuses the record's exact draft selector, immutable operation ID, saved revision and synthetic outcome, including compatible legacy records. It must not adopt another draft's unresolved operation, replace current authored fields or recapture a newer revision. Explain when an earlier saved revision is being inspected. A failed lookup creates no new capture; unresolved retries start no second transport attempt and make no delivery claim.

The broader product also includes archive where supported, trash/restore, provider folder/label actions and bulk selection; P1-08 does not claim these provider operations. Readable text and human composition with per-account signatures are required early; Phase 1 supplies restrained formatting, while best-effort received HTML display and provider-synchronized drafts come later. Local category assignment uses existing backing; provider label/folder creation requires its own shared workflow.

An alias is an additional sending address associated with an account. It is not a confirmed need, so use each account's normal sending address for MVP and defer alias management. A reply must still select the receiving account correctly.

Conversation rows open messages inline. Inbox, Sent and received-mail search results remain conversation based and reveal the relevant member/snippet. Drafts separately pages actual saved drafts within the selected account, using draft IDs and authored subject/recipient previews; text search is supported, while received-mail sender/category/state/date/attachment filters do not apply. Switching into Drafts clears those unsupported constraints. Refresh preserves displayed pagination and selected authored work. Local capture history remains clearly labeled, inspectable and separate from provider-sent mail. Local storage preserves individual received messages for replies, flags and attachments. Timed follow-up and inferred awaiting-reply tracking remain outside Phase 1.

Proposed threading: use Gmail thread IDs for Google accounts; use reference headers with conservative fallback when the later IMAP phase starts. Subject equality alone must not combine unrelated mail. Never merge conversations across accounts. Define whether a bulk action affects the entire conversation or the matching folder subset, and make that choice predictable.

## Interaction refinements from exec-buddy

Adopt the useful product patterns with the following proposed expression in our existing mail/calendar screens:

- Click a sender to apply an address-based, account-scoped filter with a visible removable chip. Configured category chips filter conversations the same way.
- Restore the selected conversation, expanded message, search/filter state and scroll position separately for each account when switching views or reopening. Missing/deleted items fall back predictably without switching account context.
- Keep read/star/category changes responsive and show pending/failure state; background refresh must not reset the user's action or discard their reading position. In an Unread filter, keep the open conversation readable until the user moves on even when its read state changes.
- Make readable human correspondence the early reading route. Prefer an available plain-text MIME part and safely extract readable text from HTML-only mail; preserve originals and attachments. Later offer best-effort HTML viewing for newsletters/automated mail without promising browser-level layout accuracy. Raw/source inspection stays secondary.
- Offer People/Automated filtering from explicit, current source-bound classification data, with missing/stale entries visibly unknown. P1-09 uses deterministic fixture annotations; it does not enable a live model or guess identity from names. Configured category filtering remains separate and account scoped.

This is product inspiration, not a port of exec-buddy's code, visuals, generic integrations, restricted history windows, or individual-message browsing model.

## Shared application behavior

Phase 1 Settings opens on General first, followed by Accounts, Local model, Mail, Calendar and Shortcuts; reopening during a session retains the visited section. General supplies theme, text size and Open on. The six typed P1-09 keys/defaults and startup behavior are defined in [Platform](platform.md#local-appearance-and-startup). Theme and font changes preserve account selection, scroll/focus and every authored draft field. Sync and Notifications remain disabled with an explicit unavailable explanation. No preference creates provider/OS activity.

Settings also persists local account display names/signatures, row density, Mail timeline hours, week start and selected-calendar visibility. Calendar settings presents independent source selection, 30/45/60-minute default event duration, then Timeline placement/hours and week start. The full Calendar uses a fixed 00:00–24:00 day/week grid starting at midnight; legacy calendar/grid-hour values remain stored for compatibility without ineffective controls. Calendar defaults to combined permitted details. Its explicit source selection is independent of the selected mailbox; switching mail accounts does not filter the combined calendar. Hiding a source changes neither mail scope nor provider permissions. Return restores the prior view/selection/draft; the timeline is shown only in Mail. Account freshness, index and offline coverage describe actual local state. Local categories remain account scoped, with no global label deletion invented from the prototype. Real OAuth, account removal and model downloads/configuration remain later; their simulations must not claim success. Delay remains untimed.

UI commands use the same account-scoped core operations exposed by the [CLI](cli.md). The UI does not own provider/sync logic or require a separate agent automation surface. Built-in AI still cannot write or send email; external command callers use their own explicit workflow and authority.

## Composition boundary

Provide authoritative plain text and a restrained authored HTML alternative with per-draft signature snapshots. Construct standards-compliant MIME alternatives through the shared Core/CLI path and retain authored content exactly. From is frozen to the draft's original account; alias management is deferred. Paste inserts plain text, formatting acts on explicit user selections, and received HTML never becomes an executable composing surface.

The composer exposes no model-generated writing, rewrite, autocomplete or send path. Local summaries are clearly identified in the received-message reader and retain access to their sources; text-first reading is the initial normal path; later HTML display is optional per message and never replaces the preserved source.

## Acceptance gates

- Enter/leave settings without changing account identity, losing the prior mail/calendar position or discarding a draft. Persist the Phase 1 local preferences and verify their visible effect after restart. Keep the timeline on Mail only; do not introduce real account/model setup side effects.
- Select mailboxes at the top and resize the split without losing draft, selection or reading position. Verify the browsable timeline in both header/footer positions on Inbox, Unread, Starred, folders/categories, search, reader and composer views, including a date other than today. Its event identity and selected-calendar scope agree with Calendar and survive mailbox changes. Calendar/Settings reserve no strip space.
- Exercise all themes and text sizes through native controls, independent CLI preference readback and a fresh GUI process; verify startup Mail/Calendar/last and preserve exact authored body/HTML/signature/attachments during appearance changes. Check palette, focus/selection and readable clipped/scrollable layout at compact sizes.
- Verify complete filtered group counts across multiple pages and collapsed groups, source-bound people/automated/unknown filtering, and exact-address sender-card counts/recent navigation without cross-account results or incidental read changes. Applying/correcting other filters must retain active kind/exact-sender constraints.
- Show unread and starred states independently, including older deferred action and first-observed star age. Read-on-open changes only unread; Delay changes only its star hold; replying changes neither flag. Done explicitly clears both on its existing-member snapshot. Test mixed groups, honest counts, stale requests and new arrivals through both clients.
- Mark a successfully user-opened conversation read once for the loaded snapshot, retain its selection while groups update, and keep later replies unread. Failed loads, prefetch, background refresh, automatic restore and CLI queries must leave flags unchanged.
- Use a paginated, million-message fixture without loading the entire mailbox into the UI. Open the exact matching message from search and preserve the account, selected member and scroll position across refresh/reopen.
- Initially read plain-text and safely extracted HTML-only fixtures; copy/select text, handle links/attachments and keep long conversations bounded. Later HTML display is judged on readable content, links and useful images rather than pixel fidelity; scripts, privileged bridges and unrequested resources remain prohibited.
- For Phase 1, compose new/reply/reply-all/forward, edit To/Cc/Bcc/subject, save/reopen from Drafts and verify exact body/HTML/signature/retained attachments across navigation and a fresh GUI process. Independent CLI reads and captured MIME must agree on recipients, subject, frozen source/reference headers and account identity. Test malformed/bounded recipients, later arrivals, wrong-account access, stale edits/discards and uncertain capture recovery without lost authorship or duplicate attempts. Live sending is a later gate; no model writing or draft access exists.
- Triage and file while background sync/inference runs; keep the open conversation readable when it leaves Unread, show pending/error state and preserve honest account totals.
- Before building the affected UI slice, review its final design and resolve entry points absent from the mockups: Inbox/Unread/Starred, folders/categories, drafts/sent, new composition, Reply-all/Forward, setup and account-scoped search. Exercise mixed read/star groups and collapsed search matches without inventing new provider states. Confirm loading, offline, summary-off, failure, compact-window and focus/scaling states.

## Open before affected behavior ships

Validate the P1-08 composer and updated native layout at reference/compact sizes and on required platforms. Phase 1 remains local and synthetic: no real Google connection, model execution, legacy protocols, received HTML renderer, new-file import, aliases or timed hold. New/Reply all/Forward/Drafts and bounded recipient editing are confirmed current scope; their source-freezing, migration, conflict, restart and MIME evidence remains required. Provider-synchronized drafts and live delivery come later. Preserve earlier passes and failed attempts without treating either source presence or an isolated compile as acceptance.

## D052 / P2-LLM consumer boundary

Revision 30 consumes [Local assistance 6](local-ai.md): shared discovery/settings/jobs/results/correction commands, schema-21 storage and manual precedence, bounded owned Ollama work, native/CLI parity and scoped acceptance. Runtime implementation/verification is in progress; fixtures 0/3 and protocol 1 remain unchanged. Earlier evidence retains its original revisions.


## D053 summary presentation and control repair

The reader follows Update 2’s bounded Summary card below thread metadata: 6-pixel radius, subtle assistance-colored outline, 2-pixel left accent, 14/18/15-pixel padding, compact uppercase heading and right-aligned model attribution. A message summary appears beside that message. Source/provenance and re-summarize controls are secondary; long output remains scrollable without displacing all original mail. All model output remains escaped/plain text. Empty, busy, failed, stale and partial states are distinct.

Summary enablement alone exposes both thread and individual-message actions, including retained sent messages. Classification/correction targets only received messages. Requests carry current account/conversation and generation identity; late start replies are canceled, old polls cannot overwrite a newer job, start/cancel admission is immediate, and disconnect ends the local busy state. Source navigation reuses the reader’s exact member expansion/scroll mechanism. Settings preserves edits on refresh/conflict, offers explicit saved-settings reload, ignores obsolete discovery/sample responses and displays readable sample results. Existing session ownership and on-demand summary semantics remain unchanged.

Verify these with native controls and rendered frames, focused engine/CLI tests and the actual local runner; retain original failures. This packet does not expand automatic inference or change schema 21, fixtures 0/3 or protocol 1.

### D061 interaction refinement

Settings → Mail owns account-specific label rows: fixed 150-pixel name/count column, description field, remove action and Enter-to-add. Failed edits retain text; confirmation retains its original account/name/mapping. Google sync shows progress and Stop while allowing other UI work; disconnect keeps edit buffers. The existing local model prompt uses these descriptions. Accounts shows the oldest cached message date as a statistic.

Event details exposes direct Delete/Delete this/Delete series confirmations after resolving the relevant event/series identity. Unknown outcomes reuse the recorded operation; following-scope deletion retains the editor’s existing exception-reset confirmation. Compact date fields keep their complete text visible at larger text sizes.
