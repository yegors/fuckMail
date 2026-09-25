//! Sender pictures for mail list rows: the brand icon of an automated sender's
//! root domain, or a person's public profile photo. Found pictures are cached
//! as PNGs shared by every account; a picture found missing is remembered for
//! a week, in memory and on disk, so it is not fetched on every launch. A
//! network failure is not remembered. The GUI draws initials otherwise.
use super::{failure, ready, stopped, Failure, Runtime};
use fm_core::remote_prefs::RemoteSettings;
use fm_core::view_cache;
use fm_core::Result;
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) const BRAND_BASE: &str = "https://www.google.com";
pub(super) const PHOTO_BASE: &str = "https://gravatar.com";
/// How long a missing picture is not asked for again.
pub(super) const MISS_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Google answers an unknown domain with a 16px globe; real icons are larger.
const MIN_BRAND_WIDTH: u32 = 32;
const MAX_REMEMBERED_MISSES: usize = 50_000;
const NO_PICTURE: &str = "No photo \u{b7} initials";

/// Second-level labels under which a two-letter country code registers names,
/// as in `osler.co.uk`.
const COUNTRY_SECOND_LEVEL: &[&str] = &[
    "co", "com", "net", "org", "gov", "ac", "edu", "ltd", "plc", "or",
];
/// Personal mail providers: a person's address there never gets a brand icon.
const PERSONAL_DOMAINS: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "msn.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "ymail.com",
    "proton.me",
    "protonmail.com",
    "protonmail.ch",
    "pm.me",
    "fastmail.com",
    "fastmail.fm",
    "mail.com",
    "zoho.com",
    "hey.com",
    "tutanota.com",
    "tuta.io",
];
/// Personal providers with a domain per country, such as `yahoo.co.uk`.
const PERSONAL_FAMILIES: &[&str] = &[
    "yahoo", "hotmail", "outlook", "live", "aol", "gmx", "yandex",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    People,
    Automated,
    Unknown,
}

#[derive(Clone, Copy)]
enum Source {
    Brand,
    Photo,
}

pub(super) struct Sender {
    /// Trimmed and lowercased, as given.
    pub(super) address: String,
    /// The address's domain in ASCII.
    pub(super) domain: String,
    kind: Kind,
}

enum Outcome {
    Ready(Value),
    Missing,
    /// The pattern, or `None` when a redirect led to a blocked host.
    Blocked(Option<String>),
    /// Not found this time; asked again next time.
    Transient,
    Proxy(&'static str),
}

/// `{"address": "<email>", "kind": "people"|"automated"|"unknown"}`.
pub(super) fn parse(params: &Value) -> Result<Sender> {
    let invalid = || failure("invalid_request", "Name one sender email address");
    let address = params["address"]
        .as_str()
        .ok_or_else(invalid)?
        .trim()
        .to_lowercase();
    if address.chars().count() > 320 || address.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(invalid());
    }
    let (local, domain) = address.split_once('@').ok_or_else(invalid)?;
    if local.is_empty() || domain.contains('@') {
        return Err(invalid());
    }
    let domain = match url::Host::parse(domain) {
        Ok(url::Host::Domain(domain)) => domain,
        _ => return Err(invalid()),
    };
    if !domain.contains('.')
        || domain.len() > 253
        || domain.split('.').any(|label| {
            label.is_empty()
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(invalid());
    }
    let kind = match params["kind"].as_str() {
        Some("people") => Kind::People,
        Some("automated") => Kind::Automated,
        Some("unknown") => Kind::Unknown,
        _ => {
            return Err(failure(
                "invalid_request",
                "Choose people, automated or unknown",
            ))
        }
    };
    Ok(Sender {
        address,
        domain,
        kind,
    })
}

/// The registrable domain, without a public suffix list: the last two labels,
/// or three under a country code's common second level (`osler.co.uk`).
pub(super) fn root_domain(domain: &str) -> &str {
    let labels: Vec<&str> = domain.rsplitn(4, '.').collect();
    let keep = match labels.as_slice() {
        [tld, second, _, ..] if tld.len() == 2 && COUNTRY_SECOND_LEVEL.contains(second) => 3,
        _ => 2,
    };
    match domain.rmatch_indices('.').nth(keep - 1) {
        Some((dot, _)) => &domain[dot + 1..],
        None => domain,
    }
}

/// Whether a root domain belongs to a personal mail provider.
pub(super) fn personal(root: &str) -> bool {
    PERSONAL_DOMAINS.contains(&root)
        || root
            .split('.')
            .next()
            .is_some_and(|name| PERSONAL_FAMILIES.contains(&name))
}

fn none(transient: bool) -> Value {
    if transient {
        json!({"state":"none","label":NO_PICTURE,"transient":true})
    } else {
        json!({"state":"none","label":NO_PICTURE})
    }
}

impl Runtime {
    /// A cached PNG of the sender's picture, or why there is none.
    pub(super) async fn sender_avatar(
        &self,
        cancel: CancellationToken,
        params: &Value,
    ) -> Result<Value> {
        let sender = parse(params)?;
        let settings = self
            .core
            .call(|store| store.remote_content_settings())
            .await?;
        if !settings.sender_pictures {
            return Ok(json!({"state":"none","label":NO_PICTURE,"off":true}));
        }
        let root = root_domain(&sender.domain);
        let brand = !personal(root);
        let sources: &[Source] = match sender.kind {
            Kind::People => &[Source::Photo],
            Kind::Automated if brand => &[Source::Brand],
            Kind::Automated => &[],
            Kind::Unknown if brand => &[Source::Photo, Source::Brand],
            Kind::Unknown => &[Source::Photo],
        };
        let mut blocked = None;
        let mut transient = false;
        for source in sources {
            match self
                .sender_picture(&cancel, &settings, *source, &sender, root)
                .await?
            {
                Outcome::Ready(found) => return Ok(found),
                Outcome::Proxy(reason) => {
                    return Ok(json!({"state":"proxy_unavailable","reason":reason}))
                }
                Outcome::Blocked(by) => {
                    blocked.get_or_insert(by);
                }
                Outcome::Transient => transient = true,
                Outcome::Missing => {}
            }
        }
        Ok(match blocked {
            Some(by) => json!({"state":"blocked","by":by}),
            None => none(transient),
        })
    }

