# The `fm` command line

`fm` is a second client for the same local engine the app uses. It reads and changes the same mail, drafts and calendars, and can print every reply as JSON for scripts. Anything you do in one client shows up in the other.

`fm --help` and `fm <command> --help` list every option; help never starts the engine.

## Where `fm` is

Nothing puts `fm` on your PATH. Call it by its full path, or add an alias.

| Install | Path |
| --- | --- |
| Windows installer | `%LOCALAPPDATA%\Programs\fuckMail\bin\fm.exe` |
| macOS app | `/Applications/fuckMail.app/Contents/MacOS/fm` |
| Linux archive | `bin/fm` in the unpacked folder |
| Source checkout | `scripts/dev.sh cli --preset release -- <args>` (`./scripts/dev.ps1` on Windows), after a build |

Builds also copy `fm` and `fm-engine` next to the app's executable. The examples below write plain `fm`.

## Global options

| Option | Meaning |
| --- | --- |
| `--profile DIR` | Profile folder, as an absolute path. Defaults to the live profile the app uses. |
| `--demo-profile` | Use the separate demo profile with synthetic accounts. |
| `--account ID` | The account for account-scoped commands. `fm accounts` lists the IDs. |
| `--json` | Print each reply, including errors, as one line of JSON on standard output. |
| `--engine PATH` | Engine to start if none is running. Defaults to `FM_ENGINE_PATH`, then `fm-engine` beside `fm`. |

**Explicit profile.** Commands that sign in or change accounts must name their profile with an absolute `--profile`, even when it's the default one, and can't use `--demo-profile`:

- `profile-init`
- every `google …` command
- every `account …` command
- `draft sources`, `draft source`, `draft authoring` and `draft copy`

The default live profile is `%LOCALAPPDATA%\fuckMail\default` on Windows, `~/Library/Application Support/fuckMail/default` on macOS and `~/.local/share/fuckMail/default` on Linux (`$XDG_DATA_HOME` if set).

**Account.** Most commands need `--account`. Commands that work across accounts don't: `accounts`, `status`, `notifications`, `watch`, `call`, `preferences`, `proxy`, `links`, `assist` (some of its subcommands read `--account`), `calendar-sources`, `calendar --combined`, `calendar search`, `calendar reminders`, and `google connect`, `google setup-status` and `google sync-status`.

## How `fm` talks to the engine

Each profile has one engine process, shared by the app and every `fm` command. `fm` finds the running engine through the profile's `engine.json`. If none is running, it starts one and waits up to 30 seconds for it.

The engine exits about a third of a second after its last client disconnects. So a one-off command like `fm mail list` starts an engine that stops right after, unless the app or another `fm` is connected. Long-running commands keep it alive for as long as they run: `google sync`, `google connect`, `watch` and `assist run`.

### Google operations need a sync session

Commands that talk to Google (sending, draft upload, remote search, Spam, calendar writes and so on) join the account's running sync session. They never start one. Without it they fail with:

```text
fm: sync_not_running: Start this account's sync session first
```

Either keep the app open with that account syncing, or run a sync in another terminal:

```sh
fm --profile /abs/profile --account ID google sync            # until Ctrl-C
fm --profile /abs/profile --account ID google sync --seconds 600
```

Local actions such as `mail done` don't need a session. They're saved at once, and Gmail is updated the next time the account syncs. `fm mail delivery` shows changes still waiting.

## Output and errors

Without `--json`, account lists, conversation lists and conversations print as short text; everything else prints as indented JSON. Errors go to standard error:

```text
fm: stale_revision: State changed; reload before applying this command
```

With `--json`, every reply is one line on standard output. Success is the engine's reply envelope (a few replies that `fm` assembles itself have only `ok` and `result`):

```json
{"version":1,"id":"2","ok":true,"result":{ … }}
```

An error has the same shape, with `id` set to `cli`:

```json
{"version":1,"id":"cli","ok":false,"error":{"code":"not_found","message":"Conversation not found in this account"}}
```

