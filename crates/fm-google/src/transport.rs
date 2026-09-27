//! Bounded, cancellable Google HTTPS. This layer submits once; it never retries.
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{Duration, SystemTime},
};
/// Cooperative cancellation shared by every async call in this crate.
pub use tokio_util::sync::CancellationToken;

/// HTTP method of an [`HttpRequest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}
impl HttpMethod {
    fn native(self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Post => reqwest::Method::POST,
            Self::Put => reqwest::Method::PUT,
            Self::Patch => reqwest::Method::PATCH,
            Self::Delete => reqwest::Method::DELETE,
        }
    }
}

/// Why one HTTP attempt produced no usable response.
///
/// No provider URL, header, body or error text is included in diagnostics.
/// `Cancelled`, `Timeout` and `Network` can happen after the request reached
/// Google, so for a write they mean the outcome is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    /// Refused locally: URL outside the allowlist, disallowed header, invalid
    /// limits, or a wrapper's policy check. Nothing was sent.
    InvalidRequest,
    /// Local admission refused dispatch after a newly learned provider cooldown.
    /// No HTTP request was submitted for this attempt.
    Cooldown { retry_after_secs: u64 },
    /// The HTTP client could not be built.
    Initialization,
    /// The caller's token was cancelled.
    Cancelled,
    /// The connect, read or whole-request deadline elapsed.
    Timeout,
    /// Connection, TLS or protocol failure, including a body cut short.
    Network,
    /// Google answered with a redirect. Redirects are never followed, so a
    /// bearer token cannot leave the allowlisted host.
    RedirectDenied,
    /// The body exceeds [`TransportLimits::max_request_bytes`]. Nothing was sent.
    RequestTooLarge,
    /// The response exceeds its cap; any partial body is discarded.
    ResponseTooLarge,
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Google transport: {self:?}")
    }
}
impl std::error::Error for TransportError {}

/// A request to an allowlisted Google HTTPS endpoint.
///
/// Construction validates the URL and every header, so an `HttpRequest` can
/// only target the Google hosts and API paths this app uses. `Debug` shows
/// only the method.
pub struct HttpRequest {
    method: HttpMethod,
    url: reqwest::Url,
    headers: HeaderMap,
    body: Vec<u8>,
    response_limit: Option<usize>,
    deadline: Option<Duration>,
    progress: Option<Arc<AtomicU64>>,
}
impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}
impl HttpRequest {
    /// Starts a request with no headers or body.
    ///
    /// Errors with `InvalidRequest` if `url` is over 16 KiB, is not
    /// `https` on port 443, has userinfo or a fragment, carries a credential
    /// query parameter (such as `access_token` or `code`), or falls outside
    /// the Gmail, Calendar and OAuth endpoints this crate calls.
    pub fn new(method: HttpMethod, url: &str) -> Result<Self, TransportError> {
        if url.len() > 16 * 1024 {
            return Err(TransportError::InvalidRequest);
        }
        let url = reqwest::Url::parse(url).map_err(|_| TransportError::InvalidRequest)?;
        if !allowed_url(&url) {
            return Err(TransportError::InvalidRequest);
        }
        Ok(Self {
            method,
            url,
            headers: HeaderMap::new(),
            body: Vec::new(),
            response_limit: None,
            deadline: None,
            progress: None,
        })
    }
    /// Sets a header, replacing any earlier value of the same name.
    ///
    /// Only `authorization`, `accept`, `content-type`, `if-match` and
    /// `if-none-match` are allowed, each up to 16 KiB. Values are marked
    /// sensitive so HTTP-layer debugging never prints them.
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, TransportError> {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| TransportError::InvalidRequest)?;
        if !matches!(
            name.as_str(),
            "authorization" | "accept" | "content-type" | "if-match" | "if-none-match"
        ) || value.len() > 16 * 1024
        {
            return Err(TransportError::InvalidRequest);
        }
        let mut value = HeaderValue::from_str(value).map_err(|_| TransportError::InvalidRequest)?;
        value.set_sensitive(true);
        self.headers.insert(name, value);
        Ok(self)
    }
    /// Attaches a body and its `Content-Type`. Refused for `Get`.
    pub fn body(mut self, bytes: Vec<u8>, content_type: &str) -> Result<Self, TransportError> {
        if self.method == HttpMethod::Get {
            return Err(TransportError::InvalidRequest);
        }
        self = self.header("content-type", content_type)?;
        self.body = bytes;
        Ok(self)
    }
    /// Counts, in `sent`, the body bytes handed to the connection so far.
    /// The count can run ahead of what Google has received by the size of
    /// the socket's buffers.
    pub fn progress(mut self, sent: Arc<AtomicU64>) -> Self {
        self.progress = Some(sent);
        self
    }
    /// Caps the response body at `bytes`.
    ///
    /// The caller may tighten the transport's cap, never enlarge it. A cap of
    /// zero makes execution fail with `InvalidRequest`.
    pub fn response_limit(mut self, bytes: usize) -> Self {
        self.response_limit = Some(bytes);
        self
    }
    /// Replaces the transport's whole-request deadline, for a download too
    /// large to finish within it. Gaps between reads still follow the
    /// transport's read timeout. A zero deadline or one over ten minutes makes
    /// execution fail with `InvalidRequest`.
    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }
    /// Accessors are for protocol adapters/fakes; never log their contents.
    pub fn method(&self) -> HttpMethod {
        self.method
    }
    pub fn url(&self) -> &str {
        self.url.as_str()
    }
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    pub fn body_bytes(&self) -> &[u8] {
        &self.body
    }
    pub fn response_limit_bytes(&self) -> Option<usize> {
        self.response_limit
    }
    pub fn request_deadline(&self) -> Option<Duration> {
        self.deadline
    }
}

