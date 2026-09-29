# Changelog

Notable changes to fuckMail. There are no tagged releases yet; everything is unreleased.

## 2026-09-29

- Opening new mail marks it read again after a relaunch, and marking it read by hand works: background labelling no longer makes those actions fail with "State changed". An account whose saved view pointed at a removed message saves its view again.
- Settings → Accounts: each card has one ⋯ menu for Sync now, Pause or Resume syncing, Edit account and Remove account (Reconnect when disconnected). Clicking anywhere on the Labels or Signature row opens its editor.
- Done is grouped This Week, This Month, then one section per older month. This Week and This Month start open and older months start folded; each section remembers being opened or closed, and folded months cost nothing to load. Settings → Mail → "Week starts on" picks the day This Week begins.
- To review and Delayed have a solid header band that stays at the top while you scroll their mail. To review has a new icon.
- The mail list and thread scrollbars hide when idle and appear as soon as you move the pointer, scroll or point at the edge.

## 2026-09-28

- Sending a reply keeps the thread still: the message you answered stays open, the reply box turns into your sent message with a short fade, and the reader never scrolls past the end into blank space. While Gmail takes the message, a thin line sweeps along the bottom of the box.
- Forwarding a message keeps the forward in the thread it came from, as Gmail does, instead of starting a new conversation. Forwards sent before this stay where they are.
- Hiding a thread's earlier messages keeps them hidden. They no longer unfold again a few seconds later in threads where one of them had been opened.
- The reply editor keeps its cursor while a draft saves, syncs to Gmail or the thread refreshes.
- The app is called fuckMail everywhere: the macOS app, Dock and app switcher, the Windows program and the Linux binary (it was "fuckmail-gui"). The Dock shows the app's icon instead of the generic one, even for development builds, and small icons (third-party docks, lists) show it too instead of a blank tile.
- Calendar no longer shows the "Showing N of M calendars" banner; the All calendars switch shows every calendar.
- Month view no longer leaves an empty gap above the grid where Day and Week show all-day events; the weekday names sit right on top of the month.
- The account "…" menu in Settings → Accounts no longer greys out the screen behind it.
- Meeting reminders ring loudly on macOS and Linux too (they used the quiet system beep), with a clearer alarm at one minute before and at the start. Mute still silences them.
- At one minute before and at the start, a reminder you have not dismissed brings the window to the front, even from behind other windows or minimized. On macOS the Dock icon bounces if focus is refused; on Windows the taskbar button flashes.
- A reminder that appears while you type no longer snoozes, joins or closes from the keys you were already typing, and the cursor returns to where you were once it closes.

## 2026-09-27

- Changing an account's mail retention no longer re-checks mail that is already downloaded: only the newly included mail downloads. Accounts download side by side instead of taking turns. When a full re-check is still needed (after a long time offline), progress says "Checking downloaded mail" and moves, instead of looking stuck.
- Emails that are only a subject (a reminder to yourself, "lunch?") are summarized and labelled from the subject instead of failing with "no text to summarize".
- Forwarded messages read clearly in text mode: a "Forwarded message" caption, the original's From/Date/To/Subject as a compact block, its text, then the history it quotes, indented with a bar per level. HTML-only mail no longer runs lines together in text mode ("Thanks,KrisOn Thursday…"), including mail downloaded earlier; new mail is searched and previewed with the same cleaner text.

- Background updates refresh only the affected Mail areas. Progress, calendar and draft updates avoid unnecessary message preparation; updates arriving while a reader loads remain pending.

- Mail reuses cached message sources without copying them. Attachment thumbnails and the image viewer use display-sized decodes, reducing memory and repeated loading during resizing.

- Progress polling reuses unchanged mail counts, and reading or recovering saved message content avoids repeated full-file reads while keeping checksum validation.

