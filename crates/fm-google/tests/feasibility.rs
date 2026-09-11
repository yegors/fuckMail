use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_google::{attachment::*, auth::*, send::*, sync::*, *};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

fn account(name: &str) -> AccountId {
    AccountId::new(name).unwrap()
}
fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    decode_json(bytes, 1024 * 1024).unwrap()
}

#[derive(Default)]
struct FixtureApi {
    calls: Vec<(AccountId, String)>,
    fail: Option<GoogleError>,
}
impl GmailTransport for FixtureApi {
    fn list(
        &mut self,
        account: &AccountId,
        token: Option<&str>,
        include_spam_trash: bool,
    ) -> Result<ListPage, GoogleError> {
        assert!(
            include_spam_trash,
            "whole-mailbox scan must not silently omit Spam/Trash"
        );
        self.calls
            .push((account.clone(), format!("list:{token:?}")));
        if let Some(error) = self.fail.take() {
            return Err(error);
        }
        Ok(match token {
            None => json(include_bytes!("fixtures/list-first.json")),
            Some("fixture-page-2") => json(include_bytes!("fixtures/list-last.json")),
            _ => panic!("unexpected list token"),
        })
    }
    fn history(
        &mut self,
        account: &AccountId,
        start: &str,
        token: Option<&str>,
    ) -> Result<HistoryPage, GoogleError> {
        self.calls
            .push((account.clone(), format!("history:{start}:{token:?}")));
        if let Some(error) = self.fail.take() {
            return Err(error);
        }
        assert_eq!(
            start, "100",
            "page tokens retain the original startHistoryId"
        );
        Ok(match token {
            None => json(include_bytes!("fixtures/history-first.json")),
            Some("fixture-history-2") => json(include_bytes!("fixtures/history-last.json")),
            _ => panic!("unexpected history token"),
        })
    }
    fn get_raw(
        &mut self,
        account: &AccountId,
        message_id: &str,
    ) -> Result<RawMessage, GoogleError> {
        self.calls
            .push((account.clone(), format!("get:{message_id}")));
        Ok(RawMessage {
            id: message_id.into(),
            thread_id: "t-common".into(),
            history_id: "120".into(),
            label_ids: vec!["UNREAD".into()],
            raw: URL_SAFE_NO_PAD.encode(MIME),
        })
    }
}

/// An in-memory contract fixture, deliberately not a substitute Core database.
struct RecordingStore {
    checkpoint: Checkpoint,
    ids: BTreeSet<String>,
    draft: String,
    pending_local_flag: bool,
    fail_once: bool,
    staged: BTreeSet<String>,
}
impl RecordingStore {
    fn new(checkpoint: Checkpoint) -> Self {
        Self {
            checkpoint,
            ids: BTreeSet::from(["absent-from-list".to_owned()]),
            draft: "local-authored-draft".to_owned(),
            pending_local_flag: true,
            fail_once: false,
            staged: BTreeSet::new(),
        }
    }
}
impl PageStore for RecordingStore {
    fn stage_message(
        &mut self,
        expected: &Checkpoint,
        message: MessageContent,
    ) -> Result<(), GoogleError> {
        if &self.checkpoint != expected {
            return Err(GoogleError::StaleCheckpoint);
        }
        assert_eq!(message.source_mime, MIME);
        self.staged.insert(message.id);
        Ok(())
    }
    fn commit_page(&mut self, plan: &PagePlan) -> Result<(), GoogleError> {
        if self.fail_once {
            self.fail_once = false;
            return Err(GoogleError::StoreUnavailable);
        }
        if plan.expected != self.checkpoint {
            return Err(GoogleError::StaleCheckpoint);
        }
        assert!(
            plan.fetch_message_ids.is_subset(&self.staged),
            "cannot advance past missing source content"
        );
        for id in &plan.fetch_message_ids {
            self.ids.insert(id.clone());
        }
        for id in &plan.explicitly_deleted_ids {
            self.ids.remove(id);
        }
        self.checkpoint = plan.next.clone();
        Ok(())
    }
}

