# Platform, lifecycle and release

State: Draft · Revision: 23 · Updated: 2026-09-27 · Implementation: D155 app-wide Mail divider and D153 window placement passes on Windows 11 (unit, real-window and real-app checks); D132 icon and themed title bar pass on Windows 11; macOS and Linux are unbuilt. D130 unsigned Windows installer passes on Windows; the macOS disk image awaits its first Mac run.

Revision 23 (D155) adds `mail_split_width` to `window.json`: the Mail list pane's width in logical pixels, one value for every account, written when a divider drag ends. QML reads it as `windowLayout.mailSplitWidth`; runs without window placement have no `windowLayout`. Consumers: Mail UI107.

Revision 22 (D153) remembers the main window's place. The GUI writes `window.json` in the profile (x, y, width and height of the normal, unmaximized window, plus `maximized`) whenever it has stayed moved, resized or maximized for 250 ms, and on quit. The next launch applies it as the window's initial properties, so it first appears in place. The saved title strip must show at least 120 by 20 px on a screen's available area; otherwise, for example when that monitor is gone, the default size and position apply. A saved size larger than its screen is capped to it and kept inside it. Minimized and full-screen windows save their last normal place. Smoke, self-test, screenshot and benchmark runs neither read nor write it. The file is skipped until the engine has created the profile. Wayland ignores the position; the size still applies. Consumers: Mail UI; engine and CLI unchanged.

Revision 21 (D132) adds the app icon and a themed native title bar. The icon comes from `gui/app-icon/app-icon.png` for the executable, window, macOS bundle and installer. The system title bar and its controls stay; its background is the window colour and its controls follow the theme's dark or light scheme, updating on theme change. Windows 10 and Linux keep their default bar colour. Consumers: GUI shell, packaging and Quality; engine, schema, fixtures and protocol unchanged. D105 remote-content runtime and vendored HTML renderer pass Windows checks; Mac and Linux builds, packages and a real proxy remain open. Revision 18's D040/P2-01C setup evidence is unchanged. Whole readiness remains 0/9; historical evidence retains its original source scope.

Revision 20 (D130) makes the package a single file for testers. Windows: a per-user Inno Setup installer (no admin, Start menu entry, uninstaller, upgrade replaces program files, app-local MSVC runtime). macOS: a disk image holding `fuckMail.app` and an Applications shortcut. Linux keeps its archive. Install, upgrade and uninstall never touch profiles. Both are unsigned until Developer ID/notarization and an optional Windows certificate are added. Consumers: developer tooling, CI and Quality; engine, GUI, schema, fixtures and protocol are unchanged.

Revision 19 (D105) adds the engine's remote-content runtime. It is the only code that fetches anything a rendered message refers to; Google API and local-model traffic are outside it.
- **Requests:** GET only, https origins on port 443. `http://` sources, and redirects to http, are never requested. At most 3 redirects, each rechecked against scheme, destination and blocklist before connecting.
- **Headers and limits:** a fixed `User-Agent: Mozilla/5.0` and an image-only `Accept`; no cookies, Referer, Accept-Language or conditional headers. 5 s connect, 15 s total, 5 MiB per image, 8 fetches at once.
- **Direct mode:** the engine resolves names itself and connects only to public addresses. Loopback, private, link-local, CGNAT, multicast, unspecified, broadcast, ULA and IPv4-mapped forms are refused.
- **Proxy mode:** every request is tunnelled through `https://host:port` with basic authentication. The proxy's certificate is checked against the OS roots. The origin is never resolved locally, and nothing falls back to a direct connection: an unreachable proxy, a rejected login, or a missing or unreadable password each fail as `proxy_unavailable`. Private-network protection behind the proxy depends on the proxy.
- **Password:** stored in the OS vault under service `fuckmail.proxy.v1.<sha256(profile namespace)>`, user `remote-content`. It is never returned over IPC. The engine reads it when it builds the proxy client, and again after the settings or password change. Unlike Google credentials there is no denial latch, so a Mac Keychain refusal may be asked again on the next fetch; that remains to be checked on Mac.
- **Cancellation:** closing the requesting connection cancels its fetch, and nothing is cached for it. Removing an account stops that account's work.

The GUI vendors litehtml (`b58fd10`, BSD-3) and its bundled gumbo (Apache-2.0) under `gui/third_party/litehtml`, built by `gui/cmake/litehtml.cmake` as two static libraries with no new Qt module. See `VENDORED.md` for updates. HTML requests are allowed on per-operation connections in synthetic and live profiles alike. Consumers: Core57, Mail UI74, CLI44 and Quality75; fixtures0/3 and protocol1 unchanged.

