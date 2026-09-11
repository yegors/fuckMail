# CLI and the shared local engine

**Delivery update under D009 (2026-09-11):** implement a small real CLI and GUI together, using both for IPC access, account scope, concurrency and recovery experiments. Expand command coverage with each Google workflow. A full CLI catalogue is not a prerequisite for GUI work; both clients must be usable and dogfooded on Google before legacy adapters start.

Research date: 2026-09-11. CLI scope is accepted; process/transport choices here are recommendations, with no prototype or benchmark. Build boundary: [CLI contract](../contracts/cli.md) and [Core](../contracts/core-storage.md).

## What changes with a full CLI

Mail, calendar, search and mutations must be usable without a window. Share the application core, operation journal and provider adapters; do not build a second mail client behind the command line. The CLI also makes a stable structured API and predictable noninteractive errors product requirements. No language has exclusive access to these capabilities.

The revised recommendation is a Qt-free Rust engine and Rust command client, with C++/QML only in the GUI. This is a project-fit choice, not a CLI requirement: Qt Core also supports non-GUI applications through QCoreApplication. C++ could share one core without loading QML/WebEngine or duplicating business logic; actual linking/startup dependencies must be verified. [QCoreApplication](https://doc.qt.io/qt-6/qcoreapplication.html).

## Proposed process model

Use one on-demand Rust engine per local profile, with the Qt GUI and Rust CLI as peer clients. Give it sole ownership of migrations, writes, provider work and durable operations. Run only while an explicitly active client/job needs it; no default login service. This costs local IPC and lifecycle coordination but avoids separate GUI/CLI synchronization implementations and concurrent migration/submission races.

A foreground CLI command or watch can outlive a closed GUI because it was separately invoked. GUI-owned reminders/background work stop on close. When the final client ends, cancel/checkpoint its work and exit the engine; pending durable intent waits until a later explicit session. Define leases/disconnect recovery so a crashed client cannot leave an orphan worker indefinitely. This clarifies the old GUI-only close rule rather than requiring an always-on daemon.

Use the existing process boundary as the language boundary: each client exchanges versioned commands, bounded query pages and revisioned changes with the engine. Share protocol definitions/fixtures between Rust and C++; do not serialize the complete mailbox or expose the database schema. Keep Qt list models and update notifications in the GUI process. Transport framing, backpressure, subscriptions and restart/version recovery still need implementation and tests. Cargo and CMake can build separate executables without a shared Rust/C++ ABI or CXX-Qt; package them as one product.

Qt provides local socket/server APIs across the required OSes. Its documentation notes that permissions behave differently on macOS, Linux and Windows; socket flags alone do not establish an identical access boundary. Design private endpoint placement, peer authorization and version negotiation explicitly, and test engine exclusivity after crashes. No network-accessible control service is required. [QLocalServer](https://doc.qt.io/qt-6/qlocalserver.html).

## Alternatives considered

- Link the core directly into every process: simple for one client, but concurrent GUI/CLI writes, migrations and sync leadership become harder to reason about.
- Route CLI through the running GUI: fewer processes but fails the required GUI-independent workflow.
- Install a permanent daemon: convenient for continuous workflows but unnecessary for explicit command/watch sessions and contrary to the default close behavior.
- In-process Rust core plus Qt UI: feasible through bindings such as CXX-Qt, but unnecessary for the proposed engine process. Consider it only if measured IPC costs or lifecycle constraints justify the additional binding/build/ownership boundary. This differs from the recommended Rust engine over IPC. [CXX-Qt](https://kdab.github.io/cxx-qt/book/).

## Candidate transport and model boundary

**Recommendation to test, not an implemented protocol:** use a local byte stream with bounded, length-prefixed UTF-8 JSON envelopes. Rust can use Tokio Unix sockets / Windows named pipes; the GUI can use QLocalSocket. Validate their interoperability before committing. Keep CLI JSON output independent of wire framing. A schema document and shared valid/invalid fixtures should define stable IDs, revisions, operation outcomes and compatibility; do not expose Rust enums or Qt objects as a wire ABI. Consider a different encoding only after measured parsing or size costs justify it.

Each connection needs a negotiated protocol version, request correlation, account/profile scope, cancellation and bounded outstanding work. Define a maximum frame/page size, incomplete-frame timeout, per-client queue budget and slow-consumer outcome before implementation. A lagging subscription must receive a detectable gap/resnapshot condition instead of silently losing changes or retaining an unbounded history. Bodies/attachments need a separately bounded transfer path; never serialize the whole mailbox or base64 every attachment into list responses. These are interface questions for P0-10, not invented command names or fixed numeric limits.

Use endpoint discovery inside a validated user-private location, with a per-profile engine lock separate from the endpoint name. On Linux, first evaluate XDG_RUNTIME_DIR, whose specification requires user ownership and restrictive permissions; define a tested private fallback/error when a headless session lacks it. Unix socket permissions alone are insufficient on macOS. Windows needs an explicit current-user access policy and remote clients rejected, not a default pipe security descriptor. Evaluate OS peer checks plus a protected per-launch handshake secret; prove the client has connected to the expected engine as well as authorizing the client. The supported boundary is the OS user/profile, not a promise to isolate hostile software already running with that user's authority. [XDG runtime directory](https://specifications.freedesktop.org/basedir/latest/), [Windows pipe access](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights).

Qt notes that multiple Windows servers can listen on the same pipe name. Tokio exposes first-pipe-instance and remote-client controls, but neither substitutes for profile locking and explicit access checks. Startup recovery must verify owner/liveness before removing a stale endpoint; never unlink a live engine's socket merely because connection failed. Keep any platform-specific unsafe/native code narrow and separately reviewed. [Tokio pipe options](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html), [Qt local-server behavior](https://doc.qt.io/qt-6/qlocalserver.html).

The C++ presentation adapter owns an incrementally loaded QAbstractItemModel. Fetch pages asynchronously, apply changes on the model's GUI thread, and preserve selection through stable IDs. Prototype cursor invalidation when rows arrive, disappear or change sorting; an expired cursor should trigger an explicit bounded refresh. Model reset on every arrival and loading all conversations into a QML array are poor candidates. Test read/star totals separately from loaded rows and never derive an account's zero state from the visible page. Qt documents fetchMore/canFetchMore, row-change notifications and the GUI-thread requirement. [Qt model documentation](https://doc.qt.io/qt-6/qabstractitemmodel.html).

## P0-10: smallest shared-engine investigation

Execution is future work under the [Phase 0 plan](../plans/phase-0.md). Prerequisites: an authorized implementation start, agreed initial synthetic identity/revision fixtures, a provisioned Rust/Qt build environment on an available first target. Windows/macOS/Linux test systems are needed for their respective platform claims; unavailable hosts stay unverified without blocking independent first-target experiments. Detailed final visuals and live accounts are unnecessary.

| Step | Future work | Exit evidence |
| --- | --- | --- |
| Transport and discovery | Rust engine, Rust command client and minimal Qt model adapter; local endpoint creation, handshake, one paged query and one invalid request | The same protocol fixtures work in both languages; no display or Qt/renderer dependency is needed by the engine/CLI; unauthorized/mismatched clients fail clearly |
| Single engine and lifecycle | Concurrent GUI/CLI starts, engine/client crashes, stale discovery data, final-client shutdown and a surviving explicit CLI watch | One writer/migration owner; no orphan work, mistaken socket deletion or GUI-owned reminder after close; queued intent remains dormant without clients |
| Durable mutation and stream | One snapshot-scoped flag mutation through either client; record local acceptance, replay request ID, reconnect stream and cancel jobs | Equivalent shared state, preserved new arrivals, detectable stream gaps and bounded queues; local deduplication is not represented as exactly-once delivery |
| Presentation and install integration | Page a large synthetic list, update selected rows while scrolling, and launch the same binaries from the first smoke packages | No whole-mailbox transfer, selection loss or growing queued-update backlog; startup, IPC and rendering costs measured separately |

CLI discovery/help, capability/version output and structured errors come first. The first workflow slice then adds account-scoped list/read, a read/star action and operation status. Include file/stdin Unicode and multiline input, stderr/stdout separation, broken pipes, Ctrl-C and noninteractive missing-credential errors. These are initial validation cases; the full [CLI contract](../contracts/cli.md) remains an MVP obligation across later mail/calendar features.

Use a bounded owner/session model for shared work. When GUI and CLI need the same sync operation, closing one must remove its interest without killing the other's valid work. Cancellation records uncertain external outcomes rather than pretending an in-flight submission was undone. A blocking worker requires its own cooperative stop/checkpoint path: Tokio documents that started spawn_blocking work cannot simply be aborted. [Tokio blocking-task cancellation](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

**Decision output:** record the proven transport/access model, wire-schema/fixture revision, lifetime rules, unresolved cases and measured startup/memory costs in Core/CLI/Platform. Passing the minimal transport/lifecycle slice can unblock local foundation work; full stream scalability and CLI workflow parity continue through their consuming feature gates. No such evidence exists yet.