#[test]
fn paginated_inventory_replay_and_interleaved_history_keep_local_intent() {
    let mut checkpoint = Checkpoint::begin_full(account("work"), 1, "100".to_owned()).unwrap();
    let mut store = RecordingStore::new(checkpoint.clone());
    let mut api = FixtureApi::default();
    store.fail_once = true;
    let original = checkpoint.clone();
    assert_eq!(
        advance(&mut checkpoint, &mut api, &mut store),
        Err(GoogleError::StoreUnavailable)
    );
    assert_eq!(checkpoint, original);
    let first = advance(&mut checkpoint, &mut api, &mut store).unwrap();
    assert!(!first.inventory_finished);
    assert!(checkpoint.committed_history_id.is_none());
    assert_eq!(store.commit_page(&first), Err(GoogleError::StaleCheckpoint));
    let last = advance(&mut checkpoint, &mut api, &mut store).unwrap();
    assert!(last.inventory_finished);
    assert!(!last.caught_up);
    assert!(matches!(checkpoint.phase, Phase::CatchUp { .. }));
    assert!(checkpoint.committed_history_id.is_none());
    advance(&mut checkpoint, &mut api, &mut store).unwrap();
    assert!(checkpoint.committed_history_id.is_none());
    let caught_up = advance(&mut checkpoint, &mut api, &mut store).unwrap();
    assert!(caught_up.caught_up);
    assert_eq!(checkpoint.committed_history_id.as_deref(), Some("120"));
    assert_eq!(checkpoint.phase, Phase::Idle);
    assert_eq!(
        store.ids,
        BTreeSet::from(
            ["absent-from-list", "m-shared", "m-tail", "m-during-import"].map(str::to_owned)
        )
    );
    assert_eq!(store.draft, "local-authored-draft");
    assert!(store.pending_local_flag);
    assert!(api.calls.iter().all(|(id, _)| id == &account("work")));
    let start_delta = checkpoint.begin_delta().unwrap();
    store.commit_page(&start_delta).unwrap();
    checkpoint = start_delta.next;
    assert!(
        matches!(checkpoint.phase, Phase::Delta { start_history_id, .. } if start_history_id == "120")
    );
}

#[test]
fn expiry_requests_new_generation_without_erase_and_rate_limit_keeps_checkpoint() {
    let mut checkpoint = Checkpoint {
        account: account("work"),
        generation: 3,
        revision: 12,
        committed_history_id: Some("100".into()),
        page_count: 0,
        phase: Phase::Delta {
            start_history_id: "100".into(),
            page_token: None,
        },
    };
    let mut store = RecordingStore::new(checkpoint.clone());
    let mut api = FixtureApi {
        fail: Some(GoogleError::RateLimited {
            retry_after_secs: Some(120),
        }),
        ..Default::default()
    };
    let prior = checkpoint.clone();
    assert!(matches!(
        advance(&mut checkpoint, &mut api, &mut store),
        Err(GoogleError::RateLimited { .. })
    ));
    assert_eq!(prior, checkpoint);
    api.fail = Some(GoogleError::HistoryExpired);
    let expired = advance(&mut checkpoint, &mut api, &mut store).unwrap();
    assert_eq!(expired.next.phase, Phase::NeedsFullSync);
    assert!(expired.explicitly_deleted_ids.is_empty());
    assert_eq!(store.draft, "local-authored-draft");
    assert!(store.pending_local_flag);
    let restarted = checkpoint.restart_full("900".to_owned()).unwrap();
    store.commit_page(&restarted).unwrap();
    assert_eq!(store.checkpoint.generation, 4);
    assert_eq!(
        store.checkpoint.committed_history_id.as_deref(),
        Some("100")
    );
    assert!(store.ids.contains("absent-from-list"));
}

#[test]
fn colliding_google_ids_cannot_commit_into_another_account() {
    let a = Checkpoint::begin_full(account("personal"), 1, "100".into()).unwrap();
    let b = Checkpoint::begin_full(account("work"), 1, "100".into()).unwrap();
    let page: ListPage = json(include_bytes!("fixtures/list-first.json"));
    let plan = prepare_full_page(&a, page).unwrap();
    let mut wrong_store = RecordingStore::new(b.clone());
    assert_eq!(
        wrong_store.commit_page(&plan),
        Err(GoogleError::StaleCheckpoint)
    );
    assert_eq!(wrong_store.checkpoint, b);
}

