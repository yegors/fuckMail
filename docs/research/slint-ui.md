# Rust + Slint + SQLite: UI investigation

Research date: 2026-09-10. Status: candidate under investigation, not an accepted implementation stack. This is source research and inspection of published interfaces; no native prototype or performance benchmark has run.

Evaluation scope updated 2026-09-11 to include daily unread/starred triage, provider-category controls and optional local summaries. These requirements do not select Slint or change the human-only composer rule.

## Current evaluation scope

Updated 2026-09-11: Linux (Ubuntu, Arch/Omarchy), full CLI and smooth installation are now mandatory. The [current recommendation](desktop-stack.md) leads with a C++20/Qt Quick UI and Rust engine/CLI over private local IPC; Slint remains a UI alternative. Earlier Windows/macOS-focused examples below are historical source evidence, not the complete new release matrix. Evaluate the candidate on all required platforms using the supplied [nonfinal mockups](design-references.md).

## Assessment

Rust + Slint + SQLite is a credible candidate for a visually polished Windows/macOS client without a JavaScript application or npm toolchain. Slint provides substantial control over layout and appearance. Its limits for this project are more likely to be HTML email, formatted composition and desktop integration than the ability to draw an attractive interface.

The earlier recommendation was to evaluate this candidate first. The subsequent design/CLI/Linux review favors a Qt Quick prototype; compare Slint against the same full set of requirements if evidence warrants it. The stack must demonstrate the desired appearance and mouse behavior on all required operating systems before selection; source screenshots cannot establish how our app will feel.

## What each part does

| Part | Proposed responsibility | Implication |
| --- | --- | --- |
| Rust | Provider adapters, synchronization, MIME processing, threading, search requests and durable operations | Keep network, parsing and storage work off the UI event loop |
| Slint | Account navigation, conversation rows, calendar, controls and application layout | Custom `.slint` UI descriptions connected to Rust; no TypeScript frontend needed |
| SQLite + FTS5 | Local records, operation journal and sender/subject/body search | Database choice does not determine visual style |
| Content storage | Complete message and attachment bytes on disk | Load content as needed for display while retaining it all offline |
| HTML/composition integration | Received HTML display and authored rich content | Still an unresolved component decision; Slint alone is not a complete mail UI toolkit |

Slint's Rust integration compiles declarative UI descriptions into the application. Its syntax is a dedicated UI language, not JavaScript. A browser-based Slint demo uses a different deployment target; it does not imply that our desktop application needs a web frontend. Native libraries such as SQLite and an optional Skia renderer still have their own build dependencies. “No JS application ecosystem” does not mean every transitive dependency is written in Rust. [Slint documentation](https://docs.slint.dev/latest/docs/slint/), [WSL Dashboard build configuration](https://github.com/owu/wsl-dashboard/blob/main/Cargo.toml).

## Published visual examples

These are actual published screenshots or recordings, not concepts for fuckMail. Aava and WSL Dashboard verify Rust + Slint use; the weather example also uses Rust. LibrePCB verifies Slint's desktop UI capabilities with a mixed C++/Qt architecture. SQLite use is not established for these examples, and none establishes large-mailbox performance. Images remain linked to their publishers and may change or become unavailable.

### WSL Dashboard — restrained desktop layout

