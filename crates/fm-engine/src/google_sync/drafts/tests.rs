use super::*;
use crate::core_tasks::CoreBridge;
use fm_google::{
    transport::{HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport, TransportError},
    AccountId,
};
use std::{collections::BTreeSet, sync::Mutex};
use tokio::sync::{mpsc, Semaphore};

const EMAIL: &str = "author@example.test";

#[derive(Default)]
struct Remote {
    raw: Option<Vec<u8>>,
    message: usize,
    writes: usize,
    attempts: usize,
    reads: usize,
    profile_email: Option<String>,
    lose_ack: bool,
    reject: bool,
    normalize_raw: bool,
    entered: Option<Arc<tokio::sync::Barrier>>,
    release: Option<Arc<tokio::sync::Barrier>>,
}
#[derive(Default)]
struct Http(Mutex<Remote>);
impl HttpTransport for Http {
    fn execute<'a>(&'a self, request: HttpRequest, _: &'a CancellationToken) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-draft-token"
            );
            let path = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/")
                .expect("Gmail only");
            if path == "profile" {
                let email = self
                    .0
                    .lock()
                    .unwrap()
                    .profile_email
                    .clone()
                    .unwrap_or_else(|| EMAIL.into());
                return response(
                    200,
                    json!({"emailAddress":email,"messagesTotal":0,"threadsTotal":0,"historyId":"100"}),
                );
            }
            if request.method() == HttpMethod::Get {
                assert_eq!(path, "drafts/remote-draft?format=raw");
                let mut state = self.0.lock().unwrap();
                state.reads += 1;
                let Some(mut raw) = state.raw.clone() else {
                    return response(404, json!({}));
                };
                if state.normalize_raw {
                    raw.extend_from_slice(b"\r\nProvider normalization\r\n");
                }
                return response(
                    200,
                    json!({"id":"remote-draft","message":{"id":format!("remote-message-{}",state.message),"threadId":"remote-thread","raw":URL_SAFE_NO_PAD.encode(raw)}}),
                );
            }
            assert!(
                matches!(
                    (request.method(), path),
                    (HttpMethod::Post, "drafts") | (HttpMethod::Put, "drafts/remote-draft")
                ),
                "no send/delete or other write"
            );
            let source: Value = serde_json::from_slice(request.body_bytes()).unwrap();
            let raw = URL_SAFE_NO_PAD
                .decode(source["message"]["raw"].as_str().unwrap())
                .unwrap();
            {
                let mut state = self.0.lock().unwrap();
                state.attempts += 1;
                if state.reject {
                    return response(
                        403,
                        json!({"error":{"code":403,"message":"Forbidden","errors":[{"reason":"forbidden"}]}}),
                    );
                }
            }
            let (entered, release) = {
                let state = self.0.lock().unwrap();
                (state.entered.clone(), state.release.clone())
            };
            if let Some(entered) = entered {
                entered.wait().await;
            }
            // Deliberately allow a late response after caller cancellation. It
            // must still record the exact durable operation, never newer text.
            if let Some(release) = release {
                release.wait().await;
            }
            let mut state = self.0.lock().unwrap();
            state.writes += 1;
            state.message += 1;
            state.raw = Some(raw);
            if state.lose_ack {
                return Err(TransportError::Network);
            }
            response(
                200,
                json!({"id":"remote-draft","message":{"id":format!("remote-message-{}",state.message),"threadId":"remote-thread"}}),
            )
        })
    }
}

#[tokio::test]
async fn definitive_provider_rejection_waits_for_new_authored_revision_before_retry() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("Rejected original revision", "first").await;
    h.http.0.lock().unwrap().reject = true;
    assert_eq!(h.upload("known-rejection").await["state"], "failed");
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Idle(_)));
    assert_eq!(h.http.0.lock().unwrap().attempts, 1);
    let edited = h
        .save("A new human revision may be synchronized", "edited")
        .await;
    h.http.0.lock().unwrap().reject = false;
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    let synced = h.status().await;
    assert_eq!(synced["state"], "synced");
    assert_eq!(synced["baseline"]["draft_revision"], edited["revision"]);
    assert_eq!(h.http.0.lock().unwrap().attempts, 2);
    assert_eq!(h.http.0.lock().unwrap().writes, 1);
    h.close();
}
fn response(status: u16, body: Value) -> std::result::Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status,
        body: serde_json::to_vec(&body).unwrap(),
        retry_after_secs: None,
        etag: None,
    })
}