#[test]
fn malformed_paging_history_and_oversized_json_fail_closed() {
    let mut checkpoint = Checkpoint::begin_full(account("work"), 1, "100".into()).unwrap();
    checkpoint.phase = Phase::Full {
        baseline_history_id: "100".into(),
        page_token: Some("loop".into()),
    };
    let page = ListPage {
        messages: vec![],
        next_page_token: Some("loop".into()),
        result_size_estimate: None,
    };
    assert!(prepare_full_page(&checkpoint, page).is_err());
    assert!(decode_json::<ListPage>(include_bytes!("fixtures/list-first.json"), 4).is_err());
    checkpoint.phase = Phase::Delta {
        start_history_id: "100".into(),
        page_token: None,
    };
    let mut history: HistoryPage = json(include_bytes!("fixtures/history-first.json"));
    history.history.swap(0, 1);
    assert!(prepare_history_page(&checkpoint, history).is_err());
}

#[test]
fn retry_budget_respects_provider_delay_without_retrying_permissions() {
    assert_eq!(
        read_retry_delay(
            &GoogleError::RateLimited {
                retry_after_secs: Some(120)
            },
            0,
            42
        ),
        Some(120_000)
    );
    assert_eq!(
        read_retry_delay(&GoogleError::ServerUnavailable, 2, 42),
        Some(4042)
    );
    assert_eq!(
        read_retry_delay(&GoogleError::ServerUnavailable, 6, 0),
        None
    );
    assert_eq!(read_retry_delay(&GoogleError::PermissionDenied, 0, 0), None);
    assert_eq!(
        read_retry_delay(&GoogleError::ReauthorizationRequired, 0, 0),
        None
    );
}

#[test]
fn error_classification_distinguishes_history_expiry_permissions_and_quota() {
    let quota = br#"{"error":{"code":403,"message":"private provider text","errors":[{"reason":"userRateLimitExceeded"}]}}"#;
    assert_eq!(
        classify_api_error(403, quota, Some(10), false),
        GoogleError::RateLimited {
            retry_after_secs: Some(10)
        }
    );
    assert_eq!(
        classify_api_error(403, b"{}", None, false),
        GoogleError::PermissionDenied
    );
    assert_eq!(
        classify_api_error(404, b"{}", None, true),
        GoogleError::HistoryExpired
    );
    assert_eq!(
        classify_api_error(404, b"{}", None, false),
        GoogleError::NotFound
    );
    assert_eq!(
        classify_api_error(401, b"{}", None, true),
        GoogleError::ReauthorizationRequired
    );
    assert_eq!(
        classify_api_error(503, b"{}", None, false),
        GoogleError::ServerUnavailable
    );
    assert!(
        !format!("{:?}", classify_api_error(403, quota, None, false))
            .contains("private provider text")
    );
}

#[test]
fn raw_source_is_preserved_and_identity_mismatch_is_rejected() {
    let raw = RawMessage {
        id: "m-shared".into(),
        thread_id: "t-common".into(),
        label_ids: vec!["UNREAD".into()],
        history_id: "120".into(),
        raw: URL_SAFE_NO_PAD.encode(MIME),
    };
    let parsed = decode_raw_message(raw.clone(), "m-shared").unwrap();
    assert_eq!(parsed.source_mime, MIME);
    assert_eq!(parsed.label_ids, vec!["UNREAD"]);
    assert!(decode_raw_message(raw, "m-from-another-request").is_err());
}

