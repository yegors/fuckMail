use super::*;
use crate::core_tasks::CoreBridge;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_google::{
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
        TransportError,
    },
    AccountId,
};
use serde_json::json;
use std::{sync::Mutex, time::Duration};
use tokio::sync::{mpsc, Barrier, Semaphore};

const EMAIL: &str = "author@example.test";
#[derive(Default)]
struct Remote {
    draft: Option<Vec<u8>>,
    sent: Vec<(String, Vec<u8>)>,
    paths: Vec<String>,
    lose_ack: bool,
    malformed: bool,
    reject: bool,
    paged: bool,
    profile_email: Option<String>,
    entered: Option<Arc<Barrier>>,
    release: Option<Arc<Barrier>>,
}
#[derive(Default)]
struct Http(Mutex<Remote>);
impl HttpTransport for Http {
    fn execute<'a>(&'a self, request: HttpRequest, _: &'a CancellationToken) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-send-token"
            );
            let path = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/")
                .unwrap()
                .to_owned();
            self.0.lock().unwrap().paths.push(path.clone());
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
                    json!({"emailAddress":email,"historyId":"100","messagesTotal":0,"threadsTotal":0}),
                );
            }
            if request.method() == HttpMethod::Get {
                let state = self.0.lock().unwrap();
                if path == "drafts/remote-draft?format=raw" {
                    return match &state.draft {
                        Some(raw) => response(
                            200,
                            json!({"id":"remote-draft","message":{"id":"draft-message","threadId":"remote-thread","raw":URL_SAFE_NO_PAD.encode(raw)}}),
                        ),
                        None => response(404, json!({})),
                    };
                }
                if path.starts_with("messages?") {
                    assert!(path.contains("maxResults=10"));
                    assert!(path.contains("includeSpamTrash=false"));
                    assert!(path.contains("rfc822msgid"));
                    let rows: Vec<Value> = state
                        .sent
                        .iter()
                        .take(10)
                        .map(|(id, _)| json!({"id":id,"threadId":"sent-thread"}))
                        .collect();
                    return response(
                        200,
                        json!({"messages":rows,"nextPageToken":if state.paged {Some("next-not-read")} else {None}}),
                    );
                }
                let id = path
                    .strip_prefix("messages/")
                    .unwrap()
                    .split('?')
                    .next()
                    .unwrap();
                assert!(path.contains("format=full") && path.contains("historyId"));
                return if state.sent.iter().any(|(known, _)| known == id) {
                    response(
                        200,
                        json!({"id":id,"threadId":"sent-thread","historyId":"201","labelIds":["SENT"]}),
                    )
                } else {
                    response(404, json!({}))
                };
            }
            let body: Value = serde_json::from_slice(request.body_bytes()).unwrap();
            if path == "drafts" {
                assert_eq!(request.method(), HttpMethod::Post);
                self.0.lock().unwrap().draft = Some(
                    URL_SAFE_NO_PAD
                        .decode(body["message"]["raw"].as_str().unwrap())
                        .unwrap(),
                );
                return response(
                    200,
                    json!({"id":"remote-draft","message":{"id":"draft-message","threadId":"remote-thread"}}),
                );
            }
            assert_eq!(request.method(), HttpMethod::Post);
            assert!(matches!(path.as_str(), "drafts/send" | "messages/send"));
            let raw = if path == "drafts/send" {
                assert_eq!(body["id"], "remote-draft");
                body["message"]["raw"].as_str().unwrap()
            } else {
                body["raw"].as_str().unwrap()
            };
            let bytes = URL_SAFE_NO_PAD.decode(raw).unwrap();
            let (entered, release) = {
                let state = self.0.lock().unwrap();
                if state.reject {
                    return response(
                        403,
                        json!({"error":{"code":403,"errors":[{"reason":"forbidden"}]}}),
                    );
                }
                (state.entered.clone(), state.release.clone())
            };
            if let Some(entered) = entered {
                entered.wait().await;
            }
            // Deliberately return an actual late ACK even after cancellation.
            if let Some(release) = release {
                release.wait().await;
            }
            let mut state = self.0.lock().unwrap();
            let id = format!("sent-{}", state.sent.len() + 1);
            state.sent.push((id.clone(), bytes));
            if path == "drafts/send" {
                state.draft = None;
            }
            if state.lose_ack {
                return Err(TransportError::Network);
            }
            if state.malformed {
                return response(200, json!({"id":id}));
            }
            response(200, json!({"id":id,"threadId":"sent-thread"}))
        })
    }
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
            let account = store
                .google_account_bindings()
                .unwrap()
                .first()
                .map(|binding| binding.account.clone())
                .unwrap_or_else(|| {
                    store
                        .bind_google_account(
                            "send-worker-fixture",
                            EMAIL,
                            &BTreeSet::from(
                                ["https://www.googleapis.com/auth/gmail.modify".into()],
                            ),
                            None,
                        )
                        .unwrap()
                });
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
            "synthetic-send-token".into(),
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
        let current = self.draft().await;
        let account = self.ctx.account.as_str().to_owned();
        let id = self.id.clone();
        let body = body.to_owned();
        let request = request.to_owned();
        self.core.call(move|store|store.handle("draft_save",Some(&account),json!({"draft_id":id,"expected_revision":current["revision"],"request_id":request,"subject":"Explicit human submission","body":body,"to_recipients":[{"name":"Recipient","email":"to@example.test"}],"cc_recipients":[{"email":"cc@example.test"}],"bcc_recipients":[{"email":"bcc@example.test"}]}))).await.unwrap()
    }
    async fn request(&self, id: &str) -> Value {
        let local = self.draft().await;
        json!({"draft_id":self.id,"expected_revision":local["revision"],"expected_google_revision":local["google"]["revision"],"request_id":id})
    }
    async fn status(&self, id: &str) -> Value {
        let account = self.ctx.account.as_str().to_owned();
        let id = id.to_owned();
        self.core
            .call(move |store| store.google_send_status(&account, &id))
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
        let scope = drafts::verified_scope(&self.ctx).await.unwrap();
        let step = self
            .core
            .call(move |store| store.google_send_begin(&scope, &request))
            .await
            .unwrap();
        let GoogleSendStep::Prepare(plan) = step else {
            panic!("expected new frozen send")
        };
        let proof = tokio::task::spawn_blocking(move || {
            fm_core::google_send::prepare_google_send(plan, &CancellationToken::new())
        })
        .await
        .unwrap()
        .unwrap();
        self.core
            .call(move |store| store.google_send_finish(proof))
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

#[tokio::test]
async fn direct_submission_retains_exact_recipients_body_ack_and_immutable_replay() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let first = h.save("Human wrote this exact message.", "save").await;
    let source = dir.path().join("authored.bin");
    let attachment: Vec<u8> = (0..=255).collect();
    std::fs::write(&source, &attachment).unwrap();
    let imported=h.local("draft_attachment_import",json!({"draft_id":h.id,"expected_revision":first["revision"],"request_id":"attach","path":source})).await;
    std::fs::remove_file(source).unwrap();
    let local = imported["draft"].clone();
    let request = h.request("explicit-first").await;
    let accepted = execute(&h.ctx, request.clone()).await.unwrap();
    assert_eq!(accepted["state"], "accepted");
    assert_eq!(
        accepted["ack"],
        json!({"message_id":"sent-1","thread_id":"sent-thread"})
    );
    assert_eq!(accepted["draft_revision"], local["revision"]);
    let raw = String::from_utf8(h.http.0.lock().unwrap().sent[0].1.clone()).unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(raw.as_bytes())
        .unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap().trim_end_matches(['\r', '\n']),
        "Human wrote this exact message."
    );
    assert_eq!(parsed.attachments().count(), 1);
    assert_eq!(parsed.attachment(0).unwrap().contents(), attachment);
    for text in [
        "to@example.test",
        "cc@example.test",
        "bcc@example.test",
        "Subject: Explicit human submission",
        "Message-ID:",
        "From:",
    ] {
        assert!(raw.contains(text), "missing {text}");
    }
    assert!(raw.contains("authored.bin"));
    assert!(raw
        .replace("\r\n", "")
        .contains(&base64::engine::general_purpose::STANDARD.encode(&attachment)));
    assert_eq!(
        execute(&h.ctx, request.clone()).await.unwrap()["state"],
        "accepted"
    );
    let mut changed = request;
    changed["expected_revision"] = json!(999);
    assert!(execute(&h.ctx, changed).await.is_err());
    assert_eq!(h.http.0.lock().unwrap().sent.len(), 1);
    assert_eq!(h.draft().await["body"], local["body"]);
    let checked = reconcile(&h.ctx, "explicit-first").await.unwrap();
    assert_eq!(checked["state"], "accepted");
    assert_eq!(checked["read_evidence"]["draft_observation"], "not_checked");
    assert_eq!(
        checked["read_evidence"]["acknowledged_message"]["message_id"],
        "sent-1"
    );
    h.close();
}

