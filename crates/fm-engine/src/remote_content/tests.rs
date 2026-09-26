use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls;

struct Pki {
    root: reqwest::Certificate,
    server: Arc<rustls::ServerConfig>,
}

/// A test CA and one server certificate for every test host name.
fn pki() -> Pki {
    let ca_key = rcgen::KeyPair::generate().unwrap();
    let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::DigitalSignature,
    ];
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let issuer = rcgen::Issuer::new(ca_params, ca_key);
    let key = rcgen::KeyPair::generate().unwrap();
    let names = ["localhost", "origin.test", "blocked.test"]
        .map(String::from)
        .to_vec();
    let cert = rcgen::CertificateParams::new(names)
        .unwrap()
        .signed_by(&key, &issuer)
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()),
        )
        .unwrap();
    Pki {
        root: reqwest::Certificate::from_der(ca.der()).unwrap(),
        server: Arc::new(server),
    }
}

fn png() -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEUlEQVR4nGP4z8DA8B+MgBgAHfAD/dPQfSYAAAAASUVORK5CYII=")
        .unwrap()
}

async fn head<S: AsyncReadExt + Unpin>(stream: &mut S) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 16 * 1024 {
        if stream.read(&mut byte).await.unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

struct Origin {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
}

/// An https origin on loopback with a few fixed routes.
async fn origin(pki: &Pki) -> Origin {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let acceptor = tokio_rustls::TlsAcceptor::from(pki.server.clone());
    let seen = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let request = head(&mut tls).await;
                seen.lock().unwrap().push(request.clone());
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, extra, body): (&str, String, Vec<u8>) = match path.as_str() {
                    "/img.png" => ("200 OK", String::new(), png()),
                    "/big" => ("200 OK", String::new(), vec![0u8; MAX_IMAGE_BYTES + 1024]),
                    "/redirect-http" => (
                        "302 Found",
                        format!("Location: http://localhost:{port}/img.png\r\n"),
                        vec![],
                    ),
                    "/redirect-loop" => {
                        ("302 Found", "Location: /redirect-loop\r\n".into(), vec![])
                    }
                    "/redirect-blocked" => (
                        "302 Found",
                        "Location: https://blocked.test/img.png\r\n".into(),
                        vec![],
                    ),
                    _ => ("404 Not Found", String::new(), vec![]),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.write_all(&body).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    Origin { port, requests }
}

struct Proxy {
    port: u16,
    connects: Arc<Mutex<Vec<String>>>,
}

/// An HTTPS forward proxy on loopback that requires basic authentication and
/// maps every CONNECT target to the test origin.
async fn proxy(pki: &Pki, origin_port: u16, user: &str, password: &str) -> Proxy {
    use base64::Engine as _;
    let expected = format!(
        "proxy-authorization: basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    )
    .to_ascii_lowercase();
    forward_proxy(pki, origin_port, Some(expected)).await
}

/// The same proxy without credentials; one that sends any is refused.
async fn open_proxy(pki: &Pki, origin_port: u16) -> Proxy {
    forward_proxy(pki, origin_port, None).await
}

async fn forward_proxy(pki: &Pki, origin_port: u16, expected: Option<String>) -> Proxy {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connects = Arc::new(Mutex::new(Vec::new()));
    let acceptor = tokio_rustls::TlsAcceptor::from(pki.server.clone());
    let seen = connects.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let seen = seen.clone();
            let expected = expected.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let request = head(&mut tls).await;
                let target = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                seen.lock().unwrap().push(target);
                let lower = request.to_ascii_lowercase();
                let allowed = match &expected {
                    Some(expected) => lower.contains(expected),
                    None => !lower.contains("proxy-authorization:"),
                };
                if !allowed {
                    let _ = tls
                        .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let Ok(mut upstream) = TcpStream::connect(("127.0.0.1", origin_port)).await else {
                    return;
                };
                let _ = tls
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await;
                let _ = tokio::io::copy_bidirectional(&mut tls, &mut upstream).await;
            });
        }
    });
    Proxy { port, connects }
}

