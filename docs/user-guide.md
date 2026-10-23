# User guide

fuckMail is a desktop client for Gmail and Google Calendar. It keeps a copy of your mail on your computer, so reading, switching accounts and searching work offline. This guide covers the app. The command-line tool is in [cli.md](cli.md), and problems are in [troubleshooting.md](troubleshooting.md).

There are no prebuilt releases. To build the app and register your own Google Cloud client, see [Build and run from source](build-development.md) and [Google sign-in setup](build-development.md#google-sign-in-setup).

## Getting started

1. Open **Settings** (`,`) → **Accounts** → **Connect Google account**.
2. Enter your Gmail or Workspace address and choose what to allow:
   - **Mail:** Read, or Read & write (send, label, file).
   - **Calendar:** Read, or Read & write (create, edit, RSVP).
   - **Local retention:** how much mail history to download (90 days, 1 year, 2 years, 5 years or Everything). The default is 1 year.
3. Choose **Continue with Google** and finish signing in in your browser.
4. Check what Google granted, give the account a display name, then choose **Start syncing**.

Nothing downloads until you choose **Start syncing**. New mail arrives first, and older mail fills in behind it. Once an account has started syncing, the app resumes its sync each time it opens. Each account's card in Settings → Accounts shows progress and has buttons to sync now, pause or resume.

To try the app without an account, start it with `--demo-profile` for sample data.

## How triage works

fuckMail uses Gmail's own two flags, so other Gmail clients see the same state:

| Flag | Meaning here |
| --- | --- |
| Unread | Not reviewed yet |
| Starred | Still needs something from you |

- **Opening a conversation marks it read.** Only a deliberate open counts. Prefetching, restoring the last view and CLI queries never mark mail read.
- **Done** (`E`) marks the conversation read and removes its stars. It does not archive it.
- **Delay** (`D`) stars it, with no due date. It doesn't change read state. Press `D` again to remove the delay.
- **Mark read / unread** (`U`) toggles the unread flag.

The Inbox is split into sections:

| Section | What's in it |
| --- | --- |
| To review | Conversations with an unread message |
| Delayed | Read, but starred |
| Done | Read and not starred, grouped by month |

The daily aim is zero unread and zero starred, reached by doing the work, not by bulk-marking.

**Undo.** Mail actions (read state, Done, Delay, labels, archive, Trash, restore), draft discards, event deletions and declines show an **Undo** notice for about 6 seconds. Press `Z` (or `Ctrl+Z`) while it's showing. A deleted or declined event is only sent to Google when that notice ends.

**Sending has no delay.** **Send** goes to Gmail straight away; there is no undo-send window in the app. (The CLI has an opt-in 6-second delay: `fm google send --undo`.)

## Mail

### Accounts and views

Accounts stay separate, and there is no unified inbox. Switch account with the tabs at the top, or `Tab` and `Shift+Tab`. Mail is always grouped into conversations.

- **Inbox**, **Sent** and **Drafts** are the main views. `S` cycles through them.
- **Spam** and **Trash** are behind the ▾ after Drafts.
- The row of labels above the list filters by label. Choose several to see mail with any of them.

### Search

Press `/` or `Ctrl+K` and type. Results come from the local copy and cover the view you're in. The dropdown shows how your text was read, suggests people after `from:` or `to:`, and keeps recent searches.

Turn on **Search remote mailbox** in the dropdown to also search Gmail itself. This sends your search text to Google and needs the account to be syncing. Matching threads that aren't on this machine join the list; opening one reads it live from Gmail without keeping a copy.

| You type | It matches |
| --- | --- |
| `quarterly report` | Words typed together form a phrase, found in the subject or message text. The last word can be partial. |
| `maya@example.com` | A bare address matches the sender or any recipient |
| `subject:board deck` | The phrase in the subject only |
| `body:invoice attached` | The phrase in the message text only |
| `from:maya` | Sender name or address contains the text |
| `to:finance` | A To or Cc recipient's name or address contains the text |
| `label:Receipts` | That label: its exact name or the only label starting with it. Quote names with spaces: `label:"Travel 2026"` |
| `has:attachment` | Conversations with attachments |
| `is:review`, `is:delayed`, `is:done` | Triage state (`is:held` also works) |

- `subject:`, `body:`, `from:` and `to:` take everything up to the next operator, so quotes are optional. `label:`, `has:` and `is:` take one word.
- Operators count only at the start or after a space. Matching ignores case and Latin accents.
- One search can name at most four people across `from:`, `to:` and bare addresses.
- There are no date operators and no OR or NOT. The CLI's `mail list` has `--after` and `--before`.

### Reading: HTML and plain text

HTML mail is rebuilt by the engine's sanitizer, which removes scripts and trackers, and shown on a light card. Mail shows as plain text by default. Use the HTML/TEXT badge on a message, or **Format** in its menu, to switch that sender to HTML (or back); all their mail in that account follows, in every thread. **View source** shows the original message.

**Remote images** stay blocked until you choose **Load images** for that message. They load over HTTPS only (plain-HTTP images never load), without cookies or a referrer. **Always load images from** a sender does this automatically for that sender in that account.

**Links** show their real host when you hover. Unencrypted links are marked "Not secure". **Block domain** adds a host to your blocklist.

### Attachments

Attachments appear as tiles under each message. Images and videos open in a viewer (`←`/`→` to move, `Space` to play, `M` to mute). Other files open in their default app. **Download** saves a copy to your Downloads folder.

### Writing

Press `N` for a new message, `R` to reply, `A` to reply all or `F` to forward. The composer is plain text first, with bold, italic and underline (`Ctrl+B`, `Ctrl+I`, `Ctrl+U` on a selection). Attach files with **Attach** or by dropping them onto the message. The app never writes text for you.

- **Drafts** are saved on this machine as you type and kept in step with Gmail's Drafts while the account syncs. Drafts saved in Gmail elsewhere are listed too.
- **Show drafts in the corner** (Settings → Mail) keeps unsent drafts one click away.
- Pressing `Esc` on a new message with text asks whether to keep editing or save it to Drafts.
- Sending uses the account's sync session. If sync is paused, the composer offers **Resume sync**.
- If you close the app while a send is still being prepared, it's cancelled and the draft is kept.

### Delete, archive, Spam and Trash

- **Delete** (the `Delete` key) moves a conversation to Gmail's Trash. In Drafts it discards the draft, after asking.
- **Archive in Gmail** (in the message menu) removes it from the Inbox.
- **Trash** is part of the local copy. Use **Restore** to move a conversation back, **Delete permanently** to remove it for good, or **Empty trash**.
- **Spam** is never downloaded or stored. The Spam view lists Gmail's Spam live. **Move to Inbox** marks a thread not spam. **Empty spam** and permanent deletion are also here.
- Deleting for good can't be undone and needs full mail access (Read & write). Google itself removes Spam and Trash after 30 days.

### Unsubscribe

Messages from a mailing list show an **Unsubscribe** button. Depending on the list, it:

- sends a one-click unsubscribe request in the background;
- sends the list's unsubscribe email through Gmail; or
- opens the list's unsubscribe page in your browser.

Unsubscribe links on your blocked-domains list are refused.

### Labels

Apply or remove labels from a conversation's right-click menu → **Labels**. Each account's labels are managed from its card in Settings → Accounts → **Labels · Edit**:

- **Name** and **Description.** The description is optional and is read only by the local model.
- **Read** and **Spam** say what happens when the local model applies the label (see [Local AI](#local-ai)).
- **Remove** takes the label off every conversation in fuckMail only. Your mail and the label in Gmail stay unchanged.
- **Sync with Google** matches local and Gmail labels by name and creates missing ones.

## Encryption (OpenPGP)

fuckMail signs and encrypts mail with OpenPGP, the standard GnuPG, Thunderbird, Proton and Mailvelope use, so keys and messages work with them too.

**Set up a key** in **Settings → Encryption**:

- **Create key…** makes a key for one account's name and address and makes it that account's key. Ed25519 (recommended) works with every current OpenPGP app; RSA 4096 is for older software. It expires after three years unless you choose otherwise, and **Change expiry…** renews it.
- **Import key…** takes a key file or pasted text: your own secret key from another app (you enter its passphrase once), someone's public key, or a revocation certificate.
- Under **Accounts**, choose each account's key and whether new mail is signed or encrypted by default.

**Back up your key** right after making it: **Back up secret key…** saves a copy protected with a passphrase of at least 12 characters. Without a backup, a lost computer means mail encrypted to you can't be read. **Save revocation certificate…** lets you tell others the key is no longer in use, even if you lose it.

**Share your public key** with **Copy public key** or **Save public key…**, and import other people's to encrypt to them. A key is only as trustworthy as where it came from: compare its fingerprint with its owner, in person or on a call.

**Writing.** For an account with a key, the composer has **Sign** and **Encrypt**. Encrypted mail can be read only by its recipients and you; the composer names anyone whose key you don't have yet. The subject, the sender and the recipients stay visible, so Gmail can keep replies together. A draft you'll encrypt is kept on this computer and not saved to Gmail Drafts. A reply to encrypted mail starts encrypted.

**Reading.** A lock marks encrypted mail and a shield a signature; hover them to see who signed and how it was checked. Encrypted mail is decrypted only while you read it, so its text isn't searchable or summarized, and the list shows "Encrypted message". Its pictures and the attachments you open or preview are cleared when fuckMail closes; a copy you **Save** elsewhere is yours to keep safe. If something is wrong (no key for it, a signature that doesn't match, a key for another address), a note above the message says so.

Secret keys stay on this computer, sealed with a key kept in your system keychain, and never go to Google.

## Calendar

Press `V` to switch between Mail and Calendar.

- **Views:** Day (`D`), Week (`W`), Month (`M`) and Agenda (`A`). `←` and `→` move, `T` returns to today.
- **All calendars** shows every account's calendars together; turn it off to see only the current account's. Calendars you can only see free/busy for show "Availability only".
- **Show declined** shows events you declined, struck through.
- Week headers and month cells show how much mail you received and sent that day. Click the count to jump to Mail.

**Events.** Press `N`, or click or drag across empty time in the grid. The editor has the calendar, title, times, time zone, repeat rule, location, **Add Google Meet**, guests, notes, reminder, and Busy/Free. `Ctrl+Enter` saves.

**Repeating events.** Repeat options are daily, weekdays, weekly, monthly or custom, ending never, on a date or after a count. When you edit or cancel a repeating event, choose **This occurrence**, **Entire series** or **This and following**. "This and following" resets later exceptions, gives a Meet call a new meeting, and Google may notify guests; the app asks first.

**RSVP.** Answer **Yes**, **Maybe** or **No** from the event, or from the invitation email in Mail. Your answer goes to Google, which may tell the organizer. Declining removes the event from your calendar, with Undo.

**Reminders.** When a reminder is due, a full-window screen shows a countdown and plays a sound:

| Key | Action |
| --- | --- |
| `Enter` | Join the Google Meet, Zoom, Teams or Webex link |
| `S` | Snooze (1, 5 or 10 minutes; set in Settings → Calendar) |
| `M` | Mute the sound |
| `Esc` | Dismiss |

Reminders only appear while the app is open. Setting notifications to **Off** also turns reminders off.

**Timeline strip.** A strip of the day's events sits along the bottom of Mail (or under the top bar). Drag it sideways for other days, and click an event for details. Settings → Calendar sets its placement and hours.

**Time zone.** On first start the app uses your computer's time zone. Change it in Settings → Calendar → **Calendars and time zone** → **Display time zone**, using a name such as `Europe/London`. It's used for the calendar grids, the timeline and Today. The same section chooses which calendars are visible.

### Calendar search

With Calendar open, the search box searches events.

| You type | It matches |
| --- | --- |
| `design review` | The phrase in the event title or notes |
| `title:standup` | The phrase in titles only |
| `with:maya` or `maya@example.com` | Guests or organizer, by name or address |
| `date:2026-10-02` | Jumps to that day; it doesn't filter |

`date:` also accepts `10-02`, `today`, `tomorrow`, `yesterday`, and offsets such as `+7` or `-3`. Free/busy-only calendars are never searched.

## Keyboard shortcuts

On macOS, `Ctrl` is `Cmd`. Most shortcuts can be changed in **Settings → Shortcuts**: click a row and press the new key. **Default** restores it and **None** clears it. A key already used in the same area moves to the new action and leaves the old one without a shortcut. Rows shown in grey are fixed.

| Mail | |
| --- | --- |
| `J` / `K` | Next / previous conversation |
| `R` / `A` / `F` | Reply / reply all / forward |
| `U` | Mark read or unread |
| `D` | Delay, or remove the delay |
| `E` | Done |
| `N` | New message |
| `S` | Inbox → Sent → Drafts |
| `/` | Search |
| `Delete` | Move to Trash (discard, in Drafts) |
| `Ctrl+K` | Search (fixed) |
| `Tab` / `Shift+Tab` | Next / previous account (fixed) |

| Calendar | |
| --- | --- |
| `D` / `W` / `M` / `A` | Day / week / month / agenda view |
| `←` / `→` | Previous / next |
| `T` | Today |
| `N` | New event |

| Everywhere | |
| --- | --- |
| `V` | Switch Mail and Calendar |
| `,` | Settings |
| `Z` (or `Ctrl+Z`) | Undo, while the Undo notice shows |
| `Ctrl+Enter` | Save the event in the event editor (fixed) |
| `Esc` | Close or cancel (fixed) |
| `` ` `` or `~` | Debug console (not rebindable) |

In the mail list, the arrow keys, `Page Up`/`Page Down` and `Home`/`End` move; add `Shift` to extend a selection. `Ctrl+A` selects visible conversations, and `Shift+F10` or the Menu key opens the row menu.

## Settings

| Page | What's there |
| --- | --- |
| General | **Theme:** Graphite (default), Paper or Terminal. **Text size:** Small, Default or Large. **Open on:** Mail, Calendar or where you left off. **Sync every:** 1 (default), 5 or 15 minutes; sending is always immediate. **Notifications:** All, Events only (default) or Off. |
| Accounts | One card per account: sync progress, labels, signature, OpenPGP, local model, and the **…** menu (Edit, Remove). |
| Encryption | OpenPGP keys and each account's key. See [Encryption](#encryption-openpgp). |
| Local LLM | Model server, model and prompts. See [Local AI](#local-ai). |
| Mail | **Row density:** Comfortable (52 px) or Compact (44 px). Drafts in the corner. Blocked domains. Image loading. |
| Calendar | New event length, default reminder, snooze length, timeline, week start, visible calendars and time zone. |
| Shortcuts | Rebind keys. |

**Notifications** are desktop alerts shown only while the app is open. **Events only** shows event reminders; **All** adds new-mail alerts; **Off** turns both off.

## Images, blocked domains and privacy settings

All of these are in **Settings → Mail**.

- **Blocked domains.** Links to these hosts are disabled, and their images never load, even with Load images. Enter `tracker.io` for one host or `*.tracker.io` for a domain and its subdomains. The list applies to every account.
- **Load through a proxy.** Remote images (and sender pictures and one-click unsubscribes) go through an HTTPS proxy you name as `host[:port]`, with an optional username and password. The password is kept in your system keychain. If the proxy is down, nothing goes direct. **Test** makes one request to `www.gstatic.com/generate_204` through it.
- **Image cache.** Loaded images are kept, up to 256 MB, so they show again without a new request. **Clear** empties it.
- **Sender pictures** (on by default). The mail list shows a brand icon for automated senders and a public profile photo for people, or initials otherwise:
  - Brand icons come from Google's site-icon service, which receives the sender's root domain (for example `example.com`). Personal mail domains such as gmail.com are never looked up.
  - Profile photos come from Gravatar, which receives a SHA-256 hash of the sender's address, not the address itself.
  - Pictures are cached; one found missing isn't asked for again for a week. Turn the switch off to stop these requests.

[PRIVACY.md](../PRIVACY.md) lists every network destination.

## Local AI

fuckMail can send each received message to a model server on your own computer. The model writes a one-line summary, marks the sender as a person or automated, and picks at most one of your labels. It never writes, sends or completes mail.

1. Run [Ollama](https://ollama.com) or another OpenAI-compatible server, and download a model.
2. In **Settings → Local LLM**, choose the provider. The default address is `http://127.0.0.1:11434` for Ollama and `http://127.0.0.1:1234` for OpenAI-compatible servers. Choose **Detect**, pick a model, then use **Test** to try a sample email.
3. On each account's card in Settings → Accounts, turn **Summarize** and **Label** on or off.

- The address must be `http://` with `localhost`, `127.0.0.1` or `[::1]` and an explicit port. There is no API key field and no cloud option; message text only goes to that local server.
- The **System prompt** and **User message** are editable. The user message must contain `{{messages}}`, and `{{labels}}` is optional. **Restore default** brings back the built-in text.
- New mail is processed first. For mail that arrived before, use **Catch up** on the account card: last week, month, year or all. Already processed mail is skipped. The pause button holds the model's queue while downloading continues.
- Label descriptions tell the model what belongs where. Per label, **Read** marks newly arriving mail read when the model applies that label (on by default for a label named Receipts), and **Spam** sends it to Spam.
- A message's summary appears on request with the summarize icon in its header, or automatically with Summarize on.

## Accounts

Each account's card in Settings → Accounts has:

- **Colour**, chosen from 16 swatches, to tell accounts apart.
- **Signature**, added to new messages and replies as ordinary text you can edit.
- **OpenPGP**: Off, or the account's key and whether new mail is signed or encrypted. It opens Settings → Encryption.
- **Edit…** (in the **…** menu): display name, local retention, and which services sync with what access (Mail and Calendar, Read or Read & write). Asking for more access than Google granted takes you through sign-in again.
- **Reconnect**, when Google needs you to sign in again.

**Local retention** limits how much mail stays on this machine. Shortening it removes older local copies when you save. Nothing is deleted from Google, and extending the period downloads it again.

**Remove** (in the **…** menu) takes the account out of the app:

- saved mail, drafts, files and history stay in a read-only archive (**Open archive**);
- the account's sign-in credentials are removed from your keychain;
- nothing changes at Google, and Google access is not revoked (see [troubleshooting](troubleshooting.md#reset-or-uninstall)).

**Restore…** signs in as the same Google account and uses the archive again. **Delete archive…** permanently deletes the saved data for that account from this app.

## Debug console

Press `` ` `` or `~` (outside a text field) to slide down the console. It has a **logs** tab with the app and engine logs, and a **state** tab with live app state. Type `help` for commands:

| Command | Does |
| --- | --- |
| `level <name>` | Show that level and worse (`emerg` … `debug`) |
| `fac <name>` or `fac all` | One area: `sync`, `gmail`, `cal`, `llm`, `db`, `net`, `ui`, `app` |
| `grep <text>` | Filter lines; `grep` alone clears |
| `pause`, `resume`, `tail`, `clear` | Control the view |
| `sync <account>` | Sync one account now, by address prefix or display name |
| `export` | Save the filtered lines to your Downloads folder |
| `copy` | Copy the filtered lines |
| `fm <args>` | Run the CLI against this app's profile and selected account; `stop` or `Ctrl+C` ends it |
| `exit` | Close the console (or `Esc`) |

## When things run

- **Closing the window ends the app's work.** Sync, reminders, notifications and pending sends stop. There is no background service or tray mode.
- The app and the `fm` CLI share one local engine per profile. The engine exits shortly after the last one disconnects.
- **The CLI has its own lifetime.** For example, `fm google sync` keeps syncing while it runs, whether or not the app is open. See [cli.md](cli.md).

## Where your data lives

Everything is in one profile folder:

| System | Folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\fuckMail\default` |
| macOS | `~/Library/Application Support/fuckMail/default` |
| Linux | `$XDG_DATA_HOME/fuckMail/default` (usually `~/.local/share/fuckMail/default`) |

The demo profile is `demo-v1` in the same place. Inside:

- `store.sqlite3` (with `-wal` and `-shm` files while open): mail, calendar, drafts, labels, settings and local model results;
- `blobs/`: original messages (including their attachments), imported draft files and retained outgoing mail;
- `exports/`: disposable attachment copies made for previewing/opening; older copies are cleaned up around a 512 MiB target, with recently used and locked files kept longer. Removed copies are recreated from local originals, including offline. Files you save to Downloads are kept;
- `cache/`: loaded images, sender pictures and prepared HTML;
- `diagnostics/`: logs (see [troubleshooting](troubleshooting.md#where-are-the-logs));
- `engine.json`: how the app and CLI find the running engine;
- `window.json`: where the window was left, so the app reopens there (maximized if it was), and where you left the Mail list/reader divider (one position for every account). If that monitor is gone, it opens in the default place.

Google sign-in credentials, the proxy password and the key that seals your OpenPGP secret keys are in your system keychain, not in the folder.

**Backup:** quit the app and any `fm` commands, then copy the whole profile folder. Never copy `store.sqlite3` on its own. Credentials aren't included, so a restored copy on another machine needs its accounts reconnected, with the same Google Cloud client. OpenPGP secret keys can't be opened there without this machine's keychain: back them up with **Back up secret key…** and import that copy.

**The app does not encrypt this data.** It relies on your operating system account and full-disk encryption (BitLocker, FileVault or LUKS). Keep the profile out of cloud-synced folders. [SECURITY.md](../SECURITY.md) has more.