D054 respects native credential denial: serialize native access, latch failures across account workers, and retry only after explicit Sync/reconnect. Preserve OS access controls, secrets and existing credential references; no trust changes or plaintext fallback. Updated unsigned development binaries can still require one OS approval.

Revision 16: D049/P2-UI keeps the startup overlay fully visible for at least 1,750 ms before a 350-ms fade, and waits longer when startup is not ready. Startup failure bypasses the hold so errors remain accessible. The lettering animation remains native QML. Consumers: Main, StartupOverlay and native UI acceptance; schema/fixtures/protocol unchanged.

Mac verification additionally requires provider-operation completion callbacks to run after the socket notification returns; a callback may immediately destroy its operation. Cancellation must fence queued handshakes and preserve one-shot results. This implements the existing lifecycle requirement without changing any command, receipt or timeout.

D040 makes the OAuth desktop-client registration an app/package input. A configured build ships `google-oauth-client.json` beside the engine; `FM_GOOGLE_CLIENT_CONFIG` is an explicit developer override. The GUI never asks an account user to locate JSON. Missing/invalid registration is a visible setup failure before browser launch. Keep setup files and staged artifacts outside Git. Stored client-ID restrictions remain: refreshing a moved package can resolve its current registration only if the ID matches; existing valid per-profile paths and explicit CLI overrides remain usable. Never substitute another client for an existing grant. Tokens remain in the native vault; ordinary setup inspection starts no browser, OAuth request or sync. Consumers: Platform 15/Gmail 13/Mail UI 26/CLI 21/Quality 24 with Core 27/Calendar 20/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged.

D038 ordinary GUI/CLI startup uses the same `fuckMail/default` user-data path and creates an empty live store. New synthetic content requires explicit `--demo-profile` or a named verification/smoke/self-test mode; `demo-v1` and explicitly selected existing profiles retain their immutable kind and data. Under D040, Settings Add/Restore/reconnect take an email and use the configured app registration, system browser and native credential store. Sign-in alone starts neither acquisition nor outbound work. CLI Google/account commands retain their explicit `--profile` requirement. Implementation and generated checks do not establish actual-account acceptance.

D038 running-app notifications use the saved `events` default, `all` (events and new incoming mail), or `off`. The open GUI polls cached state every 15 seconds; it starts no provider owner or background service, and closing it ends polling. Calendar cards require a durable local shown receipt before display, suppress restart duplicates, and discard stale account/grant/calendar-selection/source callbacks. New-mail alerts initialize a bounded local cursor at the current cache end before deferrable Calendar work, then skip history and nonincoming/archived messages; changing Calendar selection does not reset that cursor. Native notification availability and operating-system delivery remain separate from the app's local display receipt. Explicit Join stays available with alerts off and opens only the current permitted event URL returned by its local acknowledgment. Final packaged and actual-account behavior remains to be verified.

Windows paired account checks pass: `cli-accounts-04` in 7.282 s and `native-accounts-06` in 14.265 s, using isolated generated identities and provider/vault fixtures. Final `gui-dev-ctest-02` passes 18/18 (17 native checks plus one Rust build fixture), taking 196.13 s in CTest / 196.609 s wall; its integration run takes 138.25 s within the unchanged D030 aggregate 180 s limit. Seven delegate notices and one teardown notice remain, alongside prior failures. This verifies the scoped functional lifecycle, not native vault/disk protection, real Google accounts, other-host qualification, current packages or deferred performance. Interim evidence: [P2-01B manifest](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json).

Revision 13 applies D036/P2-01B local account removal and same-subject OAuth restoration. Add CredentialVault removal through the existing coordinator-owned account lock shared with refresh and grant persistence; the lock stays with a queued/running native cleanup when a caller is cancelled. Commit inactivity first and expose pending/failed cleanup rather than claim deletion on failure. A failed restore that may have persisted a new grant leaves the account removed with truthful cleanup state. Ordinary Add/reconnect cannot silently restore an archive; explicit restore rechecks generation/lifecycle before persistence and binding and starts no provider owners. Keep client-ID/config references unchanged. Use default-off synthetic account fixtures and actual loopback/PKCE/state/nonce validation for paired CLI/native acceptance; no native credential or real HTTP fallback, and no tokens/codes/auth URLs in reports. Actual provider, physical vault/disk and package gates remain separate. Consumers: Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20/schema 18; Search 5, fixtures 0/3/protocol 1 remain unchanged.

