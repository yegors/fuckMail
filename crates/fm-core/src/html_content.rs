//! Received HTML for the reader, and the images it may show.
//!
//! `message_html` rebuilds a message's HTML part with `fm_mail::html_view` and
//! returns it with a manifest that names each image and link by id. Images are
//! resolved later by id against the same retained source; a caller never names
//! a URL. Blob reads, parsing and decoding run off the Store writer.
use crate::content::BlobStore;
use crate::view_cache::{self, ViewCache};
use crate::{err, fields, id, string, Result, Store};
use base64::Engine as _;
use fm_google::transport::CancellationToken;
use fm_mail::html_view::{self, HtmlView, ResourceKind};
use mail_parser::MimeHeaders;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Larger sanitised documents travel as a private file path, not inline.
const INLINE_HTML_BYTES: usize = 512 * 1024;
/// A `cid:` part larger than this is not shown inline.
const MAX_INLINE_PART: usize = 25 * 1024 * 1024;
const MANIFESTS_KEPT: usize = 16;

/// A message's retained source at one version, for building its HTML view or
/// resolving its images off the writer.
pub struct HtmlPlan {
    blobs: BlobStore,
    pub account: String,
    /// Local message ID.
    pub message: String,
    /// Source hash the view is built from.
    pub version: String,
    /// Sender address, for "always load images from" preferences.
    pub sender: String,
    /// The view keeps quoted reply history (`quotes: true` in the request).
    /// Its image and link ids are its own.
    pub quotes: bool,
    pgp: crate::pgp::ReadKeys,
}