#[tokio::test]
async fn known_twin_sends_frozen_latest_mime_and_consumed_twin_never_reappears() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let initial = h.save("Original uploaded draft", "save1").await;
    let twin = drafts::execute(
        &h.ctx,
        json!({"draft_id":h.id,"expected_revision":initial["revision"],"request_id":"twin"}),
    )
    .await
    .unwrap();
    assert_eq!(twin["state"], "synced");
    h.save("Latest explicitly submitted human text", "save2")
        .await;
    let accepted = execute(&h.ctx, h.request("send-twin").await).await.unwrap();
    assert_eq!(accepted["provider_draft_id"], "remote-draft");
    assert_eq!(accepted["state"], "accepted");
    let bytes = h.http.0.lock().unwrap().sent[0].1.clone();
    let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap().trim_end_matches(['\r', '\n']),
        "Latest explicitly submitted human text"
    );
    h.save("A separate newer explicit send", "save3").await;
    assert!(matches!(
        drafts::step(&h.ctx).await.unwrap(),
        super::super::mail::Step::Idle(_)
    ));
    let next = execute(&h.ctx, h.request("send-newer").await)
        .await
        .unwrap();
    assert_eq!(next["state"], "accepted");
    assert!(next["provider_draft_id"].is_null());
    let paths = h.http.0.lock().unwrap().paths.clone();
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == "drafts/send")
            .count(),
        1
    );
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == "messages/send")
            .count(),
        1
    );
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == "drafts")
            .count(),
        1
    );
    h.close();
}

