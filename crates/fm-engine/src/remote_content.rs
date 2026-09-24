//! Remote message content. This is the only code that fetches anything a
//! rendered message refers to. It also serves the reader's sanitised HTML view,
//! decodes every image in memory-safe code, and indexes link hosts in the
//! background.
//!
//! Fetch policy: GET over https to port 443 only, never `http://`. No cookies,
//! referrer, language or conditional headers. At most three redirects, each
//! rechecked. Bounded size and time. When a proxy is configured, every request
//! goes through it, the origin is never resolved locally, and nothing falls back
//! to a direct connection.
use crate::core_tasks::CoreBridge;
use fm_core::html_content::{self, ImageSource};
use fm_core::remote_prefs::{ProxySettings, RemoteSettings};
use fm_core::view_cache::ViewCache;
use fm_core::{CoreError, Result};
use fm_google::transport::CancellationToken;
use fm_google::vault::NativeSecret;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const USER_AGENT: &str = "Mozilla/5.0";
const ACCEPT: &str = "image/webp,image/png,image/jpeg,image/gif";
const PROXY_TEST_URL: &str = "https://www.gstatic.com/generate_204";
const MAX_REDIRECTS: usize = 3;
const INDEX_BATCH: i64 = 100;

#[derive(Clone, PartialEq, Eq)]
struct ClientKey {
    proxy: Option<ProxySettings>,
    password_generation: u64,
}

pub(crate) struct Runtime {
    core: CoreBridge,
    cache: ViewCache,
    secret: Option<NativeSecret>,
    password_generation: std::sync::atomic::AtomicU64,
    workers: Arc<Semaphore>,
    fetches: Arc<Semaphore>,
    client: Mutex<Option<(ClientKey, reqwest::Client)>>,
    blocked: Arc<RwLock<Vec<String>>>,
    policy: Policy,
}

/// Test builds may reach a loopback origin that presents a test certificate.
#[derive(Clone, Default)]
pub(crate) struct Policy {
    #[cfg(test)]
    pub(crate) allow_loopback: bool,
    #[cfg(test)]
    pub(crate) roots: Vec<reqwest::Certificate>,
}

impl Policy {
    fn allow_loopback(&self) -> bool {
        #[cfg(test)]
        return self.allow_loopback;
        #[cfg(not(test))]
        false
    }
}

enum Failure {
    /// Shown on the image; the message and other images are unaffected.
    Failed(&'static str),
    /// The configured proxy did not carry the request. Never retried directly.
    Proxy(&'static str),
    Cancelled,
}

impl Runtime {
    pub(crate) fn new(
        core: CoreBridge,
        profile: &std::path::Path,
        namespace: Option<String>,
    ) -> Self {
        Self::with_policy(core, profile, namespace, Policy::default())
    }

    pub(crate) fn with_policy(
        core: CoreBridge,
        profile: &std::path::Path,
        namespace: Option<String>,
        policy: Policy,
    ) -> Self {
        Self {
            core,
            cache: ViewCache::new(profile),
            secret: namespace.and_then(|n| NativeSecret::proxy_password(&n).ok()),
            password_generation: Default::default(),
            workers: Arc::new(Semaphore::new(2)),
            fetches: Arc::new(Semaphore::new(8)),
            client: Mutex::new(None),
            blocked: Arc::new(RwLock::new(Vec::new())),
            policy,
        }
    }

    pub(crate) fn handles(method: &str) -> bool {
        matches!(
            method,
            "message_html"
                | "message_image"
                | "remote_proxy_test"
                | "remote_proxy_password"
                | "remote_image_cache"
        )
    }

    pub(crate) async fn handle(
        &self,
        cancel: CancellationToken,
        method: &str,
        account: Option<&str>,
        params: Value,
    ) -> Result<Value> {
        match method {
            "message_html" => {
                let account = account
                    .ok_or_else(|| failure("account_required", "Choose an account"))?
                    .to_owned();
                let plan = self
                    .core
                    .call(move |store| store.message_html_begin(&account, &params, &[]))
                    .await?;
                let cache = self.cache.clone();
                self.blocking(&cancel, move |cancel| {
                    html_content::read_html(plan, &cache, &cancel)
                })
                .await
            }
            "message_image" => {
                let account = account
                    .ok_or_else(|| failure("account_required", "Choose an account"))?
                    .to_owned();
                self.image(cancel, account, params).await
            }
            "remote_proxy_test" => {
                only(&params, &[])?;
                self.proxy_test(cancel).await
            }
            "remote_proxy_password" => {
                only(&params, &["op", "password"])?;
                self.proxy_password(&params).await
            }
            "remote_image_cache" => {
                only(&params, &["op"])?;
                let cache = self.cache.clone();
                let op = params["op"].as_str().unwrap_or("stats").to_owned();
                self.blocking(&cancel, move |_| {
                    if op == "clear" {
                        cache.clear_images()?;
                    } else if op != "stats" {
                        return Err(failure("invalid_request", "Choose stats or clear"));
                    }
                    Ok(json!({"bytes":cache.image_bytes(),"limit":fm_core::view_cache::IMAGE_CACHE_LIMIT}))
                })
                .await
            }
            _ => Err(failure(
                "unsupported_method",
                "Unknown remote content operation",
            )),
        }
    }

    async fn blocking<T, F>(&self, cancel: &CancellationToken, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Result<T> + Send + 'static,
    {
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(stopped()),
            permit = self.workers.clone().acquire_owned() => permit.map_err(|_| stopped())?,
        };
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if cancel.is_cancelled() {
                return Err(stopped());
            }
            operation(cancel)
        })
        .await
        .map_err(|_| failure("content_unavailable", "Message content preparation failed"))?
    }