impl Store {
    /// Validate that `id` is this account's message at exactly `source_version`.
    /// An optional `quotes` boolean asks for the view with reply history kept.
    pub fn message_html_begin(
        &self,
        account: &str,
        request: &Value,
        extra: &[&str],
    ) -> Result<HtmlPlan> {
        let mut allowed = vec!["id", "source_version", "quotes"];
        allowed.extend_from_slice(extra);
        fields(request, &allowed)?;
        let quotes = match request.get("quotes") {
            None => false,
            Some(Value::Bool(v)) => *v,
            _ => return Err(err("invalid_request", "quotes must be true or false")),
        };
        let mid = id(string(request, "id")?)?;
        let found: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT m.source_hash,m.sender_email FROM messages m JOIN accounts a ON a.id=m.account WHERE m.account=?1 AND m.id=?2",
                params![id(account)?, mid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (version, sender) =
            found.ok_or_else(|| err("not_found", "Message is outside this account"))?;
        if version != string(request, "source_version")? {
            return Err(err(
                "stale_source",
                "Message content changed. Reopen the message.",
            ));
        }
        Ok(HtmlPlan {
            blobs: self.google_blob_store(),
            account: account.into(),
            message: mid.into(),
            version,
            sender: sender.trim().to_ascii_lowercase(),
            quotes,
            pgp: self.pgp_read_keys()?,
        })
    }
}

/// The view cache's name for one of the plan's inline images. Ids of the view
/// with quotes kept number a different document, so they get their own names.
fn inline_key(plan: &HtmlPlan, resource: usize) -> String {
    let kind = if plan.quotes {
        "inline-quotes"
    } else {
        "inline"
    };
    view_cache::key(&[kind, &plan.version, &resource.to_string()])
}

/// The message's source, decrypted when it is encrypted. A message that
/// can't be decrypted shows no HTML.
fn parse_source(plan: &HtmlPlan, cancel: &CancellationToken) -> Result<crate::pgp::Readable> {
    if cancel.is_cancelled() {
        return Err(err("cancelled", "Message read cancelled"));
    }
    let raw = crate::local_content::read_source(&plan.blobs, &plan.version)?;
    Ok(plan.pgp.readable(raw))
}

fn unparsable() -> crate::CoreError {
    err(
        "content_unavailable",
        "The retained message cannot be parsed",
    )
}

/// The sanitised view of a source, shared by the reader and image requests,
/// and whether it came from decrypting. The last 16 views are memoised by
/// source hash and `quotes`, so image requests right after a read do not
/// re-parse the message. A message that couldn't be decrypted isn't kept, so
/// it is tried again once its key is available.
fn view(plan: &HtmlPlan, cancel: &CancellationToken) -> Result<(Option<Arc<HtmlView>>, bool)> {
    type Manifest = (String, bool, Option<Arc<HtmlView>>, bool);
    static MANIFESTS: Mutex<VecDeque<Manifest>> = Mutex::new(VecDeque::new());
    if let Ok(cache) = MANIFESTS.lock() {
        if let Some((_, _, view, decrypted)) = cache
            .iter()
            .find(|(v, q, _, _)| v == &plan.version && *q == plan.quotes)
        {
            return Ok((view.clone(), *decrypted));
        }
    }
    let readable = parse_source(plan, cancel)?;
    let decrypted = readable.decrypted();
    let undecrypted = readable.opened.as_ref().is_some_and(|o| o.encrypted) && !decrypted;
    if undecrypted {
        return Ok((None, false));
    }
    let parsed = mail_parser::MessageParser::default()
        .parse(&readable.bytes[..])
        .ok_or_else(unparsable)?;
    let sanitize = if plan.quotes {
        html_view::sanitize_with_quotes
    } else {
        html_view::sanitize
    };
    let view = html_view::html_part(&parsed).map(|html| sanitize(html).map(Arc::new));
    let view = match view {
        None => None,
        Some(None) => {
            return Err(err(
                "too_large",
                "This message is too large for the HTML view",
            ))
        }
        Some(Some(view)) => Some(view),
    };
    if cancel.is_cancelled() {
        return Err(err("cancelled", "Message read cancelled"));
    }
    if let Ok(mut cache) = MANIFESTS.lock() {
        cache.push_front((plan.version.clone(), plan.quotes, view.clone(), decrypted));
        cache.truncate(MANIFESTS_KEPT);
    }
    Ok((view, decrypted))
}

/// Builds the sanitised HTML view off the writer and returns it with its image
/// and link manifests.
///
/// Documents over about 512 KiB of JSON are written to the view cache and
/// returned as `html_path` instead of inline `html`. Images already decoded in
/// the cache carry their file path. A message without an HTML part returns
/// `has_html: false`; one too large to sanitise returns `too_large: true`.
pub fn read_html(plan: HtmlPlan, cache: &ViewCache, cancel: &CancellationToken) -> Result<Value> {
    let mut result = json!({"account":plan.account,"id":plan.message,"source_version":plan.version,
        "sanitizer":html_view::SANITIZER_VERSION,"sender":plan.sender,"quotes":plan.quotes});
    let (view, decrypted) = match view(&plan, cancel) {
        Ok(view) => view,
        Err(e) if e.code == "too_large" => {
            result["has_html"] = true.into();
            result["too_large"] = true.into();
            return Ok(result);
        }
        Err(e) => return Err(e),
    };
    let Some(view) = view else {
        result["has_html"] = false.into();
        return Ok(result);
    };
    result["has_html"] = true.into();
    result["too_large"] = false.into();
    // Measure the JSON-escaped size: quotes and backslashes grow on the wire.
    let size = serde_json::to_vec(&view.html)?.len();
    // Decrypted HTML never goes to the disk cache; the reader shows the text.
    if decrypted && size > INLINE_HTML_BYTES {
        result["too_large"] = true.into();
        return Ok(result);
    }
    if size > INLINE_HTML_BYTES {
        let quotes = if plan.quotes { "-quotes" } else { "" };
        let name = format!("{}-{}{quotes}", plan.version, html_view::SANITIZER_VERSION);
        let path = cache.html_file(&name, &view.html)?;
        result["html_path"] = path.to_string_lossy().into_owned().into();
    } else {
        result["html"] = view.html.clone().into();
    }
    result["resources"] = view
        .resources
        .iter()
        .map(|r| {
            let mut item = json!({"id":r.id,"kind":r.kind.as_str(),"alt":r.alt});
            if !r.host.is_empty() {
                item["host"] = html_view::display_host(&r.host).into();
            }
            if let Some(w) = r.width {
                item["width"] = w.into();
            }
            if let Some(h) = r.height {
                item["height"] = h.into();
            }
            // Already decoded images need no request: remote ones fetched
            // earlier, and this version's own inline images, which for
            // decrypted mail are kept apart from the image cache.
            let hit = match r.kind {
                ResourceKind::Remote => {
                    cache.cached_image(&plan.account, &view_cache::key(&["remote", &r.source]))
                }
                ResourceKind::Cid | ResourceKind::Data if decrypted => {
                    cache.cached_decrypted(&inline_key(&plan, r.id))
                }
                ResourceKind::Cid | ResourceKind::Data => {
                    cache.cached_image(&plan.account, &inline_key(&plan, r.id))
                }
                ResourceKind::Insecure => None,
            };
            if let Some((path, width, height)) = hit {
                item["image"] =
                    json!({"path":path.to_string_lossy(),"width":width,"height":height});
            }
            item
        })
        .collect::<Vec<_>>()
        .into();
    result["links"] = view
        .links
        .iter()
        .map(|l| json!({"id":l.id,"href":l.href,"scheme":l.scheme,"host":html_view::display_host(&l.host)}))
        .collect::<Vec<_>>()
        .into();
    result["trackers"] = view.trackers.into();
    result["quote_removed"] = view.quote_removed.into();
    result["limited"] = view.limited.into();
    Ok(result)
}

/// What an image id refers to, after checking it against the current source.
pub enum ImageSource {
    /// Fetch through the remote-content path. `key` names its cache file.
    Remote {
        url: String,
        host: String,
        key: String,
    },
    /// Already decoded and cached, or decoded now from this message's own bytes.
    Ready {
        path: String,
        width: u32,
        height: u32,
    },
}

/// Resolve `resource` in the message's current source. Local images are decoded
/// and cached here; remote ones are only described. Never touches the network.
pub fn resolve_image(
    plan: &HtmlPlan,
    resource: usize,
    cache: &ViewCache,
    cancel: &CancellationToken,
) -> Result<ImageSource> {
    let (view, decrypted) = view(plan, cancel)?;
    let view = view.ok_or_else(|| err("not_found", "This message has no HTML images"))?;
    let item = view
        .resources
        .iter()
        .find(|r| r.id == resource)
        .ok_or_else(|| err("not_found", "Image is not in this message"))?;
    match item.kind {
        ResourceKind::Insecure => Err(err("insecure", "Images over http:// are never loaded")),
        ResourceKind::Remote => Ok(ImageSource::Remote {
            url: item.source.clone(),
            host: item.host.clone(),
            key: view_cache::key(&["remote", &item.source]),
        }),
        ResourceKind::Cid | ResourceKind::Data => {
            let key = inline_key(plan, resource);
            let cached = if decrypted {
                cache.cached_decrypted(&key)
            } else {
                cache.cached_image(&plan.account, &key)
            };
            if let Some((path, width, height)) = cached {
                return Ok(ImageSource::Ready {
                    path: path.to_string_lossy().into_owned(),
                    width,
                    height,
                });
            }
            let bytes = if item.kind == ResourceKind::Cid {
                inline_part(plan, &item.source, cancel)?
            } else {
                data_uri(&item.source)?
            };
            let image = fm_mail::image_view::normalize_image(&bytes)
                .map_err(|e| err(e.code(), "This image cannot be shown"))?;
            let path = if decrypted {
                cache.store_decrypted(&key, &image.png)?
            } else {
                cache.store_image(&plan.account, &key, &image.png)?
            };
            Ok(ImageSource::Ready {
                path: path.to_string_lossy().into_owned(),
                width: image.width,
                height: image.height,
            })
        }
    }
}

fn inline_part(plan: &HtmlPlan, cid: &str, cancel: &CancellationToken) -> Result<Vec<u8>> {
    let readable = parse_source(plan, cancel)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&readable.bytes[..])
        .ok_or_else(unparsable)?;
    let part = parsed
        .parts
        .iter()
        .find(|part| {
            part.content_id()
                .map(|id| id.trim().trim_start_matches('<').trim_end_matches('>'))
                .is_some_and(|id| id.eq_ignore_ascii_case(cid))
        })
        .ok_or_else(|| err("not_found", "The inline image is missing from this message"))?;
    let contents = part.contents();
    if contents.len() > MAX_INLINE_PART {
        return Err(err("too_large", "This inline image is too large"));
    }
    Ok(contents.to_vec())
}

