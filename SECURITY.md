# Security policy

## Reporting a vulnerability

Please report security problems privately, using GitHub's **Report a vulnerability**
button on this repository's Security tab. Don't open a public issue. Include what you
found, how to reproduce it, and which platform and commit you tested. There are no
releases yet, so fixes land on `main`.

## What fuckMail stores and where

- **Credentials.** Google access and refresh tokens, and the optional image-proxy password,
  live in the operating system's credential store (Windows Credential Manager, the macOS
  Keychain, or the Secret Service on Linux), not in the profile folder.
- **Mail and calendar data.** Messages, original MIME sources, attachments and calendar
  events are stored in an SQLite database and files in the profile folder:
  - `%LOCALAPPDATA%\fuckMail\default\` on Windows;
  - `~/Library/Application Support/fuckMail/default/` on macOS;
  - `$XDG_DATA_HOME/fuckMail/default/` (usually `~/.local/share/fuckMail/default/`) on Linux.

  The demo profile lives beside it in `demo-v1/`.

  **The app does not encrypt this data.** It refuses profile folders owned by or shared with
  other users, but beyond that it relies on your OS account and full-disk encryption (BitLocker,
  FileVault or LUKS). Keep the profile out of cloud-synced folders.
- **Google app registration.** A build's OAuth client file (`google-oauth-client.json`)
  identifies the app, not a user, and contains no tokens. See
  [Google sign-in setup](docs/build-development.md#google-sign-in-setup).

## Network access

fuckMail has no servers of its own and sends no telemetry. The engine connects to:

- Google's OAuth, Gmail and Calendar APIs, for the accounts you connect;
- remote images in a message, over HTTPS only, and only after you choose to load them.
  They can go through an HTTPS proxy you configure;
- sender pictures (on by default; turn off in Settings → Mail):
  - Google's public site-icon service, with the root domain of automated senders;
  - Gravatar, with a SHA-256 hash of a person's address, for their public profile photo;
- a mailing list's one-click unsubscribe URL, only when you choose Unsubscribe (mailto
  unsubscribes are sent through Gmail);
- `www.gstatic.com/generate_204`, only when you test an image proxy you configured;
- a local model server such as Ollama, only if you enable local AI. It must be on this
  machine (`localhost` with an explicit port).

## Local attack surface

- The GUI and the `fm` CLI talk to the engine over a named pipe (Windows) or a Unix socket.
  Only the same OS user can connect: on Windows through the pipe's access list, on
  macOS and Linux through socket permissions and a peer-credential check.
- Received HTML is rebuilt by the engine's sanitizer, which drops scripts and
  trackers, before the GUI renders it with litehtml. litehtml has no scripting or
  networking.
- No component listens on a network port, except during Google sign-in: a temporary
  listener on `127.0.0.1` receives the OAuth callback, then closes.