struct Harness {
    ctx: SyncContext,
    core: CoreBridge,
    id: String,
    http: Arc<Http>,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new(path: &std::path::Path, http: Arc<Http>) -> Self {
        let path = path.to_owned();
        let (queue, mut receive) = mpsc::channel(16);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let bindings = store.google_account_bindings().unwrap();
            let account = if let Some(binding) = bindings.first() {
                binding.account.clone()
            } else {
                store
                    .bind_google_account(
                        "draft-worker-fixture",
                        EMAIL,
                        &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
                        None,
                    )
                    .unwrap()
            };
            let generation = store
                .google_account_bindings()
                .unwrap()
                .into_iter()
                .find(|binding| binding.account == account)
                .unwrap()
                .connection_generation;
            let rows = store
                .handle("drafts", Some(&account), json!({"limit":1}))
                .unwrap();
            let draft = if rows["drafts"].as_array().unwrap().is_empty() {
                store
                    .handle(
                        "draft_create",
                        Some(&account),
                        json!({"mode":"new","request_id":"authored-new"}),
                    )
                    .unwrap()
            } else {
                rows["drafts"][0].clone()
            };
            ready
                .send((
                    account,
                    draft["draft_id"].as_str().unwrap().to_owned(),
                    generation,
                ))
                .unwrap();
            while let Some(work) = receive.blocking_recv() {
                match work {
                    crate::Work::Provider(call) => call.run(&mut store),
                    _ => panic!("unexpected writer job"),
                }
            }
        });
        let (account, id, generation) = wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let core = CoreBridge::new(queue, Arc::new(Semaphore::new(8)));
        let ctx = SyncContext::for_test(
            core.clone(),
            AccountId::new(account).unwrap(),
            generation,
            http.clone(),
            "synthetic-draft-token".into(),
            CancellationToken::new(),
        );
        Self {
            ctx,
            core,
            id,
            http,
            writer,
        }
    }
    async fn draft(&self) -> Value {
        let account = self.ctx.account.as_str().to_owned();
        let id = self.id.clone();
        self.core
            .call(move |store| store.handle("draft_get", Some(&account), json!({"draft_id":id})))
            .await
            .unwrap()
    }
    async fn save(&self, body: &str, request: &str) -> Value {
        let draft = self.draft().await;
        let account = self.ctx.account.as_str().to_owned();
        let id = self.id.clone();
        let body = body.to_owned();
        let request = request.to_owned();
        self.core.call(move |store|store.handle("draft_save",Some(&account),json!({"draft_id":id,"expected_revision":draft["revision"],"request_id":request,"body":body,"subject":"Human authored Google draft"}))).await.unwrap()
    }
    async fn upload(&self, request: &str) -> Value {
        let draft = self.draft().await;
        execute(
            &self.ctx,
            json!({"draft_id":self.id,"expected_revision":draft["revision"],"request_id":request}),
        )
        .await
        .unwrap()
    }
    async fn status(&self) -> Value {
        let account = self.ctx.account.as_str().to_owned();
        let id = self.id.clone();
        self.core
            .call(move |store| store.google_draft_status(&account, &id))
            .await
            .unwrap()
    }
    async fn local(&self, method: &str, params: Value) -> Value {
        let account = self.ctx.account.as_str().to_owned();
        let method = method.to_owned();
        let mut next = self
            .core
            .call(move |store| store.local_content_begin(&method, &account, params))
            .await
            .unwrap();
        loop {
            match next {
                fm_core::local_content::LocalContentStep::Reply(reply) => return reply,
                fm_core::local_content::LocalContentStep::Prepare(plan) => {
                    let proof = tokio::task::spawn_blocking(move || {
                        fm_core::local_content::prepare_local_content(
                            plan,
                            &CancellationToken::new(),
                        )
                    })
                    .await
                    .unwrap()
                    .unwrap();
                    next = self
                        .core
                        .call(move |store| store.local_content_finish(proof))
                        .await
                        .unwrap();
                }
            }
        }
    }
    async fn freeze(&self, request: Value) -> Value {
        let scope = verified_scope(&self.ctx).await.unwrap();
        let begun = self
            .core
            .call(move |store| store.google_draft_begin(&scope, &request))
            .await
            .unwrap();
        let GoogleDraftStep::Prepare(plan) = begun else {
            panic!("expected a new immutable operation");
        };
        let proof = tokio::task::spawn_blocking(move || {
            fm_core::google_drafts::prepare_google_draft(plan, &CancellationToken::new())
        })
        .await
        .unwrap()
        .unwrap();
        self.core
            .call(move |store| store.google_draft_finish(proof))
            .await
            .unwrap()
    }
    fn close(self) {
        let Self {
            ctx, core, writer, ..
        } = self;
        drop(ctx);
        drop(core);
        writer.join().unwrap();
    }
}