    async fn image(
        &self,
        cancel: CancellationToken,
        account: String,
        params: Value,
    ) -> Result<Value> {
        let resource = params["resource"]
            .as_u64()
            .filter(|v| (1..=10_000).contains(v))
            .ok_or_else(|| failure("invalid_request", "Name an image of this message"))?
            as usize;
        let cached_only = match params.get("cached_only") {
            None => false,
            Some(Value::Bool(v)) => *v,
            _ => {
                return Err(failure(
                    "invalid_request",
                    "cached_only must be true or false",
                ))
            }
        };
        let begin_account = account.clone();
        let plan = self
            .core
            .call(move |store| {
                store.message_html_begin(&begin_account, &params, &["resource", "cached_only"])
            })
            .await?;
        let version = plan.version.clone();
        let message = plan.message.clone();
        let cache = self.cache.clone();
        let source = match self
            .blocking(&cancel, move |cancel| {
                html_content::resolve_image(&plan, resource, &cache, &cancel)
            })
            .await
        {
            Ok(source) => source,
            Err(e) if e.code == "insecure" => return Ok(json!({"state":"insecure"})),
            Err(e) if matches!(e.code.as_str(), "unsupported" | "too_large" | "invalid") => {
                return Ok(json!({"state":"failed","reason":e.code}))
            }
            Err(e) => return Err(e),
        };
        let (url, host, key) = match source {
            ImageSource::Ready {
                path,
                width,
                height,
            } => return Ok(json!({"state":"ready","path":path,"width":width,"height":height})),
            ImageSource::Remote { url, host, key } => (url, host, key),
        };
        if let Some((path, width, height)) = self.cache.cached_image(&account, &key) {
            return Ok(ready(path, width, height));
        }
        if cached_only {
            return Ok(json!({"state":"not_cached"}));
        }
        let settings = self
            .core
            .call(|store| store.remote_content_settings())
            .await?;
        if let Some(by) =
            fm_mail::html_view::blocked_by(settings.blocked.iter().map(String::as_str), &host)
        {
            return Ok(json!({"state":"blocked","by":by}));
        }
        let client = match self.client(&settings) {
            Ok(client) => client,
            Err(reason) => return Ok(json!({"state":"proxy_unavailable","reason":reason})),
        };
        let bytes = match self
            .fetch(&client, &url, &cancel, settings.proxy.is_some())
            .await
        {
            Ok(bytes) => bytes,
            Err(Failure::Cancelled) => return Err(stopped()),
            Err(Failure::Failed(reason)) => return Ok(json!({"state":"failed","reason":reason})),
            Err(Failure::Proxy(reason)) => {
                return Ok(json!({"state":"proxy_unavailable","reason":reason}))
            }
        };
        // Recheck: a source that changed while fetching gets no cached result.
        let recheck = json!({"id":message,"source_version":version});
        let recheck_account = account.clone();
        self.core
            .call(move |store| {
                store
                    .message_html_begin(&recheck_account, &recheck, &[])
                    .map(|_| ())
            })
            .await?;
        let cache = self.cache.clone();
        self.blocking(&cancel, move |_| {
            let image = match fm_mail::image_view::normalize_image(&bytes) {
                Ok(image) => image,
                Err(e) => return Ok(json!({"state":"failed","reason":e.code()})),
            };
            let path = cache.store_image(&account, &key, &image.png)?;
            Ok(ready(path, image.width, image.height))
        })
        .await
    }