fn allowed_url(url: &reqwest::Url) -> bool {
    // Bearer tokens ride on these requests, so pin the exact hosts and API
    // prefixes the app calls. Secrets in a query string would land in
    // server-side logs, so credential-looking parameters are refused.
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || url.fragment().is_some()
    {
        return false;
    }
    if url.query_pairs().any(|(key, _)| {
        matches!(
            key.to_ascii_lowercase().as_str(),
            "access_token" | "refresh_token" | "id_token" | "client_secret" | "code" | "token"
        )
    }) {
        return false;
    }
    let path = url.path();
    match url.host_str() {
        Some("gmail.googleapis.com") => {
            path.starts_with("/gmail/v1/") || path.starts_with("/upload/gmail/v1/")
        }
        Some("www.googleapis.com") => {
            path.starts_with("/calendar/v3/")
                || path.starts_with("/gmail/v1/")
                || path.starts_with("/upload/gmail/v1/")
                || path == "/oauth2/v3/certs"
        }
        Some("oauth2.googleapis.com") => matches!(path, "/token" | "/revoke"),
        Some("openidconnect.googleapis.com") => path == "/v1/userinfo",
        Some("accounts.google.com") => path == "/.well-known/openid-configuration",
        _ => false,
    }
}

/// A request body sent in 64 KiB frames, recording how many bytes the
/// connection has taken.
struct CountedBody {
    bytes: bytes::Bytes,
    offset: usize,
    sent: Arc<AtomicU64>,
}
impl http_body::Body for CountedBody {
    type Data = bytes::Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.offset >= self.bytes.len() {
            return Poll::Ready(None);
        }
        let end = (self.offset + 64 * 1024).min(self.bytes.len());
        let frame = self.bytes.slice(self.offset..end);
        self.offset = end;
        // The connection asks for a frame once it has room for it.
        self.sent.store(end as u64, Ordering::Release);
        Poll::Ready(Some(Ok(http_body::Frame::data(frame))))
    }
    fn is_end_stream(&self) -> bool {
        self.offset >= self.bytes.len()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::with_exact((self.bytes.len() - self.offset) as u64)
    }
}