#[test]
fn partial_message_retrieval_never_advances_inventory_token() {
    struct InterruptedApi {
        inner: FixtureApi,
        gets: usize,
    }
    impl GmailTransport for InterruptedApi {
        fn list(
            &mut self,
            account: &AccountId,
            token: Option<&str>,
            all: bool,
        ) -> Result<ListPage, GoogleError> {
            self.inner.list(account, token, all)
        }
        fn history(
            &mut self,
            account: &AccountId,
            start: &str,
            token: Option<&str>,
        ) -> Result<HistoryPage, GoogleError> {
            self.inner.history(account, start, token)
        }
        fn get_raw(&mut self, account: &AccountId, id: &str) -> Result<RawMessage, GoogleError> {
            self.gets += 1;
            if self.gets == 2 {
                return Err(GoogleError::TransportInterrupted);
            }
            self.inner.get_raw(account, id)
        }
    }
    let mut checkpoint = Checkpoint::begin_full(account("work"), 1, "100".into()).unwrap();
    let original = checkpoint.clone();
    let mut store = RecordingStore::new(checkpoint.clone());
    let mut api = InterruptedApi {
        inner: FixtureApi::default(),
        gets: 0,
    };
    assert_eq!(
        advance(&mut checkpoint, &mut api, &mut store),
        Err(GoogleError::TransportInterrupted)
    );
    assert_eq!(checkpoint, original);
    assert_eq!(store.checkpoint, original);
    assert_eq!(store.staged.len(), 1);
    assert!(!store.ids.contains("m-old"));
    advance(&mut checkpoint, &mut api, &mut store).unwrap();
    assert!(store.ids.contains("m-old"));
    assert!(store.ids.contains("m-shared"));
    assert_eq!(store.staged.len(), 2);
}