    /// A client for the current proxy choice. Errors name why the proxy cannot be used.
    fn client(
        &self,
        settings: &RemoteSettings,
    ) -> std::result::Result<reqwest::Client, &'static str> {
        *self.blocked.write().map_err(|_| "busy")? = settings.blocked.clone();
        let key = ClientKey {
            proxy: settings.proxy.clone(),
            password_generation: self
                .password_generation
                .load(std::sync::atomic::Ordering::SeqCst),
        };
        let mut current = self.client.lock().map_err(|_| "busy")?;
        if let Some((existing, client)) = current.as_ref() {
            if existing == &key {
                return Ok(client.clone());
            }
        }
        let password = match &settings.proxy {
            None => None,
            Some(_) => {
                let secret = self.secret.as_ref().ok_or("no_password")?;
                Some(secret.load().map_err(|_| "vault")?.ok_or("no_password")?)
            }
        };
        let client = build_client(
            settings
                .proxy
                .as_ref()
                .zip(password.as_deref().map(String::as_str)),
            self.blocked.clone(),
            &self.policy,
        )
        .map_err(|_| "invalid")?;
        *current = Some((key, client.clone()));
        Ok(client)
    }

    async fn fetch(
        &self,
        client: &reqwest::Client,
        url: &str,
        cancel: &CancellationToken,
        proxied: bool,
    ) -> std::result::Result<Vec<u8>, Failure> {
        let url = reqwest::Url::parse(url).map_err(|_| Failure::Failed("refused"))?;
        let blocked = self.blocked.read().map(|b| b.clone()).unwrap_or_default();
        check_url(&url, &blocked, self.policy.allow_loopback()).map_err(Failure::Failed)?;
        let _permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(Failure::Cancelled),
            permit = self.fetches.clone().acquire_owned() => permit.map_err(|_| Failure::Cancelled)?,
        };
        let mut response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(Failure::Cancelled),
            response = client.get(url).send() => response.map_err(|e| classify(&e, proxied))?,
        };
        if response.status() != reqwest::StatusCode::OK {
            return Err(Failure::Failed("unavailable"));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_IMAGE_BYTES as u64)
        {
            return Err(Failure::Failed("too_large"));
        }
        let mut bytes = Vec::new();
        loop {
            let chunk = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(Failure::Cancelled),
                chunk = response.chunk() => chunk.map_err(|e| classify(&e, proxied))?,
            };
            let Some(chunk) = chunk else { break };
            if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err(Failure::Failed("too_large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    async fn proxy_test(&self, cancel: CancellationToken) -> Result<Value> {
        let settings = self
            .core
            .call(|store| store.remote_content_settings())
            .await?;
        if settings.proxy.is_none() {
            return Err(failure(
                "invalid_request",
                "Turn on the proxy and enter its host first",
            ));
        }
        let client = match self.client(&settings) {
            Ok(client) => client,
            Err(reason) => return Ok(json!({"ok":false,"reason":reason})),
        };
        let started = Instant::now();
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(stopped()),
            response = client.get(PROXY_TEST_URL).send() => response,
        };
        Ok(match response {
            Ok(r) if r.status().is_success() => {
                json!({"ok":true,"ms":started.elapsed().as_millis() as u64})
            }
            Ok(_) => json!({"ok":false,"reason":"unavailable"}),
            Err(e) => json!({"ok":false,"reason":match classify(&e, true) {
                Failure::Proxy(r) | Failure::Failed(r) => r,
                Failure::Cancelled => "cancelled",
            }}),
        })
    }

    async fn proxy_password(&self, params: &Value) -> Result<Value> {
        let secret = self
            .secret
            .as_ref()
            .ok_or_else(|| failure("store_unavailable", "The system keychain is unavailable"))?;
        let unavailable = |_| {
            failure(
                "store_unavailable",
                "The system keychain did not allow this",
            )
        };
        match params["op"].as_str() {
            Some("status") => {}
            Some("set") => {
                let password = params["password"]
                    .as_str()
                    .filter(|p| {
                        !p.is_empty() && p.len() <= 1024 && !p.chars().any(char::is_control)
                    })
                    .ok_or_else(|| failure("invalid_request", "Enter the proxy password"))?;
                secret.store(password).map_err(unavailable)?;
                self.password_generation
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            Some("clear") => {
                secret.remove().map_err(unavailable)?;
                self.password_generation
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            _ => return Err(failure("invalid_request", "Choose status, set or clear")),
        }
        let saved = secret.load().map_err(unavailable)?.is_some();
        Ok(json!({"saved":saved}))
    }

    /// Account removal drops that account's cached images.
    pub(crate) fn forget_account(&self, account: Option<&str>) {
        if let Some(account) = account {
            self.cache.remove_account(account);
        }
    }

    /// Index link hosts of every local message, in the background, until the
    /// engine stops. New mail is picked up on the next pass.
    pub(crate) fn start_link_indexer(self: &Arc<Self>, stop: CancellationToken) {
        let runtime = self.clone();
        tokio::spawn(async move {
            loop {
                if stop.is_cancelled() {
                    return;
                }
                let pause = match runtime
                    .core
                    .call(|store| store.link_index_begin(INDEX_BATCH))
                    .await
                {
                    Ok(Some(plan)) => {
                        let prepared = runtime
                            .blocking(&stop, move |cancel| {
                                fm_core::link_index::link_index_prepare(plan, &cancel)
                            })
                            .await;
                        match prepared {
                            Ok(batch) => match runtime
                                .core
                                .call(move |store| store.link_index_finish(batch))
                                .await
                            {
                                Ok(_) => Duration::from_millis(50),
                                Err(_) => Duration::from_secs(60),
                            },
                            Err(_) => Duration::from_secs(60),
                        }
                    }
                    Ok(None) => Duration::from_secs(20),
                    Err(_) => Duration::from_secs(60),
                };
                tokio::select! {
                    _ = stop.cancelled() => return,
                    _ = tokio::time::sleep(pause) => {}
                }
            }
        });
    }
}

fn ready(path: impl AsRef<std::path::Path>, width: u32, height: u32) -> Value {
    json!({"state":"ready","path":path.as_ref().to_string_lossy(),"width":width,"height":height})
}

fn build_client(
    proxy: Option<(&ProxySettings, &str)>,
    blocked: Arc<RwLock<Vec<String>>>,
    policy: &Policy,
) -> reqwest::Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static(ACCEPT),
    );
    let allow_loopback = policy.allow_loopback();
    let redirect = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        let blocked = blocked.read().map(|b| b.clone()).unwrap_or_default();
        match check_url(attempt.url(), &blocked, allow_loopback) {
            Ok(()) => attempt.follow(),
            Err(reason) => attempt.error(reason),
        }
    });
    let mut builder = reqwest::Client::builder()
        .https_only(true)
        .referer(false)
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .redirect(redirect)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(2);
    #[cfg(test)]
    for root in &policy.roots {
        builder = builder.add_root_certificate(root.clone());
    }
    builder = match proxy {
        Some((proxy, password)) => builder.proxy(
            reqwest::Proxy::all(format!("https://{}:{}", proxy.host, proxy.port))?
                .basic_auth(&proxy.username, password),
        ),
        None => builder
            .no_proxy()
            .dns_resolver(Arc::new(PublicResolver { allow_loopback })),
    };
    builder.build()
}