/// A complete response of any status. `Debug` shows only the status.
///
/// The transport does not interpret status codes; callers classify them, for
/// example with [`crate::classify_api_error`].
pub struct HttpResponse {
    pub status: u16,
    /// The whole body, within the request's response cap.
    pub body: Vec<u8>,
    /// `Retry-After` in whole seconds, when present and parseable.
    pub retry_after_secs: Option<u64>,
    /// The `ETag` header, when it is visible ASCII of at most 1024 bytes.
    pub etag: Option<String>,
}
impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}
/// The future returned by [`HttpTransport::execute`].
pub type HttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponse, TransportError>> + Send + 'a>>;

/// Sends one HTTP request to Google.
///
/// Every Google client in this crate goes through this trait, so tests and the
/// engine can substitute or wrap it. Implementations must:
/// - submit the request at most once, never retrying or following redirects;
/// - stop and return `Cancelled` once `cancel` fires;
/// - return a complete body or an error, never a partial body;
/// - keep URLs, headers and bodies out of errors and logs.
///
/// Retry and backoff belong to the caller, which knows whether the request is
/// safe to repeat.
pub trait HttpTransport: Send + Sync {
    /// Executes `request` once. A non-2xx status is an `Ok` response.
    fn execute<'a>(&'a self, request: HttpRequest, cancel: &'a CancellationToken)
        -> HttpFuture<'a>;
}

/// Timeouts and byte caps for [`ReqwestTransport`].
///
/// All values must be nonzero; connect and read timeouts may not exceed
/// the request timeout, which is at most 600 s; each byte cap is at most 128 MiB.
#[derive(Clone, Copy, Debug)]
pub struct TransportLimits {
    /// Time to establish the TCP and TLS connection.
    pub connect_timeout: Duration,
    /// Deadline for the whole request, including the body download.
    pub request_timeout: Duration,
    /// Longest gap allowed between successive reads.
    pub read_timeout: Duration,
    /// Largest request body sent.
    pub max_request_bytes: usize,
    /// Largest response body accepted; requests may lower it.
    pub max_response_bytes: usize,
}
impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(120),
            read_timeout: Duration::from_secs(30),
            max_request_bytes: 64 * 1024 * 1024,
            // Every request sets its own lower cap; the largest is a Gmail
            // message's raw source.
            max_response_bytes: 256 * 1024 * 1024,
        }
    }
}