async fn hold_mutation_lane(ctx: &SyncContext) -> crate::google_sync::quota::Lease {
    ctx.quota
        .acquire(
            fm_google::scheduler::Work {
                account: ctx.account.clone(),
                service: fm_google::scheduler::Service::Gmail,
                class: fm_google::scheduler::WorkClass::DraftMutation,
                units: 10,
            },
            &ctx.cancel,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn quota_blocked_draft_keeps_core_available_and_preparation_memory_unclaimed() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let draft = h.save("Never load this queued body", "save").await;
    let held = hold_mutation_lane(&h.ctx).await;
    let ctx = h.ctx.clone();
    let id = h.id.clone();
    let mut task = tokio::spawn(async move {
        execute(
            &ctx,
            json!({"draft_id":id,"expected_revision":draft["revision"],"request_id":"queued"}),
        )
        .await
    });
    assert!(tokio::time::timeout(Duration::from_millis(40), &mut task)
        .await
        .is_err());
    let local = tokio::time::timeout(Duration::from_secs(1), h.status())
        .await
        .unwrap();
    assert_eq!(local["state"], "local");
    assert!(local["operation"].is_null());
    assert_eq!(h.ctx.preparation.available_permits(), 1);
    assert_eq!(h.http.0.lock().unwrap().writes, 0);
    h.ctx.cancel.cancel();
    assert!(tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    drop(held);
    h.close();
}

#[tokio::test]
async fn cancelled_preparation_retains_memory_until_the_actual_blocking_work_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let memory = Arc::new(h.ctx.prepare_permit().await.unwrap());
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let ctx = h.ctx.clone();
    let held = memory.clone();
    let task = tokio::spawn(async move {
        prepare(&ctx, &held, move |cancel| {
            entered.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(cancel.is_cancelled());
            Ok(())
        })
        .await
    });
    drop(memory);
    tokio::time::timeout(Duration::from_secs(2), ready)
        .await
        .unwrap()
        .unwrap();
    h.ctx.cancel.cancel();
    task.abort();
    assert_eq!(
        h.ctx.preparation.available_permits(),
        0,
        "a dropped async waiter does not release active MIME memory"
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while h.ctx.preparation.available_permits() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.close();
}

#[tokio::test]
async fn cancellation_immediately_after_core_claim_does_not_lose_the_returned_lease() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let draft = h.save("Claim cancellation boundary", "save").await;
    let prepared=h.freeze(json!({"draft_id":h.id,"expected_revision":draft["revision"],"request_id":"claim-boundary"})).await;
    let scope = verified_scope(&h.ctx).await.unwrap();
    let cancel = h.ctx.cancel.clone();
    let memory = Arc::new(h.ctx.prepare_permit().await.unwrap());
    let lease = memory_core(&h.ctx, &memory, move |store| {
        let lease = store.google_draft_claim(
            &scope,
            "claim-boundary",
            prepared["operation"]["revision"].as_i64().unwrap(),
        )?;
        cancel.cancel(); // Exact boundary: the durable claim exists, reply is not delivered yet.
        Ok(lease)
    })
    .await
    .unwrap();
    assert!(h.ctx.cancel.is_cancelled());
    let receipt = h
        .core
        .call(move |store| {
            store.google_draft_outcome(
                &lease,
                GoogleDraftOutcome::Undispatched {
                    code: "cancelled".into(),
                },
            )
        })
        .await
        .unwrap();
    assert_eq!(receipt["state"], "prepared");
    assert_eq!(h.http.0.lock().unwrap().writes, 0);
    drop(memory);
    assert_eq!(h.ctx.preparation.available_permits(), 1);
    h.close();
}

#[tokio::test]
async fn prepared_update_survives_quota_cancellation_and_replays_old_bytes_before_new_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("Initial remote baseline", "initial").await;
    h.upload("create").await;
    let old = h.save("Frozen pending update", "old").await;
    let request =
        json!({"draft_id":h.id,"expected_revision":old["revision"],"request_id":"frozen-update"});
    let frozen = h.freeze(request.clone()).await;
    let newest = h
        .save("Newer text must wait for the exact pending bytes", "newest")
        .await;
    let held = hold_mutation_lane(&h.ctx).await;
    let ctx = h.ctx.clone();
    let mut task = tokio::spawn(async move { execute(&ctx, request).await });
    assert!(tokio::time::timeout(Duration::from_millis(40), &mut task)
        .await
        .is_err());
    assert_eq!(h.ctx.preparation.available_permits(), 1);
    h.ctx.cancel.cancel();
    assert!(tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(h.status().await["operation"], frozen["operation"]);
    assert_eq!(h.http.0.lock().unwrap().writes, 1);
    drop(held);
    h.ctx.cancel = CancellationToken::new();
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    let replay = h.status().await;
    assert_eq!(replay["baseline"]["draft_revision"], old["revision"]);
    assert_eq!(
        replay["baseline"]["upload_sha256"],
        frozen["operation"]["upload_sha256"]
    );
    assert_eq!(replay["operation"]["request_id"], "frozen-update");
    assert_eq!(h.draft().await["revision"], newest["revision"]);
    assert_eq!(h.http.0.lock().unwrap().writes, 2);
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    assert_eq!(
        h.status().await["baseline"]["draft_revision"],
        newest["revision"]
    );
    assert_eq!(h.http.0.lock().unwrap().writes, 3);
    h.close();
}

#[tokio::test]
async fn changed_prepared_update_is_stopped_and_revision_bound_replace_handles_missing_twin() {
    for missing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path(), Arc::new(Http::default()));
        h.save("Baseline", "initial").await;
        h.upload("create").await;
        let local = h.save("Explicitly retained replacement", "local").await;
        let request =
            json!({"draft_id":h.id,"expected_revision":local["revision"],"request_id":"prepared"});
        h.freeze(request.clone()).await;
        {
            let mut remote = h.http.0.lock().unwrap();
            if missing {
                remote.raw = None;
            } else {
                remote.message += 9;
            }
        }
        let conflict = execute(&h.ctx, request).await.unwrap();
        assert_eq!(conflict["state"], "conflict");
        assert_eq!(h.http.0.lock().unwrap().writes, 1);
        let replacement = json!({"draft_id":h.id,"expected_revision":local["revision"],"expected_google_revision":conflict["revision"],"action":"replace_local","request_id":"approved-replace"});
        let mut stale = replacement.clone();
        stale["expected_google_revision"] = json!(1);
        assert_eq!(
            execute(&h.ctx, stale).await.unwrap_err().code,
            "stale_google_revision"
        );
        let replaced = execute(&h.ctx, replacement).await.unwrap();
        assert_eq!(replaced["state"], "synced");
        assert_eq!(h.http.0.lock().unwrap().writes, 2);
        assert_eq!(h.draft().await["body"], "Explicitly retained replacement");
        h.close();
    }
}