Branch on `error.code`, not the message. `error.details` appears only for send and draft recovery facts, such as `send_invocation_finished_before_dispatch`. `watch` and `google sync` print a new line each time the status changes.

## Exit codes

| Code | When |
| --- | --- |
| 0 | Success. Also when the reader of the output goes away (for example `fm mail list \| head`). |
| 1 | Any other error, such as `sync_not_running`, a Google or network failure, or an engine that couldn't start |
| 2 | Invalid input: `invalid_request`, `invalid_params`, `request_id_conflict`, or a bad command line |
| 3 | `permission_denied`, `unauthorized` or `authentication_required` |
| 4 | `stale_revision` or `stale_cursor`: something changed since you read it |
| 5 | `not_found`, `unsupported` or `content_unavailable` |
| 101 | `fm` crashed |

## Common tasks

Examples use `ID` for an account ID, `CONV` for a conversation ID and `P` for an absolute profile path. `jq` is optional.

### Accounts and status

```sh
fm accounts                       # id, name <email>, unread and starred counts
fm accounts --include-removed     # also removed accounts' archives
fm status                         # engine, schema and profile kind
```

### List and search mail

```sh
fm --account ID mail list                                  # Inbox, 30 at a time
fm --account ID mail list --grouped --month-zone Europe/London
fm --account ID mail list --status review                  # unread only
fm --account ID mail list --folder received --offset 30    # includes archived mail
fm --account ID mail list --query 'from:maya subject:board deck'
fm --account ID mail list --query 'has:attachment' --after 2026-09-01 --before 2026-10-01
```

`--query` uses the app's search syntax (see the [user guide](user-guide.md#search)). `--after` and `--before` take UTC dates. Other filters: `--folder` (`inbox`, `received`, `sent`, `all`, `trash`), `--category`, `--sender`, `--exact-sender`, `--kind`, `--has-attachment`, `--limit`, `--cursor`.

To search Gmail itself for threads that aren't on this machine (needs a sync session):

```sh
fm --profile P --account ID google search 'invoice 2023' --folder all
fm --profile P --account ID google thread THREAD_ID
```

### Read a conversation

```sh
fm --account ID mail read CONV
fm --account ID --json mail read CONV | jq '.result.messages[] | {id, source_version, attachments}'
```

`mail read` never marks anything read. The JSON reply has the conversation's `revision`, and each message's `id`, `source_version` and `attachments` (`id`, `name`, `mime_type`, `size`). Message-level commands need both:

```sh
fm --account ID mail html MESSAGE_ID --source-version V     # sanitised HTML
fm --account ID mail source MESSAGE_ID --source-version V   # headers and source
fm --account ID mail unsubscribe MESSAGE_ID --source-version V
```

### Review, Done, Delay and Undo

```sh
fm --account ID mail review CONV          # mark read
fm --account ID mail done CONV            # read + unstarred
fm --account ID mail delay CONV           # star, read state unchanged
fm --account ID mail undelay CONV         # remove stars
fm --account ID mail flag CONV --read false
fm --account ID mail category CONV LABEL_ID [--remove]
fm --account ID mail filing CONV --action archive   # or trash, restore
```

Each action first reads the conversation and acts on exactly the messages it found, so a reply that arrives meanwhile is left alone. The reply is the updated conversation plus `mail_undo` (`request_id`, `revision`) and, for real accounts, `delivery`. To undo:

```sh
fm --account ID mail undo CONV --undo-request-id REQUEST_ID --expected-revision REVISION
```

Undo fails with `stale_revision` if the conversation changed again after the action.

### Attachments

```sh
fm --account ID attachment ATTACHMENT_ID                      # state only
fm --account ID attachment ATTACHMENT_ID --output ./report.pdf
```

`--output` never overwrites an existing file.

### Write and send

Drafts are saved locally, uploaded to Gmail's Drafts while the account syncs, and sent only by an explicit `google send`.