/// The production [`HttpTransport`]: an HTTPS-only `reqwest` client.
///
/// It follows no redirects, never retries, ignores proxy settings, sends no
/// `Referer` and does no content decoding. Clones share one connection pool.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    limits: TransportLimits,
}
impl ReqwestTransport {
    /// Builds the client. Errors with `InvalidRequest` when `limits` are out
    /// of range and `Initialization` when the TLS client cannot be built.
    pub fn new(limits: TransportLimits) -> Result<Self, TransportError> {
        Self::build(limits, true)
    }
    fn build(limits: TransportLimits, https_only: bool) -> Result<Self, TransportError> {
        if limits.connect_timeout.is_zero()
            || limits.request_timeout.is_zero()
            || limits.read_timeout.is_zero()
            || limits.request_timeout > Duration::from_secs(600)
            || limits.connect_timeout > limits.request_timeout
            || limits.read_timeout > limits.request_timeout
            || limits.max_response_bytes == 0
            || limits.max_request_bytes == 0
            || limits.max_request_bytes > 128 * 1024 * 1024
            || limits.max_response_bytes > 256 * 1024 * 1024
        {
            return Err(TransportError::InvalidRequest);
        }
        // Content decoding is off so the byte caps count what arrives on the
        // wire, and a small compressed body cannot expand past them.
        let client = reqwest::Client::builder()
            .https_only(https_only)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .referer(false)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .connect_timeout(limits.connect_timeout)
            .read_timeout(limits.read_timeout)
            .timeout(limits.request_timeout)
            .pool_max_idle_per_host(2)
            .build()
            .map_err(|_| TransportError::Initialization)?;
        Ok(Self { client, limits })
    }
    async fn submit(
        &self,
        request: HttpRequest,
        deadline: Duration,
    ) -> Result<HttpResponse, TransportError> {
        if request.body.len() > self.limits.max_request_bytes {
            return Err(TransportError::RequestTooLarge);
        }
        let cap = request
            .response_limit
            .unwrap_or(self.limits.max_response_bytes)
            .min(self.limits.max_response_bytes);
        if cap == 0 {
            return Err(TransportError::InvalidRequest);
        }
        let empty_submission = request.body.is_empty()
            && matches!(
                request.method,
                HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
            );
        let body = match request.progress {
            Some(sent) => reqwest::Body::wrap(CountedBody {
                bytes: request.body.into(),
                offset: 0,
                sent,
            }),
            None => request.body.into(),
        };
        let mut builder = self
            .client
            .request(request.method.native(), request.url)
            .headers(request.headers)
            .timeout(deadline)
            .body(body);
        // An empty Vec does not make reqwest emit Content-Length. Google's
        // empty-body POST endpoints still need explicit HTTP body framing.
        if empty_submission {
            builder = builder.header(reqwest::header::CONTENT_LENGTH, 0);
        }
        let mut response = builder.send().await.map_err(map_error)?;
        // 304 is a conditional-read answer, not a redirect.
        if response.status().is_redirection()
            && response.status() != reqwest::StatusCode::NOT_MODIFIED
        {
            return Err(TransportError::RedirectDenied);
        }
        // Refuse a declared oversize body before reading it; the chunk loop
        // below still enforces the cap when no length is declared.
        if response
            .content_length()
            .is_some_and(|length| length > cap as u64)
        {
            return Err(TransportError::ResponseTooLarge);
        }
        let status = response.status().as_u16();
        let retry_after_secs = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| parse_retry_after(v, SystemTime::now()));
        let etag = response
            .headers()
            .get("etag")
            .filter(|v| v.len() <= 1024)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        // A declared length is at most the cap here, so the body is read into
        // one allocation instead of regrowing through larger copies.
        let mut body = Vec::with_capacity(response.content_length().unwrap_or(0) as usize);
        while let Some(chunk) = response.chunk().await.map_err(map_error)? {
            if chunk.len() > cap.saturating_sub(body.len()) {
                return Err(TransportError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            body,
            retry_after_secs,
            etag,
        })
    }
}
impl HttpTransport for ReqwestTransport {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            let deadline = request.deadline.unwrap_or(self.limits.request_timeout);
            if deadline.is_zero() || deadline > Duration::from_secs(600) {
                return Err(TransportError::InvalidRequest);
            }
            // The outer deadline also covers the body loop in `submit`, so a
            // server that keeps trickling bytes cannot outlast the request's
            // deadline. `biased` lets cancellation win a tie.
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(TransportError::Cancelled),
                result = tokio::time::timeout(deadline, self.submit(request, deadline)) =>
                    result.unwrap_or(Err(TransportError::Timeout)),
            }
        })
    }
}
fn map_error(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Network
    }
}