Revision 12 defines D026/P2-08A's default-off simulated Google engine build. It runs only against a newly created private disposable profile, marked before store initialization. Ordinary startup always refuses a marked verification profile before native vault/network construction, including a later attempt without the feature flag. Verification builds inject only an in-memory vault and allowlisted synthetic transport, with no native credential or real HTTP fallback; bounded trace recording performs no file I/O on the Core writer. Separate build output prevents packaging it as the ordinary engine. Account, protocol, token coordination, quotas and owner lifetimes remain real application paths. Consumers: engine startup/acquisition, verification runner, Core 17, Gmail 7, Calendar 15, Mail UI 15, CLI 10 and Quality 12. Schema 11 and fixtures 0/3 are unchanged. Tests must prove isolation/refusal and final-owner cancellation before any performance claim.

[P2-08A process and native acquisition checks](../research/phase-2-execution.md#p2-08a-native-navigation-during-gmail-acquisition-under-d026) pass the default/feature startup fences, explicit-owner admission, GUI-close independence, separate CLI account lifetimes and cancellation of held RAW work on final-owner exit. The native engine exits 0 and removes discovery about 486 ms after that final owner ends. These generated-provider Windows development checks do not verify native vault access, real profile encryption/backup, physical input or current packages; the separate Qt shutdown warning remains recorded.

Revision 11 prepares D020's real-account pilot. The founder explicitly selected OS account permissions and verified full-disk/device encryption for local mail, indexes, attachments and backups; app-level mail-store encryption is deferred. Verify the actual host/volumes and private profile/temporary-file permissions before real import. Credential protection remains separate: native Windows/macOS/Linux vault adapters must fail clearly when locked/unavailable, with no plaintext fallback or secrets exposed to QML, CLI output or logs. Windows is the first physical pilot; other-host live access waits for each host's vault, data protection and lifecycle qualification.

Desktop Google login uses the system browser, bound one-use loopback callback, PKCE/state/nonce and validated identity/grants. Ordinary queries cannot silently open a browser. Reconnect, local disconnect, project-wide revocation and local data deletion are distinct; preserve drafts/pending work and explain scope before an explicit destructive action. The real profile is separate from fixtures and SDK/repository/cloud-synced paths. Routine diagnostics omit private mail/calendar fields and credentials; protected private oracles are not CI artifacts.

Enable existing sync/notification/account controls only as actual provider and OS capabilities pass. Bound cooperative network/parse/commit work and honor quota cancellation; close ends GUI polling/reminders, while separately invoked CLI work has explicit lifetime. Native reminders deduplicate account/calendar/occurrence and reconcile on resume. Backups/restores, disk-full behavior, SDK-free packages and unchanged navigation/rendering budgets are pilot gates; no hidden service or new server is added. Consumers: Core 14, Gmail 4, Calendar 12, Mail UI 14, CLI 8, Quality 11. See [Phase 2 packets](../plans/phase-2.md); the existing runtime/toolchain remains unchanged by this plan.

Revision 10 applies D018's September 12 Update 2 reference. Shared native palettes, rounded text scaling and typed startup preferences apply across Mail, Calendar, Settings and secondary controls without changing selected accounts, authored drafts or source state. General becomes the initial Settings page; timeline placement remains Mail-only. Provider sync and desktop notification settings remain visibly unavailable in this local phase. Consumers: Core 13, CLI 7, Mail UI 12, Calendar 11, Quality 9 and native acceptance tooling; schema 7/fresh fixture 3/additive protocol 1. Implementation and integrated validation are active; earlier acceptance retains its original source scope.

Revision 9 adds safe destruction of a still-connected GUI adapter: socket and timer callbacks detach before request/view storage is destroyed. Normal close continues to flush through the explicit shutdown path. For pinned Qt 6.11.2 on Windows, the GUI defaults `QT_D3D_NO_VBLANK_THREAD=1` before creating the application, preserving an explicit environment override. This selects Qt's timer update fallback while retaining D3D rendering and presentation vsync. The host's original path delivered one frame and no subsequent update requests; the fallback delivered 2,881 synchronization/render/swap callbacks and passed all four 100-observation paint classes. This private Qt switch must be rechecked when changing the pinned toolkit. Consumers: Qt GUI, Windows packages, developer launches and Quality. Physical input/scaling and trusted distribution remain separate.

Revision 8 records the home Windows handoff correction: developer subprocesses resolve bare executable names through the selected project PATH before spawning, so packaging works with project-local Rust and no global Rust installation. The existing compiler/SDK are reused; no global PATH change or runtime/schema/protocol change is introduced. Dev/release CTests, relocated packaging and native Windows/D3D11 rendering now pass on the founder's PC. Consumers: developer tooling, Core/CLI/GUI build and Quality. Manual input, accessibility, wider scaling and OS vault checks remain separate.

Revision 7 records the final Phase 0 platform corrections and packaged upgrade verifier. Windows discovery creation explicitly sets the process-user owner and private DACL, with a native named-pipe trust test; startup allowance is 30 seconds and offscreen tests use system fonts. Rust/Qt now retry startup while a previous store writer finishes closing; retries are bounded and preserve explicit GUI/CLI ownership. Linux category focus preserves reply selection. Developer launchers use CMake's generated executable path. Available-host native/package results and actual Mac schema 2→4 handoff/refusal evidence were recorded at that revision. Consumers: Core, CLI, Qt UI and Quality. Physical desktop/keyring qualification and trusted delivery remain separate gates.

Revision 6 records the next Phase 0 integration checkpoint. Mac CI has passed build, tests and relocated package checks. Native Windows compilation passed, but its test compilation exceeded the original execution timeout and GUI failures still need diagnostics. Linux bootstrap now installs the official Qt ICU archive. Consumers: Core, CLI, Qt UI and Quality; physical Windows/Linux desktop qualification remains unverified.

Revision 5 records D012’s implemented experimental slice. Pinned native developer tooling and Mac/Windows wrappers now exist; commands and observed host evidence are in [development setup](../build-development.md). Native CI jobs and package recipes require actual runner/runtime results. Consumers: Core, CLI, GUI and Quality. No trusted release or physical Windows/Linux desktop qualification is claimed.

Revision 4 applies D010: development alternates between the work Mac and home Windows PC, with reproducible native setup, shared commands and explicit handoff/CI evidence. D009's QML/no-Node and paired GUI/CLI boundaries remain. Affected consumers: Core, Mail UI, CLI and Quality.

Owner: G — Platform/release. Accepted scope and proposed behavior are separated below; concrete APIs, libraries and outstanding choices await the relevant feasibility work. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Windows/macOS/Linux application shell, credentials, dialogs/OS integration, renderer isolation facilities, notification delivery, shutdown and personal packaging |
| Consumes | Supplied shared design; Core checkpoint/unsent state; Calendar reminder intents; assistance job cancellation and runner state |
| Produces | Consistent application shell, credential references, lifecycle/cancellation signals, notifications and installable GUI/engine/CLI packages |
| Independent start | Minimal shell/renderer/credential prototypes and fake app jobs; validate the recommended Qt Quick baseline against mockups and the required platform matrix |

Related: [mail UI](mail-ui.md), [calendar](calendar.md), [local assistance](local-ai.md), [desktop research](../research/desktop-stack.md).

## Shared design, required OS behavior

Windows, macOS and Linux are required. Linux includes Ubuntu, Arch and Omarchy; it is no longer optional. Maintain a tested release/architecture/compositor matrix, with Ubuntu LTS and recorded Arch/Omarchy snapshots. Exact version floors and CPU coverage remain to be pinned; begin with representative Windows x64, macOS Apple Silicon and Linux x64 systems rather than claiming every configuration.

Use the same opinionated design on all OSes. Verify native Wayland (including the Omarchy desktop), supported X11/XWayland paths, IME/clipboard, file dialogs/portals, scaling, graphics, credentials and notifications. Do not require GNOME/KDE-specific infrastructure without a working path on the targeted Omarchy setup.

The recommended baseline is a Rust engine/CLI and C++20/Qt Quick/QML GUI, connected through private local IPC; it is not yet a locked or measured stack. Build Rust components with Cargo and the GUI with CMake/Ninja, packaging compatible binaries together. In-process Rust/Qt bindings are not a required dependency. Core owns domain behavior and the engine protocol; Platform owns the presentation/OS adapter and release integration. QML expressions are accepted. The product build must not require Node.js or a separate JavaScript compilation/bundling pipeline. An upstream WebEngine source build requiring Node is not an acceptable default prerequisite; received HTML is later work and any chosen dependency route must respect this boundary. See [desktop research](../research/desktop-stack.md).

## Local appearance and startup

Use the exact Graphite, Paper and Harbour palettes from the [active reference inventory](../research/flight-progress-v2.md). Derive muted text, overlays, shadows, selection, semantic accents and timeline states from the selected palette, including Paper's light surfaces and warm shadow. Apply the palette to native control roles and secondary popups as well as the main canvas. Geist remains the shared typeface, with the licensed bundle and native fallback; this adds no font download or browser runtime. Text size scales label, input, reader and popup base sizes by 13/14, 1 or 15.5/14 and rounds the result to integer native pixels. Retain usable clipping, elision, focus indicators and scroll access at compact sizes. Scaling must not rewrite authored formatting or clear undo history.

These app-wide preferences use the existing revisioned Core preferences boundary and CLI, rather than a GUI-only settings file:

| Key | Values | Default |
| --- | --- | --- |
| `theme` | `graphite`, `paper`, `harbour` | `graphite` |
| `text_size` | `small`, `default`, `large` | `default` |
| `startup_view` | `mail`, `calendar`, `last` | `mail` |
| `last_workspace` | `mail`, `calendar` | `mail` |
| `timeline_position` | `footer`, `header` | `footer` |
| `default_event_duration_minutes` | integer `30`, `45`, `60` | `60` |

`startup_view` selects the initial workspace after hydration; `last` resolves to the last Mail/Calendar workspace, never Settings. Saving workspace navigation does not alter per-account mail selection or draft identity. Opening and closing Settings retains the active workspace, independent scroll positions and focus. Theme/text changes take effect immediately through the saved preference projection and survive a new GUI process; failures retain pending changes and expose the existing explicit retry path. Migration adds defaults without reseeding existing profiles or changing their recorded fixture revision.

General shows Sync every and Notifications as unavailable, disabled choices with a reason. It must not report a default as an applied provider/OS setting, equate local IPC polling with provider sync or add a hidden always-on process. Account/model setup, notification delivery, OS vault qualification and trusted distribution remain separate gates. Native control/CLI readback, draft preservation, fresh-process startup, all three palette captures and compact text-size checks are required evidence; implementing controls alone does not satisfy them.

## Installation and upgrades

### Development on both founder machines

macOS and Windows are active development hosts from the first implementation package. Supply repeatable setup, diagnostic, build, test, run and development-package entry points usable by the founder and coding agents. Pin mutually compatible Rust/Qt/native build dependencies after validating them; keep machine-specific SDK paths outside tracked configuration. Reuse shared orchestration and fixtures, with thin native shell/PowerShell setup where OS differences require it.

Use native macOS and Windows CI builds to catch regressions on the other host and retain versioned development artifacts and failure logs. Local and CI builds consume the same dependency/configuration inputs. Caches may accelerate incremental work but must not be required for a clean build. Test actual desktop behavior on the corresponding machine; CI compilation alone does not verify its graphics/input or credential store. Linux's existing product gates remain required.

Transfer source, lockfiles, fixtures and an accurate handoff/status record through Git. Build caches, local SDK paths, live profiles/databases and credentials remain machine-local; do not use a cloud-synced database as a development handoff. Record the source revision, commands, verified host and outstanding checks so the next session can resume without relying on chat history. CI uses synthetic data and requires no provider secrets.

### Application delivery

First smoke packages contain the engine, a small CLI and a working GUI. They need not bundle WebEngine or any other HTML display engine. Later renderer packages add their own resource/isolation/update checks before that capability ships. Pair GUI and CLI from the first slice to test local IPC access, permissions and ownership rather than building the entire CLI first.

End-user installation is a required product feature. Ship GUI, engine and CLI together with bundled or declared runtime dependencies and a working CLI launcher/PATH integration. Users do not compile source or install developer SDKs; a local model is optional and separately configured.

Proposed routes: signed Windows installer, signed/notarized macOS distribution with a simple CLI installation path, prebuilt Ubuntu .deb and prebuilt Arch/Omarchy pacman package. Choose exact formats/channels through clean-machine tests; neither store publication nor a universal Linux bundle is required. See [distribution research](../research/linux-and-distribution.md).

Supply application OAuth registration and direct user sign-in; developer/provider verification work belongs to release preparation rather than the user's install instructions. Verify first launch/OAuth, credential store availability, text/composer resources, desktop integration and CLI discovery. Upgrades must coordinate engine shutdown/migrations, keep data outside the installation directory and refuse incompatible schema rollback safely. Normal uninstall removes app/launchers, preserving user mail/drafts unless explicit data deletion was requested. Supply a maintained update route and dependency/license notices; broad public launch work remains separate.

## App lifecycle

Closing the main window quits the GUI and its session: save drafts/checkpoints, stop its synchronization and inference, and remove its scheduled reminders. No tray/menu-bar or startup service remains simply because the GUI was used.

The full CLI can run independently. A separately invoked command or foreground watch owns its requested work, even with the GUI closed. Recommended implementation: one private on-demand local engine, active only for explicit GUI/CLI clients/jobs. When the last client ends, cancel/checkpoint and stop; queued durable intent alone does not keep the engine alive. CLI-only sessions do not emit GUI reminder notifications by default. See [CLI](cli.md) and [Core](core-storage.md).

Closing a window must not kill an independently invoked CLI command or a user's separately installed model runner. A shared runner may retain its model for other apps, but no inference request continues without an active owning session.

## Durable close and process ownership

Never require a network acknowledgement to save a draft or quit. Cancel work according to the owning session and retain unconfirmed operations, including uncertain sends, for reconciliation. Explicit CLI operation status must distinguish durable acceptance from remote completion.

Verify runner-side cancellation, client-disconnect recovery and engine exit after the last client. An independent foreground CLI watch is visible, explicit activity; no automatic detached helper is installed. Count GUI, engine, CLI and renderer resources separately and in aggregate; report local-runner incremental memory/VRAM separately.

## Rendering, privacy, and recovery

Treat message HTML and calendar descriptions as untrusted content. Sanitize and isolate rendering; it must have no privileged app bridge or script execution. Remote image loading is a product-controlled policy. Opening links or attachments uses explicit user actions.

Keep tokens/passwords in the OS credential store and out of UI storage and logs. Exclude source mail, prompts and generated summaries from routine diagnostics. App-level encryption of message data, generated results and indexes is an open decision; credential storage alone does not encrypt cached mail. Account removal, backups, upgrades and index rebuilds must have explicit behavior for local drafts, retained attachments, category overrides, pending inference and cached results. Remove account-scoped model results and cancel its jobs when an account is removed.

Keep the database and live indexes on local storage, outside cloud-synced folders. Add migration/recovery fixtures before real daily-driver use. No telemetry upload or central mail service is assumed.

## Trust and recovery

Proposed baseline: credentials in the OS credential store, account-scoped records, isolated HTML rendering and user-controlled remote content. Routine diagnostics contain no message contents, generated summaries, model prompts, credentials or private calendar URLs. The local model has no tools or provider credentials; validate category output before applying the configured policy. Rebuilds and resyncs preserve drafts, manual category overrides and pending operations. App-level mail-store encryption remains a separate choice from storing credentials securely.

## Acceptance gates

- Install GUI/engine/CLI from prebuilt packages on clean Windows, macOS, Ubuntu, Arch and Omarchy systems with no developer tools. Verify command discovery, Wayland/other supported sessions, input, scaling, keyring/portals and accessibility.
- Close during GUI synchronization, drafting, notifications and inference; cancel that session and retain durable intent. Repeat with an independent CLI command/watch active, then end it and verify engine shutdown. No GUI reminder fires after close; no unrelated runner is killed.
- Upgrade/reinstall/uninstall through the intended end-user path; preserve data and keep GUI/CLI schema/protocol versions compatible.
- Isolate received HTML and control remote resources, with no script execution or privileged app bridge. Account removal, upgrades and recovery respect durable user data and clear account-scoped derived content.
- Include every app/renderer process in resource measurements and report the external runner separately. See [quality](quality.md).

## Open before affected behavior ships

Pin OS/version/architecture support, the Rust/Qt/renderer/toolchain versions, package formats and trusted signing/update routes after the prototype and clean-install evidence. Finalize headless credential provisioning, local transport access checks and engine/client lifetime rules. Decide mail-store encryption, remote images and backup/export expectations. License/dependency review and deliverable installation are MVP work; app-store listings and a broad public launch remain optional later scope.

## D052 / P2-LLM consumer boundary

Revision 17 consumes [Local assistance 6](local-ai.md): shared discovery/settings/jobs/results/correction commands, schema-21 storage and manual precedence, bounded owned Ollama work, native/CLI parity and scoped acceptance. Runtime implementation/verification is in progress; fixtures 0/3 and protocol 1 remain unchanged. Earlier evidence retains its original revisions.