#[tokio::test]
async fn same_revision_coalesces_but_exact_request_reuse_still_validates_its_original_fields() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let original = h.save("One immutable upload", "first").await;
    let receipt = h.upload("original-request").await;
    assert_eq!(h.upload("coalesced-revision").await["state"], "synced");
    assert_eq!(h.http.0.lock().unwrap().writes, 1);
    let newer = h.save("Newer local fields", "newer").await;
    assert_eq!(execute(&h.ctx,json!({"draft_id":h.id,"expected_revision":newer["revision"],"request_id":"original-request"})).await.unwrap_err().code,"request_id_conflict");
    let replay=execute(&h.ctx,json!({"draft_id":h.id,"expected_revision":original["revision"],"request_id":"original-request"})).await.unwrap();
    assert_eq!(replay["operation"], receipt["operation"]);
    assert_eq!(h.http.0.lock().unwrap().writes, 1);
    h.close();
}

#[tokio::test]
async fn unknown_update_requires_fresh_observation_and_new_explicit_replacement_identity() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("Initial known twin", "initial").await;
    h.upload("create").await;
    let local = h
        .save("Unconfirmed update stays authored locally", "update")
        .await;
    let unknown_request = json!({"draft_id":h.id,"expected_revision":local["revision"],"request_id":"unknown-update"});
    h.http.0.lock().unwrap().lose_ack = true;
    assert_eq!(
        execute(&h.ctx, unknown_request.clone()).await.unwrap()["state"],
        "acceptance_unknown"
    );
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Idle(_)));
    let observed = reconcile(&h.ctx, &h.id).await.unwrap();
    assert_eq!(observed["state"], "conflict");
    assert_eq!(observed["conflict"]["reason"], "acceptance_unknown");
    assert_eq!(h.http.0.lock().unwrap().writes, 2);
    h.http.0.lock().unwrap().lose_ack = false;
    let replaced=execute(&h.ctx,json!({"draft_id":h.id,"expected_revision":local["revision"],"expected_google_revision":observed["revision"],"request_id":"approved-new-attempt","action":"replace_local"})).await.unwrap();
    assert_eq!(replaced["state"], "synced");
    assert_eq!(h.http.0.lock().unwrap().writes, 3);
    let original = execute(&h.ctx, unknown_request).await.unwrap();
    assert_eq!(original["state"], "acceptance_unknown");
    assert_eq!(original["operation"]["request_id"], "unknown-update");
    assert_eq!(h.http.0.lock().unwrap().writes, 3);
    h.close();
}

