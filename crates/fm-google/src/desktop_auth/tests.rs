use super::*;
use crate::auth::{TokenCoordinator, GOOGLE_SCOPES};
use crate::transport::{HttpFuture, HttpResponse, TransportError};
use crate::vault::NativeCredentialVault;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicUsize, Ordering},
};

const CLIENT: &str = "synthetic-client.apps.googleusercontent.com";
const NOW: u64 = 1_800_000_000;
const JWKS: &[u8] = include_bytes!("../../tests/fixtures/auth-v1/jwks.json");
const KEY: &[u8] = include_bytes!("../../tests/fixtures/auth-v1/synthetic-rsa-private.der");

fn claims(nonce: &str) -> Value {
    json!({"iss":"https://accounts.google.com","aud":CLIENT,"sub":"synthetic-subject-001",
        "iat":NOW-10,"exp":NOW+600,"nonce":nonce,"email":"fixture@example.invalid","email_verified":true})
}
fn signed(value: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("synthetic-rsa-v1".into());
    encode(&header, value, &EncodingKey::from_rsa_der(KEY)).unwrap()
}

#[test]
fn signed_identity_checks_signature_claims_nonce_and_expected_subject() {
    let nonce = CsrfToken::new("fixture-nonce".into());
    let original = claims(nonce.secret());
    let identity = verify_id_token(
        &signed(&original),
        JWKS,
        CLIENT,
        Some(&nonce),
        None,
        "fixture-access",
        NOW,
    )
    .unwrap();
    assert_eq!(identity.sub, "synthetic-subject-001");
    assert!(identity.email_verified);
    for (field, wrong) in [
        ("iss", json!("https://not-google.invalid")),
        ("aud", json!("another-client")),
        ("exp", json!(NOW)),
        ("iat", json!(NOW + 61)),
        ("nonce", json!("wrong")),
        ("sub", json!("")),
        ("email_verified", json!(false)),
        ("azp", json!("another-client")),
        ("aud", json!([CLIENT, "another-client"])),
        ("at_hash", json!("mismatched-access-token")),
    ] {
        let mut changed = original.clone();
        changed[field] = wrong;
        assert!(
            verify_id_token(
                &signed(&changed),
                JWKS,
                CLIENT,
                Some(&nonce),
                None,
                "fixture-access",
                NOW
            )
            .is_err(),
            "field {field}"
        );
    }
    for field in [
        "iss",
        "aud",
        "exp",
        "iat",
        "sub",
        "nonce",
        "email",
        "email_verified",
    ] {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            verify_id_token(
                &signed(&changed),
                JWKS,
                CLIENT,
                Some(&nonce),
                None,
                "fixture-access",
                NOW
            )
            .is_err(),
            "missing {field}"
        );
    }
    assert!(verify_id_token(
        &signed(&original),
        JWKS,
        CLIENT,
        Some(&nonce),
        Some("another-subject"),
        "fixture-access",
        NOW
    )
    .is_err());
    let mut damaged = signed(&original).into_bytes();
    let middle = damaged.len() - 20;
    damaged[middle] = if damaged[middle] == b'A' { b'B' } else { b'A' };
    assert!(verify_id_token(
        std::str::from_utf8(&damaged).unwrap(),
        JWKS,
        CLIENT,
        Some(&nonce),
        None,
        "fixture-access",
        NOW
    )
    .is_err());
    let hmac = encode(
        &Header::new(Algorithm::HS256),
        &original,
        &EncodingKey::from_secret(b"synthetic-only"),
    )
    .unwrap();
    assert!(verify_id_token(
        &hmac,
        JWKS,
        CLIENT,
        Some(&nonce),
        None,
        "fixture-access",
        NOW
    )
    .is_err());
    let mut bad_jwks: Value = serde_json::from_slice(JWKS).unwrap();
    bad_jwks["keys"][0]["kid"] = json!("unknown-key");
    assert!(verify_id_token(
        &signed(&original),
        &serde_json::to_vec(&bad_jwks).unwrap(),
        CLIENT,
        Some(&nonce),
        None,
        "fixture-access",
        NOW
    )
    .is_err());
}