#[tokio::test]
async fn lost_ack_restart_and_sent_match_never_replay_or_promote_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    let h = Harness::new(dir.path(), http.clone());
    h.save("Frozen uncertain text", "save").await;
    let request = h.request("lost-ack").await;
    http.0.lock().unwrap().lose_ack = true;
    assert_eq!(
        execute(&h.ctx, request.clone()).await.unwrap()["state"],
        "acceptance_unknown"
    );
    h.close();
    let h = Harness::new(dir.path(), http.clone());
    http.0.lock().unwrap().lose_ack = false;
    http.0.lock().unwrap().paged = true;
    assert_eq!(
        execute(&h.ctx, request).await.unwrap()["state"],
        "acceptance_unknown"
    );
    let before = http.0.lock().unwrap().paths.len();
    let checked = reconcile(&h.ctx, "lost-ack").await.unwrap();
    assert_eq!(checked["state"], "acceptance_unknown");
    assert!(checked["ack"].is_null());
    assert_eq!(checked["read_evidence"]["truncated"], true);
    assert_eq!(
        checked["read_evidence"]["matches"][0]["message_id"],
        "sent-1"
    );
    assert_eq!(checked["read_evidence"]["draft_observation"], "not_checked");
    assert!(!http.0.lock().unwrap().paths[before..]
        .iter()
        .any(|path| path.starts_with("drafts/")));
    h.save("Newer unsent local text", "save2").await;
    assert!(matches!(
        drafts::step(&h.ctx).await.unwrap(),
        super::super::mail::Step::Idle(_)
    ));
    assert!(execute(&h.ctx, h.request("must-not-bypass-unknown").await)
        .await
        .is_err());
    assert_eq!(http.0.lock().unwrap().sent.len(), 1);
    h.close();
}

#[tokio::test]
async fn prepared_restart_resume_keeps_original_mime_despite_newer_authored_revision() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    let h = Harness::new(dir.path(), http.clone());
    let original = h.save("Original explicitly frozen text", "save1").await;
    let request = h.request("prepared").await;
    assert_eq!(h.freeze(request.clone()).await["state"], "prepared");
    h.save("Newer local text must survive", "save2").await;
    h.close();
    let h = Harness::new(dir.path(), http.clone());
    let sent = execute(&h.ctx, request).await.unwrap();
    assert_eq!(sent["state"], "accepted");
    assert_eq!(sent["draft_revision"], original["revision"]);
    let raw = http.0.lock().unwrap().sent[0].1.clone();
    let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap().trim_end_matches(['\r', '\n']),
        "Original explicitly frozen text"
    );
    assert_eq!(h.draft().await["body"], "Newer local text must survive");
    h.close();
}

