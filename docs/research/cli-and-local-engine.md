# CLI and the shared local engine

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

## Next experiment

P0-10: build a Qt-free Rust engine with fake providers, a Rust headless client and a C++/Qt model adapter against shared protocol fixtures. Package the three components. Run a paginated query, a journaled mutation, cancellation and a foreground change stream with the GUI absent; then repeat while a UI client edits the same fixture. Prove one writer, protocol/version mismatch handling, restart behavior and exit after the last client. Test credentials both unlocked and unavailable. Measure CLI startup/engine cost separately from GUI rendering.