/// Scheme, port, host shape and blocklist for a request or redirect target.
fn check_url(
    url: &reqwest::Url,
    blocked: &[String],
    allow_loopback: bool,
) -> std::result::Result<(), &'static str> {
    if url.scheme() != "https" {
        return Err("insecure");
    }
    if url.port().is_some_and(|p| p != 443) && !allow_loopback {
        return Err("refused");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("refused");
    }
    let host = match url.host() {
        Some(url::Host::Domain(d)) => d.to_ascii_lowercase(),
        Some(url::Host::Ipv4(ip)) => {
            return allowed_ip(IpAddr::V4(ip), allow_loopback)
                .then_some(())
                .ok_or("refused")
        }
        Some(url::Host::Ipv6(ip)) => {
            return allowed_ip(IpAddr::V6(ip), allow_loopback)
                .then_some(())
                .ok_or("refused")
        }
        None => return Err("refused"),
    };
    let local = !host.contains('.')
        || [
            "localhost",
            ".localhost",
            ".local",
            ".internal",
            ".lan",
            ".home.arpa",
            ".intranet",
            ".corp",
        ]
        .iter()
        .any(|suffix| host == suffix.trim_start_matches('.') || host.ends_with(suffix));
    if local && !(allow_loopback && host == "localhost") {
        return Err("refused");
    }
    if fm_mail::html_view::blocked_by(blocked.iter().map(String::as_str), &host).is_some() {
        return Err("blocked");
    }
    Ok(())
}