```sh
# 1. Create a draft (or --mode reply|reply_all|forward --conversation-id CONV --source-message-id MSG)
fm --account ID --json draft create | jq '.result | {draft_id, revision}'

# 2. Save its content. Recipients are {"email", "name"} objects.
printf 'Hi Maya,\n\nThe deck is attached.\n' > body.txt
echo '{"to_recipients":[{"email":"maya@example.com","name":"Maya"}]}' > to.json
fm --account ID draft save --draft-id D --subject 'Board deck' --recipients-file to.json --body-file body.txt
fm --account ID draft attach --draft-id D --file ./deck.pdf --expected-revision 2 --request-id attach-deck-1

# 3. With a sync session running: check the Google copy, then send
fm --profile P --account ID google draft-status --draft-id D     # note revision and local_revision
fm --profile P --account ID google send --draft-id D \
   --expected-revision 3 --expected-google-revision 1 --request-id send-deck-1
fm --profile P --account ID google send-status --request-id send-deck-1
```

- `draft save` defaults `--expected-revision` to the current revision. `draft attach` needs it given.
- `--expected-revision` is the saved local revision to send (`local_revision`). `--expected-google-revision` is the Google copy's `revision` from `google draft-status`; if the draft was never uploaded, it's 0.
- A running sync uploads saved drafts on its own. `google draft-sync` does it explicitly.
- `--undo` waits six seconds before sending; `Ctrl+C` in that time keeps the draft unsent.
- If a send's outcome is unknown (for example the connection dropped), it is never retried automatically. Use `google send-status`, then `google send-reconcile` to look for it in Sent.

Other draft commands: `draft list`, `draft show`, `draft discard` / `draft undo-discard`, and `draft transfer` to move a draft to another account.

### Calendar

```sh
fm calendar-sources                                            # every calendar and your access
fm --account ID calendar --from 2026-10-05 --to 2026-10-12     # one account
fm calendar --combined --from 2026-10-05 --to 2026-10-12       # selected calendars of all accounts
fm calendar search 'with:maya@example.com standup' --around 2026-10-05
fm calendar search 'title:review' --from 2026-10-01 --to 2026-10-31
```

- `--to` is the day after the last day; a range is 1 to 42 days.
- `calendar search` works across accounts. `--around DAY` finds each matching event's next time within a year, then its latest time in the 60 days before.
- Search syntax: bare words match titles and notes, `title:` titles only, `with:` or a bare address guests and the organizer. `date:` doesn't filter here.
- Combined views need a display time zone. The app sets one on first start; otherwise set `calendar_display_time_zone` with `fm preferences save`.
- Changes to real calendars go through `fm --profile P --account ID google calendar-write --params-file FILE` (needs a sync session). The request format is in the [local protocol](local-protocol.md).

### Reminders, notifications and watching

```sh
fm calendar reminders                           # reminders due now, not acknowledged
fm --account ID calendar reminder-ack --calendar-id CAL --occurrence-id OCC --start START --state dismissed
fm --json notifications                         # returns a cursor ("after") at the current end
fm --json notifications --after 1234 --since 2026-10-05T09:00:00Z
fm watch                                        # print status whenever anything changes
```

`notifications` starts at the newest cached mail, so the first call returns just a cursor. Pass it back with `--after` (which needs `--since`) to get mail that has arrived since. Reminders are empty when notifications are set to Off.

### Labels and local AI

```sh
fm --account ID labels list
fm --account ID labels describe LABEL_ID 'Receipts and invoices from shops' --expected-description '' --request-id desc-1
fm --account ID labels auto-read LABEL_ID on --expected off --request-id read-1
fm --account ID labels sync --request-id sync-1   # match Gmail labels by name
fm assist settings                                # model server, model and prompts
fm --account ID assist catch-up --range month     # queue older mail
fm assist run                                     # keep the queue working until Ctrl-C
```

The engine works through the local model's queue only while the app or `fm assist run` is connected.

### Settings and accounts