- The Mail list/reader divider stays put when you switch accounts: one position for the whole app, remembered across restarts.
- Mail shows as plain text by default. The HTML/TEXT badge now switches just that sender to HTML (or back), and all their mail follows.
- The app reopens where you left it, at the same size, and maximized if it was. If that monitor is gone, it opens in the default place.
- Account tabs lose their pill: a thin underline in the account's colour marks the selected one and turns into a gently pulsing progress bar while an account syncs (amber downloading, silver-blue while the local model works). The ring around the letter is gone; the red disconnected dot stays.
- The account hover card shows whole numbers ("18,420 of 41,300"). Settings → Accounts keeps the short form.
- The Settings button sits in a tray like the Mail / Calendar switch.
- The composer has a title bar: Compose Email, Reply, Reply All or Forward. Starting a reply or forward scrolls the thread just enough to show the whole box, footer included.
- Draft rows no longer have a Discard link; discard from the composer, or right-click drafts in the list.
- Right-click menus on message text, the composer and text fields look like the app's other menus.
- Label chips no longer show a tooltip. The Spam / Trash menu closes when you click anywhere else or switch mailboxes.
- Acting on selected conversations no longer leaves an "N of N saved locally" line; only a problem shows there.
- Reading the last message in To review shows "Inbox zero" as soon as the row folds away.
- The mail list is its full length from the start, so the scrollbar no longer jumps as older mail loads, and dragging it far down loads straight to that point. The Load more button and its "Loading…" flash are gone.
- Switching between Inbox, Sent and Drafts is faster, most of all Sent after scrolling through many pages.
- Clicking a label chip, button or menu no longer leaves a box around it; outlines show only when you move with the keyboard. An opened message's header highlights on hover without an outline, as in the design.
- Sender photos fill their circle instead of showing only their top-left corner.
- Gmail drafts sync again on profiles made before the draft-sync change.
- Every message in Gmail downloads, however large (up to Gmail's own 150 MB limit), including messages with more than 50 attachments. Messages skipped before download on the next sync, and progress no longer lists "unavailable" messages.

## 2026-09-26

- Received attachments share the saved original message instead of keeping a second permanent copy. Previews and downloads reuse a local cache; older duplicate copies are verified and cleaned up after restart. Drafts and offline access are preserved.
- Replies to some Gmail messages, and their drafts, no longer fail to send or save. Messages that still can't be built say why.
- A reply shows a thin progress line along its bottom while it uploads, then a check once Gmail accepts it. It no longer flashes a red error first.
- Reply opens the whole reply box at the bottom of the thread and keeps it there while the thread loads. It no longer closes on its own when the previous reply finishes sending, or right after you discard a reply.
- Reply, Reply all and Forward stay under the thread while you write. Switching turns a reply into a forward (or back) and keeps what you wrote. Boxes open with a short fade, with no jump in the thread above.
- Discarding a new message goes back to where you started it.
- Attachment previews and videos open straight away, even during sync, and sent replies preview their own attachments. The viewer's arrows play every video, video cards always show their first frame, and the attachments row folds away like a month.
- Toggling Inbox months, even quickly with many folded, keeps the clicked header in place.
- Old mail keeps downloading while Gmail history catches up on a bulk change, and that catch-up runs faster. Mail deleted for good in Gmail is skipped without asking Gmail about each message.
- People/Automated works in every account again: label edits no longer wipe the model's answers, and lost answers come back on restart. Senders the model hasn't read show no mark instead of "?".
- People/Automated, labels, day chips, folders and "All N with" switch instantly: the list updates in place, hovering loads the next view ahead, and a quick fade covers the rare wait.
- Empty trash also clears local copies of mail you already deleted in Gmail, and its confirm counts what is here instead of saying 0.
- Mail list rows no longer show a tooltip on hover.
- Settings → Accounts no longer flickers during sync; each account's labels stay put.
- The Calendar search dropdown no longer flickers while you type.
- Done and Delay no longer show "Saved locally · waiting for Google". If Google refuses a change, the reason appears under the reader's buttons.
- Inbox month headers have no tooltip, and showing or hiding a month no longer flashes "Searching…".
- Attach opens the file picker straight away, with no menu first.
- Calendar search has its own box and recent searches. It understands `date:`, `with:`, `title:` and plain words, lists upcoming and recent events, and filters Day, Week, Month and Agenda as you type. CLI: `calendar search --around`.
- Mail download progress shows the real total, a percentage and time left; the account tab's ring shows the same share.
- New mail arrives promptly during long downloads: label changes no longer re-download messages, and new-mail checks get their own slot. The Sync button no longer says "Syncing" while older mail downloads. New mail also arrives while Gmail history catches up on a bulk change made elsewhere.
- Collapsing and expanding Inbox months is instant, and list reads no longer rescan the mailbox during sync.
- Contact card counts every thread the person is on (sender, To or Cc), "All N with …" opens them, and its name and address can be selected and copied.
- Mail polish: alternate row shading, read rows fold out of To review, hover highlights on buttons and folded messages, and a one-message thread's text no longer jumps when its delivery status appears.
- The composer warns when a message mentions an attachment but has none; the event editor warns about overlapping events.
- Settings sits beside the search box, and draft chips get a Show/Hide setting.
- New app icon. The native title bar follows the theme, and the account-colour strip under it is gone.
- The developer console runs `fm` CLI commands against the running app, fills the window and slides in on `~`.
- Open-source preparation: MIT license; packages ship Qt, FFmpeg and Rust crate license texts; new user guide, CLI reference, troubleshooting, privacy and architecture docs; finished plans, internal test harnesses and dead code removed.
- CLI fixes: `unsubscribe` no longer fails on the message-source step, deep commands no longer overflow the stack on Windows, and a late reply to a timed-out request no longer breaks the next one.

## 2026-09-25

- Single-file installers for testers: a per-user Windows setup program (no admin prompt, clean upgrade and uninstall, mail data kept) and a macOS disk image. Linux stays a tarball. Both are unsigned.
- Search matches what you type as written and highlights matches in the list, subject and body. `from:` finds senders and `to:` finds recipients.
- Optional remote search also queries the Gmail account on the server; results open live and nothing is kept.
- Every Gmail draft is an ordinary editable draft. Changes made in Gmail show up within a minute, and discarding deletes it in Gmail after Undo. New messages open inline in the reader; unsent drafts show as chips.
- Answer invitations with Yes, Maybe or No from the email or the event card. Events list guests with their answers and offer Join for Teams, Zoom, Webex and other meeting links.
- Sender pictures (brand icon or Gravatar, otherwise initials), optional, fetched through the HTTPS proxy when one is set.
- Spam and Trash are proper folders with Restore and Delete permanently, which needs full Gmail access. Storage restarts from a fresh schema: delete old profiles and reconnect.
- Reader notices: Unsubscribe for mailing lists (one-click, email or link) and a Load images / Always load images row.
- To review keeps the open item in place while you read; Done, Delay or filing moves it at once.
- Fixes: replies use Reply-To, reminders no longer reappear or chime twice, embedded images are no longer listed as attachments, and current repeating events show in calendars with many old series.

## 2026-09-24

- HTML view renders the HTML that was sent, sanitised, with nothing remote loaded until you ask. Remote images use HTTPS only, optionally through an HTTPS proxy, and hosts can be blocked. CLI: `mail html`, `links`, `proxy`.
- One search box for GUI and CLI with `subject:`, `body:` and `from:`, recent searches and removable chips.
- Inbox is split into To review, Delayed and Done by month. To review counts Inbox only, and Spam is read live from Gmail instead of being synced.
- Accounts show each account's download and local-model progress. Account colours (16 swatches) also mark calendar events.
- Local model: one queue summarizes and labels one email at a time, with a test run and editable prompts. Existing mail is analysed only if you ask.
- Signatures are plain draft text, edited on each account's card.
- Thread view redesign: sender pills that open contact cards, alternating message tones and Summarize in the message header. List rows show "+N" other participants.
- The GUI uses about a quarter less memory and starts faster.
- Metadata-only diagnostic logs, and a developer console with Mail, Calendar, Sync, Model and App scopes.
- macOS fixes: the first account's sync starts, and an old profile is refused at once with a clear reason. Deleting an account no longer breaks Settings saves.

## 2026-09-23

- Local storage is rebuilt on a fresh SQLite schema. Older profiles are refused, so accounts need to be reconnected and re-synced.
- Calendar view switches and paging draw from memory and are several times faster. Use the release build preset day to day; the debug build is much slower.
- New accounts import their Gmail labels once. With a local model set up, synced mail is queued for summaries and labels; history is never marked read automatically.
- Full-window reminders with sound, saved reminder and snooze preferences, and Auto-read per label.
- Manual Sync also catches up Calendar. An All calendars switch and Show all make restricted calendar selections visible.
- Attachment and video viewing with export to Downloads, single and bulk Undo, account editing and reconnect, and editable keyboard shortcuts.
- Fixes: Undo changes only what it recorded, events with reminders off no longer alert, and retention follows Gmail's received date.

## 2026-09-22

- Fixed a crash when moving quickly between messages. The reader prepares text off the UI thread and prefetches nearby threads.
- Quoted reply history is hidden, and each message, including the earlier-messages group, can collapse and expand.
- Faster repeated paging; complex HTML tables no longer block layout.
- Mail list: sender and preview columns, recipient previews, one shared mouse and keyboard cursor, and range selection. Older mail loads as you scroll.
- Account backlog: summarize and label older mail in a chosen date range, with progress and Stop, also from the CLI.
- Calendar day headings show received and sent mail counts.
- A session debug console opens with backtick or tilde.
- More reliable local summaries: malformed model output is retried once.
- Scrollbars always show on the list and thread, elsewhere only when content overflows.

## 2026-09-11 – 2026-09-16 · Foundations

- Rust engine and `fm` CLI with a Qt Quick desktop GUI for Windows, macOS and Linux. Both share one local SQLite store with full-text search, and each has its own lifetime; there is no background service.
- Review-based triage: read on open, Done, untimed Delay, separate accounts and conversation-only browsing, in three themes.
- Google sign-in in the system browser, starting from your email address. Add, remove and restore accounts in the GUI and CLI.
- Gmail and Google Calendar sync, with a choice of how much history to download.
- New message, reply, reply all and forward with attachments, Gmail drafts and explicit Send with Undo.
- Calendar: day, week, month and agenda views; create and edit events, invitations, recurring events, RSVP, reminders and cached event search.
- Optional local summaries and labels through Ollama.
- Packages for all three platforms, with CI on each.
