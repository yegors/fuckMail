//! Bounded, cancellable Google HTTPS. This layer submits once; it never retries.
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::{
    fmt,
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};
pub use tokio_util::sync::CancellationToken;

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

/// No provider URL, header, body or error text is included in diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    InvalidRequest,
    /// Local admission refused dispatch after a newly learned provider cooldown.
    /// No HTTP request was submitted for this attempt.
    Cooldown {
        retry_after_secs: u64,
    },
    Initialization,
    Cancelled,
    Timeout,
    Network,
    RedirectDenied,
    RequestTooLarge,
    ResponseTooLarge,
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Google transport: {self:?}")
    }
}
impl std::error::Error for TransportError {}

pub struct HttpRequest {
    method: HttpMethod,
    url: reqwest::Url,
    headers: HeaderMap,
    body: Vec<u8>,
    response_limit: Option<usize>,
}
impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}
impl HttpRequest {
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
        })
    }
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
    pub fn body(mut self, bytes: Vec<u8>, content_type: &str) -> Result<Self, TransportError> {
        if self.method == HttpMethod::Get {
            return Err(TransportError::InvalidRequest);
        }
        self = self.header("content-type", content_type)?;
        self.body = bytes;
        Ok(self)
    }
    /// The caller may tighten the transport's cap, never enlarge it.
    pub fn response_limit(mut self, bytes: usize) -> Self {
        self.response_limit = Some(bytes);
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
}

fn allowed_url(url: &reqwest::Url) -> bool {
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

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after_secs: Option<u64>,
    pub etag: Option<String>,
}
impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}
pub type HttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponse, TransportError>> + Send + 'a>>;
pub trait HttpTransport: Send + Sync {
    fn execute<'a>(&'a self, request: HttpRequest, cancel: &'a CancellationToken)
        -> HttpFuture<'a>;
}

#[derive(Clone, Copy, Debug)]
pub struct TransportLimits {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub read_timeout: Duration,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}
impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(120),
            read_timeout: Duration::from_secs(30),
            max_request_bytes: 64 * 1024 * 1024,
            max_response_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    limits: TransportLimits,
}
impl ReqwestTransport {
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
            || limits.max_response_bytes > 128 * 1024 * 1024
        {
            return Err(TransportError::InvalidRequest);
        }
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
    async fn submit(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
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
        let mut builder = self
            .client
            .request(request.method.native(), request.url)
            .headers(request.headers)
            .body(request.body);
        // An empty Vec does not make reqwest emit Content-Length. Google's
        // empty-body POST endpoints still need explicit HTTP body framing.
        if empty_submission {
            builder = builder.header(reqwest::header::CONTENT_LENGTH, 0);
        }
        let mut response = builder.send().await.map_err(map_error)?;
        if response.status().is_redirection()
            && response.status() != reqwest::StatusCode::NOT_MODIFIED
        {
            return Err(TransportError::RedirectDenied);
        }
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
        let mut body = Vec::new();
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
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(TransportError::Cancelled),
                result = tokio::time::timeout(self.limits.request_timeout, self.submit(request)) =>
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

    /// Explicit public-metadata probe; ordinary tests/CI remain network-free.
    /// This is native TLS-root evidence, not OAuth or mailbox interoperability.
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
