# Changelog

Notable changes to fuckMail. There are no tagged releases yet; everything is unreleased.

## 2026-09-26

- People/Automated works in every account again: label edits no longer wipe the model's answers, and lost answers come back on restart. Senders the model hasn't read show no mark instead of "?".
- People/Automated, labels, day chips, folders and "All N with" switch instantly: the list updates in place, hovering loads the next view ahead, and a quick fade covers the rare wait.
- Empty trash also clears local copies of mail you already deleted in Gmail, and its confirm counts what is here instead of saying 0.
- Mail list rows no longer show a tooltip on hover.
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