#[test]
fn identity_accepts_documented_issuer_and_explicit_authorized_party() {
    let nonce = CsrfToken::new("nonce".into());
    let mut value = claims(nonce.secret());
    value["iss"] = json!("accounts.google.com");
    value["aud"] = json!([CLIENT, "other-audience"]);
    value["azp"] = json!(CLIENT);
    value["at_hash"] = json!(URL_SAFE_NO_PAD.encode(&Sha256::digest(b"fixture-access")[..16]));
    value["hd"] = json!("workspace.example.invalid");
    let identity = verify_id_token(
        &signed(&value),
        JWKS,
        CLIENT,
        Some(&nonce),
        None,
        "fixture-access",
        NOW,
    )
    .unwrap();
    assert_eq!(
        identity.hosted_domain.as_deref(),
        Some("workspace.example.invalid")
    );
}

#[test]
fn callback_parser_rejects_wrong_target_host_duplicates_and_injection() {
    let good = "GET /oauth2/callback?code=synthetic-code&state=synthetic-state HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n";
    assert_eq!(
        parse_callback(good.as_bytes(), "127.0.0.1:12345")
            .unwrap()
            .code
            .as_deref(),
        Some("synthetic-code")
    );
    for wrong in [
        good.replace("GET", "POST"),
        good.replace("/oauth2/callback?", "/other?"),
        good.replace("Host: 127.0.0.1:12345", "Host: rebound.invalid:12345"),
        good.replace("&state=", "&state=duplicate&state="),
        good.replace("&state=", "&error=denied&state="),
        good.replace("synthetic-code", "%0d%0ainjected"),
        good.replace("\r\n\r\n", "\r\nContent-Length: 5\r\n\r\n"),
    ] {
        assert!(parse_callback(wrong.as_bytes(), "127.0.0.1:12345").is_err());
    }
}

async fn callback_request(redirect: &str, query: &str) {
    let url = Url::parse(redirect).unwrap();
    let host = format!("127.0.0.1:{}", url.port().unwrap());
    let mut socket = TcpStream::connect(&host).await.unwrap();
    let request = format!(
        "GET {}?{query} HTTP/1.1\r\nHost: {host}\r\n\r\n",
        url.path()
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).await.unwrap();
    assert!(!String::from_utf8_lossy(&response).contains(query));
}

#[tokio::test]
async fn bound_login_has_full_scopes_and_accepts_only_matching_one_use_callback() {
    let login = DesktopLogin::begin(CLIENT.into(), Duration::from_secs(5))
        .await
        .unwrap();
    let url = Url::parse(login.authorization_url()).unwrap();
    let fields: BTreeMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    for scope in GOOGLE_SCOPES {
        assert!(fields["scope"].split(' ').any(|v| v == scope));
    }
    assert_eq!(fields["code_challenge_method"], "S256");
    assert_eq!(fields["prompt"], "select_account");
    assert!(fields.contains_key("nonce"));
    let redirect = login.redirect_uri().to_owned();
    let cancel = CancellationToken::new();
    let waiter = tokio::spawn(async move { login.wait_for_callback(&cancel).await });
    callback_request(&redirect, "code=unrelated&state=wrong-state").await;
    callback_request(
        &redirect,
        &format!("code=synthetic-accepted&state={}", fields["state"]),
    )
    .await;
    let code = waiter.await.unwrap().unwrap();
    assert_eq!(code.code, "synthetic-accepted");
    assert_eq!(
        oauth2::PkceCodeChallenge::from_code_verifier_sha256(&code.verifier).as_str(),
        fields["code_challenge"]
    );
    assert_eq!(code.nonce.secret(), &fields["nonce"]);
    assert!(TcpStream::connect(
        Url::parse(&redirect)
            .unwrap()
            .socket_addrs(|| None)
            .unwrap()[0]
    )
    .await
    .is_err());
}