```sh
fm preferences show
echo '{"sync_every_minutes":5}' | fm preferences save --preferences-file - --expected-revision 7 --request-id prefs-1
fm --account ID account-update --name 'Work' --signature-file sig.txt --mail-retention all \
   --expected-revision 4 --request-id acct-1       # metadata_revision from `fm accounts --json`
fm --profile P google connect --email you@gmail.com
fm --profile P --account ID account remove-preview
```

## Retries and conflicts

Changes take two safety values:

- **`--request-id`** names one change. Repeating a command with the same ID and the same parameters returns the saved result instead of doing it twice; the same ID with different parameters fails with `request_id_conflict`. Many commands generate one if you leave it out. For sends and other Google writes, always reuse the original ID when retrying.
- **`--expected-revision`** is the revision you last read. If the item changed since, the command fails with `stale_revision` (exit 4) and changes nothing. Read it again and decide. Mail actions default to the revision they just loaded.

## Raw protocol access

`fm call` sends any engine method with JSON parameters from a file or standard input:

```sh
echo '{"id":"CONV"}' | fm --account ID call conversation --params-file -
```

The methods and their parameters are described in [local-protocol.md](local-protocol.md).

## Command overview

| Command | What it does |
| --- | --- |
| `accounts` | List accounts and their coverage |
| `account` | `remove-preview`, `remove` (keeps a read-only archive, clears credentials), `removal-status`, `delete-archive` |
| `account-update` | Display name, signature, retention, access, colour |
| `status` | Engine and profile status |
| `profile-init` | Create an empty live profile at `--profile` |
| `google` | Sign-in and everything that talks to Google (below) |
| `mail` | `list`, `read`, `senders`, `sender`, `recipients`, `source`, `html`, `images`, `invite`, `unsubscribe`, `review`, `done`, `delay`, `undelay`, `flag`, `category`, `filing`, `undo`, `delivery` |
| `draft` | `create`, `save`, `show`, `list`, `attach`, `discard`, `discard-status`, `undo-discard`, `transfer`, `transfer-status`, `transfer-retry`, `sources`, `source`, `authoring`, `copy`, `history`, `capture` |
| `attachment` | Copy a stored attachment to a file |
| `categories` | List the account's labels |
| `labels` | `list`, `create`, `describe`, `remove`, `auto-read`, `spam`, `sync` |
| `calendar` | Read a date range; `search`, `reminders`, `reminder-ack` |
| `calendar-sources` | List calendars and permissions |
| `calendar-operations` | The account's calendar change history |
| `notifications` | Newly arrived mail since a cursor |
| `watch` | Stay connected and print status changes |
| `preferences` | `show`, `save` |
| `links` | `summary`, `blocked`, `block`, `unblock` |
| `proxy` | `show`, `set`, `clear`, `test`, `cache` for the image proxy |
| `assist` | Local model: `settings`, `configure`, `discover`, `test`, `analyze`, `results`, `correct`, `accept`, `progress`, `backlog`, `catch-up`, `pause`, `resume`, `run` |
| `call` | Send a raw protocol method |

`google` subcommands:

| Group | Subcommands |
| --- | --- |
| Setup and sync | `setup-status`, `connect`, `restore`, `disconnect`, `sync`, `sync-status`, `sync-stop` |
| Sending | `send`, `send-status`, `send-reconcile`, `send-abandon`, `sent`, `sent-message` |
| Drafts | `draft-sync`, `draft-status`, `draft-reconcile`, `draft-replace`, `draft-adopt`, `draft-delete`, `draft-delete-status`, `draft-delete-reconcile`, `draft-delete-abandon`, `drafts-refresh`, `drafts-status` |
| Remote mail | `search`, `thread`, `attachment`, `spam`, `spam-show`, `not-spam`, `bin-count`, `delete-forever`, `empty` |
| Labels | `labels`, `labels-refresh`, `category-bind` |
| Calendar | `calendar-write`, `calendar-occurrence`, `calendar-operation`, `calendar-reconcile`, `calendar-resume`, `calendar-abandon` |

**Demo profile only:** `calendar-write`, `calendar-ack` and `draft capture` simulate Google locally and are refused for real accounts. Use `google calendar-write` and `google send` instead.