#[tokio::test]
async fn concurrent_request_and_late_cancelled_ack_keep_exact_receipt_and_newer_local_text() {
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(Http::default());
    let mut h = Harness::new(dir.path(), http.clone());
    let old = h.save("Exact admitted message", "save1").await;
    let request = h.request("late-ack").await;
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    {
        let mut state = http.0.lock().unwrap();
        state.entered = Some(entered.clone());
        state.release = Some(release.clone());
    }
    let ctx = h.ctx.clone();
    let task = tokio::spawn(async move { execute(&ctx, request).await });
    tokio::time::timeout(Duration::from_secs(5), entered.wait())
        .await
        .unwrap();
    assert_eq!(h.status("late-ack").await["state"], "in_flight");
    // The message's size is known while it uploads; nothing else is listed.
    let upload = progress(h.ctx.account.as_str(), "late-ack");
    assert_eq!(upload["uploading"], true);
    assert!(upload["total"].as_u64().unwrap() > 0);
    assert_eq!(
        progress(h.ctx.account.as_str(), "simultaneous")["uploading"],
        false
    );
    assert!(execute(&h.ctx, h.request("simultaneous").await)
        .await
        .is_err());
    h.save("Newer human edits are still here", "save2").await;
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
    h.ctx.cancel.cancel();
    release.wait().await;
    let receipt = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(receipt["state"], "accepted");
    assert_eq!(
        progress(h.ctx.account.as_str(), "late-ack")["uploading"],
        false
    );
    assert_eq!(receipt["draft_revision"], old["revision"]);
    assert_eq!(receipt["ack"]["thread_id"], "sent-thread");
    assert_eq!(h.draft().await["body"], "Newer human edits are still here");
    assert_eq!(http.0.lock().unwrap().sent.len(), 1);
    h.ctx.connection_generation = generation;
    h.ctx.cancel = CancellationToken::new();
    assert!(matches!(
        drafts::step(&h.ctx).await.unwrap(),
        super::super::mail::Step::Idle(_)
    ));
    h.close();
}

#[tokio::test]
async fn quota_wait_retains_identifiers_only_and_cached_queries_progress_before_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("Queued authored body", "save").await;
    let held = h
        .ctx
        .quota
        .acquire(
            fm_google::scheduler::Work {
                account: h.ctx.account.clone(),
                service: fm_google::scheduler::Service::Gmail,
                class: fm_google::scheduler::WorkClass::DraftMutation,
                units: 10,
            },
            &h.ctx.cancel,
        )
        .await
        .unwrap();
    let ctx = h.ctx.clone();
    let request = h.request("queued").await;
    let mut task = tokio::spawn(async move { execute(&ctx, request).await });
    assert!(tokio::time::timeout(Duration::from_millis(50), &mut task)
        .await
        .is_err());
    let current = tokio::time::timeout(Duration::from_secs(1), h.draft())
        .await
        .unwrap();
    assert!(current["google_send"].is_null());
    assert_eq!(h.ctx.preparation.available_permits(), 1);
    assert!(h.http.0.lock().unwrap().sent.is_empty());
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
async fn recipientless_and_wrong_primary_from_fail_before_any_submission() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    let invalid=execute(&h.ctx,json!({"draft_id":h.id,"expected_revision":1,"expected_google_revision":0,"request_id":"x".repeat(129)})).await.unwrap_err();
    assert_eq!(invalid.code, "invalid_request");
    assert_eq!(
        invalid.details["send_invocation_finished_before_dispatch"],
        true
    );
    assert!(h.http.0.lock().unwrap().paths.is_empty());
    // A durable B1-valid recipient-free draft is not valid explicit Send input.
    let addressed = h.save("Recipientless authored draft", "save-empty").await;
    h.local("draft_save",json!({"draft_id":h.id,"expected_revision":addressed["revision"],"request_id":"remove-recipients","body":"Recipientless authored draft","to_recipients":[],"cc_recipients":[],"bcc_recipients":[]})).await;
    let refused = execute(&h.ctx, h.request("recipientless").await)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "invalid_draft");
    assert_eq!(
        refused.details["send_invocation_finished_before_dispatch"],
        true
    );
    h.save("Saved addressed message", "save").await;
    h.http.0.lock().unwrap().profile_email = Some("other@example.test".into());
    assert!(execute(&h.ctx, h.request("wrong-from").await)
        .await
        .is_err());
    assert!(h.http.0.lock().unwrap().sent.is_empty());
    assert!(h.draft().await["google_send"].is_null());
    h.close();
}