#[tokio::test]
async fn denial_timeout_and_cancel_close_the_listener() {
    let recovery = DesktopLogin::begin_reauthorization(CLIENT.into(), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(Url::parse(recovery.authorization_url())
        .unwrap()
        .query_pairs()
        .any(|(key, value)| key == "prompt" && value == "consent select_account"));
    drop(recovery);
    let login = DesktopLogin::begin(CLIENT.into(), Duration::from_secs(5))
        .await
        .unwrap();
    let url = Url::parse(login.authorization_url()).unwrap();
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let redirect = login.redirect_uri().to_owned();
    let waiter =
        tokio::spawn(async move { login.wait_for_callback(&CancellationToken::new()).await });
    callback_request(&redirect, &format!("error=access_denied&state={state}")).await;
    assert!(matches!(
        waiter.await.unwrap(),
        Err(GoogleError::PermissionDenied)
    ));
    let login = DesktopLogin::begin(CLIENT.into(), Duration::from_millis(10))
        .await
        .unwrap();
    assert!(matches!(
        login.wait_for_callback(&CancellationToken::new()).await,
        Err(GoogleError::TransportInterrupted)
    ));
    let login = DesktopLogin::begin(CLIENT.into(), Duration::from_secs(5))
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        login.wait_for_callback(&cancel).await,
        Err(GoogleError::TransportInterrupted)
    ));
}

struct FakeHttp {
    replies: Mutex<VecDeque<HttpResponse>>,
    requests: Mutex<Vec<HttpRequest>>,
}
impl FakeHttp {
    fn new(values: Vec<Value>) -> Self {
        Self {
            replies: Mutex::new(
                values
                    .into_iter()
                    .map(|value| HttpResponse {
                        status: 200,
                        body: serde_json::to_vec(&value).unwrap(),
                        retry_after_secs: None,
                        etag: None,
                    })
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        }
    }
}
impl HttpTransport for FakeHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            self.requests.lock().unwrap().push(request);
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(TransportError::Network)
        })
    }
}
#[derive(Default)]
struct MemoryVault {
    tokens: Mutex<BTreeMap<AccountId, StoredToken>>,
    fail: bool,
}
impl CredentialVault for MemoryVault {
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        if self.fail {
            return Err(GoogleError::StoreUnavailable);
        }
        Ok(self.tokens.lock().unwrap().get(account).cloned())
    }
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        if self.fail {
            return Err(GoogleError::StoreUnavailable);
        }
        self.tokens.lock().unwrap().insert(account.clone(), token);
        Ok(())
    }
}

