# fuckMail

A fast, focused desktop mail and calendar client for Gmail and Google Workspace. It runs natively on Windows, macOS and Linux, keeps your mail on your own machine, and comes with a command-line client that shares the same data.

It is built around one daily habit: read what's new, finish what needs doing, and end the day with nothing unread and nothing starred.

> **Status:** early and under active development. There is no installer or release yet; you build it from source. See [current status](docs/status.md) for what works today.

## Features

- **Separate accounts.** Personal Gmail and Workspace accounts stay apart. There is no unified inbox, and mail is always shown as conversations.
- **Simple triage.** Unread means "not reviewed yet" and a star means "action pending". **Done** clears both; **Delay** holds a conversation for later.
- **Offline mail and search.** Messages, original sources and attachments are stored locally. You can search by sender, subject or body text without a connection.
- **Writing.** New message, reply, reply all and forward, with attachments. Drafts sync with Gmail, and recent mail actions can be undone.
- **Safe HTML mail.** Messages show as they were designed, cleaned of scripts and trackers first. Remote images stay hidden until you load them, only ever over https and through your own HTTPS proxy if you set one. Hovering a link shows where it goes and lets you block its domain. HTML or plain text is one choice for every message.
- **Gmail labels.** Organize mail with your existing labels.
- **Calendar.** Day, week and month views across your accounts. Create and edit events, including recurring ones, answer invitations and get reminders. A timeline on the mail screen keeps your day in view.
- **Optional local AI.** Point it at [Ollama](https://ollama.com) or another local model server. It can summarize messages, tell people from automated mail and suggest labels. It never writes or sends email, and it is off by default.
- **Command line.** `fm` works with the same accounts and data as the app, so scripts and external tools can use it.
- **Themes and shortcuts.** Four built-in themes and editable keyboard shortcuts.
- **Debug console.** Press `~` outside text fields for a large centered console with Mail, Calendar, Sync, Model and App filters. It follows new events until you scroll up; select a row for full metadata. Diagnostic files in each profile's `diagnostics/` directory are bounded to about 10 MiB.

## Build and run

You need:

- **Windows:** Visual Studio 2022 C++ build tools. The bootstrap step can install them.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Linux (Ubuntu 24.04):** a C++ compiler, Python with venv, and the graphics libraries listed in [the CI workflow](.github/workflows/native.yml).

Bootstrap downloads the pinned Rust, Qt and build tools into `.tools/` inside the checkout, leaving system installs alone.

On macOS or Linux, run this once:

```sh
scripts/dev.sh bootstrap
```

Then build and open the app:

```sh
scripts/dev.sh run
```

On Windows, in PowerShell, run this once:

```powershell
./scripts/dev.ps1 bootstrap
```

Then build and open the app:

```powershell
./scripts/dev.ps1 run
```

`run` builds a debug build for development. For everyday use, add `--preset release` (for example `./scripts/dev.ps1 run --preset release`); it is several times faster.

To try the app with sample data instead of a real account, pass `--demo-profile`. It uses a separate demo profile:

```sh
scripts/dev.sh run -- --demo-profile
```

Other commands: `test` runs the test suite, `cli -- --help` shows the CLI, and `package --preset release` builds an optimized package. The [build guide](docs/build-development.md) covers these in detail.

## Connecting a Google account

Google sign-in needs a Google Cloud **Desktop app** OAuth client, set up once per build:

```sh
python3 scripts/configure_google.py /path/to/desktop-client.json
```

The file stays in the ignored `.tools/` folder, so it never goes into Git. Rebuild after running this, then open **Settings → Accounts → Connect Google account**. Signing in imports nothing until you start sync for that account. Access tokens are kept in your operating system's credential store, not in the app's files. [Setup details](docs/build-development.md#email-first-google-setup-d040).

## Built with

Rust (sync engine and CLI), C++20 with Qt Quick (desktop app) and SQLite with full-text search. No Node.js or web runtime is involved.

## More

- [Current status](docs/status.md): what works, what's next and known issues
- [Product scope](docs/product-scope.md) and [north star](docs/north-star.md): what the product is and is not
- [Documentation index](docs/README.md): design, contracts, decisions and research