#[test]
fn attachments_publish_only_complete_verified_content() {
    let bytes = b"hello";
    let verified = decode_attachment(
        GmailAttachment {
            size: 5,
            data: URL_SAFE_NO_PAD.encode(bytes),
        },
        5,
        64,
    )
    .unwrap();
    assert_eq!(verified.bytes, bytes);
    assert_eq!(
        verified.sha256,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert!(decode_attachment(
        GmailAttachment {
            size: 5,
            data: "aGVs".into()
        },
        5,
        64
    )
    .is_err());
    assert!(decode_attachment(
        GmailAttachment {
            size: 5,
            data: "aGVsbG8".into()
        },
        6,
        64
    )
    .is_err());
    assert!(decode_attachment(
        GmailAttachment {
            size: 5,
            data: "invalid%".into()
        },
        5,
        64
    )
    .is_err());
    assert!(decode_attachment(
        GmailAttachment {
            size: 5,
            data: "aGVsbG8".into()
        },
        5,
        4
    )
    .is_err());
}

struct Journal {
    saved: SendOperation,
    fail_commit: Option<usize>,
    commits: usize,
}
impl SendJournal for Journal {
    fn transition(
        &mut self,
        expected: &SendOperation,
        next: &SendOperation,
    ) -> Result<(), GoogleError> {
        self.commits += 1;
        if Some(self.commits) == self.fail_commit {
            return Err(GoogleError::StoreUnavailable);
        }
        if &self.saved != expected {
            return Err(GoogleError::StaleCheckpoint);
        }
        self.saved = next.clone();
        Ok(())
    }
}
struct SendFake {
    calls: usize,
    outcome: Option<SubmissionResult>,
}
impl SubmissionTransport for SendFake {
    fn send(&mut self, account_id: &AccountId, mime: &[u8]) -> SubmissionResult {
        assert_eq!(account_id, &account("work"));
        assert_eq!(mime, b"From: me@example.test\r\n\r\nAuthored by me");
        self.calls += 1;
        self.outcome.take().unwrap()
    }
}
const MIME: &[u8] = b"From: me@example.test\r\n\r\nAuthored by me";
fn prepared() -> SendOperation {
    SendOperation::prepared(account("work"), "local-intent-1".into(), MIME).unwrap()
}

#[test]
fn uncertain_send_is_never_automatically_submitted_again() {
    let mut operation = prepared();
    let mut journal = Journal {
        saved: operation.clone(),
        commits: 0,
        fail_commit: None,
    };
    let mut api = SendFake {
        calls: 0,
        outcome: Some(SubmissionResult::NoDefinitiveResponse),
    };
    submit_once(&mut operation, MIME, &mut api, &mut journal).unwrap();
    assert_eq!(operation.state, SendState::AcceptanceUnknown);
    assert_eq!(
        submit_once(&mut operation, MIME, &mut api, &mut journal),
        Err(GoogleError::StaleCheckpoint)
    );
    assert_eq!(api.calls, 1);
}

#[test]
fn journal_before_send_and_unknown_recovery_after_receipt_commit_failure() {
    for fail_at in [1, 2] {
        let mut operation = prepared();
        let mut journal = Journal {
            saved: operation.clone(),
            commits: 0,
            fail_commit: Some(fail_at),
        };
        let mut api = SendFake {
            calls: 0,
            outcome: Some(SubmissionResult::Accepted {
                message_id: "sent-1".into(),
                thread_id: "t-common".into(),
            }),
        };
        assert_eq!(
            submit_once(&mut operation, MIME, &mut api, &mut journal),
            Err(GoogleError::StoreUnavailable)
        );
        assert_eq!(api.calls, usize::from(fail_at == 2));
        if fail_at == 2 {
            assert_eq!(journal.saved.recover().state, SendState::AcceptanceUnknown);
            assert_eq!(
                submit_once(&mut operation, MIME, &mut api, &mut journal),
                Err(GoogleError::StaleCheckpoint)
            );
        } else {
            assert_eq!(journal.saved.state, SendState::Prepared);
        }
    }
}

#[test]
fn modified_mime_and_duplicate_operation_are_not_sent() {
    let mut operation = prepared();
    let mut journal = Journal {
        saved: operation.clone(),
        commits: 0,
        fail_commit: None,
    };
    let mut api = SendFake {
        calls: 0,
        outcome: Some(SubmissionResult::Rejected {
            reason: "permissionDenied",
        }),
    };
    assert!(submit_once(&mut operation, b"different body", &mut api, &mut journal).is_err());
    assert_eq!(api.calls, 0);
    let mut competing = operation.clone();
    submit_once(&mut operation, MIME, &mut api, &mut journal).unwrap();
    assert!(matches!(operation.state, SendState::Rejected { .. }));
    assert!(submit_once(&mut competing, MIME, &mut api, &mut journal).is_err());
    assert_eq!(api.calls, 1);
}

#[test]
fn desktop_authorization_uses_system_browser_url_pkce_and_one_time_state() {
    let authorization =
        DesktopAuthorization::new("fixture-client.apps.googleusercontent.com".into(), 54321)
            .unwrap();
    let url = oauth2::url::Url::parse(&authorization.url).unwrap();
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host_str(), Some("accounts.google.com"));
    let params: BTreeMap<_, _> = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    assert_eq!(
        params["redirect_uri"],
        "http://127.0.0.1:54321/oauth2/callback"
    );
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["access_type"], "offline");
    assert_eq!(params["response_type"], "code");
    assert!(!params.contains_key("client_secret"));
    let code = authorization
        .accept_callback(&params["state"], "fixture-authorization-code".into())
        .unwrap();
    let challenge = oauth2::PkceCodeChallenge::from_code_verifier_sha256(&code.verifier);
    assert_eq!(challenge.as_str(), params["code_challenge"]);
    let different = DesktopAuthorization::new("fixture-client".into(), 54322).unwrap();
    assert!(matches!(
        different.accept_callback("wrong-state", "code".into()),
        Err(GoogleError::PermissionDenied)
    ));
}

#[derive(Default)]
struct Vault {
    tokens: Mutex<BTreeMap<AccountId, StoredToken>>,
    fail_store: bool,
}
impl CredentialVault for Vault {
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        Ok(self.tokens.lock().unwrap().get(account).cloned())
    }
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        if self.fail_store {
            return Err(GoogleError::StoreUnavailable);
        }
        self.tokens.lock().unwrap().insert(account.clone(), token);
        Ok(())
    }
}
#[derive(Default)]
struct RefreshFake {
    calls: AtomicUsize,
    revoke: bool,
}
impl RefreshTransport for RefreshFake {
    fn refresh(&self, id: &AccountId, refresh: &str) -> Result<RefreshResult, GoogleError> {
        assert_eq!(id, &account("work"));
        assert_eq!(refresh, "synthetic-refresh");
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.revoke {
            return Err(GoogleError::ReauthorizationRequired);
        }
        Ok(RefreshResult {
            access_token: "synthetic-new-access".into(),
            rotated_refresh_token: None,
            expires_at_secs: 4000,
        })
    }
}
fn expired_vault(fail_store: bool) -> Vault {
    Vault {
        fail_store,
        tokens: Mutex::new(BTreeMap::from([(
            account("work"),
            StoredToken {
                access_token: "synthetic-expired".into(),
                refresh_token: "synthetic-refresh".into(),
                expires_at_secs: 100,
            },
        )])),
    }
}