#[tokio::test]
async fn known_rejection_and_malformed_acceptance_have_distinct_durable_states() {
    for (malformed, expected) in [(false, "rejected"), (true, "acceptance_unknown")] {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path(), Arc::new(Http::default()));
        h.save("Explicit response classification", "save").await;
        {
            let mut state = h.http.0.lock().unwrap();
            state.reject = !malformed;
            state.malformed = malformed;
        }
        let request = h.request("response").await;
        let reply = execute(&h.ctx, request.clone()).await.unwrap();
        assert_eq!(reply["state"], expected);
        assert_eq!(execute(&h.ctx, request).await.unwrap()["state"], expected);
        assert_eq!(
            h.http
                .0
                .lock()
                .unwrap()
                .paths
                .iter()
                .filter(|path| path.as_str() == "messages/send")
                .count(),
            1
        );
        h.close();
    }
}

#[tokio::test]
async fn explicit_same_request_resume_racing_memory_wait_dispatches_once() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("One explicit identity", "save").await;
    let request = h.request("same-request").await;
    let memory = h.ctx.preparation.clone().acquire_owned().await.unwrap();
    let ctx = h.ctx.clone();
    let first_request = request.clone();
    let mut first = tokio::spawn(async move { execute(&ctx, first_request).await });
    assert!(tokio::time::timeout(Duration::from_millis(40), &mut first)
        .await
        .is_err());
    let ctx = h.ctx.clone();
    let mut second = tokio::spawn(async move { execute(&ctx, request).await });
    assert!(tokio::time::timeout(Duration::from_millis(40), &mut second)
        .await
        .is_err());
    assert!(h.draft().await["google_send"].is_null());
    assert!(h.http.0.lock().unwrap().sent.is_empty());
    drop(memory);
    for task in [first, second] {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap()["state"],
            "accepted"
        );
    }
    assert_eq!(h.http.0.lock().unwrap().sent.len(), 1);
    h.close();
}

#[tokio::test]
async fn claimed_unavailable_mime_is_known_unsent_and_only_revision_bound_abandon_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("Frozen source", "save").await;
    let request = h.request("unavailable-after-claim").await;
    let frozen = h.freeze(request.clone()).await;
    let blob = dir
        .path()
        .join("blobs")
        .join(frozen["upload_sha256"].as_str().unwrap());
    std::fs::remove_file(blob).unwrap();
    let receipt = execute(&h.ctx, request).await.unwrap();
    assert_eq!(receipt["state"], "prepared");
    assert_eq!(
        receipt["revision"].as_i64().unwrap(),
        frozen["revision"].as_i64().unwrap() + 2
    );
    assert!(!receipt["error"].is_null());
    assert!(h.http.0.lock().unwrap().sent.is_empty());
    let account = h.ctx.account.as_str().to_owned();
    let stale = frozen["revision"].clone();
    assert!(h
        .core
        .call(move |store| store.google_send_abandon(
            &account,
            &json!({"request_id":"unavailable-after-claim","expected_revision":stale})
        ))
        .await
        .is_err());
    let account = h.ctx.account.as_str().to_owned();
    let abandoned=h.core.call(move|store|store.google_send_abandon(&account,&json!({"request_id":"unavailable-after-claim","expected_revision":receipt["revision"]}))).await.unwrap();
    assert_eq!(abandoned["state"], "abandoned");
    assert_eq!(h.draft().await["body"], "Frozen source");
    h.close();
}