/// Parses a `Retry-After` value into whole seconds from `now`.
///
/// Accepts delay-seconds or an HTTP date. A past date yields 0, a partial
/// second rounds up, and negative, fractional or malformed values yield `None`.
pub fn parse_retry_after(value: &str, now: SystemTime) -> Option<u64> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value.parse().ok();
    }
    let date = httpdate::parse_http_date(value).ok()?;
    let delta = date.duration_since(now).unwrap_or_default();
    Some(
        delta
            .as_secs()
            .saturating_add(u64::from(delta.subsec_nanos() != 0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn endpoint(
        response: &'static [u8],
        delay: Duration,
    ) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let count = socket.read(&mut request).await.unwrap();
            request.truncate(count);
            tokio::time::sleep(delay).await;
            let _ = socket.write_all(response).await;
            request
        });
        (url, handle)
    }
    fn local_request(url: &str) -> HttpRequest {
        let mut request = HttpRequest::new(
            HttpMethod::Get,
            "https://gmail.googleapis.com/gmail/v1/users/me/profile",
        )
        .unwrap();
        request.url = reqwest::Url::parse(url).unwrap();
        request
    }
    fn transport() -> ReqwestTransport {
        ReqwestTransport::build(TransportLimits::default(), false).unwrap()
    }

    /// Opt-in probe of public Google metadata; ordinary test runs stay offline.
    /// It checks only that the native TLS roots reach Google, not OAuth or Gmail.
    #[tokio::test]
    #[ignore = "explicit native HTTPS probe of public Google JWKS; no credentials or mailbox access"]
    async fn native_roots_public_google_jwks_smoke() {
        let transport = ReqwestTransport::new(TransportLimits {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(15),
            read_timeout: Duration::from_secs(5),
            max_request_bytes: 16 * 1024,
            max_response_bytes: 256 * 1024,
        })
        .expect("native TLS client initialization failed");
        let request = HttpRequest::new(
            HttpMethod::Get,
            "https://www.googleapis.com/oauth2/v3/certs",
        )
        .expect("fixed public Google JWKS endpoint rejected")
        .response_limit(256 * 1024);
        assert!(request.headers().get("authorization").is_none());
        let started = std::time::Instant::now();
        let response = transport
            .execute(request, &CancellationToken::new())
            .await
            .expect("public Google JWKS native TLS request failed");
        assert_eq!(
            response.status, 200,
            "public JWKS returned a non-200 status"
        );
        let keys: jsonwebtoken::jwk::JwkSet = serde_json::from_slice(&response.body)
            .unwrap_or_else(|_| panic!("public JWKS response is not valid key-set JSON"));
        assert!(!keys.keys.is_empty(), "public JWKS key set is empty");
        assert!(
            keys.keys
                .iter()
                .all(|key| jsonwebtoken::DecodingKey::from_jwk(key).is_ok()),
            "public JWKS contains an unusable verification key"
        );
        eprintln!(
            "public_jwks_native_tls: status=200; valid_key_set=true; key_count={}; elapsed_ms={}; bearer=false; retries=false; redirects=false; native_roots=true",
            keys.keys.len(), started.elapsed().as_millis()
        );
    }

    #[test]
    fn endpoints_and_debug_are_bounded_and_private() {
        for url in [
            "http://gmail.googleapis.com/gmail/v1/users/me",
            "https://gmail.googleapis.com.evil.test/gmail/v1/",
            "https://user@gmail.googleapis.com/gmail/v1/",
            "https://gmail.googleapis.com:8443/gmail/v1/",
            "https://www.googleapis.com/other",
            "https://oauth2.googleapis.com/token?access_token=secret",
            "https://www.googleapis.com/calendar/v3/a#secret",
        ] {
            assert_eq!(
                HttpRequest::new(HttpMethod::Get, url).unwrap_err(),
                TransportError::InvalidRequest
            );
        }
        let request = HttpRequest::new(HttpMethod::Post, "https://oauth2.googleapis.com/token")
            .unwrap()
            .header("authorization", "Bearer private-token")
            .unwrap()
            .body(
                b"private-body".to_vec(),
                "application/x-www-form-urlencoded",
            )
            .unwrap();
        let debug = format!("{request:?}");
        assert!(!debug.contains("private") && !debug.contains("googleapis"));
        assert!(HttpRequest::new(
            HttpMethod::Get,
            "https://www.googleapis.com/oauth2/v3/certs"
        )
        .unwrap()
        .header("host", "other.test")
        .is_err());
        let response = HttpResponse {
            status: 403,
            body: b"private-error".to_vec(),
            retry_after_secs: None,
            etag: Some("private-etag".into()),
        };
        assert!(!format!("{response:?}").contains("private"));
    }
    #[test]
    fn retry_after_delta_date_and_invalid_values() {
        let now = httpdate::parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT").unwrap();
        assert_eq!(parse_retry_after("120", now), Some(120));
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2015 07:30:00 GMT", now),
            Some(120)
        );
        assert_eq!(
            parse_retry_after(
                "Wed, 21 Oct 2015 07:28:00 GMT",
                now + Duration::from_secs(1)
            ),
            Some(0)
        );
        for value in ["-1", "NaN", "1.5", "99999999999999999999999999999999"] {
            assert_eq!(parse_retry_after(value, now), None);
        }
    }
    #[tokio::test]
    async fn returns_status_once_and_caps_actual_chunked_body() {
        let (url, server) = endpoint(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 2\r\nRetry-After: 17\r\nConnection: close\r\n\r\n{}", Duration::ZERO).await;
        let response = transport()
            .execute(local_request(&url), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            (response.status, response.retry_after_secs, response.body),
            (429, Some(17), b"{}".to_vec())
        );
        server.await.unwrap();
        let (url, server) = endpoint(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n1234\r\n4\r\n5678\r\n0\r\n\r\n", Duration::ZERO).await;
        assert_eq!(
            transport()
                .execute(
                    local_request(&url).response_limit(7),
                    &CancellationToken::new()
                )
                .await
                .unwrap_err(),
            TransportError::ResponseTooLarge
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn conditional_read_preserves_not_modified_status_and_etag() {
        let (url, server) = endpoint(
            b"HTTP/1.1 304 Not Modified\r\nETag: \"opaque-etag\"\r\nConnection: close\r\n\r\n",
            Duration::ZERO,
        )
        .await;
        let request = local_request(&url)
            .header("if-none-match", "\"opaque-etag\"")
            .unwrap();
        let response = transport()
            .execute(request, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(response.status, 304);
        assert_eq!(response.etag.as_deref(), Some("\"opaque-etag\""));
        assert!(response.body.is_empty());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn size_checks_and_cancel_precede_network() {
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            transport()
                .execute(local_request("http://127.0.0.1:1/"), &token)
                .await
                .unwrap_err(),
            TransportError::Cancelled
        );
        let mut request = local_request("http://127.0.0.1:1/");
        request.body = vec![0; 9];
        let limited = ReqwestTransport::build(
            TransportLimits {
                max_request_bytes: 8,
                ..TransportLimits::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            limited
                .execute(request, &CancellationToken::new())
                .await
                .unwrap_err(),
            TransportError::RequestTooLarge
        );
        let (url, server) = endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 9000\r\n\r\n",
            Duration::ZERO,
        )
        .await;
        assert_eq!(
            transport()
                .execute(
                    local_request(&url).response_limit(8),
                    &CancellationToken::new()
                )
                .await
                .unwrap_err(),
            TransportError::ResponseTooLarge
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn redirect_never_forwards_authorization() {
        let other = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let location = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", other.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            socket.write_all(location.as_bytes()).await.unwrap();
        });
        let request = local_request(&url)
            .header("authorization", "Bearer private-token")
            .unwrap();
        assert_eq!(
            transport()
                .execute(request, &CancellationToken::new())
                .await
                .unwrap_err(),
            TransportError::RedirectDenied
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(60), other.accept())
                .await
                .is_err()
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn cancellation_interrupts_headers_and_body_and_read_deadline() {
        for send_headers in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let token = CancellationToken::new();
            let peer_token = token.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let _ = socket.read(&mut buffer).await;
                if send_headers {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\na")
                        .await
                        .unwrap();
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
                peer_token.cancel();
                tokio::time::sleep(Duration::from_millis(30)).await;
            });
            assert_eq!(
                transport()
                    .execute(local_request(&url), &token)
                    .await
                    .unwrap_err(),
                TransportError::Cancelled
            );
            server.await.unwrap();
        }
        let (url, server) = endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
            Duration::from_millis(150),
        )
        .await;
        let limited = ReqwestTransport::build(
            TransportLimits {
                connect_timeout: Duration::from_millis(50),
                request_timeout: Duration::from_millis(80),
                read_timeout: Duration::from_millis(50),
                ..TransportLimits::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            limited
                .execute(local_request(&url), &CancellationToken::new())
                .await
                .unwrap_err(),
            TransportError::Timeout
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn whole_deadline_bounds_a_body_that_keeps_making_progress() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n")
                .await
                .unwrap();
            for _ in 0..100 {
                if socket.write_all(b"a").await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let limited = ReqwestTransport::build(
            TransportLimits {
                connect_timeout: Duration::from_millis(80),
                request_timeout: Duration::from_millis(100),
                read_timeout: Duration::from_millis(80),
                ..TransportLimits::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            limited
                .execute(local_request(&url), &CancellationToken::new())
                .await
                .unwrap_err(),
            TransportError::Timeout
        );
        server.abort();
    }

    #[tokio::test]
    async fn a_request_deadline_lets_a_slow_download_finish_and_is_bounded() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 30\r\n\r\n")
                .await
                .unwrap();
            for _ in 0..30 {
                socket.write_all(b"a").await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let limited = ReqwestTransport::build(
            TransportLimits {
                connect_timeout: Duration::from_millis(80),
                request_timeout: Duration::from_millis(100),
                read_timeout: Duration::from_millis(80),
                ..TransportLimits::default()
            },
            false,
        )
        .unwrap();
        let response = limited
            .execute(
                local_request(&url).deadline(Duration::from_secs(10)),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.body, vec![b'a'; 30]);
        server.await.unwrap();
        for deadline in [Duration::ZERO, Duration::from_secs(601)] {
            assert_eq!(
                limited
                    .execute(
                        local_request("http://127.0.0.1:1/").deadline(deadline),
                        &CancellationToken::new()
                    )
                    .await
                    .unwrap_err(),
                TransportError::InvalidRequest
            );
        }
        for (bytes, ok) in [(256 * 1024 * 1024, true), (256 * 1024 * 1024 + 1, false)] {
            let limits = TransportLimits {
                max_response_bytes: bytes,
                ..TransportLimits::default()
            };
            assert_eq!(ReqwestTransport::build(limits, false).is_ok(), ok);
        }
    }

    #[tokio::test]
    async fn counted_upload_keeps_its_length_and_bytes_and_reports_all_of_them() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/send", listener.local_addr().unwrap());
        let body: Vec<u8> = (0..300_000u32).map(|n| (n % 251) as u8).collect();
        let expected = body.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = Vec::new();
            let mut buffer = vec![0; 64 * 1024];
            let start = loop {
                let n = socket.read(&mut buffer).await.unwrap();
                received.extend_from_slice(&buffer[..n]);
                if let Some(at) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            while received.len() - start < 300_000 {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0, "upload ended early");
                received.extend_from_slice(&buffer[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            let headers = String::from_utf8_lossy(&received[..start]).to_ascii_lowercase();
            assert!(headers.contains("content-length: 300000\r\n"), "{headers}");
            assert!(!headers.contains("transfer-encoding"), "{headers}");
            received[start..].to_vec()
        });
        let sent = Arc::new(AtomicU64::new(0));
        let mut request = local_request(&url);
        request.method = HttpMethod::Post;
        let request = request
            .body(body, "application/json")
            .unwrap()
            .progress(sent.clone());
        let reply = transport()
            .execute(request, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(server.await.unwrap(), expected);
        assert_eq!(sent.load(Ordering::Acquire), 300_000);
    }

    #[tokio::test]
    async fn empty_post_has_explicit_zero_content_length() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/trash", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            let headers = String::from_utf8_lossy(&bytes[..n]).to_ascii_lowercase();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            assert!(
                headers.contains("content-length: 0\r\n"),
                "Empty POST must declare its zero-length body: {headers}"
            );
        });
        let mut request = local_request(&url);
        request.method = HttpMethod::Post;
        transport()
            .execute(request, &CancellationToken::new())
            .await
            .unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn interrupted_submission_is_one_attempt_and_never_returns_partial_content() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/private-account?q=private-message",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let size = socket.read(&mut bytes).await.unwrap();
            assert!(bytes[..size].starts_with(b"POST "));
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nprivate-partial")
                .await
                .unwrap();
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut request = local_request(&url);
        request.method = HttpMethod::Post;
        request = request
            .body(b"private-authored-MIME".to_vec(), "message/rfc822")
            .unwrap();
        let error = transport()
            .execute(request, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error, TransportError::Network);
        assert!(!format!("{error:?} {error}").contains("private"));
        server.await.unwrap();
    }
}
