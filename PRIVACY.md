# Privacy

fuckMail is software you build and run on your own computer. It has no servers of its own, no accounts with us, no analytics and no telemetry. This page describes what the software does with your data. [SECURITY.md](SECURITY.md) covers how it protects it.

## What it connects to

The engine makes network requests only to these destinations:

| Destination | When | What it receives |
| --- | --- | --- |
| Google sign-in, Gmail and Calendar APIs (`accounts.google.com`, `oauth2.googleapis.com`, `openidconnect.googleapis.com`, `gmail.googleapis.com`, `www.googleapis.com`) | For accounts you connect: sign-in, sync, and actions you take, such as sending, labelling or remote search | What those APIs need, including your search text when you search the remote mailbox |
| Servers that host a message's remote images | Only when you choose **Load images**, or for senders you set to load automatically | An HTTPS request for the image, without cookies or a referrer |
| Google's site-icon service (`www.google.com`) | Sender pictures, on by default | The root domain of an automated sender, such as `example.com` |
| Gravatar (`gravatar.com`) | Sender pictures, on by default | A SHA-256 hash of a sender's address, not the address |
| A mailing list's one-click unsubscribe address | Only when you choose **Unsubscribe** | A standard unsubscribe request. Lists that unsubscribe by email get that email through Gmail. |
| `www.gstatic.com/generate_204` | Only when you test an image proxy you configured | An empty request |
| An HTTPS proxy you configure | Instead of a direct connection, for images, sender pictures and one-click unsubscribes | Those requests |
| A local model server, such as Ollama | Only if you enable local AI | Each message's sender, recipients, subject, date and text, with your label names and descriptions. It must be on the same computer (`localhost` with an explicit port). |

Turn sender pictures off in **Settings → Mail**. Links you click and unsubscribe pages open in your own web browser, not in the app. During Google sign-in, a temporary listener on `127.0.0.1` receives Google's reply and then closes. The app never listens on a network port otherwise.

## What it stores

Everything stays on your computer, in the profile folder:

- Windows: `%LOCALAPPDATA%\fuckMail\default`
- macOS: `~/Library/Application Support/fuckMail/default`
- Linux: `$XDG_DATA_HOME/fuckMail/default`, usually `~/.local/share/fuckMail/default`

It holds your mail (including original messages and attachments), calendar events, drafts, labels, settings, local model summaries and labels, cached images and sender pictures, and logs. **The app does not encrypt this folder**; it relies on your operating system account and disk encryption.

Google sign-in tokens, the optional proxy password and the key that seals your OpenPGP secret keys are stored in your operating system's credential store (Windows Credential Manager, the macOS Keychain or the Linux Secret Service), not in the folder.

OpenPGP keys you create or import are kept in the folder, secret keys only in sealed form. The app never looks keys up on keyservers or other people's domains; it uses only keys you import. Encrypted mail is stored as received and decrypted only in memory while you read it.

## What the logs contain

The `diagnostics/` folder holds `engine.log` and `gui.log`, each capped at about 5 MiB including one previous file. They record events, error codes, counts and timings, not message content. Engine logs replace account and message identifiers with hashes, and the app's log removes addresses, links, file paths and quoted text before writing. Logs are never sent anywhere; you choose whether to share lines in a bug report.

## Google user data

fuckMail uses the data it receives from Google APIs only to provide its mail and calendar features on your device. It does not send that data to the fuckMail developers or sell it, and does not share it with anyone else, apart from the requests in the table above that it makes on your behalf. Local AI, when you turn it on, processes mail on your own computer.

Every build uses a Google Cloud OAuth client supplied by whoever built it (see [Google sign-in setup](docs/build-development.md#google-sign-in-setup)). If you distribute builds with your own client to other people, you are the app's publisher in Google's eyes, and Google's verification may require you to publish your own privacy policy.

## Deleting your data and revoking access

- **Remove an account:** Settings → Accounts → **…** → **Remove** clears its credentials and keeps a read-only local archive. **Delete archive…** then deletes that account's saved data from the app. Neither changes anything at Google.
- **Delete everything:** quit the app and any `fm` commands, then delete the profile folder and any keychain entries named `fuckmail…`.
- **Revoke Google access:** visit [myaccount.google.com/permissions](https://myaccount.google.com/permissions). Removing an account in the app doesn't revoke it.

Step-by-step instructions are in [docs/troubleshooting.md](docs/troubleshooting.md#reset-or-uninstall).