#[test]
fn simultaneous_mail_and_calendar_requests_share_one_refresh() {
    let coordinator = Arc::new(TokenCoordinator::default());
    let vault = Arc::new(expired_vault(false));
    let transport = Arc::new(RefreshFake::default());
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let coordinator = coordinator.clone();
            let vault = vault.clone();
            let transport = transport.clone();
            std::thread::spawn(move || {
                coordinator.access_token(&account("work"), 200, &*vault, &*transport)
            })
        })
        .collect();
    for thread in threads {
        assert_eq!(thread.join().unwrap().unwrap(), "synthetic-new-access");
    }
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        vault.load(&account("work")).unwrap().unwrap().refresh_token,
        "synthetic-refresh"
    );
    assert!(matches!(
        coordinator.access_token(&account("personal"), 200, &*vault, &*transport),
        Err(GoogleError::ReauthorizationRequired)
    ));
}

#[test]
fn refreshed_tokens_are_not_returned_before_vault_persistence() {
    let coordinator = TokenCoordinator::default();
    let vault = expired_vault(true);
    let transport = RefreshFake::default();
    assert_eq!(
        coordinator.access_token(&account("work"), 200, &vault, &transport),
        Err(GoogleError::StoreUnavailable)
    );
    let revoked = RefreshFake {
        revoke: true,
        ..Default::default()
    };
    assert_eq!(
        coordinator.access_token(&account("work"), 200, &expired_vault(false), &revoked),
        Err(GoogleError::ReauthorizationRequired)
    );
}

#[test]
fn locked_vault_read_releases_no_token_and_never_refreshes_or_falls_back() {
    struct LockableVault {
        locked: AtomicBool,
        loads: AtomicUsize,
        writes: AtomicUsize,
        token: StoredToken,
    }
    impl CredentialVault for LockableVault {
        fn load(&self, id: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
            assert_eq!(id, &account("work"));
            self.loads.fetch_add(1, Ordering::SeqCst);
            if self.locked.load(Ordering::SeqCst) {
                return Err(GoogleError::StoreUnavailable);
            }
            Ok(Some(self.token.clone()))
        }
        fn store(&self, _: &AccountId, _: StoredToken) -> Result<(), GoogleError> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Err(GoogleError::StoreUnavailable)
        }
    }
    let vault = LockableVault {
        locked: AtomicBool::new(true),
        loads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
        token: StoredToken {
            access_token: "synthetic-unexpired-vault-token".into(),
            refresh_token: "synthetic-refresh".into(),
            expires_at_secs: 4000,
        },
    };
    let transport = RefreshFake::default();
    let coordinator = TokenCoordinator::default();
    // A locked/unavailable vault is an error, not an absent token, permission to
    // refresh, or a reason to create another (plaintext) credential store.
    assert_eq!(
        coordinator.access_token(&account("work"), 200, &vault, &transport),
        Err(GoogleError::StoreUnavailable)
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert_eq!(vault.writes.load(Ordering::SeqCst), 0);

    // An explicit subsequent unlock works; the coordinator still must not cache
    // that token as a fallback when the same vault later becomes unavailable.
    vault.locked.store(false, Ordering::SeqCst);
    assert_eq!(
        coordinator.access_token(&account("work"), 200, &vault, &transport),
        Ok("synthetic-unexpired-vault-token".into())
    );
    vault.locked.store(true, Ordering::SeqCst);
    for now in [200, 5000] {
        assert_eq!(
            coordinator.access_token(&account("work"), now, &vault, &transport),
            Err(GoogleError::StoreUnavailable)
        );
    }
    assert_eq!(vault.loads.load(Ordering::SeqCst), 4);
    assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert_eq!(vault.token.access_token, "synthetic-unexpired-vault-token");
    assert_eq!(vault.token.refresh_token, "synthetic-refresh");
}