#[tokio::test]
async fn authored_binary_attachment_and_all_recipient_headers_reach_exact_retained_mime() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let draft = h.save("Human body with binary attachment", "first").await;
    let source = dir.path().join("exact-evidence.bin");
    let bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(&source, &bytes).unwrap();
    let imported=h.local("draft_attachment_import",json!({"draft_id":h.id,"expected_revision":draft["revision"],"request_id":"import-exact","path":source})).await;
    let saved=h.local("draft_save",json!({"draft_id":h.id,"expected_revision":imported["draft"]["revision"],"request_id":"recipients",
        "to_recipients":[{"name":"Alice","email":"alice@example.test"}],"cc_recipients":[{"email":"cc@example.test"}],"bcc_recipients":[{"email":"hidden@example.test"}],
        "body":"Human body with binary attachment"})).await;
    assert_eq!(saved["attachment_ids"][0], imported["attachment"]["id"]);
    std::fs::remove_file(&source).unwrap(); // MIME uses the retained immutable source.
    let result = h.upload("with-binary").await;
    assert_eq!(result["state"], "synced");
    let raw = h.http.0.lock().unwrap().raw.clone().unwrap();
    let mime = String::from_utf8(raw).unwrap();
    assert!(
        mime.contains("alice@example.test")
            && mime.contains("Cc: cc@example.test")
            && mime.contains("Bcc: hidden@example.test")
    );
    assert!(mime.contains("exact-evidence.bin"));
    let compact = mime.replace("\r\n", "");
    assert!(compact.contains(&base64::engine::general_purpose::STANDARD.encode(&bytes)));
    assert_eq!(
        result["baseline"]["upload_sha256"],
        result["baseline"]["observed_sha256"]
    );
    h.close();
}

#[tokio::test]
async fn oversized_oldest_draft_is_reported_without_starving_a_newer_valid_draft() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    // Non-ASCII binary forces base64; its 34,952,536 encoded bytes plus this
    // valid text part cross Google's decimal ceiling below the local 35MiB cap.
    let body = format!(
        "Keep this oversized authored draft\n{}",
        "Authored detail\n".repeat(8192)
    );
    let first = h.save(&body, "oldest").await;
    let path = dir.path().join("within-local-attachment-limit.bin");
    std::fs::write(&path, vec![0xffu8; 25 * 1024 * 1024]).unwrap();
    let imported=h.local("draft_attachment_import",json!({"draft_id":h.id,"expected_revision":first["revision"],"request_id":"large-local-file","path":path})).await;
    let frozen = imported["draft"].clone();
    tokio::time::sleep(Duration::from_secs(1)).await; // Store timestamps are civil seconds.
    let account = h.ctx.account.as_str().to_owned();
    let valid = h
        .core
        .call(move |store| {
            store.handle(
                "draft_create",
                Some(&account),
                json!({"mode":"new","request_id":"valid-newer"}),
            )
        })
        .await
        .unwrap();
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    let failed = h.status().await;
    assert_eq!(failed["state"], "failed");
    assert!(failed["operation"].is_null());
    assert_eq!(failed["preparation_failure"]["code"], "limit_exceeded");
    assert_eq!(h.http.0.lock().unwrap().writes, 0);
    let retained = h.draft().await;
    for key in [
        "draft_id",
        "revision",
        "body",
        "attachment_ids",
        "from",
        "subject",
    ] {
        assert_eq!(retained[key], frozen[key], "{key}");
    }
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    let account = h.ctx.account.as_str().to_owned();
    let valid_id = valid["draft_id"].as_str().unwrap().to_owned();
    let result = h
        .core
        .call(move |store| store.google_draft_status(&account, &valid_id))
        .await
        .unwrap();
    assert_eq!(result["state"], "synced");
    assert_eq!(h.http.0.lock().unwrap().writes, 1);
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Idle(_)));
    // A genuine authored edit, removing the oversize attachment, re-enables the
    // older draft; it is not permanently hidden or altered by the failure.
    h.local("draft_save",json!({"draft_id":h.id,"expected_revision":frozen["revision"],"request_id":"remove-large","body":body,"attachment_ids":[]})).await;
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    assert_eq!(h.status().await["state"], "synced");
    assert_eq!(h.http.0.lock().unwrap().writes, 2);
    h.close();
}

