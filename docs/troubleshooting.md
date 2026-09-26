# Troubleshooting

Common problems and how to fix them. For setup, see [Google sign-in setup](build-development.md#google-sign-in-setup); for everyday use, the [user guide](user-guide.md).

## Google sign-in

### Google says it "hasn't verified this app"

That's expected for your own Cloud project in **Testing**. Your build uses the OAuth client you created, so Google shows its warning for unverified apps. If you trust the build, continue past the warning with an account that's on the test user list.

### `Error 403: access_denied`

The Google account isn't allowed to use your OAuth client. While the app is in Testing, only listed test users can sign in (up to 100). Add the address under the OAuth consent screen's test users in the [Google Cloud console](https://console.cloud.google.com/), then try again. You also get this if you cancel on Google's page.

### `Error 400: redirect_uri_mismatch`

The OAuth client isn't of type **Desktop app**. The app receives Google's reply on a temporary `http://127.0.0.1:<port>/oauth2/callback` address with a random port, which a Desktop app client accepts and other client types don't. Create a new client of type Desktop app, register it and rebuild:

```sh
python3 scripts/configure_google.py /absolute/path/to/desktop-client.json   # python on Windows
scripts/dev.sh build --preset release
```

If accounts were already connected with the old client, see `client_mismatch` below.

### I have to sign in again every week

Google expires sign-ins after 7 days for apps in Testing that use Gmail's restricted scopes. When it happens, the account card in Settings → Accounts shows **Reconnect** ("Google requires an explicit reconnect"). Your mail stays; reconnecting resumes sync. Ways to avoid it:

- **Google Workspace:** set the consent screen's audience to **Internal**. Only accounts in your organisation can sign in, and the test-user list doesn't apply.
- **Publishing the app:** moving it to production with Gmail scopes requires Google's verification. Check Google's current rules before relying on this.

### My Workspace account is blocked

Workspace administrators can restrict which apps may use Gmail and Calendar data. If Google's page mentions your administrator or a policy, ask them to allow your OAuth client in the Admin console's API controls, or use an Internal app in your own Workspace.

### Only part of the access was granted

Google's consent page lets you untick permissions. The last step of Connect shows what was granted, for example "Google granted: Mail read". Features that need more stay unavailable; remote search, for instance, says "This account has not allowed reading mail".

Reconnect from the account card and tick every box, or choose **Edit…** to change the access you ask for. **Ask Google to renew access** makes Google show its consent page again. In the CLI:

```sh
fm --profile /abs/profile --account ID google connect --reauthorize
```

Deleting Spam or Trash for good needs full mail access (Read & write).

### `client_mismatch`

> This profile uses a different Google app registration. Use its original configured build or desktop-client override.

A profile stays bound to the OAuth client its accounts were connected with. You get this after switching to another Cloud project or client. Either register the original client file again with `configure_google.py` and rebuild, or move the profile aside (see [Profile refused](#profile-refused)) and reconnect your accounts with the new client.

### "Google sign-in is not configured in this build"

The build has no OAuth client file. Register one and rebuild, as above. To check without the app:

```sh
fm --profile /abs/profile google setup-status
```

You can also point a single run at a client file by starting the app (or `fm`) with `FM_GOOGLE_CLIENT_CONFIG` set to its absolute path. Quit the app first so a new engine starts. "The Google app registration is unavailable or invalid" means the file exists but isn't a valid Desktop client.

## Keychains, Gatekeeper and SmartScreen

### macOS asks about the keychain after every update

Builds aren't signed yet, so each new build looks like a new program to macOS. When asked whether `fm-engine` may use your keychain, choose **Always Allow**. If you deny it, the app shows "Credential access was denied or the keychain is locked. Sync is paused; click Sync to try again."

### macOS won't open the app, or Windows warns about it

- **macOS:** open System Settings → Privacy & Security and choose **Open Anyway**. On macOS 14 and earlier, Control-click the app and choose **Open**.
- **Windows:** SmartScreen warns about unsigned installers. Choose **More info** → **Run anyway**.

### Linux: sign-in works but nothing is saved

Credentials go to the Secret Service. Your desktop needs a provider running, such as gnome-keyring or KWallet with its Secret Service support enabled. Without one, sign-in can't store credentials and you see the keychain message above.

## Profile refused

The app shows the reason with **Show profile folder**; the CLI prints it after `fm:`.

| Message | Cause |
| --- | --- |
| This profile was made by an older fuckMail and cannot be opened. Move its folder aside, then try again to start fresh and reconnect your accounts. | Profiles aren't migrated between incompatible builds |
| This profile needs a newer engine | The profile came from a newer build |
| profile must be a private, user-owned directory (mode 0700) | macOS or Linux: the folder is shared with or owned by another user |
| profile belongs to another Windows user | Windows: the folder is owned by another user |
| This profile already has a writer | Another engine is still using the profile |

To move a profile aside:

1. Quit the app and stop any running `fm` commands.
2. Rename or move the **whole** profile folder, for example `default` to `default-old`. Never move `store.sqlite3` on its own.
3. Start the app. It creates a new, empty profile; connect your accounts again.

Profile folders are listed in the [user guide](user-guide.md#where-your-data-lives).

## Where are the logs?

In the profile's `diagnostics/` folder:

| File | Contents |
| --- | --- |
| `engine.log` | One JSON object per line from the engine: time, level, area, event and metadata such as error codes, counts and timings. Account and message identifiers are replaced by hashes that change each time the engine starts. |
| `gui.log` | The app's log lines. Addresses, links, file paths, quoted text, tokens and long opaque values are replaced before they're written. |

Each file rolls over at 2.5 MiB and keeps one previous file (`engine.log.1`, `gui.log.1`), so logs use at most about 10 MiB. Nothing is sent anywhere.

To look at them in the app, press `` ` `` or `~` for the debug console. `export` there saves the filtered lines to your Downloads folder.

## Reporting a bug

fuckMail takes bug reports and fixes. For new features or other mail providers, please fork. Open an issue with:

- what happened and what you expected;
- steps to reproduce, with `--demo-profile` if you can;
- the commit (`git rev-parse --short HEAD`) or installer file name, and your operating system;
- relevant lines from `engine.log` or `gui.log`. They hold metadata, not message content, but read them before pasting.

Never paste real mail, tokens or your OAuth client file. Report security problems privately, as described in [SECURITY.md](../SECURITY.md).

## Reset or uninstall

1. **Remove accounts** (optional): Settings → Accounts → **…** → **Remove** clears each account's credentials from your keychain.
2. **Quit** the app and any `fm` commands.
3. **Move or delete the profile folder** (`default`, and `demo-v1` if you used the demo). The locations are in the [user guide](user-guide.md#where-your-data-lives). Installing, upgrading and uninstalling never touch it.
4. **Remove keychain entries** you didn't clear in step 1. Search your credential store for `fuckmail`: entries contain `fuckmail.google.v1.` (Google sign-in) or `fuckmail.proxy.v1.` (image proxy password). Use Credential Manager on Windows, Keychain Access on macOS, or your Secret Service tool (such as Seahorse) on Linux.
5. **Revoke Google access** at [myaccount.google.com/permissions](https://myaccount.google.com/permissions). Removing an account in the app doesn't do this.
6. **Uninstall the app:** Settings → Apps on Windows, delete `fuckMail.app` on macOS, or delete the unpacked folder on Linux.

You can also delete the OAuth client or the whole project in the Google Cloud console.