#[tokio::test]
async fn read_only_grant_and_disconnected_context_cannot_submit() {
    for disconnected in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path(), Arc::new(Http::default()));
        h.save("Preserved unsent text", "save").await;
        let account = h.ctx.account.as_str().to_owned();
        let generation = h
            .core
            .call(move |store| {
                if disconnected {
                    store.set_google_connection_state(
                        &account,
                        fm_core::live_profile::ConnectionState::Disconnected,
                    )?;
                } else {
                    store.bind_google_account(
                        "send-worker-fixture",
                        EMAIL,
                        &BTreeSet::from(["https://www.googleapis.com/auth/gmail.readonly".into()]),
                        None,
                    )?;
                }
                Ok(store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|b| b.account == account)
                    .unwrap()
                    .connection_generation)
            })
            .await
            .unwrap();
        h.ctx.connection_generation = generation;
        let error = execute(&h.ctx, h.request("unavailable-capability").await)
            .await
            .unwrap_err();
        assert_eq!(
            error.details["send_invocation_finished_before_dispatch"],
            true
        );
        assert!(h.http.0.lock().unwrap().sent.is_empty());
        assert_eq!(h.draft().await["body"], "Preserved unsent text");
        h.close();
    }
}

#[tokio::test]
async fn reconciliation_bounds_first_page_and_reads_distinct_ack_without_raw_or_resend() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save("Read evidence only", "save").await;
    execute(&h.ctx, h.request("accepted").await).await.unwrap();
    let before = {
        let mut state = h.http.0.lock().unwrap();
        for index in 0..10 {
            state
                .sent
                .insert(index, (format!("other-{index}"), Vec::new()));
        }
        state.paged = true;
        state.paths.len()
    };
    let checked = reconcile(&h.ctx, "accepted").await.unwrap();
    assert_eq!(checked["state"], "accepted");
    assert_eq!(
        checked["read_evidence"]["matches"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(
        checked["read_evidence"]["acknowledged_message"]["message_id"],
        "sent-1"
    );
    assert_eq!(checked["read_evidence"]["truncated"], true);
    let paths = h.http.0.lock().unwrap().paths[before..].to_vec();
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.starts_with("messages?"))
            .count(),
        1
    );
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.starts_with("messages/"))
            .count(),
        11
    );
    assert!(!paths.iter().any(|path| path.contains("format=raw")
        || path.ends_with("send")
        || path.starts_with("drafts")));
    h.close();
}

#[tokio::test]
async fn obsolete_unrecorded_request_invalidates_held_mime_proof_and_allows_newer_explicit_send() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path(), Arc::new(Http::default()));
    h.save(
        "Original request may have lost its preparation reply",
        "old",
    )
    .await;
    let original = h.request("unrecorded-original").await;
    let scope = drafts::verified_scope(&h.ctx).await.unwrap();
    let preparing = original.clone();
    let step = h
        .core
        .call(move |store| store.google_send_begin(&scope, &preparing))
        .await
        .unwrap();
    let GoogleSendStep::Prepare(plan) = step else {
        panic!("expected unrecorded MIME preparation")
    };
    let old_proof = tokio::task::spawn_blocking(move || {
        fm_core::google_send::prepare_google_send(plan, &CancellationToken::new())
    })
    .await
    .unwrap()
    .unwrap();
    assert!(h.draft().await["google_send"].is_null());
    h.save("Newer independently authored text", "newer").await;
    let refusal = execute(&h.ctx, original).await.unwrap_err();
    assert_eq!(refusal.code, "stale_revision");
    assert_eq!(
        refusal.details["send_request_obsolete_without_dispatch"],
        true
    );
    let late_finish = h
        .core
        .call(move |store| store.google_send_finish(old_proof))
        .await
        .unwrap_err();
    assert_eq!(
        late_finish.details["send_request_obsolete_without_dispatch"],
        true
    );
    let scope = drafts::verified_scope(&h.ctx).await.unwrap();
    let claim = h
        .core
        .call(move |store| store.google_send_claim(&scope, "unrecorded-original", 1))
        .await;
    assert!(
        matches!(claim,Err(ref error) if error.code=="not_found" && error.details["send_request_obsolete_without_dispatch"].is_null())
    );
    assert!(h.http.0.lock().unwrap().sent.is_empty());
    assert!(h.draft().await["google_send"].is_null());
    let accepted = execute(&h.ctx, h.request("new-explicit-request").await)
        .await
        .unwrap();
    assert_eq!(accepted["state"], "accepted");
    let bytes = h.http.0.lock().unwrap().sent[0].1.clone();
    let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap().trim_end_matches(['\r', '\n']),
        "Newer independently authored text"
    );
    assert_eq!(h.http.0.lock().unwrap().sent.len(), 1);
    h.close();
}