#[tokio::test]
async fn retryable_missing_oldest_blob_defers_only_that_draft_and_later_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let first = h
        .save("The retained source will become available again", "oldest")
        .await;
    let path = dir.path().join("small-local-source.bin");
    let bytes = b"Exact small retained attachment";
    std::fs::write(&path, bytes).unwrap();
    let imported=h.local("draft_attachment_import",json!({"draft_id":h.id,"expected_revision":first["revision"],"request_id":"small-source","path":path})).await;
    let frozen = imported["draft"].clone();
    let retained = dir
        .path()
        .join("blobs")
        .join(imported["attachment"]["sha256"].as_str().unwrap());
    std::fs::remove_file(&retained).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let account = h.ctx.account.as_str().to_owned();
    let valid = h
        .core
        .call(move |store| {
            store.handle(
                "draft_create",
                Some(&account),
                json!({"mode":"new","request_id":"valid-after-io"}),
            )
        })
        .await
        .unwrap();
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    let deferred = h.status().await;
    assert_ne!(deferred["state"], "failed");
    assert!(deferred["operation"].is_null());
    assert_eq!(h.http.0.lock().unwrap().writes, 0);
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    let account = h.ctx.account.as_str().to_owned();
    let id = valid["draft_id"].as_str().unwrap().to_owned();
    assert_eq!(
        h.core
            .call(move |store| store.google_draft_status(&account, &id))
            .await
            .unwrap()["state"],
        "synced"
    );
    assert_eq!(h.http.0.lock().unwrap().writes, 1);
    std::fs::write(retained, bytes).unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    assert_eq!(h.status().await["state"], "synced");
    assert_eq!(h.http.0.lock().unwrap().writes, 2);
    let restored = h.draft().await;
    for key in [
        "draft_id",
        "revision",
        "body",
        "attachment_ids",
        "from",
        "subject",
    ] {
        assert_eq!(restored[key], frozen[key], "{key}");
    }
    h.close();
}

#[tokio::test]
async fn acknowledged_raw_retention_failure_defers_without_changing_the_exact_ack() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    let entered = Arc::new(tokio::sync::Barrier::new(2));
    let release = Arc::new(tokio::sync::Barrier::new(2));
    {
        let mut state = http.0.lock().unwrap();
        state.entered = Some(entered.clone());
        state.release = Some(release.clone());
    }
    let mut h = Harness::new(dir.path(), http.clone());
    let draft = h.save("Acknowledged author text", "first").await;
    let ctx = h.ctx.clone();
    let id = h.id.clone();
    let task = tokio::spawn(async move {
        execute(&ctx,json!({"draft_id":id,"expected_revision":draft["revision"],"request_id":"ack-before-read"})).await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.wait())
        .await
        .unwrap();
    h.ctx.cancel.cancel();
    release.wait().await;
    let receipt = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(receipt["state"], "acknowledged");
    {
        let mut state = http.0.lock().unwrap();
        state.entered = None;
        state.release = None;
    }
    h.ctx.cancel = CancellationToken::new();
    let source = dir
        .path()
        .join("blobs")
        .join(receipt["operation"]["upload_sha256"].as_str().unwrap());
    let original_raw = std::fs::read(&source).unwrap();
    std::fs::write(&source, b"Temporarily unreadable retained source").unwrap();
    assert!(local_retry_error(
        &reconcile(&h.ctx, &h.id).await.unwrap_err().code
    ));
    let deferred = h.status().await;
    assert_eq!(deferred["state"], "acknowledged");
    assert_eq!(deferred["operation"], receipt["operation"]);
    assert!(deferred["baseline"].is_null());
    tokio::time::sleep(Duration::from_secs(1)).await;
    let account = h.ctx.account.as_str().to_owned();
    h.core
        .call(move |store| {
            store.handle(
                "draft_create",
                Some(&account),
                json!({"mode":"new","request_id":"valid-after-ack-io"}),
            )
        })
        .await
        .unwrap();
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Progress));
    assert_eq!(http.0.lock().unwrap().writes, 2);
    assert_eq!(h.draft().await["body"], "Acknowledged author text");
    std::fs::write(source, original_raw).unwrap();
    h.close();
}

#[tokio::test]
async fn recipient_free_create_update_ack_raw_and_changed_message_identity() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    let h = Harness::new(dir.path(), http);
    let first = h
        .save("First locally authored body — Montréal", "save-first")
        .await;
    assert!(first["to_recipients"].as_array().unwrap().is_empty());
    let created = h.upload("google-create").await;
    assert_eq!(created["state"], "synced");
    assert_eq!(created["baseline"]["draft_id"], "remote-draft");
    assert_eq!(created["baseline"]["message_id"], "remote-message-1");
    assert_eq!(
        created["baseline"]["upload_sha256"],
        created["baseline"]["observed_sha256"]
    );
    let raw = h.http.0.lock().unwrap().raw.clone().unwrap();
    let mime = String::from_utf8(raw).unwrap();
    assert!(
        mime.contains("From: author@example.test\r\n")
            || mime.contains("<author@example.test>\r\n")
    );
    assert!(!mime.contains("\r\nTo:") && !mime.contains("\r\nCc:") && !mime.contains("\r\nBcc:"));
    h.save("Second exact local body", "save-second").await;
    let updated = h.upload("google-update").await;
    assert_eq!(updated["state"], "synced");
    assert_eq!(updated["baseline"]["draft_id"], "remote-draft");
    assert_eq!(updated["baseline"]["message_id"], "remote-message-2");
    assert_ne!(
        updated["baseline"]["observed_sha256"],
        created["baseline"]["observed_sha256"]
    );
    assert_eq!(h.http.0.lock().unwrap().writes, 2);
    h.close();
}

