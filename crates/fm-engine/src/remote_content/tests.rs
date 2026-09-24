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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connects = Arc::new(Mutex::new(Vec::new()));
    let acceptor = tokio_rustls::TlsAcceptor::from(pki.server.clone());
    let expected = format!(
        "proxy-authorization: basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    )
    .to_ascii_lowercase();
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
                if !request.to_ascii_lowercase().contains(&expected) {
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
        Some((&settings, "secret pass")),
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
        Some((&settings, "wrong")),
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
        Some((&down, "secret pass")),
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
}