![WSL Dashboard: light sidebar, instance rows and compact controls](https://madewithslint.com/assets/img/madewithslint-wsl.png)

The clearest visual reference for our shell: distinct navigation, readable rows, restrained color and unobtrusive controls. The project identifies Rust, Slint and Skia as its stack. This gallery screenshot represents the version pictured, not necessarily the latest release. [Product](https://www.wslui.com/), [source](https://github.com/owu/wsl-dashboard), [Slint showcase attribution](https://slint.dev/showcase).

### Aava — strongly customized controls and surfaces

![Aava audio plugin: custom dials, waveforms, colors and panels](https://slint.dev/success/viiri-audio/aava.png)

A released audio plugin built with Rust and Slint. It demonstrates a distinctive visual identity, shaped controls, layered surfaces and graphical content. Its visual complexity is useful evidence of flexibility; our mail UI should have much less on screen. The creator also reports friction at the Rust/Slint boundary and long compilation times for complex interfaces. [Creator interview and screenshot](https://slint.dev/success/viiri-audio).

### LibrePCB 2.0 — substantial desktop application structure

![LibrePCB's published animation of its new Slint window and split editors](https://librepcb.org/blog/2026-01-28_release_2.0.0/new-window.gif)

The relevant evidence is adaptable panes, document tabs, sidebars and a large working area. Version 2.0 shipped the redesigned Slint interface. Its implementation retains C++/Qt business logic and legacy dialogs, so this is not evidence for an all-Rust app. [Released UI](https://librepcb.org/blog/2026-01-28_release_2.0.0/), [migration architecture](https://librepcb.org/blog/2025-09-12_preview_of_next_gen_ui/). Screenshot/animation credit: LibrePCB; its site states CC BY 3.0 or later unless otherwise noted.

### Official weather demo — custom, responsive presentation

![Slint weather demo: custom blue layout and forecast graphics](https://slint.dev/assets/img/weather.png)

This is a demonstration, not an independently shipped desktop product. It combines custom widgets and responsive layouts with a Rust backend and Tokio. Useful for seeing how far the UI can move from standard desktop forms. [Live demo](https://slint.dev/demos/weather-demo/), [source](https://github.com/slint-ui/slint/tree/master/demos/weather-demo), [official description](https://slint.dev/demos).

For hands-on control behavior, also try the [Fluent widget gallery](https://slint.dev/demos/gallery/). The [full showcase](https://slint.dev/showcase) includes more applications, but examples differ substantially in age, polish, language and maturity. A browser demonstration cannot validate native scrolling, accessibility or memory consumption.

## UI capabilities relevant to fuckMail

| Capability | Evidence and assessment | Work for our product |
| --- | --- | --- |
| Custom appearance | Custom components, colors, gradients and property animation support a coherent visual design | Define our own typography, spacing, icons, light/dark colors, selected states and density |
| Richer surface effects | Slint 1.17 adds inner/spread shadows for Skia | Choose and test a renderer deliberately; do not assume all effects work with all renderers |
| Large scrolling lists | `ListView` creates visible delegates instead of every row at once | Supply a paged Rust model, bounded caches and nonblocking queries; test long text and variable heights |
| Resizable desktop layouts | Layout primitives and real desktop examples support complex arrangements | Build account navigation, conversation list/reader and calendar layouts; preserve useful space at smaller sizes |
| Mouse and keyboard | Controls, focus, text editing, menus and tooltips provide building blocks | Verify selection, context menus, tab order, shortcuts, clipboard and platform conventions |
| Internal drag/drop | 1.17 provides single-window drag/drop | Implement calendar move/resize semantics and visual feedback; data correctness remains our responsibility |
| Files dragged from the OS | 1.17 release notes say cross-application drag/drop is still under development | Validate the pinned version/backend and native integration; do not promise attachment drops from Finder/Explorer yet |
| Calendar views | Custom layout and pointer handling are suitable building blocks | Build day/week/month/agenda, overlap layout, all-day rows and editing; these are not turnkey Google Calendar controls |
| Accessibility and scaling | Slint exposes accessibility properties and platform integration | Test VoiceOver/Narrator, focus, text scaling and mixed-DPI monitors on our actual custom controls |
| Styled text | Newer releases support styled/Markdown text display | Useful for application text; this does not establish arbitrary email HTML or WYSIWYG composition |

Sources: [colors and brushes](https://docs.slint.dev/latest/docs/slint/reference/colors-and-brushes/), [animations](https://docs.slint.dev/latest/docs/slint/guide/language/coding/animation/), [ListView](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/views/listview/), [common/accessibility properties](https://docs.slint.dev/latest/docs/slint/reference/common/), [1.17 release](https://slint.dev/blog/slint-1.17-released), [TextEdit](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/views/textedit/). Rows describing our implementation are proposals, not claims that Slint supplies complete mail/calendar components.

## Native rendering versus native OS controls

Slint is a native compiled UI toolkit, but its controls are generally drawn by Slint. It does not automatically provide an AppKit interface on macOS and a WinUI interface on Windows.

Starting with Slint 1.16, Fluent is the default appearance across platforms. The maintainers plan to reduce support for other built-in styles, including the Cupertino-like style, while retaining platform-specific behavior. We should plan a deliberate, consistent application design and test platform behavior ourselves. The founder now explicitly wants the same design across OSes, so visual conformity to macOS is not a selection criterion. [Official style policy](https://slint.dev/blog/default-native-style-change).

The earlier exploratory visual proposal was a quiet desktop design: compact account navigation, well-spaced conversation rows, a readable conversation pane, restrained account colors and a calendar with clear event hierarchy. Visual quality should come from typography, alignment and predictable interaction. Shadows and animation can support those choices, but must not delay everyday actions. No particular palette or reading-pane geometry has been approved; the founder's forthcoming design takes precedence over this exploratory proposal.

## HTML reader and composer are feasibility gates

Received email requires more than a styled label: tables, inline images, quotations, links, selection, copying, search highlights and inconsistent HTML/CSS need representative fixtures. `TextEdit` is a multiline text editor, not a documented complete HTML authoring surface. Do not silently reduce the accepted plain-text/HTML requirement to Markdown or plain text.

Slint has a Servo integration example. The current README checked 2026-09-11 exposes a reusable WebView wrapper but still labels Servo experimental and not ready for production. The May 2026 blog's future-work list should not be treated as the current feature inventory. Validate the pinned wrapper, required desktop behavior and packaging directly; experimental integration is not evidence that Slint cannot draw this design. [Integration update](https://slint.dev/blog/servo-with-slint-update), [current example and prerequisites](https://github.com/slint-ui/slint/tree/master/examples/servo).

Possible evaluation paths are an isolated OS webview for received mail, the Servo experiment, or a restricted native document renderer with clearly measured fidelity. OS webview embedding in Slint is integration work, not assumed to be a ready-made cross-platform widget. Formatted composition needs its own design and editing implementation. If acceptable composition requires introducing the JS application tooling the founder rejected, reconsider the stack instead of quietly adding it.

The HTML surface must preserve existing requirements for script-disabled, isolated received content and user-controlled remote images. Its processes, memory, dependencies and shutdown behavior count toward the application's budgets. Sharing a language with the UI is not evidence of safe handling of untrusted mail.

## Storage and search fit

SQLite/FTS5 supports field-scoped full-text queries, prefixes, relevance ranking and result snippets. This maps well to subject/body keyword search; sender addresses also need deliberately normalized fields and suitable ordinary indexes. Email punctuation and partial-address matching need explicit semantics. [SQLite FTS5 documentation](https://sqlite.org/fts5.html).

Propose message-level indexes with account-scoped conversation results. Query, group and fetch a bounded visible page on workers; apply the resulting model updates through the UI event loop. Do not load a million messages into a UI vector or execute database reads while drawing rows. Keep attachment bytes out of list queries while retaining all of them offline. Database design, write contention, query shape and indexing policy must be measured; Rust and SQLite alone do not guarantee the search targets.

## Proposed phase 0 evaluation

Run this candidate-specific slice with the nonfinal references if needed; the [current stack context](desktop-stack.md) controls selection.

1. **Visual and interaction slice:** build a custom mail screen and calendar week view with realistic synthetic data, light/dark treatments and window resizing. Include Unread/Starred conversation filters, category/sender chips, per-account view restoration, compact all-day rows, current/next meetings and Join. Review typography and scrolling on Windows, macOS, Ubuntu, Arch and Omarchy. Palette/layout remain proposed choices.
2. **Reader and composer slice:** render a representative HTML corpus; exercise selection, copying, inline images, quotations, a long conversation and secondary plain-text fallback. Display optional local summaries with source access and partial/stale states. Compose/reopen a human-written formatted reply with signature, links, lists and attachments; no AI authoring. Resolve rendering/editing before committing to Slint.
3. **Storage-backed scale slice:** connect SQLite/FTS5 and a paged model. Test 100k/1m/3m message tiers, account/category search, accurate triage counts, concurrent ingestion and optional inference, cancellation, coverage and memory. Use the approximately 25 GB supplied store size as context, not a measured message count.
4. **Platform slice:** verify file dialogs and required attachment interactions, IME/clipboard, accessibility, DPI, notifications and complete process exit on close. Record build/deployment dependencies and the selected Slint licensing route before distribution.

Selection requires an appealing interface, responsive measured behavior and a credible complete reader/composer on all required platforms, including headless CLI and package installation. A beautiful mock-data screen alone is insufficient. If these gates fail, record the specific limitation and compare other candidates against the same design. No implementation effort estimate or pass result is asserted here.