#[tokio::test]
async fn lost_ack_survives_restart_without_automatic_duplicate_creation() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    http.0.lock().unwrap().lose_ack = true;
    let h = Harness::new(dir.path(), http.clone());
    h.save("Retain this unknown upload", "save").await;
    assert_eq!(h.upload("lost-ack").await["state"], "acceptance_unknown");
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Idle(_)));
    assert_eq!(http.0.lock().unwrap().writes, 1);
    h.close();
    let h = Harness::new(dir.path(), http.clone());
    assert_eq!(h.status().await["state"], "acceptance_unknown");
    assert!(matches!(step(&h.ctx).await.unwrap(), Step::Idle(_)));
    assert_eq!(h.upload("lost-ack").await["state"], "acceptance_unknown");
    let draft = h.draft().await;
    assert_eq!(execute(&h.ctx,json!({"draft_id":h.id,"expected_revision":draft["revision"],"request_id":"must-not-recreate"})).await.unwrap_err().code,"google_draft_pending");
    assert_eq!(http.0.lock().unwrap().writes, 1);
    h.close();
}

#[tokio::test]
async fn changed_remote_message_or_missing_twin_blocks_automatic_overwrite() {
    for missing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let http = Arc::new(Http::default());
        let h = Harness::new(dir.path(), http.clone());
        h.save("Keep the local text", "save").await;
        h.upload("create").await;
        {
            let mut remote = http.0.lock().unwrap();
            if missing {
                remote.raw = None;
            } else {
                remote.message += 100;
                remote.raw =
                    Some(b"From: external@example.test\r\n\r\nA different client\r\n".to_vec());
            }
        }
        h.save("Newer local text remains authoritative", "newer")
            .await;
        let result = h.upload("update-refused").await;
        assert_eq!(result["state"], "conflict");
        assert_eq!(http.0.lock().unwrap().writes, 1);
        assert_eq!(
            h.draft().await["body"],
            "Newer local text remains authoritative"
        );
        assert!(matches!(step(&h.ctx).await.unwrap(), Step::Idle(_)));
        h.close();
    }
}

#[tokio::test]
async fn normalized_raw_has_a_distinct_retained_baseline_not_claimed_byte_fidelity() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    http.0.lock().unwrap().normalize_raw = true;
    let h = Harness::new(dir.path(), http);
    h.save("Exact authored MIME", "save").await;
    let result = h.upload("create-normalized").await;
    assert_eq!(result["state"], "synced");
    assert_ne!(
        result["baseline"]["upload_sha256"],
        result["baseline"]["observed_sha256"]
    );
    assert_eq!(result["fidelity_warning"], false);
    assert_eq!(result["baseline"]["content_matches"], true);
    // The acknowledged contained-message identity is the observed baseline;
    // a later change to those fetched bytes must still stop an overwrite.
    h.http.0.lock().unwrap().normalize_raw = false;
    assert_eq!(reconcile(&h.ctx, &h.id).await.unwrap()["state"], "conflict");
    assert_eq!(h.draft().await["body"], "Exact authored MIME");
    h.close();
}

#[tokio::test]
async fn primary_mailbox_mismatch_never_dispatches_the_frozen_from() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    http.0.lock().unwrap().profile_email = Some("other@example.test".into());
    let h = Harness::new(dir.path(), http.clone());
    let draft = h.save("Do not retarget this identity", "save").await;
    let result = execute(
        &h.ctx,
        json!({"draft_id":h.id,"expected_revision":draft["revision"],"request_id":"wrong-primary"}),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(http.0.lock().unwrap().writes, 0);
    assert_eq!(h.draft().await["from"], EMAIL);
    h.close();
}

