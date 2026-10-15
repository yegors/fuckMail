# litehtml (vendored)

- Source: https://github.com/litehtml/litehtml
- Commit: `b58fd1023244557949d859b61dd52149420b8762` (2026-09-19)
- Licence: BSD-3-Clause (`LICENSE`); bundled gumbo parser under Apache-2.0 (`src/gumbo/LICENSE`).
- Kept from upstream: `include/`, `src/` (including `src/gumbo`), `LICENSE`, `README.md`,
  with the local changes below.
- Omitted: upstream CMake files, sample containers, tests, docs and support scripts.
  `gui/cmake/litehtml.cmake` builds the two static libraries for fuckMail.

litehtml only ever receives HTML rebuilt by the engine's sanitiser
(`crates/fm-mail/src/html_view.rs`). It has no networking or scripting; the app's
container resolves only `fm-res:N` images and `fm-link:N` anchors.

## Local changes

Each is marked `fuckMail:` in the source. Reapply them when updating.

- **Measurement reuse** (`document.h/.cpp`, `render_item.h/.cpp`, `render_table.cpp`):
  table min/max width passes call `render_item::measure`, which reuses a
  result for the same containing block within one document render. Nested
  newsletter tables otherwise re-measure exponentially (seconds per message).
- **Fixed-width table columns** (`render_table.cpp`, `table.cpp`): a cell's
  declared width is a preference; it shrinks to its content minimum when the
  table is narrower, as in Chromium, instead of forcing the table wider.
- **Relative inline shift** (`render_item.cpp`): `position:relative` on an inline
  box is applied from zero, so repeated line layouts do not add it up.
- **Percentage heights** (`document.cpp`, `render_item.cpp`): the message has no
  definite height, and a percentage (min/max) height of an auto height computes
  to auto (CSS 2.1 §10.5) instead of resolving against the viewport.
- **Media breakpoint changes** (`document.cpp`, `element.h/.cpp`, `html_tag.h/.cpp`):
  restyling after a media query flips starts from the recorded master-CSS and
  attribute styles (`width`, `align`, `bgcolor`...) instead of dropping them,
  rebuilds the anonymous boxes of CSS tables, and releases the old render tree
  so elements point at their current render items.
- **Splitting inline boxes** (`render_item.h/.cpp`, `render_block.cpp`): an
  inline box's block children are split out in one pass, and only inline boxes
  are split. Upstream split one block per call and moved the remaining children
  each time (quadratic in the blocks), pulled blocks out of inline-blocks and
  floats, and put the text after a nested block out of order.
- **Render list sweep** (`element.cpp`): `add_render` drops expired items from
  the front each time and sweeps the whole list only when it doubles. A sweep
  on every add was quadratic for an inline box split around thousands of
  blocks (30 s for a 34 KB message).
- **Height-only images** (`render_image.cpp`): an image sized by its height alone
  stays within its column, keeping its shape.

The app's container (`gui/HtmlDocument.cpp`) also adds break points inside long
runs without spaces and makes `pre` wrap by default; those are not litehtml edits.

To update: replace these directories from a reviewed upstream commit, update the
commit above, reapply the local changes, and rerun the HTML reader native tests.
