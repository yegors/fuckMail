# litehtml (vendored)

- Source: https://github.com/litehtml/litehtml
- Commit: `b58fd1023244557949d859b61dd52149420b8762` (2026-09-19)
- Licence: BSD-3-Clause (`LICENSE`); bundled gumbo parser under Apache-2.0 (`src/gumbo/LICENSE`).
- Kept unchanged: `include/`, `src/` (including `src/gumbo`), `LICENSE`, `README.md`.
- Omitted: upstream CMake files, sample containers, tests, docs and support scripts.
  `gui/cmake/litehtml.cmake` builds the two static libraries for fuckMail.

litehtml only ever receives HTML rebuilt by the engine's sanitiser
(`crates/fm-mail/src/html_view.rs`). It has no networking or scripting; the app's
container resolves only `fm-res:N` images and `fm-link:N` anchors.

To update: replace these directories from a reviewed upstream commit, update the
commit above, and rerun the HTML reader native tests.