#[tokio::test]
async fn actual_cached_queries_and_newer_edits_progress_during_upload_and_late_ack() {
    for cancel in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let http = Arc::new(Http::default());
        let entered = Arc::new(tokio::sync::Barrier::new(2));
        let release = Arc::new(tokio::sync::Barrier::new(2));
        {
            let mut state = http.0.lock().unwrap();
            state.entered = Some(entered.clone());
            state.release = Some(release.clone());
        }
        let h = Harness::new(dir.path(), http.clone());
        let first = h.save("Frozen old revision", "first").await;
        let ctx = h.ctx.clone();
        let id = h.id.clone();
        let upload = tokio::spawn(async move {
            execute(&ctx,json!({"draft_id":id,"expected_revision":first["revision"],"request_id":"held-upload"})).await
        });
        tokio::time::timeout(Duration::from_secs(3), entered.wait())
            .await
            .unwrap();
        let account = h.ctx.account.as_str().to_owned();
        let cached = tokio::time::timeout(
            Duration::from_secs(1),
            h.core.call(move |store| {
                store.handle("conversations", Some(&account), json!({"limit":1}))
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(cached["conversations"].is_array());
        let newer = tokio::time::timeout(
            Duration::from_secs(1),
            h.save("Newer local revision while upload is held", "newer"),
        )
        .await
        .unwrap();
        if cancel {
            h.ctx.cancel.cancel();
        }
        release.wait().await;
        tokio::time::timeout(Duration::from_secs(3), upload)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let status = h.status().await;
        assert_eq!(status["operation"]["ack"]["draft_id"], "remote-draft");
        assert_eq!(status["operation"]["ack"]["message_id"], "remote-message-1");
        assert_eq!(h.draft().await["revision"], newer["revision"]);
        assert_eq!(
            h.draft().await["body"],
            "Newer local revision while upload is held"
        );
        assert_eq!(http.0.lock().unwrap().writes, 1);
        assert_eq!(h.ctx.preparation.available_permits(), 1);
        h.close();
    }
}

#[tokio::test]
async fn late_ack_after_connection_generation_change_is_durable_without_publishing_a_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    let entered = Arc::new(tokio::sync::Barrier::new(2));
    let release = Arc::new(tokio::sync::Barrier::new(2));
    {
        let mut state = http.0.lock().unwrap();
        state.entered = Some(entered.clone());
        state.release = Some(release.clone());
    }
    let mut h = Harness::new(dir.path(), http.clone());
    let draft = h.save("Frozen identity across reconnect", "first").await;
    let ctx = h.ctx.clone();
    let id = h.id.clone();
    let task = tokio::spawn(async move {
        execute(&ctx,json!({"draft_id":id,"expected_revision":draft["revision"],"request_id":"old-generation"})).await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.wait())
        .await
        .unwrap();
    let account = h.ctx.account.as_str().to_owned();
    let generation = h
        .core
        .call(move |store| {
            store.set_google_connection_state(
                &account,
                fm_core::live_profile::ConnectionState::Connected,
            )?;
            Ok(store
                .google_account_bindings()?
                .into_iter()
                .find(|b| b.account == account)
                .unwrap()
                .connection_generation)
        })
        .await
        .unwrap();
    release.wait().await;
    let receipt = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(receipt["state"], "acknowledged");
    assert!(h.status().await["baseline"].is_null());
    assert_eq!(
        receipt["operation"]["ack"]["message_id"],
        "remote-message-1"
    );
    h.ctx.connection_generation = generation;
    let observed = reconcile(&h.ctx, &h.id).await.unwrap();
    assert_eq!(observed["state"], "conflict");
    assert_eq!(observed["conflict"]["reason"], "connection_changed");
    assert!(observed["baseline"].is_null());
    assert_eq!(http.0.lock().unwrap().writes, 1);
    let local = h.draft().await;
    let replace = json!({"draft_id":h.id,"expected_revision":local["revision"],"expected_google_revision":observed["revision"],"request_id":"prepared-reconnect-replace","action":"replace_local"});
    h.freeze(replace.clone()).await;
    // This prepared target exists even though no valid baseline was ever
    // published. Its opaque target must still trigger RAW preflight on resume.
    http.0.lock().unwrap().message += 1;
    assert_eq!(execute(&h.ctx, replace).await.unwrap()["state"], "conflict");
    assert_eq!(http.0.lock().unwrap().writes, 1);
    assert_eq!(h.draft().await["body"], "Frozen identity across reconnect");
    h.close();
}

#[test]
fn ipc_grammar_is_exact_and_send_is_absent() {
    assert!(validate(&json!({"draft_id":"d"}), false).is_ok());
    assert!(validate(
        &json!({"draft_id":"d","expected_revision":1,"request_id":"r"}),
        true
    )
    .is_ok());
    assert!(validate(&json!({"draft_id":"d","expected_revision":1,"expected_google_revision":2,"request_id":"r","action":"replace_local"}),true).is_ok());
    for bad in [
        json!({"draft_id":"d","send":true}),
        json!({"draft_id":"d","expected_revision":0,"request_id":"r"}),
        json!({"draft_id":"d","expected_revision":1,"request_id":"r","action":"send","expected_google_revision":1}),
    ] {
        assert!(validate(&bad, true).is_err());
    }
}