fn data_uri(uri: &str) -> Result<Vec<u8>> {
    let payload = uri
        .split_once(";base64,")
        .map(|(_, payload)| payload)
        .ok_or_else(|| err("unsupported", "Only base64 data images are shown"))?;
    let compact: String = payload
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|_| err("invalid", "This inline image is damaged"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(source: &[u8]) -> (tempfile::TempDir, Store, String, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let (account, mid): (String, String) = store
            .db
            .query_row("SELECT account,id FROM messages LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let hash = store.stage_blob(source).unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id=?3",
                params![hash, account, mid],
            )
            .unwrap();
        (temp, store, account, mid, hash)
    }

    fn png_base64() -> String {
        let image = image_bytes();
        base64::engine::general_purpose::STANDARD.encode(image)
    }

    fn image_bytes() -> Vec<u8> {
        // A 2x2 PNG.
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEUlEQVR4nGP4z8DA8B+MgBgAHfAD/dPQfSYAAAAASUVORK5CYII=")
            .unwrap()
    }

    #[test]
    fn html_view_is_scoped_sanitised_and_resolves_local_images() {
        let source = format!(
            "From: A <a@x.test>\r\nContent-Type: multipart/related; boundary=b\r\n\r\n--b\r\nContent-Type: text/html\r\n\r\n<p>Hi <a href=\"https://www.news.test/a\">news</a><img src=\"cid:logo@x\" width=\"2\" height=\"20\"><img src=\"https://img.test/p.png\" width=\"600\" height=\"200\"><img src=\"data:image/png;base64,{}\"><img src=\"http://old.test/x.gif\"><script>x()</script></p>\r\n--b\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--b--\r\n",
            png_base64(),
            png_base64()
        );
        let (temp, store, account, mid, hash) = store_with(source.as_bytes());
        let cache = ViewCache::new(temp.path());
        let request = json!({"id":mid,"source_version":hash});
        let plan = store.message_html_begin(&account, &request, &[]).unwrap();
        let result = read_html(plan, &cache, &CancellationToken::new()).unwrap();
        assert_eq!(result["has_html"], true);
        let html = result["html"].as_str().unwrap();
        assert!(
            html.contains("fm-link:1") && !html.contains("script") && !html.contains("img.test")
        );
        assert_eq!(result["links"][0]["host"], "news.test");
        let kinds: Vec<_> = result["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["kind"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(kinds, ["cid", "remote", "data", "insecure"]);
        assert!(
            result["resources"][1].get("source").is_none(),
            "URLs never leave the engine"
        );
        let plan = store.message_html_begin(&account, &request, &[]).unwrap();
        match resolve_image(&plan, 1, &cache, &CancellationToken::new()).unwrap() {
            ImageSource::Ready { width, height, .. } => assert_eq!((width, height), (2, 2)),
            ImageSource::Remote { .. } => panic!("cid is local"),
        }
        match resolve_image(&plan, 3, &cache, &CancellationToken::new()).unwrap() {
            ImageSource::Ready { .. } => {}
            ImageSource::Remote { .. } => panic!("data is local"),
        }
        match resolve_image(&plan, 2, &cache, &CancellationToken::new()).unwrap() {
            ImageSource::Remote { url, host, .. } => {
                assert_eq!(
                    (url.as_str(), host.as_str()),
                    ("https://img.test/p.png", "img.test")
                )
            }
            ImageSource::Ready { .. } => panic!("remote is not fetched here"),
        }
        // Reopening reports the decoded inline images; the remote one is not cached.
        let again = read_html(
            store.message_html_begin(&account, &request, &[]).unwrap(),
            &cache,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(again["resources"][0]["image"]["width"], 2);
        assert!(again["resources"][2]["image"]["path"].is_string());
        assert!(again["resources"][1].get("image").is_none());
        assert_eq!(
            resolve_image(&plan, 4, &cache, &CancellationToken::new())
                .err()
                .unwrap()
                .code,
            "insecure"
        );
        assert!(resolve_image(&plan, 99, &cache, &CancellationToken::new()).is_err());
        assert!(store.message_html_begin("other", &request, &[]).is_err());
        assert!(store
            .message_html_begin(&account, &json!({"id":mid,"source_version":"x"}), &[])
            .is_err());
    }

    #[test]
    fn quoted_history_is_its_own_view_on_request() {
        let source = format!(
            "From: A <a@x.test>\r\nContent-Type: text/html\r\n\r\n<p>Answers inline.</p><blockquote type=\"cite\"><p>Question?</p><p>Answer.</p><img src=\"data:image/png;base64,{}\"></blockquote>",
            png_base64()
        );
        let (temp, store, account, mid, hash) = store_with(source.as_bytes());
        let cache = ViewCache::new(temp.path());
        let read = |request: Value| {
            let plan = store.message_html_begin(&account, &request, &[]).unwrap();
            read_html(plan, &cache, &CancellationToken::new()).unwrap()
        };
        let trimmed = read(json!({"id":mid,"source_version":hash}));
        assert_eq!(trimmed["quote_removed"], true);
        assert!(!trimmed["html"].as_str().unwrap().contains("Answer."));
        assert_eq!(trimmed["resources"].as_array().unwrap().len(), 0);
        let full = read(json!({"id":mid,"source_version":hash,"quotes":true}));
        assert_eq!(
            (full["quote_removed"].clone(), full["quotes"].clone()),
            (false.into(), true.into())
        );
        assert!(full["html"].as_str().unwrap().contains("Answer."));
        // The quote's image resolves in the view that shows it.
        let plan = store
            .message_html_begin(
                &account,
                &json!({"id":mid,"source_version":hash,"quotes":true}),
                &[],
            )
            .unwrap();
        assert!(matches!(
            resolve_image(&plan, 1, &cache, &CancellationToken::new()).unwrap(),
            ImageSource::Ready { .. }
        ));
        assert!(store
            .message_html_begin(
                &account,
                &json!({"id":mid,"source_version":hash,"quotes":"yes"}),
                &[]
            )
            .is_err());
    }

    #[test]
    fn plain_only_mail_reports_no_html() {
        let (temp, store, account, mid, hash) =
            store_with(b"Content-Type: text/plain\r\n\r\nhello");
        let plan = store
            .message_html_begin(&account, &json!({"id":mid,"source_version":hash}), &[])
            .unwrap();
        let result = read_html(
            plan,
            &ViewCache::new(temp.path()),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(result["has_html"], false);
    }
}
