# Platform, lifecycle and release

State: Draft · Revision: 7 · Updated: 2026-09-11 · Implementation: Phase 0 synthetic experiment verified; whole area remains Draft.

Revision 7 records the final Phase 0 platform corrections and packaged upgrade verifier. Windows discovery creation explicitly sets the process-user owner and private DACL, with a native named-pipe trust test; startup allowance is 30 seconds and offscreen tests use system fonts. Rust/Qt now retry startup while a previous store writer finishes closing; retries are bounded and preserve explicit GUI/CLI ownership. Linux category focus preserves reply selection. Developer launchers use CMake's generated executable path. Available-host native/package results and actual Mac schema 2→4 handoff/refusal evidence are recorded in [execution evidence](../research/phase-0-execution.md). Consumers: Core, CLI, Qt UI and Quality. Physical desktop/keyring qualification and trusted delivery remain separate gates.

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

## Installation and upgrades

### Development on both founder machines

macOS and Windows are active development hosts from the first implementation package. Supply repeatable setup, diagnostic, build, test, run and development-package entry points usable by the founder and coding agents. Pin mutually compatible Rust/Qt/native build dependencies after validating them; keep machine-specific SDK paths outside tracked configuration. Reuse shared orchestration and fixtures, with thin native shell/PowerShell setup where OS differences require it.

Use native macOS and Windows CI builds to catch regressions on the other host and retain versioned development artifacts and failure logs. Local and CI builds consume the same dependency/configuration inputs. Caches may accelerate incremental work but must not be required for a clean build. Test actual desktop behavior on the corresponding machine; CI compilation alone does not verify its graphics/input or credential store. Linux's existing product gates remain required.

Transfer source, lockfiles, fixtures and an accurate handoff/status record through Git. Build caches, local SDK paths, live profiles/databases and credentials remain machine-local; do not use a cloud-synced database as a development handoff. Record the source revision, commands, verified host and outstanding checks so the next session can resume without relying on chat history. The [first deliverable](../plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) uses synthetic data and requires no provider secrets in CI.

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