    async fn sender_picture(
        &self,
        cancel: &CancellationToken,
        settings: &RemoteSettings,
        source: Source,
        sender: &Sender,
        root: &str,
    ) -> Result<Outcome> {
        let (key, url, label, name) = match source {
            Source::Brand => (
                view_cache::key(&["brand", root]),
                format!(
                    "{}/s2/favicons?domain={root}&sz=64",
                    self.policy.brand_base()
                ),
                format!("Brand icon \u{b7} {root}"),
                "brand",
            ),
            Source::Photo => (
                view_cache::key(&["photo", &sender.address]),
                format!(
                    "{}/avatar/{}?s=64&d=404",
                    self.policy.photo_base(),
                    view_cache::sha256_hex(&sender.address)
                ),
                format!("Profile photo \u{b7} {}", sender.address),
                "photo",
            ),
        };
        let found = |path: std::path::PathBuf, width: u32, height: u32| {
            let mut value = ready(path, width, height);
            value["source"] = name.into();
            value["label"] = label.as_str().into();
            Outcome::Ready(value)
        };
        if let Some(known) = self.known_sender(&key, &found) {
            return Ok(known);
        }
        let host = reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        if let Some(by) =
            fm_mail::html_view::blocked_by(settings.blocked.iter().map(String::as_str), &host)
        {
            return Ok(Outcome::Blocked(Some(by.to_owned())));
        }
        let client = match self.client(settings) {
            Ok(client) => client,
            Err(reason) => return Ok(Outcome::Proxy(reason)),
        };
        // One fetch per picture; a request that waited finds its result cached.
        let lock = self.sender_lock(&key);
        let _held = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(stopped()),
            held = lock.lock_owned() => held,
        };
        if let Some(known) = self.known_sender(&key, &found) {
            return Ok(known);
        }
        let bytes = match self
            .fetch_found(&client, &url, cancel, settings.proxy.is_some())
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(Failure::Failed("too_large")) => {
                self.remember_missing(&key);
                return Ok(Outcome::Missing);
            }
            Err(Failure::Cancelled) => return Err(stopped()),
            Err(Failure::Proxy(reason)) => return Ok(Outcome::Proxy(reason)),
            Err(Failure::Failed("blocked")) => return Ok(Outcome::Blocked(None)),
            Err(Failure::Failed(_)) => return Ok(Outcome::Transient),
        };
        let cache = self.cache.clone();
        let stored_key = key.clone();
        let minimum = match source {
            Source::Brand => MIN_BRAND_WIDTH,
            Source::Photo => 1,
        };
        let stored = self
            .blocking(cancel, move |_| {
                let image = match fm_mail::image_view::normalize_image(&bytes) {
                    Ok(image) if image.width >= minimum => image,
                    _ => return Ok(None),
                };
                let path = cache.store_sender(&stored_key, &image.png)?;
                Ok(Some((path, image.width, image.height)))
            })
            .await?;
        Ok(match stored {
            Some((path, width, height)) => found(path, width, height),
            None => {
                self.remember_missing(&key);
                Outcome::Missing
            }
        })
    }

    /// A cached picture, or a miss remembered in memory or on disk.
    fn known_sender(
        &self,
        key: &str,
        found: &dyn Fn(std::path::PathBuf, u32, u32) -> Outcome,
    ) -> Option<Outcome> {
        if let Some((path, width, height)) = self.cache.cached_sender(key) {
            return Some(found(path, width, height));
        }
        let mut misses = self.sender_misses.lock().ok()?;
        match misses.get(key) {
            Some(at) if at.elapsed() < MISS_TTL => return Some(Outcome::Missing),
            Some(_) => {
                misses.remove(key);
            }
            None => {}
        }
        if self.cache.sender_missing(key, MISS_TTL) {
            misses.insert(key.to_owned(), Instant::now());
            return Some(Outcome::Missing);
        }
        None
    }

    fn remember_missing(&self, key: &str) {
        if let Ok(mut misses) = self.sender_misses.lock() {
            if misses.len() >= MAX_REMEMBERED_MISSES {
                misses.clear();
            }
            misses.insert(key.to_owned(), Instant::now());
        }
        let _ = self.cache.mark_sender_missing(key);
    }

    fn sender_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .sender_locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(key.to_owned(), Arc::downgrade(&lock));
        lock
    }

    /// Forget remembered misses, as when the image cache is cleared.
    pub(super) fn forget_sender_misses(&self) {
        if let Ok(mut misses) = self.sender_misses.lock() {
            misses.clear();
        }
    }
}