#[tokio::test]
async fn exchange_checks_identity_reports_partial_grants_and_persists_only_explicitly() {
    let authorization = DesktopAuthorization::new(CLIENT.into(), 12345).unwrap();
    let fields: BTreeMap<String, String> = Url::parse(&authorization.url)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let response = json!({"access_token":"fixture-access","refresh_token":"fixture-refresh","expires_in":3600,"token_type":"Bearer",
        "scope":"openid https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/calendar.events.readonly",
        "id_token":signed(&claims(&fields["nonce"]))});
    let http = Arc::new(FakeHttp::new(vec![
        response,
        serde_json::from_slice(JWKS).unwrap(),
    ]));
    let oauth = GoogleOAuth::new(
        OAuthClientConfig::new(CLIENT.into(), Some("synthetic-desktop-secret".into())).unwrap(),
        http.clone(),
    );
    let grant = oauth
        .exchange(
            authorization
                .accept_callback(&fields["state"], "synthetic-code".into())
                .unwrap(),
            NOW,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(grant.granted_scopes.contains("email"));
    assert!(!grant
        .granted_scopes
        .contains("https://www.googleapis.com/auth/gmail.modify"));
    assert_eq!(grant.identity.sub, "synthetic-subject-001");
    let vault = Arc::new(MemoryVault::default());
    let account = AccountId::new("local-account").unwrap();
    assert!(vault.load(&account).unwrap().is_none());
    grant.persist(&account, vault.clone()).await.unwrap();
    assert_eq!(
        vault.load(&account).unwrap().unwrap().refresh_token,
        "fixture-refresh"
    );
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests[0].url(), TOKEN_URL);
    assert_eq!(requests[1].url(), JWKS_URL);
    let form: BTreeMap<String, String> = form_urlencoded::parse(requests[0].body_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(form["code"], "synthetic-code");
    assert_eq!(
        form["redirect_uri"],
        "http://127.0.0.1:12345/oauth2/callback"
    );
    assert!(!format!("{:?}", requests[0]).contains("synthetic-desktop-secret"));
}

#[tokio::test]
async fn refused_grants_and_vault_write_failures_never_publish_tokens() {
    for defect in [
        "scope",
        "id_token",
        "refresh_token",
        "token_type",
        "expires_in",
    ] {
        let authorization = DesktopAuthorization::new(CLIENT.into(), 12345).unwrap();
        let fields: BTreeMap<String, String> = Url::parse(&authorization.url)
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let mut response = json!({"access_token":"fixture-access","refresh_token":"fixture-refresh","expires_in":3600,
            "token_type":"Bearer","scope":"openid email","id_token":signed(&claims(&fields["nonce"]))});
        match defect {
            "scope" => response["scope"] = json!("openid"),
            "token_type" => response["token_type"] = json!("Other"),
            "expires_in" => response["expires_in"] = json!(60),
            field => {
                response.as_object_mut().unwrap().remove(field);
            }
        }
        let http = Arc::new(FakeHttp::new(vec![
            response,
            serde_json::from_slice(JWKS).unwrap(),
        ]));
        let oauth = GoogleOAuth::new(OAuthClientConfig::new(CLIENT.into(), None).unwrap(), http);
        let code = authorization
            .accept_callback(&fields["state"], "fixture-code".into())
            .unwrap();
        assert!(
            oauth
                .exchange(code, NOW, &CancellationToken::new())
                .await
                .is_err(),
            "defect {defect}"
        );
    }
    let grant = VerifiedGrant {
        identity: GoogleIdentity {
            sub: "fixture-sub".into(),
            email: "fixture@example.invalid".into(),
            email_verified: true,
            hosted_domain: None,
        },
        granted_scopes: BTreeSet::new(),
        token: StoredToken {
            access_token: "fixture-access".into(),
            refresh_token: "fixture-refresh".into(),
            expires_at_secs: NOW + 3600,
        },
    };
    let vault = Arc::new(MemoryVault {
        fail: true,
        ..Default::default()
    });
    assert_eq!(
        grant
            .persist(&AccountId::new("fixture-local").unwrap(), vault.clone())
            .await,
        Err(GoogleError::StoreUnavailable)
    );
    assert!(vault.tokens.lock().unwrap().is_empty());
}

#[tokio::test]
async fn refresh_requires_bound_subject_and_rejects_changed_identity() {
    let http = Arc::new(FakeHttp::new(vec![
        json!({"access_token":"fixture-access","expires_in":3600,"token_type":"Bearer",
        "id_token":signed(&claims("unused-refresh-nonce"))}),
        serde_json::from_slice(JWKS).unwrap(),
    ]));
    let oauth = GoogleOAuth::new(
        OAuthClientConfig::new(CLIENT.into(), None).unwrap(),
        http.clone(),
    );
    let account = AccountId::new("local").unwrap();
    let cancel = CancellationToken::new();
    assert!(matches!(
        oauth
            .refresh_async(&account, "fixture-refresh", NOW, &cancel)
            .await,
        Err(GoogleError::PermissionDenied)
    ));
    assert!(http.requests.lock().unwrap().is_empty());
    oauth
        .bind_account(account.clone(), "different-subject".into())
        .unwrap();
    assert!(matches!(
        oauth
            .refresh_async(&account, "fixture-refresh", NOW, &cancel)
            .await,
        Err(GoogleError::PermissionDenied)
    ));
}

struct AsyncRefresh {
    calls: AtomicUsize,
}
impl AsyncRefreshTransport for AsyncRefresh {
    fn refresh_async<'a>(
        &'a self,
        _: &'a AccountId,
        _: &'a str,
        now: u64,
        _: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshResult, GoogleError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(RefreshResult {
                access_token: "renewed-fixture-access".into(),
                rotated_refresh_token: Some("rotated-fixture-refresh".into()),
                expires_at_secs: now + 3600,
            })
        })
    }
}
#[tokio::test]
async fn eight_async_consumers_share_one_durable_refresh_and_locked_vault_fails_closed() {
    let account = AccountId::new("local").unwrap();
    let vault = Arc::new(MemoryVault::default());
    vault
        .store(
            &account,
            StoredToken {
                access_token: "old-fixture".into(),
                refresh_token: "fixture-refresh".into(),
                expires_at_secs: NOW - 1,
            },
        )
        .unwrap();
    let coordinator = Arc::new(TokenCoordinator::default());
    let transport = Arc::new(AsyncRefresh {
        calls: AtomicUsize::new(0),
    });
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let account = account.clone();
        let vault = vault.clone();
        let coordinator = coordinator.clone();
        let transport = transport.clone();
        tasks.push(tokio::spawn(async move {
            coordinator
                .access_token_async(
                    &account,
                    NOW,
                    vault,
                    transport.as_ref(),
                    &CancellationToken::new(),
                )
                .await
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap(), "renewed-fixture-access");
    }
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        vault.load(&account).unwrap().unwrap().refresh_token,
        "rotated-fixture-refresh"
    );
    let locked = Arc::new(MemoryVault {
        fail: true,
        ..Default::default()
    });
    assert_eq!(
        coordinator
            .access_token_async(
                &account,
                NOW,
                locked,
                transport.as_ref(),
                &CancellationToken::new()
            )
            .await,
        Err(GoogleError::StoreUnavailable)
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
}

struct BlockingStoreVault {
    token: Mutex<StoredToken>,
    loads: AtomicUsize,
    stores: AtomicUsize,
    entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl CredentialVault for BlockingStoreVault {
    fn load(&self, _: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(self.token.lock().unwrap().clone()))
    }
    fn store(&self, _: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        if self.stores.fetch_add(1, Ordering::SeqCst) == 0 {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                let _ = entered.send(());
            }
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| GoogleError::StoreUnavailable)?;
        }
        *self.token.lock().unwrap() = token;
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_waiter_keeps_refresh_serialized_until_blocking_vault_store_finishes() {
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    // Unblock the native-call fake on a failing assertion too, so a failure cannot
    // leave the runtime waiting forever for its detached blocking task.
    struct ReleaseOnDrop(Option<std::sync::mpsc::Sender<()>>);
    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    let release = ReleaseOnDrop(Some(release_tx));
    let account = AccountId::new("abort-fixture-account").unwrap();
    let vault = Arc::new(BlockingStoreVault {
        token: Mutex::new(StoredToken {
            access_token: "old-fixture-access".into(),
            refresh_token: "old-fixture-refresh".into(),
            expires_at_secs: NOW - 1,
        }),
        loads: AtomicUsize::new(0),
        stores: AtomicUsize::new(0),
        entered: Mutex::new(Some(entered_tx)),
        release: Mutex::new(release_rx),
    });
    let coordinator = Arc::new(TokenCoordinator::default());
    let transport = Arc::new(AsyncRefresh {
        calls: AtomicUsize::new(0),
    });
    let consumer = || {
        let (account, vault, coordinator, transport) = (
            account.clone(),
            vault.clone(),
            coordinator.clone(),
            transport.clone(),
        );
        tokio::spawn(async move {
            coordinator
                .access_token_async(
                    &account,
                    NOW,
                    vault,
                    transport.as_ref(),
                    &CancellationToken::new(),
                )
                .await
        })
    };
    let first = consumer();
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .unwrap()
        .unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    let mut second = consumer();
    let early_result = tokio::time::timeout(Duration::from_millis(100), &mut second).await;
    let loads_before_release = vault.loads.load(Ordering::SeqCst);
    let refreshes_before_release = transport.calls.load(Ordering::SeqCst);
    drop(release);
    assert!(
        early_result.is_err(),
        "the second consumer must wait for durable rotation"
    );
    assert_eq!(loads_before_release, 1);
    assert_eq!(refreshes_before_release, 1);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "renewed-fixture-access"
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(vault.stores.load(Ordering::SeqCst), 1);
    assert_eq!(vault.loads.load(Ordering::SeqCst), 2);
    assert_eq!(
        vault.load(&account).unwrap().unwrap().refresh_token,
        "rotated-fixture-refresh"
    );
}

#[test]
#[ignore = "explicit isolated native-vault exercise; never part of credential-free CI"]
fn native_vault_synthetic_roundtrip_and_cleanup() {
    let namespace = format!("synthetic-test-{}", CsrfToken::new_random().secret());
    let account = AccountId::new("synthetic-vault-test").unwrap();
    let vault = NativeCredentialVault::new(&namespace).unwrap();
    assert!(
        vault.load(&account).unwrap().is_none(),
        "refuse to overwrite any existing entry"
    );
    struct Cleanup<'a> {
        vault: &'a NativeCredentialVault,
        account: &'a AccountId,
    }
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self.vault.remove(self.account);
        }
    }
    let _cleanup = Cleanup {
        vault: &vault,
        account: &account,
    };
    vault
        .store(
            &account,
            StoredToken {
                access_token: "synthetic-native-access-only".into(),
                refresh_token: "synthetic-native-refresh-only".into(),
                expires_at_secs: NOW,
            },
        )
        .unwrap();
    let reopened = NativeCredentialVault::new(&namespace).unwrap();
    let loaded = reopened.load(&account).unwrap().unwrap();
    assert!(
        loaded.access_token == "synthetic-native-access-only"
            && loaded.refresh_token == "synthetic-native-refresh-only"
    );
    let another = NativeCredentialVault::new(&format!("other-{namespace}")).unwrap();
    assert!(another.load(&account).unwrap().is_none());
    vault.remove(&account).unwrap();
    assert!(reopened.load(&account).unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verified_reconnect_and_refresh_share_durable_finalization_after_caller_abort() {
    fn grant() -> VerifiedGrant {
        VerifiedGrant {
            identity: GoogleIdentity {
                sub: "synthetic-subject".into(),
                email: "fixture@example.invalid".into(),
                email_verified: true,
                hosted_domain: None,
            },
            granted_scopes: GOOGLE_SCOPES.iter().map(|s| s.to_string()).collect(),
            token: StoredToken {
                access_token: "reconnected-fixture-access".into(),
                refresh_token: "reconnected-fixture-refresh".into(),
                expires_at_secs: NOW + 600,
            },
        }
    }
    for login_first in [false, true] {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        struct ReleaseOnDrop(Option<std::sync::mpsc::Sender<()>>);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                if let Some(send) = self.0.take() {
                    let _ = send.send(());
                }
            }
        }
        let release = ReleaseOnDrop(Some(release_tx));
        let account = AccountId::new("reconnect-race-fixture").unwrap();
        let vault = Arc::new(BlockingStoreVault {
            token: Mutex::new(StoredToken {
                access_token: "old-fixture-access".into(),
                refresh_token: "old-fixture-refresh".into(),
                expires_at_secs: NOW - 1,
            }),
            loads: AtomicUsize::new(0),
            stores: AtomicUsize::new(0),
            entered: Mutex::new(Some(entered_tx)),
            release: Mutex::new(release_rx),
        });
        let coordinator = Arc::new(TokenCoordinator::default());
        let transport = Arc::new(AsyncRefresh {
            calls: AtomicUsize::new(0),
        });
        let consumer = |login| {
            let (account, vault, coordinator, transport) = (
                account.clone(),
                vault.clone(),
                coordinator.clone(),
                transport.clone(),
            );
            tokio::spawn(async move {
                if login {
                    coordinator
                        .persist_verified_grant(&account, grant(), vault, &CancellationToken::new())
                        .await
                } else {
                    coordinator
                        .access_token_async(
                            &account,
                            NOW,
                            vault,
                            transport.as_ref(),
                            &CancellationToken::new(),
                        )
                        .await
                        .map(|_| ())
                }
            })
        };
        let first = consumer(login_first);
        tokio::time::timeout(Duration::from_secs(2), entered_rx)
            .await
            .unwrap()
            .unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        let mut second = consumer(!login_first);
        let early = tokio::time::timeout(Duration::from_millis(100), &mut second).await;
        let stores_before_release = vault.stores.load(Ordering::SeqCst);
        drop(release);
        assert!(
            early.is_err(),
            "the next operation must wait for the abandoned native store"
        );
        assert_eq!(stores_before_release, 1);
        tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let retained = vault.load(&account).unwrap().unwrap();
        assert_eq!(retained.access_token, "reconnected-fixture-access");
        assert_eq!(retained.refresh_token, "reconnected-fixture-refresh");
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            usize::from(!login_first)
        );
        assert_eq!(
            vault.stores.load(Ordering::SeqCst),
            if login_first { 1 } else { 2 }
        );
    }
}