/// Public unicast destinations only: never this machine or its networks.
fn allowed_ip(ip: IpAddr, allow_loopback: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => allowed_v4(v4, allow_loopback),
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return allow_loopback;
            }
            if let Some(v4) = v6.to_ipv4_mapped() {
                return allowed_v4(v4, allow_loopback);
            }
            let s = v6.segments();
            let nat64 = s[0] == 0x64 && s[1] == 0xff9b;
            if nat64 {
                let v4 =
                    Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
                return allowed_v4(v4, allow_loopback);
            }
            !(v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] & 0xffc0) == 0xfec0
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || (s[0] == 0x0100 && s[1] == 0 && s[2] == 0 && s[3] == 0)
                || v6 == Ipv6Addr::UNSPECIFIED
                || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0))
        }
    }
}

fn allowed_v4(v4: Ipv4Addr, allow_loopback: bool) -> bool {
    if v4.is_loopback() {
        return allow_loopback;
    }
    let o = v4.octets();
    !(v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
        || v4.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || (o[0] == 198 && (o[1] & 0xfe) == 18)
        || o[0] >= 240)
}

/// Direct mode resolves names itself and drops every non-public address, so
/// the connection only ever goes to a checked address.
struct PublicResolver {
    allow_loopback: bool,
}

impl reqwest::dns::Resolve for PublicResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        let allow_loopback = self.allow_loopback;
        Box::pin(async move {
            let found = tokio::net::lookup_host((host.as_str(), 443)).await;
            let addrs: Vec<SocketAddr> = match found {
                Ok(found) => found
                    .filter(|a| allowed_ip(a.ip(), allow_loopback))
                    .collect(),
                Err(e) => return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            };
            if addrs.is_empty() {
                return Err("destination resolves to a private address".into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Map a transport error to what the image shows. With a proxy, failures to
/// reach or authenticate with the proxy are distinguished from the origin.
fn classify(error: &reqwest::Error, proxied: bool) -> Failure {
    let mut chain = String::new();
    let mut source: Option<&dyn std::error::Error> = Some(error);
    while let Some(e) = source {
        chain.push_str(&e.to_string());
        chain.push('|');
        source = e.source();
    }
    if chain.contains("proxy authorization required") {
        return Failure::Proxy("auth");
    }
    if chain.contains("private address") {
        return Failure::Failed("refused");
    }
    if error.is_redirect() {
        return Failure::Failed(if chain.contains("blocked") {
            "blocked"
        } else {
            "refused"
        });
    }
    if error.is_timeout() {
        return if proxied && !chain.contains("tunnel error: unsuccessful") {
            Failure::Proxy("timeout")
        } else {
            Failure::Failed("timeout")
        };
    }
    if proxied && error.is_connect() && !chain.contains("tunnel error: unsuccessful") {
        return Failure::Proxy("connect");
    }
    Failure::Failed("network")
}

/// Reject unknown request fields, like Core's own request checks.
fn only(params: &Value, allowed: &[&str]) -> Result<()> {
    let object = params
        .as_object()
        .ok_or_else(|| failure("invalid_request", "Parameters must be an object"))?;
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(failure("invalid_request", "Unknown request field"));
    }
    Ok(())
}

fn stopped() -> CoreError {
    failure("cancelled", "Remote content work stopped")
}

fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}

#[cfg(test)]
mod tests;