fn runtime(pki: &Pki) -> Runtime {
    let temp = std::env::temp_dir();
    let (queue, _) = tokio::sync::mpsc::channel(1);
    Runtime::with_policy(
        CoreBridge::new(queue, Arc::new(Semaphore::new(1))),
        &temp,
        None,
        Policy {
            allow_loopback: true,
            roots: vec![pki.root.clone()],
            ..Default::default()
        },
    )
}

fn reason(result: std::result::Result<Vec<u8>, Failure>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(Failure::Failed(r)) => format!("failed:{r}"),
        Err(Failure::Proxy(r)) => format!("proxy:{r}"),
        Err(Failure::Cancelled) => "cancelled".into(),
    }
}

#[test]
fn only_public_https_targets_are_allowed() {
    let url = |s: &str| reqwest::Url::parse(s).unwrap();
    let blocked = vec!["*.tracker.io".to_owned()];
    assert_eq!(
        check_url(&url("https://img.example.com/a.png"), &blocked, false),
        Ok(())
    );
    for (target, reason) in [
        ("http://img.example.com/a.png", "insecure"),
        ("https://img.example.com:8443/a.png", "refused"),
        ("https://user:pw@img.example.com/a.png", "refused"),
        ("https://localhost/a.png", "refused"),
        ("https://printer/a.png", "refused"),
        ("https://nas.local/a.png", "refused"),
        ("https://router.home.arpa/a.png", "refused"),
        ("https://127.0.0.1/a.png", "refused"),
        ("https://10.0.0.5/a.png", "refused"),
        ("https://192.168.1.1/a.png", "refused"),
        ("https://169.254.169.254/latest/meta-data", "refused"),
        ("https://100.64.0.1/a.png", "refused"),
        ("https://[::1]/a.png", "refused"),
        ("https://[fd00::1]/a.png", "refused"),
        ("https://[fe80::1]/a.png", "refused"),
        ("https://[::ffff:192.168.0.1]/a.png", "refused"),
        ("https://[64:ff9b::a00:1]/a.png", "refused"),
        ("https://ads.tracker.io/p.gif", "blocked"),
        ("https://tracker.io/p.gif", "blocked"),
    ] {
        assert_eq!(
            check_url(&url(target), &blocked, false),
            Err(reason),
            "{target}"
        );
    }
    assert!(allowed_ip("93.184.216.34".parse().unwrap(), false));
    assert!(allowed_ip(
        "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap(),
        false
    ));
    assert!(!allowed_ip("0.0.0.0".parse().unwrap(), false));
    assert!(!allowed_ip("255.255.255.255".parse().unwrap(), false));
    assert!(!allowed_ip("224.0.0.1".parse().unwrap(), false));
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_fetches_send_only_fixed_headers_and_enforce_limits() {
    let pki = pki();
    let origin = origin(&pki).await;
    let runtime = runtime(&pki);
    let client = build_client(None, runtime.blocked.clone(), &runtime.policy).unwrap();
    let base = format!("https://localhost:{}", origin.port);
    let cancel = CancellationToken::new();
    let bytes = runtime
        .fetch(&client, &format!("{base}/img.png"), &cancel, false)
        .await
        .ok()
        .unwrap();
    assert_eq!(bytes, png());
    let request = origin.requests.lock().unwrap()[0].to_ascii_lowercase();
    assert!(
        request.starts_with("get /img.png http/1.1\r\n"),
        "{request}"
    );
    assert!(request.contains("user-agent: mozilla/5.0\r\n"), "{request}");
    assert!(
        request.contains(&format!("accept: {ACCEPT}\r\n")),
        "{request}"
    );
    for header in [
        "cookie:",
        "referer:",
        "accept-language:",
        "if-none-match:",
        "if-modified-since:",
        "authorization:",
        "accept-encoding:",
    ] {
        assert!(!request.contains(header), "{header} sent: {request}");
    }
    assert_eq!(
        reason(
            runtime
                .fetch(&client, &format!("{base}/big"), &cancel, false)
                .await
        ),
        "failed:too_large"
    );
    assert_eq!(
        reason(
            runtime
                .fetch(&client, &format!("{base}/missing"), &cancel, false)
                .await
        ),
        "failed:unavailable"
    );
    assert_eq!(
        reason(
            runtime
                .fetch(&client, &format!("{base}/redirect-http"), &cancel, false)
                .await
        ),
        "failed:refused"
    );
    assert_eq!(
        reason(
            runtime
                .fetch(&client, &format!("{base}/redirect-loop"), &cancel, false)
                .await
        ),
        "failed:refused"
    );
    *runtime.blocked.write().unwrap() = vec!["blocked.test".into()];
    let before = origin.requests.lock().unwrap().len();
    assert_eq!(
        reason(
            runtime
                .fetch(&client, &format!("{base}/redirect-blocked"), &cancel, false)
                .await
        ),
        "failed:blocked"
    );
    assert_eq!(
        origin.requests.lock().unwrap().len(),
        before + 1,
        "the blocked host is never contacted"
    );
    assert_eq!(
        reason(
            runtime
                .fetch(
                    &client,
                    &format!("http://localhost:{}/img.png", origin.port),
                    &cancel,
                    false
                )
                .await
        ),
        "failed:insecure"
    );
    let stopped = CancellationToken::new();
    stopped.cancel();
    assert_eq!(
        reason(
            runtime
                .fetch(&client, &format!("{base}/img.png"), &stopped, false)
                .await
        ),
        "cancelled"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn proxy_mode_tunnels_every_request_and_never_goes_direct() {
    let pki = pki();
    let origin = origin(&pki).await;
    let proxy = proxy(&pki, origin.port, "user", "secret pass").await;
    let runtime = runtime(&pki);
    let settings = ProxySettings {
        host: "localhost".into(),
        port: proxy.port,
        username: "user".into(),
    };
    let cancel = CancellationToken::new();
    // `origin.test` has no DNS record: success proves it was resolved only at the proxy.
    let client = build_client(
        Some((&settings, Some("secret pass"))),
        runtime.blocked.clone(),
        &runtime.policy,
    )
    .unwrap();
    let bytes = runtime
        .fetch(&client, "https://origin.test/img.png", &cancel, true)
        .await
        .ok()
        .unwrap();
    assert_eq!(bytes, png());
    assert_eq!(
        proxy.connects.lock().unwrap().as_slice(),
        ["origin.test:443"]
    );
    assert_eq!(origin.requests.lock().unwrap().len(), 1);

    let wrong = build_client(
        Some((&settings, Some("wrong"))),
        runtime.blocked.clone(),
        &runtime.policy,
    )
    .unwrap();
    assert_eq!(
        reason(
            runtime
                .fetch(&wrong, "https://origin.test/img.png", &cancel, true)
                .await
        ),
        "proxy:auth"
    );
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let down = ProxySettings {
        port: closed.local_addr().unwrap().port(),
        ..settings.clone()
    };
    drop(closed);
    let unreachable = build_client(
        Some((&down, Some("secret pass"))),
        runtime.blocked.clone(),
        &runtime.policy,
    )
    .unwrap();
    assert_eq!(
        reason(
            runtime
                .fetch(&unreachable, "https://origin.test/img.png", &cancel, true)
                .await
        ),
        "proxy:connect"
    );
    assert_eq!(
        origin.requests.lock().unwrap().len(),
        1,
        "no request reached the origin without the proxy"
    );
    // A proxy with no username or password is asked without credentials.
    let open = open_proxy(&pki, origin.port).await;
    let anonymous = ProxySettings {
        host: "localhost".into(),
        port: open.port,
        username: String::new(),
    };
    let client = build_client(
        Some((&anonymous, None)),
        runtime.blocked.clone(),
        &runtime.policy,
    )
    .unwrap();
    assert_eq!(
        runtime
            .fetch(&client, "https://origin.test/img.png", &cancel, true)
            .await
            .ok()
            .unwrap(),
        png()
    );
    assert_eq!(
        open.connects.lock().unwrap().as_slice(),
        ["origin.test:443"]
    );
}

/// A grey PNG of any size, written by hand with stored deflate blocks.
fn png_sized(width: u32, height: u32) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = !0u32;
        for byte in bytes {
            c ^= u32::from(*byte);
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::new();
    for _ in 0..height {
        raw.push(0);
        raw.resize(raw.len() + width as usize, 0x80);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend(len.to_le_bytes());
        zlib.extend((!len).to_le_bytes());
        zlib.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    let mut header = width.to_be_bytes().to_vec();
    header.extend(height.to_be_bytes());
    header.extend([8, 0, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

const PHOTO_ADDRESS: &str = "person@example.org";

/// Brand icons and photos the way Google and Gravatar answer them.
fn picture_route(path: &str) -> (&'static str, Vec<u8>) {
    if let Some(query) = path.strip_prefix("/s2/favicons?") {
        let domain = query
            .strip_prefix("domain=")
            .and_then(|q| q.strip_suffix("&sz=64"))
            .unwrap_or_default();
        return match domain {
            "deel.com" | "osler.co.uk" => ("200 OK", png_sized(64, 64)),
            "tiny.example" => ("200 OK", png_sized(16, 16)),
            "flaky.example" => ("503 Service Unavailable", vec![]),
            // Google's generic globe comes with a 404.
            _ => ("404 Not Found", png_sized(16, 16)),
        };
    }
    let photo = format!(
        "/avatar/{}?s=64&d=404",
        fm_core::view_cache::sha256_hex(PHOTO_ADDRESS)
    );
    if path == photo {
        return ("200 OK", png_sized(64, 64));
    }
    ("404 Not Found", vec![])
}

/// An https origin on loopback that answers with `route`.
async fn serve(pki: &Pki, route: fn(&str) -> (&'static str, Vec<u8>)) -> Origin {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let acceptor = tokio_rustls::TlsAcceptor::from(pki.server.clone());
    let seen = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let request = head(&mut tls).await;
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                seen.lock().unwrap().push(path.clone());
                let (status, body) = route(&path);
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.write_all(&body).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    Origin { port, requests }
}

struct Senders {
    profile: tempfile::TempDir,
    core: CoreBridge,
    pki: Pki,
    origin: Origin,
    runtime: Runtime,
}

impl Senders {
    async fn new() -> Self {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().to_owned();
        let (queue, mut receive) = tokio::sync::mpsc::channel(16);
        std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            while let Some(crate::Work::Provider(call)) = receive.blocking_recv() {
                call.run(&mut store);
            }
        });
        let core = CoreBridge::new(queue, Arc::new(Semaphore::new(16)));
        let pki = pki();
        let origin = serve(&pki, picture_route).await;
        let base = format!("https://localhost:{}", origin.port);
        let runtime = Self::runtime_for(&core, profile.path(), &pki, &base);
        Self {
            profile,
            core,
            pki,
            origin,
            runtime,
        }
    }

    fn runtime_for(core: &CoreBridge, profile: &std::path::Path, pki: &Pki, base: &str) -> Runtime {
        Runtime::with_policy(
            core.clone(),
            profile,
            None,
            Policy {
                allow_loopback: true,
                roots: vec![pki.root.clone()],
                brand_base: Some(base.to_owned()),
                photo_base: Some(base.to_owned()),
            },
        )
    }

    /// A new engine on the same profile: nothing remembered in memory.
    fn restarted(&self, base: Option<&str>) -> Runtime {
        let base = base
            .map(str::to_owned)
            .unwrap_or_else(|| format!("https://localhost:{}", self.origin.port));
        Self::runtime_for(&self.core, self.profile.path(), &self.pki, &base)
    }

    fn requests(&self) -> Vec<String> {
        self.origin.requests.lock().unwrap().clone()
    }

    async fn save(&self, patch: Value) {
        self.core
            .call(move |store| {
                let revision = store.handle("preferences_get", None, json!({}))?["revision"].clone();
                store.handle(
                    "preferences_save",
                    None,
                    json!({"preferences":patch,"expected_revision":revision,"request_id":format!("prefs-{revision}")}),
                )
            })
            .await
            .unwrap();
    }
}

async fn avatar(runtime: &Runtime, address: &str, kind: &str) -> Result<Value> {
    runtime
        .handle(
            CancellationToken::new(),
            "sender_avatar",
            None,
            json!({"address":address,"kind":kind}),
        )
        .await
}

fn no_picture() -> Value {
    json!({"state":"none","label":"No photo \u{b7} initials"})
}

#[test]
fn root_domains_follow_the_country_second_level_heuristic() {
    for (domain, root) in [
        ("e.deel.com", "deel.com"),
        ("deel.com", "deel.com"),
        ("mail.osler.co.uk", "osler.co.uk"),
        ("osler.co.uk", "osler.co.uk"),
        ("a.b.example.com.au", "example.com.au"),
        ("notifications.aircanada.ca", "aircanada.ca"),
        ("news.bbc.co", "bbc.co"),
        ("co.uk", "co.uk"),
        ("mail.example.org.nz", "example.org.nz"),
        ("x.shop.example.ltd", "example.ltd"),
    ] {
        assert_eq!(senders::root_domain(domain), root, "{domain}");
    }
    for personal in [
        "gmail.com",
        "googlemail.com",
        "hotmail.co.uk",
        "outlook.com",
        "live.com",
        "yahoo.com",
        "yahoo.co.jp",
        "gmx.de",
        "yandex.ru",
        "proton.me",
        "pm.me",
        "icloud.com",
        "tuta.io",
    ] {
        assert!(senders::personal(personal), "{personal}");
    }
    for company in ["deel.com", "osler.co.uk", "mail.co.uk", "gmailify.com"] {
        assert!(!senders::personal(company), "{company}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sender_avatar_rejects_invalid_requests() {
    let senders = Senders::new().await;
    let long = format!("{}@deel.com", "a".repeat(312));
    for address in [
        "",
        "   ",
        "no-at-sign",
        "a@localhost",
        "@deel.com",
        "a@@deel.com",
        "a@b@deel.com",
        "a b@deel.com",
        "a@deel..com",
        "a@.deel.com",
        "a@deel.com.",
        "a@1.2.3.4",
        "a@[::1]",
        "a@deel.com/x",
        "a@deel.com?x",
        "a\u{7}@deel.com",
        long.as_str(),
    ] {
        let error = avatar(&senders.runtime, address, "automated")
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_request", "{address:?}");
    }
    for params in [
        json!({"address":"a@deel.com"}),
        json!({"address":"a@deel.com","kind":"robot"}),
        json!({"address":"a@deel.com","kind":"people","account":"x"}),
        json!({"address":7,"kind":"people"}),
        json!(["a@deel.com"]),
    ] {
        let error = senders
            .runtime
            .handle(CancellationToken::new(), "sender_avatar", None, params)
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_request");
    }
    let normalized =
        senders::parse(&json!({"address":" Billing@E.Deel.COM ","kind":"automated"})).unwrap();
    assert_eq!(normalized.address, "billing@e.deel.com");
    assert_eq!(normalized.domain, "e.deel.com");
    assert!(senders.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn brand_icons_are_fetched_once_per_root_domain_and_cached() {
    let senders = Senders::new().await;
    let first = avatar(&senders.runtime, "billing@e.deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(first["state"], "ready", "{first}");
    assert_eq!(first["source"], "brand");
    assert_eq!(first["label"], "Brand icon \u{b7} deel.com");
    assert_eq!(
        (first["width"].as_u64(), first["height"].as_u64()),
        (Some(64), Some(64))
    );
    let path = std::path::PathBuf::from(first["path"].as_str().unwrap());
    assert!(path.is_absolute() && path.exists());
    assert!(path.starts_with(std::fs::canonicalize(senders.profile.path()).unwrap()));
    assert_eq!(senders.requests(), ["/s2/favicons?domain=deel.com&sz=64"]);
    // Every address at the root shares the icon, across restarts.
    let second = avatar(&senders.runtime, "news@mail.deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(second, first);
    let restarted = avatar(&senders.restarted(None), "deel.com@deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(restarted, first);
    assert_eq!(senders.requests().len(), 1);
    let country = avatar(&senders.runtime, "alerts@mail.osler.co.uk", "automated")
        .await
        .unwrap();
    assert_eq!(country["label"], "Brand icon \u{b7} osler.co.uk");
    assert_eq!(
        senders.requests()[1],
        "/s2/favicons?domain=osler.co.uk&sz=64"
    );
    // Concurrent requests for one new picture fetch it once.
    let (a, b) = tokio::join!(
        avatar(&senders.runtime, "a@example.com.au", "automated"),
        avatar(&senders.runtime, "b@www.example.com.au", "automated")
    );
    assert_eq!(a.unwrap(), no_picture());
    assert_eq!(b.unwrap(), no_picture());
    assert_eq!(senders.requests().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_pictures_are_remembered_on_disk_until_the_cache_is_cleared() {
    let senders = Senders::new().await;
    assert_eq!(
        avatar(&senders.runtime, "a@globe.example", "automated")
            .await
            .unwrap(),
        no_picture()
    );
    assert_eq!(senders.requests().len(), 1);
    assert_eq!(
        avatar(&senders.runtime, "b@globe.example", "automated")
            .await
            .unwrap(),
        no_picture()
    );
    assert_eq!(
        avatar(&senders.restarted(None), "a@globe.example", "automated")
            .await
            .unwrap(),
        no_picture()
    );
    assert_eq!(
        senders.requests().len(),
        1,
        "the miss is remembered on disk"
    );
    // A 16px icon is Google's placeholder, not a brand.
    assert_eq!(
        avatar(&senders.runtime, "a@tiny.example", "automated")
            .await
            .unwrap(),
        no_picture()
    );
    assert_eq!(
        avatar(&senders.restarted(None), "a@tiny.example", "automated")
            .await
            .unwrap(),
        no_picture()
    );
    assert_eq!(senders.requests().len(), 2);
    // A server failure is not remembered.
    let failed = json!({"state":"none","label":"No photo \u{b7} initials","transient":true});
    assert_eq!(
        avatar(&senders.runtime, "a@flaky.example", "automated")
            .await
            .unwrap(),
        failed
    );
    assert_eq!(
        avatar(&senders.runtime, "a@flaky.example", "automated")
            .await
            .unwrap(),
        failed
    );
    assert_eq!(senders.requests().len(), 4);
    avatar(&senders.runtime, "a@deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(senders.requests().len(), 5);
    senders
        .runtime
        .handle(
            CancellationToken::new(),
            "remote_image_cache",
            None,
            json!({"op":"clear"}),
        )
        .await
        .unwrap();
    avatar(&senders.runtime, "a@globe.example", "automated")
        .await
        .unwrap();
    let ready = avatar(&senders.runtime, "a@deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(ready["state"], "ready");
    assert_eq!(
        senders.requests().len(),
        7,
        "clearing forgets pictures and misses"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn people_get_photos_and_personal_domains_never_get_brand_icons() {
    let senders = Senders::new().await;
    let photo = avatar(&senders.runtime, " Person@Example.org", "people")
        .await
        .unwrap();
    assert_eq!(photo["state"], "ready", "{photo}");
    assert_eq!(photo["source"], "photo");
    assert_eq!(photo["label"], "Profile photo \u{b7} person@example.org");
    let hash = fm_core::view_cache::sha256_hex(PHOTO_ADDRESS);
    assert_eq!(senders.requests(), [format!("/avatar/{hash}?s=64&d=404")]);
    assert_eq!(
        avatar(&senders.runtime, PHOTO_ADDRESS, "unknown")
            .await
            .unwrap(),
        photo
    );
    assert_eq!(senders.requests().len(), 1);
    // People never get a brand icon, even at a company domain.
    assert_eq!(
        avatar(&senders.runtime, "someone@deel.com", "people")
            .await
            .unwrap(),
        no_picture()
    );
    assert!(senders.requests()[1].starts_with("/avatar/"));
    // Personal providers: a photo at most, never their brand.
    assert_eq!(
        avatar(&senders.runtime, "friend@gmail.com", "automated")
            .await
            .unwrap(),
        no_picture()
    );
    assert_eq!(senders.requests().len(), 2, "no request at all");
    for (address, kind) in [
        ("friend@gmail.com", "unknown"),
        ("pal@mail.yahoo.co.uk", "unknown"),
        ("mate@hotmail.co.uk", "people"),
    ] {
        assert_eq!(
            avatar(&senders.runtime, address, kind).await.unwrap(),
            no_picture()
        );
    }
    let requests = senders.requests();
    assert_eq!(requests.len(), 5);
    assert!(
        requests.iter().all(|r| r.starts_with("/avatar/")),
        "{requests:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_senders_fall_back_from_a_missing_photo_to_the_brand() {
    let senders = Senders::new().await;
    let found = avatar(&senders.runtime, "nobody@e.deel.com", "unknown")
        .await
        .unwrap();
    assert_eq!(found["state"], "ready");
    assert_eq!(found["source"], "brand");
    let requests = senders.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("/avatar/"));
    assert_eq!(requests[1], "/s2/favicons?domain=deel.com&sz=64");
    // Both answers are remembered.
    assert_eq!(
        avatar(&senders.restarted(None), "nobody@e.deel.com", "unknown")
            .await
            .unwrap(),
        found
    );
    assert_eq!(senders.requests().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn sender_pictures_go_through_the_proxy_and_never_direct() {
    let senders = Senders::new().await;
    let open = open_proxy(&senders.pki, senders.origin.port).await;
    senders
        .save(json!({"remote_proxy":{"enabled":true,"host":"localhost","port":open.port,"username":""}}))
        .await;
    let found = avatar(&senders.runtime, "a@deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(found["state"], "ready");
    assert_eq!(
        open.connects.lock().unwrap().as_slice(),
        [format!("localhost:{}", senders.origin.port)]
    );
    assert_eq!(senders.requests().len(), 1);
    // A proxy that is down reports so; nothing goes direct.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    senders
        .save(json!({"remote_proxy":{"enabled":true,"host":"localhost","port":port,"username":""}}))
        .await;
    let down = avatar(&senders.restarted(None), "b@osler.co.uk", "automated")
        .await
        .unwrap();
    assert_eq!(down["state"], "proxy_unavailable", "{down}");
    assert_eq!(senders.requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn blocked_disabled_and_cancelled_requests_fetch_nothing() {
    let senders = Senders::new().await;
    senders
        .save(
            json!({"blocked_hosts":[{"pattern":"blocked.test","source":"settings","added_at":1}]}),
        )
        .await;
    let blocked = senders.restarted(Some(&format!(
        "https://blocked.test:{}",
        senders.origin.port
    )));
    assert_eq!(
        avatar(&blocked, "a@deel.com", "automated").await.unwrap(),
        json!({"state":"blocked","by":"blocked.test"})
    );
    assert_eq!(
        avatar(&blocked, "a@deel.com", "unknown").await.unwrap(),
        json!({"state":"blocked","by":"blocked.test"})
    );
    assert!(senders.requests().is_empty());
    let stopped = CancellationToken::new();
    stopped.cancel();
    let error = senders
        .runtime
        .handle(
            stopped,
            "sender_avatar",
            None,
            json!({"address":"a@deel.com","kind":"automated"}),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "cancelled");
    assert!(senders.requests().is_empty());
    let ready = avatar(&senders.runtime, "a@deel.com", "automated")
        .await
        .unwrap();
    assert_eq!(ready["state"], "ready");
    assert_eq!(senders.requests().len(), 1);
    senders.save(json!({"sender_pictures":false})).await;
    let off = json!({"state":"none","label":"No photo \u{b7} initials","off":true});
    for (address, kind) in [
        ("a@deel.com", "automated"),
        ("new@other.example", "automated"),
        (PHOTO_ADDRESS, "people"),
        ("x@gmail.com", "unknown"),
    ] {
        assert_eq!(avatar(&senders.runtime, address, kind).await.unwrap(), off);
    }
    assert_eq!(senders.requests().len(), 1);
}
