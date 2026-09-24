# HTML reader, remote images and link cards — plan

**Status:** proposed under [D100](../decisions.md). [D101](../decisions.md) records the founder's answers and the review changes; [D102](../decisions.md) rules out `http://` loading. [D105](../decisions.md) implements H0–H5 on Windows, following the fifth export's link card, image bar and placeholders; see [As built](#as-built-d105). The H1 go criteria that need the founder, other hosts, fuzzing or budget measurements are still open, so the renderer choice is not final. Readiness stays 0/9.

Owning contracts: [Mail UI](../contracts/mail-ui.md), [Core](../contracts/core-storage.md), [Platform](../contracts/platform.md), [CLI](../contracts/cli.md), [Quality](../contracts/quality.md). Renderer evidence: [HTML mail research](../research/html-mail.md#2026-09-23-renderer-candidates). Execution record: [D105](../research/html-mail.md#d105--html-reader-implementation-2026-09-24).

## As built (D105)

The rest of this document is the plan as proposed. Where the build differs, this section wins.

**Engine**
- `message_html {id, source_version}` returns `{has_html, too_large, html | html_path, resources[], links[], trackers, quote_removed, sanitizer_version}`. It reads the retained MIME, allows archived accounts and inlines up to 512 KiB; anything larger goes in a private cache file, and the newest 16 are kept. A resource already in the image cache carries `image {path, width, height}`, so reopening needs no request. A plain-only message has no HTML part (`has_html: false`), which settles the stray toggle. `message_source` uses the same part choice.
- Sanitiser limits are as planned. Images declared at most 3×3, zero-sized or hidden are counted in `trackers` and never become resources, so they are never fetched. `http://` sources become `insecure` resources and are never requested.
- One method serves every image: `message_image {id, source_version, resource, cached_only?}` returns `{state: ready|not_cached|blocked|insecure|proxy_unavailable|failed, path?, width?, height?, reason?, by?}`. It covers `cid:`, `data:` and remote images. `cached_only` never touches the network. Every image is decoded in Rust (PNG, JPEG, GIF first frame, WebP; at most 16 megapixels and 1600 px wide) and stored as PNG in a 256 MiB cache per profile, sharded by account.
- The fetch policy is as planned: GET over https to port 443 only; headers are a fixed User-Agent and an image Accept; at most 3 redirects, each rechecked; 5 s connect, 15 s total and 5 MiB per image; 8 fetches at once. Direct mode resolves names itself and connects only to public addresses. Proxy mode tunnels every request through `https://host:port` with basic authentication, never resolves the origin locally and never falls back to a direct connection.
- Proxy password: `remote_proxy_password {op: status|set|clear, password?}`. It is kept in the system keychain and never returned. `remote_proxy_test` makes one request through the proxy to `https://www.gstatic.com/generate_204`. `remote_image_cache {op: stats|clear}` reports or clears the cache.
- **Links are counted by host, not by registrable domain,** because blocking works by host and by `*.domain`.
  - Schema34 adds `link_hosts` (reversed labels, so `*.example.com` is one index range), `message_link_hosts` (cascades with its message) and a one-row `link_index_state` cursor.
  - An engine background task indexes 100 messages per batch in rowid order, reading retained sources off the writer.
  - `link_host_summary {host, conversation?}` counts messages in every account outside the current conversation and reports `complete`.
  - `link_pattern_counts {patterns}` counts the messages each blocked pattern covers.
- Preferences:
  - `blocked_hosts`: at most 1,000 entries of `{pattern, source: settings|link, added_at}`. A pattern is an exact host (ignoring `www.`) or `*.domain` for the domain and its subdomains, stored lowercase in ASCII. This follows the fifth export.
  - `remote_images_from`: at most 2,000 entries of `{account, email}`, for the export's "Always from" sender.
  - `remote_proxy {enabled, host, port, username}`.
  - Core refuses patterns that are not already normalised.
- Plain-text links use the same extractor for the index. Conversation payloads do not yet carry `text_links`, so the GUI's text linkifier stays.

**GUI**
- `FmHtmlView` (`FuckMail.Html`) draws litehtml at commit `b58fd10` with its bundled gumbo, built as static libraries by `gui/cmake/litehtml.cmake`.
  - A one-thread pool parses, lays out and paints 512 px tiles. Tiles are kept within one screen of the viewport, under a 32 MiB tile budget and a 24 MiB image budget.
  - Wide layouts scale to fit down to 0.5×. Selection, copy and hover run on the snapshot. Layout over 2 s, or an `html_path` over 4 MiB, reports a failure, and the reader falls back to Text.
  - The scene graph root is a transform node, so the software renderer places tiles created after scrolling.
- Changes to content, links or font lay out once, at the next polish.
  - The view remembers the last 128 layout heights, keyed by content, width, font, blocked links and image states. **A message shown again has its final height on its first frame**, and its tiles paint a frame or so later.
  - Blocking or unblocking a link keeps the height and the old tiles until the new layout paints.
  - A message's first visit still waits for the IPC round trip and the worker layout.
- `backend.htmlContent` (`HtmlContent`, owned by MailBackend) sends every request on its own `GoogleOperation` connection.
  - Results go through an 8-message LRU; `cached()` answers it synchronously and includes images loaded since.
  - Image requests run 6 at a time. Link summaries are cached per account, host and conversation, and an incomplete one is asked again at most every 5 s.
  - It also provides the blocklist helpers (normalise, blocked-by, block and unblock), sender allowance, proxy settings with password and Test, and cache size and Clear.
- `HtmlMessageBody.qml` holds a message's image states.
  - It shows inline and cached images at once. Remote images wait for **Load images**, or load at once for an allowed sender.
  - Blocked hosts and `http://` images never load, and blocked links are struck through and do not open.
  - The bar reads "N images hidden to keep senders from tracking you · Load images · Always from <sender>", or "Proxy unavailable — images not loaded · Retry".
  - Placeholders follow the export: a dashed rounded box with pale stripes, an icon and alt or state text, a hairline box under 48×32, and a chip when the size is undeclared.
- `RemoteContentSettings.qml` in Settings → Mail holds the proxy switch, host, username, password, Test, and the cache size with Clear. The D104 packet built the link card (`LinkCard.qml`), the Blocked domains list and the MailView wiring on this service. `FmHoverCard` was not extracted.

**CLI:** `fm mail html <id> --source-version V`, `fm mail images <id> --source-version V [--load]`, `fm links summary <host> [--conversation C]`, `fm links blocked|block <pattern>|unblock <pattern>`, and `fm proxy show|set <host[:port]> [--username U] [--password-stdin]|clear|test|cache [--clear]`.

**Still open:**
- H1 go criteria: the founder's fidelity review, Mac and Linux runs, cargo-fuzz and libFuzzer/ASan runs, and the litehtml differential tree check.
- Every budget in [Budgets to measure](#budgets-to-measure).
- The 100k-message link statistics benchmark.
- The prepared reader's first-frame guarantee for a first HTML visit.
- Checks with real newsletters and a real proxy.

## Goal

1. **HTML view renders the HTML that was sent,** with real CSS, tables, colours and images, while staying safe, fast and light on memory.
2. **Nothing remote loads by default.** Remote images show as placeholders, and one click loads them through the engine, never the GUI. **Every network request a rendered message can cause goes through that one path, and only to https origins.** If the user has configured an HTTPS proxy, all of those requests use it; otherwise they go direct.
3. **Every link, in HTML and plain text, gets a hover card:** where it goes, how often its domain appears across all local mail, and Block. Blocked domains are listed in Settings → Mail.

**Not in scope:**
- Browser fidelity, scripts, forms, web fonts, remote CSS, video and favicons.
- Resolving redirects or shortened links, which would contact the tracker.
- The Google API and local-model traffic; the proxy is not used for them.
- Links opened in the browser; they use the browser's own network settings.
- Later: per-sender "always load images" and dark-mode colour rewriting.

## Where we are today

| Area | Current behaviour | Where |
| --- | --- | --- |
| HTML toggle | HTML/TEXT header button and a Format menu. The choice is per message, not saved, and resets to HTML. | `gui/MailView.qml:1157`, `gui/MessageSource.qml:16` |
| "HTML" rendering | A regex formatter converts to Qt's rich-text subset. Images become `[alt]`, CSS is dropped, and nested or large tables are flattened (D087). Each section is a `RichText` `TextArea`. | `gui/ReplyDocument.cpp:13`, `gui/ReceivedBody.h:42` |
| HTML source | `message_source` returns the raw first HTML part, capped at 256 KiB. Anything larger disables the HTML view. | `crates/fm-core/src/message_source.rs:83` |
| Engine sanitising | ammonia runs only to extract text from HTML-only mail. The sanitised HTML is discarded. | `crates/fm-mail/src/incoming.rs:60` |
| Quote stripping | Two copies with slightly different rules: Rust at acquisition, C++ for HTML sections. | `crates/fm-mail/src/reply_text.rs`, `gui/ReceivedBody.h:108` |
| Images | Nothing is ever fetched. `cid:` parts appear only as attachment tiles, and `attachments` has no Content-ID column. | `gui/AttachmentTile.qml` |
| Links | Plain text is linkified by a regex. `openLink` accepts http, https and mailto with no confirmation. There is no hover handling, and links are not extracted or indexed. | `gui/ReceivedBody.h:30`, `gui/MailBackend.cpp:1272` |
| HTTP | reqwest 0.12 with rustls and native root certificates. Both clients call `.no_proxy()`. The Google transport allows only a fixed list of hosts. | `crates/fm-google/src/transport.rs:231`, `crates/fm-engine/src/ollama.rs:47` |
| Qt | 6.11.2. WebEngine and WebView are not installed; Svg is. | `scripts/toolchain.json` |
| Probable stray toggle | The `html_body.is_empty()` guard probably does not catch plain-only mail. mail-parser starts with `need_html_body: true`, so a single inline text/plain part is added to `html_body` too; `body_html(0)` then builds HTML from the plain text. This comes from reading the source (mail-parser 0.11.9 `parsers/message.rs:98, 291-309`) and has not been tested. | `message_source.rs:84` |

## Decisions proposed

| Choice | Proposal | Why | Not chosen |
| --- | --- | --- | --- |
| Renderer | **litehtml**, running in the app process and drawn by a native Qt Quick item. Kept only if the H1 spike passes. | Small C++ HTML/CSS layout engine with no scripting or networking; the app supplies every image and font. Claws Mail replaced WebKit with it for security reasons, and Qt Creator's help viewer uses it. BSD-3, with its gumbo parser under Apache-2.0. It suits email's tables and inline CSS. | **QtWebEngine:** Chromium processes, hundreds of MB, not installed. **Qt rich text (today):** no CSS, images or real tables. **Blitz (Rust):** beta and GPU-first; the first alternative if the spike fails. |
| Sanitiser | **Rust in the engine:** an html5ever tree walk (already a dependency through ammonia) plus cssparser. **Strict allowlists for tags, attributes, CSS properties and CSS values.** | One memory-safe implementation for GUI and CLI. litehtml only ever sees reserialised, allowlisted input. | Keeping the C++ regex formatter |
| Remote-content path | **One engine fetch path over the existing private IPC.** No localhost HTTP port. | A listening port is reachable by any local process or web page. The IPC is already private and token-checked. | A localhost HTTP proxy |
| Upstream proxy | **HTTPS proxy only:** TLS to the proxy, basic authentication, certificate checked against the OS root store. No plain-HTTP proxy, no SOCKS. | Credentials and requests never cross the local network in clear text. This matches Windscribe-style extension proxies, which take a hostname, username and password. | HTTP CONNECT without TLS, SOCKS5 |
| Image bytes | **Decoded in Rust** (`image` crate) and re-encoded as bounded PNG | Qt never decodes sender-supplied bytes, and images are shrunk to display size. | Qt image plugins decoding sender bytes |
| Link statistics | **Global counts** from a table of registrable domains per message (schema34) | The founder chose global counts. Counting becomes an index range scan. | Per-account counts; counting at query time |
| Blocklist | **A global preference list,** enforced by the engine fetch path and when a link is opened | Reuses the revisioned preference path shared with the CLI. | A separate table with its own receipts |

## Architecture

```
 retained MIME blob ──► engine: message_html ──► sanitised HTML + resource list + link list
                          (Rust: html5ever/cssparser, strict allowlists, limits, quote removal)
                                    │ IPC (inline ≤512 KiB, else private file path)
                                    ▼
 GUI: FmHtmlView (QQuickItem) ── worker thread: litehtml parse + layout + tile paint
        │ hover/click by link id               │ image request by resource id (never a URL)
        ▼                                      ▼
   link hover card ◄── link_domain_summary   engine: remote_image ─► recheck identity ─► egress policy
                        (schema34, global)          ─► HTTPS proxy, or direct ─► internet
                                                    ─► decode in Rust ─► PNG ─► bounded disk cache ─► file path
```

The GUI has no network stack for content and never gets a URL it could fetch. The engine fetches only URLs listed in the current source version of that message.

## 1. Engine: sanitised HTML view

`message_html {id, source_version}` is read-only, allowed for archived accounts, and runs in the existing `local_content` pool. It reads the **retained MIME blob directly**, so it is not bound by `message_source`'s 256 KiB display cap; the existing 40 MiB raw limit still applies. It returns `{html | html_path, resources[], links[], notes, sanitizer_version}`.

**Pipeline** (new `fm-mail::html_view`, a pure function that can be fuzzed):

1. **Choose the first real `text/html` part.** Check each part's content type rather than trusting `html_body`, so plain-only mail has no HTML view. H0 starts with a failing or passing test for the stray toggle.
2. **Parse with html5ever**, which turns malformed input into a normal tree.
3. **Tags:** an allowlist of structural, text, list and table tags plus `img`, `a`, `font`, `center` and `style`.
   - `script`, `iframe`, `object`, `embed`, form controls, `svg`, `math`, media, `link`, `meta`, `base` and `template` are removed together with their content.
   - Any other tag is dropped and its text kept.
4. **Attributes:** an allowlist of presentation attributes: `width`, `height`, `align`, `valign`, `bgcolor`, `color`, `border`, `cellpadding`, `cellspacing`, `colspan`, `rowspan`, `dir`, `alt`, `title` and `style`.
   - Each value must parse as a number, length, colour or keyword.
   - `class` and `id` get a per-message prefix so scoped `<style>` rules still match.
   - Everything else is removed.
5. **CSS is accepted only if it matches the allowlist, for both properties and values.** It is tokenised with cssparser, and each declaration must be a known property whose value fully parses against that property's grammar.
   - **Allowed values:** lengths, percentages, colours (hex, named, `rgb`/`rgba`/`hsl`/`hsla`), keywords, and simple lists for fonts, margins, padding and borders.
   - **Rejected outright:** unknown functions, escapes, comments inside values and anything left unparsed.
   - **`url()`** is accepted only in `background`/`background-image`. After strict URL parsing it becomes `fm-res:N`, and malformed or escaped URLs are dropped.
   - **At-rules:** only `@media` with width or `prefers-color-scheme` conditions. `@import`, `@font-face` and every other at-rule are dropped.
   - **Selectors:** a type/class/id/descendant/child subset.
   - Rules are scoped to the message root, and lengths, font sizes and z-index are clamped.
6. **URLs:**
   - `a[href]` is kept only for http, https and mailto after strict parsing, and becomes `fm-link:N`.
   - `img[src]` becomes `fm-res:N`.
   - `cid:` maps to the MIME part with that Content-ID.
   - `data:` becomes a local resource.
   - Every other scheme becomes inert text.
   - The renderer's resource and anchor callbacks accept manifest ids only.
7. **Quoted history:** one Rust rule set ported from `ReceivedBody.h` (`gmail_quote`, `blockquote[type=cite]`, `divRplyFwdMsg`, Yahoo and Proton markers). The C++ copy is removed, and the result is flagged `quote_removed`.
8. **Serialise** into a bounded in-memory cache keyed by `(source_hash, sanitizer_version)`. Nothing new goes into SQLite.

**Records:**
- Each resource: `{id, kind: remote|cid|data, host, domain, declared_w, declared_h, likely_tracker, insecure, blocked}`. `insecure` marks an `http://` source. Those sources are never fetched.
- Each link: `{id, href, host, domain, scheme, text_mismatch_domain?, idn_ascii?}`.

**Limits** (starting values, tuned against the corpus in H0):

| Item | Limit | Beyond it |
| --- | --- | --- |
| HTML part | 4 MiB | "Too large for HTML view", with Text and View source still available |
| Nodes | 50,000 | Rendering stops |
| Depth / table nesting | 256 / 16 | Flattened |
| CSS rules | 2,000 | Remaining rules dropped |
| Resources / links | 500 / 2,000 | Extra images stay placeholders; extra links are inert |
| Output over IPC | 512 KiB inline | Private profile file, following the attachment export pattern |

Plain text gets `conversation[].text_links: [{start, len, link}]`. These come from the same extractor (the `linkify` crate), and the GUI's regex linkifier is removed.

## 2. GUI: litehtml renderer

- **Build:** litehtml is vendored at a pinned commit under `gui/third_party/litehtml` as a static CMake library. No new Qt modules and no Node.
- **`FmHtmlView`** is a native QQuickItem registered like `FmIconItem`. It replaces the section `Repeater`/`TextArea` in HTML mode; Text mode is unchanged. Its `document_container` maps CSS fonts to system fonts. It has no web fonts, no `import_css`, and no base URL, and it resolves only `fm-res:N` and `fm-link:N`.
- **Threading:** parsing, layout and painting run on a dedicated one-thread HTML pool. After layout the worker publishes a read-only snapshot: size, link and image rectangles, and text runs for selection. The UI thread uses only the snapshot, which keeps hover and hit tests cheap.
- **Painting:** the worker paints 512-logical-px tiles into QImage at the device pixel ratio. Only tiles within one screen of the viewport are kept. Scrolling never repaints per frame.
- **Memory caps:** tiles share a **32 MiB** budget across all HTML views, and loaded images for live documents share **24 MiB**. Both evict the least recently used entries; evicted images are re-read from the disk cache. A litehtml document exists only for expanded messages near the viewport and is rebuilt from the engine cache when needed. The spike sets the final numbers.
- **Layout:**
  - HTML is laid out at the column width.
  - Wider fixed layouts, such as a 600 px newsletter, are scaled to fit down to 0.5×; beyond that, the card scrolls sideways.
  - Mail is shown on a light "paper" card in all three themes, keeping the sender's colours.
  - The app's text size sets the default font size.
- **Selection and copy** are required, because today's HTML mode has them. They work over the snapshot's text runs. qlitehtml (Qt Creator) is a reference if its licence fits.
- **Links:** the pointer changes on hover. A click goes from the link id to the manifest, then `openLink`, then the blocklist. The renderer never navigates.
- **Fallback:** on a reported limit, a layout failure or more than 2 s of worker time, the message shows Text with a one-line notice. Text and View source are always available.
- **Accessibility:** Text mode is the screen-reader path in v1. QAccessible exposure of HTML comes later.

## 3. Images and the fetch path

**What shows without asking:**
- `cid:` and `data:` images come from the message itself. They are decoded in Rust under the same byte, dimension and decode limits as remote images.
- Remote images already in the disk cache also show immediately when a message is reopened, because no request is made.
- Everything else waits for Load.

**Image bar** (only when uncached remote images exist):

```
▢ 7 images hidden · 3 likely trackers skipped · 1 insecure     [ Load images ]
  through proxy.example.net                              (or: "direct from senders")
```

After loading, the bar goes away. If the proxy fails, the bar reads "Proxy unavailable — images not loaded [Retry]". **There is never a direct fallback.**

**Placeholders**, drawn by the container on the paper card as vectors, with no textures:

- **Declared size of at least 48×32:** a 4 px rounded box with a pale fill, hairline border and a centred 16 px image icon, with a truncated `alt` line below it. Declared sizes never reflow.
- **Declared size smaller than 48×32:** a hairline box only.
- **No declared size:** a 24 px chip with the icon and the alt text (or "Image"), up to 240 px wide. It reflows when the image loads. Reflows are grouped every 50 ms, and the scroll position is anchored.
- **States:** hidden → loading (dimmed, no animation) → loaded, failed (crossed, with the reason in a tooltip) or blocked (⊘ "Blocked domain"), or insecure (⊘ "Not loaded: http"). Linked images keep their link card.
- **Likely trackers are never fetched,** even after Load. That means images declared at most 3×3, zero-sized, or hidden with `display:none`, `visibility:hidden` or `opacity:0`. They are only counted.

**Fetch method:** `remote_image {account, message, source_version, resource}` returns `{state, path, width, height}`. It lives in a new engine `remote_content` runtime with its own reqwest client; the Google transport is untouched. The GUI uses a small dedicated socket pool, because the IPC allows one request in flight per connection, and requests visible images first.

**Every request that a rendered message can cause uses this runtime:** images, CSS backgrounds, and any remote resource type added later. `fm mail images --load` does too. No other code path may fetch message content.

**Lifecycle:**
- At fetch time, the engine rechecks that the account still exists and is not removed. It also rechecks the message and the current `source_version`, and that the resource id appears in that version's manifest.
- Pending fetches are cancelled when:
  - the requesting connection closes, for example when the GUI closes;
  - the account is removed;
  - the message's source version changes;
  - the proxy settings change.
- A late result for a stale identity is discarded, not cached.

**Egress policy**, applied to every request and every redirect hop:

| Rule | Value |
| --- | --- |
| Method and schemes | GET only, **https origins on port 443 only**, whether direct or through the proxy. `http://` resources are never requested, and a redirect to http is refused. |
| Direct-mode destinations | Refuse loopback, private, link-local, CGNAT, multicast, ULA, IPv4-mapped and `localhost`. The check runs after DNS, and the connection goes to the checked address, which protects the local Ollama port and the LAN. |
| Proxy-mode destinations | **The engine never resolves origin hostnames locally, because a local lookup would leak each fetch to the local DNS resolver.** The proxy does the DNS lookup and makes the connection. The engine still refuses literal private or loopback addresses and localhost names. It does **not** claim private-network protection behind the proxy: a public-looking name can resolve to something internal to the proxy's network, and a proxy on the user's own LAN could reach local hosts. This limit is stated in the contract and in Settings help. |
| Headers | Fixed, versionless User-Agent and an image-only `Accept`. No cookies, Referer or Accept-Language. No conditional requests, because an ETag can act as a cookie. |
| Redirects | At most 3; each is rechecked against the scheme, destination and blocklist rules |
| Limits | 5 s connect, 15 s total; 5 MiB per image, 25 MiB per message; 6 concurrent requests per message, 8 overall |
| Content | Magic bytes checked. PNG, JPEG, GIF (first frame) and WebP are decoded up to 16 megapixels; SVG is refused. Output is at most 1600 px wide, as PNG. |
| Blocklist | Every hop is checked before connecting |

**Proxy (HTTPS only):**

- **Settings:** hostname, username and password. The hostname accepts `host` or `host:port`, with port 443 by default. There is no TLS toggle and no other proxy type.
- **Storage:** the preference holds `remote_proxy {enabled, host, port, username}`. The password goes in the OS vault under a separate service, `fuckmail.proxy.v1.<namespace>`, following `NativeCredentialVault`'s pattern, including its denial latch. It is never returned over IPC.
- **Transport:** `reqwest::Proxy::all("https://host:port")` with `basic_auth`. The proxy's certificate is checked against the OS root store, and there is no "ignore certificate" option. Only the proxy's own hostname is resolved locally. H4 must show that reqwest with rustls handles TLS to the proxy before the Settings UI is built.
- **Failure:** if the proxy is enabled but unreachable, rejects authentication or its password is locked in the vault, fetches fail with `proxy_unavailable`. Nothing goes direct.
- **Test:** `remote_proxy_test` sends one request through the proxy to `https://www.gstatic.com/generate_204`, a Google host the app already contacts.

**Cache (bounded):**

- **Disk:** at most **256 MiB in total** across accounts. It lives at `<profile>/cache/remote-images/<account-hash>/<sha256(url)>.png`, so removing an account deletes its images. The oldest entries are evicted on insert whenever the cap is exceeded, and a sweep also runs at idle. Only re-encoded pixels are stored, with no headers.
- **Engine memory:** image bytes are not kept after they are written. Decoding streams within the limits above. Failures are remembered only in memory, up to 4,096 entries per session.
- **GUI memory:** the 24 MiB image budget from section 2.
- **Settings:** shows the disk size and has a Clear button.

## 4. Link hover card

HTML and plain text share one card. Its chrome is extracted from `SenderCard.qml` into a common `FmHoverCard.qml` that both cards use: the popup, clamping, 220 ms close grace, `HoverHandler` keep-open and shortcut blocking.

```
┌──────────────────────────────────────────┐
│ news.example.com                          │  host; registrable part emphasised
│ https://news.example.com/r/abc…?utm=…     │  full URL, 2 lines, middle ellipsis
│ ⚠ Text says paypal.com                    │  only if the visible text disagrees
│ ⚠ Not encrypted (http) · ⚠ Raw IP address │  only when true
│ ⚠ Shown as xn--pypal-4ve.com              │  only for IDN / mixed-script hosts
│ Linked in 37 other messages · since Mar   │  mailbox context, not a safety verdict
│ [ Copy link ]              [ Block domain ]│
└──────────────────────────────────────────┘
```

- **Opening:** the card opens after 350 ms of hover and closes on Escape or an account switch. In plain text, `onLinkHovered` places it at the pointer.
- **Data:**
  - Everything except the count comes from the link record, so it is instant.
  - The count comes from `link_domain_summary {domain, exclude: {account, message}}` and covers **all accounts**. Requests are guarded by a generation number like `requestSenderCard` and cached for the session per domain.
  - The card makes no network request.
- **Honest counts:**
  - Until every account's index is complete, the line reads "Linked in at least 37 other messages · still indexing".
  - "Not linked in any other message" appears only when indexing is complete.
  - The count is presented as mailbox context only. Frequency is not a safety signal, because a sender can plant the same domain in many messages.
  - The factual flags (text mismatch, http, raw IP, IDN) describe the link and make no judgement about it.
- **Optional guess:** when a query parameter holds a complete http(s) URL, show "Probably goes to …". This only parses the URL, needs no network, and is labelled as a guess.
- **Blocked link:** it does not open, and the card shows "Blocked · Unblock". `openLink` applies the engine's suffix rule to the current preference list.

## 5. Link index and blocklist

**Schema34**, appended to `POST_RESET_MIGRATIONS` and mirrored in `fresh-schema.sql`:

```sql
CREATE TABLE link_domain (id INTEGER PRIMARY KEY, domain TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE message_link_domain (
  domain  INTEGER NOT NULL REFERENCES link_domain(id),
  account TEXT NOT NULL,
  message TEXT NOT NULL,
  at      INTEGER NOT NULL,             -- provider date, ms
  PRIMARY KEY (domain, account, message)
) STRICT, WITHOUT ROWID;
CREATE INDEX message_link_domain_message ON message_link_domain(account, message);
CREATE TABLE link_index_state (account TEXT PRIMARY KEY, complete INTEGER NOT NULL, cursor TEXT) STRICT;
```

- **What counts as a domain:** the registrable domain from the `psl` crate, using its compiled-in list with no runtime fetch. For example, `click.news.example.com` counts as `example.com`. IP literals count as themselves.
- **When rows are written:** domains are extracted during the existing off-writer message preparation, from both HTML and plain-text links, and inserted in the transaction that publishes the message.
- **When rows are deleted:** rows live and die with the local message row, whether it is removed by retention pruning, provider deletion or account data removal.
- **Query:** `SELECT count(*), min(at) … WHERE domain=?1 AND NOT (account=?2 AND message=?3)` is a range scan on the primary key's `domain` prefix. It returns `{messages, first_seen, complete}`, where `complete` means every account's `link_index_state` is complete.
- **Backfill:** a resumable idle job reparses retained MIME blobs 200 messages per transaction, records a per-account cursor, and yields to sync and user work.
- **Blocklist:**
  - A global `blocked_link_domains: [..]` preference with at most 1,000 entries. Entries are stored lowercase in IDNA ASCII form, with no scheme, path, port or wildcard.
  - An entry blocks its host and every subdomain.
  - The card's Block adds the registrable domain. Settings accepts a more specific host.
- **Enforcement:**
  1. `message_html` marks blocked resources and links.
  2. The engine refuses every hop to a blocked host.
  3. `openLink` refuses blocked links.
  4. The CLI shares the same fetch path.

## 6. Settings → Mail → Remote content

A new collapsible section, following the Signatures pattern and built lazily with the Mail page:

- **Proxy for message content:**
  - Off or On, with hostname (`host[:port]`), username, password (stored in the system keychain) and **Test**.
  - The status line reads "Remote images load directly from senders' servers" or "Remote images load through proxy.example.net".
  - The help line reads "Links open in your browser and do not use this proxy. The proxy resolves addresses, so private-network protection depends on it."
- **Image cache:** its size (at most 256 MiB) and **Clear**.
- **Blocked domains:** rows following the `LabelSettings` pattern, each with a remove button, plus an "Add domain" field that validates on Enter. Saved through `setPreferences` with the existing revision retry.

## Shared boundary

| Method or key | Owner | Notes |
| --- | --- | --- |
| `message_html` | Core via `local_content` | Read-only; allowed for archived accounts; reads the retained MIME |
| `message_inline_image` | Core | `cid:` or `data:` image as a decoded, verified PNG path |
| `remote_image`, `remote_image_cache_{stats,clear}` | New `remote_content` runtime | Takes a resource id, never a URL; identity rechecked; cancellable |
| `remote_proxy_test`, `remote_proxy_password_{set,clear}` | `remote_content` | The secret is never returned |
| `link_domain_summary` | Core | Global; returns `complete` |
| `conversation[].text_links` | Core | Plain-text link spans |
| Preferences `remote_proxy`, `blocked_link_domains` | Core preferences | Default, allowlist entry and validator in `preferences.rs` |

**CLI:**

- `fm mail html <id>`
- `fm mail images <id> [--load]`
- `fm links summary <url|domain>`
- `fm links block|unblock|list`
- `fm proxy show|set|clear|test`, where `set` takes `--host`, `--username` and `--password-stdin`

## Work packets, in order

**Dependencies:**
- H1 can start alongside H0, using a fixed corpus sanitised by an early H0 build.
- H2 depends on the H1 go decision.
- H5 depends only on H0.

| Packet | Delivers |
| --- | --- |
| **H0 — Engine HTML view** | The `html_view` sanitiser with strict allowlists and limits, Rust-only quote removal, resource and link lists, `message_html` reading retained MIME, `fm mail html`, and a test for the stray toggle, with a fix if it fails |
| **H1 — Renderer spike (go/no-go)** | A small native litehtml harness with no product UI, run on representative long newsletters, malformed and nested tables and hostile fixtures |
| **H2 — litehtml reader** | `FmHtmlView`, worker layout and tiles, paper card, scale-to-fit, selection and copy, link clicks, every image as a placeholder, and Text fallback. The received-mail uses of `limitedHtml` and the C++ quote rules are removed. |
| **H3 — Inline images** | `cid:` and `data:` images resolved from MIME and decoded in Rust under the remote-image limits |
| **H4 — Fetch path and proxy** | `remote_content`, egress policy, lifecycle rechecks and cancellation, bounded cache, image bar and states, tracker skipping, HTTPS proxy preference and vault password, Test, fail-closed behaviour, Settings proxy and cache, `fm mail images`, `fm proxy` |
| **H5 — Link cards, index, blocklist** | Schema34 and backfill, shared extractor, global `link_domain_summary`, `FmHoverCard` and the link card in both views, blocklist preference, Settings list and enforcement, `fm links` |

**H0 done when:**
- A hostile corpus is neutralised. It includes scripts, `on*` handlers, `javascript:`/`vbscript:`/`file:` URLs, and CSS `url()`/`@import`/`expression`. It also includes escaped and malformed CSS URLs and values, meta refresh, `<base>`, forms, SVG, entity and nesting bombs, huge attributes, MSO conditionals and a 10k-deep tree.
- Reparsing the output in Rust finds only allowlisted tags, attributes, properties and values.
- cargo-fuzz runs without a crash, and sanitising speed is measured on the corpus.
- CLI output equals the GUI's payload.
- The stray-toggle test result is recorded.

**H1 go when all of these hold on Windows, and on Mac and Linux where available** (hosts not checked are recorded):
- Fidelity on the newsletter corpus is acceptable to the founder.
- The selection and copy prototype works over text runs.
- Worker-thread painting of fonts and images works.
- Scrolling paints no frames beyond tile uploads.
- Layout time and total GUI memory fit the budgets below, measured with the D099 method.
- **Differential check:** the sanitiser's output is fed to litehtml, and its parsed tree is walked. It must hold only allowlisted elements and attributes, match the Rust tree's structure, and reference no resource other than manifest ids.
- A libFuzzer/ASan run of sanitiser output through litehtml layout and paint is clean.

**If the spike fails**, compare Blitz, or a separate render process, before building anything else. That result goes back to the founder.

**H2 done when:**
- Native tests cover rendering, selection and copy, link activation, placeholder geometry and fallback.
- The D087 nested-table fixture never blocks the UI thread.
- Existing navigation routes, now including HTML, keep the 100 ms p95.
- Memory stays within the tile and image caps.

**H3 done when:**
- Inline fixtures render.
- Hostile, oversized and truncated image fixtures are rejected under the shared limits.
- A test shows no socket is opened.

**H4 done when** tests against a local HTTPS server and a local authenticating HTTPS proxy show:
- the exact headers sent;
- refusal in direct mode of every private target, including names that resolve to private addresses;
- no local DNS lookups for origins in proxy mode;
- no request ever made for `http://` resources or redirects to http;
- redirect rechecks, size and time limits, and blocked-domain refusal;
- no direct connection when the proxy is down or rejects authentication;
- identity rechecks, and cancellation on close, removal, source change and proxy change;
- zero requests on a cache hit;
- the cache staying under 256 MiB under insert pressure.

Native tests show the bar and placeholder states, no reflow for declared sizes, and a stable scroll position.

**H5 done when:**
- Global counts match an independent oracle over a multi-account fixture.
- The backfill is bounded and resumable, and the card shows "at least … still indexing" until it completes.
- Pruning and account removal delete their rows.
- The card shows the mismatch, http, raw-IP and IDN flags.
- Block takes effect immediately for fetching and clicking.
- Settings changes are confirmed by CLI readback.
- The stats query p95 is measured on a 100k-message fixture.

No automated test fetches from the real internet or uses a real mailbox. The founder checks real newsletters, and the proxy of their choice, by hand.

## Budgets to measure

These are targets, not results. Each packet records its own measurements.

| What | Target | Method |
| --- | --- | --- |
| Opening an HTML message | Existing 100 ms p95 navigation target | Existing navigation oracle with HTML fixtures |
| Engine sanitising, ≤256 KiB | ≤10 ms p95 | Corpus benchmark |
| litehtml parse + layout, typical newsletter | ≤30 ms p95 on the worker; pathological input never blocks the UI | Spike and D087 fixture |
| Scrolling | No paint per frame | Frame timing on a long newsletter |
| GUI memory | Tiles ≤32 MiB, images ≤24 MiB, documents freed off-screen; report the change against text-only | D099 method |
| Disk cache | ≤256 MiB | Insert-pressure test |
| Link card statistics | ≤5 ms p95 at 100k messages | Core benchmark |
| Package size | Report the litehtml/gumbo cost | Package |

## Risks

- **litehtml and gumbo are C++/C code that parses untrusted input.**
  - They only receive reserialised, allowlisted and bounded input, checked by the differential test and fuzzing.
  - They are pinned and reviewed, and the container has no network or file access.
  - If problems show up, the fallback is a low-privilege render process that returns tiles, at some memory and latency cost.
- **Fidelity gaps:** newer CSS will degrade. That is acceptable under C42, and Text is one click away.
- **Selection** is the largest custom GUI piece. It is proven in the spike, not deferred.
- **Fonts off the UI thread:** confirm on each platform in the spike.
- **TLS to the proxy:** prove it with reqwest and rustls in H4 before building the UI.
- **Private networks in proxy mode:** protection depends on the proxy, as stated in the egress table and Settings.
- **Tracker heuristics** miss some trackers. They only decide what is skipped after Load; the real protection is that nothing loads by default.

## Founder answers (D101)

1. **Proxy scope:** everything a rendered message can reach goes through the proxy. Google API and local-model traffic are outside it.
2. **Link counts:** global.
3. **Cached images** show when a message is reopened, under the bounded disk and memory caches above.
4. **Proxy types:** HTTPS only. No SOCKS5 and no plain-HTTP proxy. The user supplies a hostname, username and password; Windscribe's extension proxies are one example, and nothing is provider-specific.

5. **No `http://` loading (D102):** only https origins are fetched, with or without a proxy. An http resource keeps an "insecure" placeholder and is never requested, even though a forward proxy could carry it. Links to http pages can still be opened in the browser, and the card flags them as not encrypted.

## Review incorporated (2026-09-24)

A second review's notes are included as follows:
- **Renderer spike first:** H1 is a go/no-go gate.
- **Strict allowlist for CSS values, checked inside litehtml:** section 1 step 5 and the H1 differential check.
- **Source and resource lifecycle:** direct MIME reads, rechecks at fetch time, cancellation, and shared limits for `cid:`/`data:` images.
- **Proxy private-network rule:** no local DNS lookups and an explicit limit, stated in the contract and Settings; TLS-only removes the risk of exposing the proxy password in clear text.
- **Honest link counts:** "at least … still indexing", and no safety framing.

One correction to the review: its claim that the `html_body.is_empty()` guard already handles plain-only mail disagrees with the mail-parser source (see "Where we are today"). H0 settles it with a test.
