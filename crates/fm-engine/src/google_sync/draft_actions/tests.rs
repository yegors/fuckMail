use super::*;
use crate::{
    core_tasks::CoreBridge,
    google_sync::{mail::Step, remote_drafts::RemoteDraftWorker},
};
use fm_google::{
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
        TransportError,
    },
    AccountId,
};
use serde_json::json;
use std::{collections::BTreeSet, sync::Mutex, time::Duration};
use tokio::sync::{mpsc, Notify, Semaphore};

#[derive(Default)]
struct Gate {
    entered: Notify,
    release: Notify,
}
#[derive(Default)]
struct State {
    missing: bool,
    version: u8,
    deletes: usize,
    lose: bool,
    hold_read: Option<Arc<Gate>>,
    hold_delete: Option<Arc<Gate>>,
    uploads: usize,
    sends: usize,
    uploaded: Option<Vec<u8>>,
    reads: usize,
}
struct Http {
    states: [Mutex<State>; 2],
}
fn raw(index: usize, version: u8) -> Vec<u8> {
    format!("From: Author <author{index}@example.test>\r\nTo: Receiver <receiver@example.test>\r\nBcc: Blind <blind@example.test>\r\nSubject: Authored version {version}\r\nMessage-ID: <source-{index}-{version}@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nRetained text {index}/{version}").into_bytes()
}
fn response(status: u16, body: Value) -> std::result::Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status,
        body: if status == 204 {
            vec![]
        } else {
            serde_json::to_vec(&body).unwrap()
        },
        retry_after_secs: None,
        etag: None,
    })
}
impl HttpTransport for Http {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            let index = if request.headers()["authorization"] == "Bearer action-0" {
                0
            } else {
                assert_eq!(request.headers()["authorization"], "Bearer action-1");
                1
            };
            let path = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/")
                .unwrap();
            if path == "profile" {
                return response(
                    200,
                    json!({"emailAddress":format!("author{index}@example.test"),"historyId":"100","messagesTotal":0,"threadsTotal":0}),
                );
            }
            if path == "drafts?maxResults=50" {
                return response(
                    200,
                    json!({"drafts":[{"id":"target","message":{"id":"m0","threadId":"thread"}}]}),
                );
            }
            if request.method() == HttpMethod::Get {
                assert_eq!(path, "drafts/target?format=raw");
                let hold = self.states[index].lock().unwrap().hold_read.take();
                if let Some(hold) = hold {
                    hold.entered.notify_one();
                    tokio::select! {_=cancel.cancelled()=>return Err(TransportError::Cancelled),_=hold.release.notified()=>{}}
                }
                let mut s = self.states[index].lock().unwrap();
                s.reads += 1;
                if s.missing {
                    return response(404, json!({}));
                }
                return response(
                    200,
                    json!({"id":"target","message":{"id":format!("m{}",s.version),"threadId":"thread","raw":URL_SAFE_NO_PAD.encode(s.uploaded.clone().unwrap_or_else(||raw(index,s.version)))}}),
                );
            }
            if request.method() == HttpMethod::Delete {
                assert_eq!(path, "drafts/target");
                assert!(request.body_bytes().is_empty());
                let hold = {
                    let mut s = self.states[index].lock().unwrap();
                    s.deletes += 1;
                    s.hold_delete.take()
                };
                // Deliberately allow a late definitive response after cancellation.
                if let Some(hold) = hold {
                    hold.entered.notify_one();
                    hold.release.notified().await;
                }
                let mut s = self.states[index].lock().unwrap();
                let existed = !s.missing;
                s.missing = true;
                if s.lose {
                    return Err(TransportError::Network);
                }
                return response(if existed { 204 } else { 404 }, json!({}));
            }
            let value: Value = serde_json::from_slice(request.body_bytes()).unwrap();
            let mut s = self.states[index].lock().unwrap();
            if path == "drafts/target" && request.method() == HttpMethod::Put {
                s.uploads += 1;
                s.version += 1;
                s.uploaded = Some(
                    URL_SAFE_NO_PAD
                        .decode(value["message"]["raw"].as_str().unwrap())
                        .unwrap(),
                );
                return response(
                    200,
                    json!({"id":"target","message":{"id":format!("m{}",s.version),"threadId":"thread"}}),
                );
            }
            assert_eq!(path, "drafts/send");
            assert_eq!(request.method(), HttpMethod::Post);
            assert_eq!(value["id"], "target");
            s.sends += 1;
            s.missing = true;
            response(200, json!({"id":"sent","threadId":"thread"}))
        })
    }
}
struct Harness {
    _directory: tempfile::TempDir,
    ctx: Vec<SyncContext>,
    http: Arc<Http>,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_owned();
        let (queue, mut receive) = mpsc::channel(16);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let accounts = [0, 1].map(|i| {
                store
                    .bind_google_account(
                        &format!("draft-actions-{i}"),
                        &format!("author{i}@example.test"),
                        &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
                        None,
                    )
                    .unwrap()
            });
            ready.send(accounts).unwrap();
            while let Some(crate::Work::Provider(call)) = receive.blocking_recv() {
                call.run(&mut store);
            }
        });
        let accounts = wait.recv_timeout(Duration::from_secs(5)).unwrap();
        let core = CoreBridge::new(queue, Arc::new(Semaphore::new(16)));
        let http = Arc::new(Http {
            states: Default::default(),
        });
        let mut ctx: Vec<_> = accounts
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                SyncContext::for_test(
                    core.clone(),
                    AccountId::new(id).unwrap(),
                    1,
                    http.clone(),
                    format!("action-{i}"),
                    CancellationToken::new(),
                )
            })
            .collect();
        ctx[1].quota = ctx[0].quota.clone();
        ctx[1].preparation = ctx[0].preparation.clone();
        Self {
            _directory: directory,
            ctx,
            http,
            writer,
        }
    }
    async fn imported(&self, index: usize) -> Value {
        let mut worker = RemoteDraftWorker::default();
        for _ in 0..6 {
            if matches!(worker.step(&self.ctx[index]).await.unwrap(), Step::Idle(_)) {
                break;
            }
        }
        let account = self.ctx[index].account.as_str().to_owned();
        self.ctx[index]
            .core
            .call(move |store| store.handle("drafts", Some(&account), json!({})))
            .await
            .unwrap()["drafts"][0]
            .clone()
    }
    fn close(self) {
        drop(self.ctx);
        self.writer.join().unwrap();
    }
}
fn params(row: &Value, id: &str) -> Value {
    json!({"draft_id":row["draft_id"],"source_revision":row["remote_source"]["source_revision"],"request_id":id})
}

#[test]
fn draft_action_fields_and_identifier_bounds_are_exact() {
    let mut p =
        json!({"draft_id":"x".repeat(256),"source_revision":1,"request_id":"r".repeat(128)});
    assert!(validate("google_draft_delete", &p).is_ok());
    p["request_id"] = json!("r".repeat(129));
    assert!(validate("google_draft_delete", &p).is_err());
    p["request_id"] = json!("r");
    p["source_revision"] = json!(0);
    assert!(validate("google_draft_delete", &p).is_err());
    assert!(validate(
        "google_draft_delete_reconcile",
        &json!({"request_id":"r","retry":true})
    )
    .is_err());
}

#[tokio::test]
async fn two_account_delete_unknown_replay_and_absence_never_become_acknowledged() {
    let h = Harness::new();
    let one = h.imported(0).await;
    let two = h.imported(1).await;
    h.http.states[0].lock().unwrap().lose = true;
    let request = params(&one, "same-request");
    assert_eq!(
        delete(&h.ctx[0], request.clone()).await.unwrap()["state"],
        "acceptance_unknown"
    );
    assert_eq!(
        delete(&h.ctx[0], request).await.unwrap()["state"],
        "acceptance_unknown"
    );
    let evidence = reconcile(&h.ctx[0], "same-request").await.unwrap();
    assert_eq!(evidence["state"], "acceptance_unknown");
    assert_eq!(evidence["read_evidence"]["draft_exists"], false);
    assert_eq!(evidence["read_evidence"]["deletion_proven"], false);
    assert_eq!(
        delete(&h.ctx[1], params(&two, "same-request"))
            .await
            .unwrap()["state"],
        "deleted"
    );
    assert_eq!(h.http.states[0].lock().unwrap().deletes, 1);
    assert_eq!(h.http.states[1].lock().unwrap().deletes, 1);
    assert_eq!(h.ctx[0].preparation.available_permits(), 1);
    h.close();
}

#[tokio::test]
async fn changed_source_and_cancelled_read_do_not_claim_but_late_delete_receipt_survives_close() {
    let h = Harness::new();
    let other = h.imported(1).await;
    h.http.states[1].lock().unwrap().version = 1;
    let refusal = adopt(&h.ctx[1], params(&other, "fresh-stale-adopt"))
        .await
        .unwrap_err();
    assert_eq!(refusal.code, "stale_source_revision");
    assert_eq!(
        refusal.details["draft_authoring_invocation_finished_without_commit"],
        true
    );
    assert_eq!(h.http.states[1].lock().unwrap().uploads, 0);
    let row = h.imported(0).await;
    h.http.states[0].lock().unwrap().version = 1;
    let error = delete(&h.ctx[0], params(&row, "stale")).await.unwrap_err();
    assert_eq!(error.code, "stale_source_revision");
    assert_eq!(
        error.details["draft_delete_invocation_finished_before_dispatch"],
        true
    );
    assert_eq!(h.http.states[0].lock().unwrap().deletes, 0);
    h.http.states[0].lock().unwrap().version = 0;
    let row = h.imported(0).await;
    let gate = Arc::new(Gate::default());
    h.http.states[0].lock().unwrap().hold_read = Some(gate.clone());
    let mut child = h.ctx[0].clone();
    child.cancel = h.ctx[0].cancel.child_token();
    let cancel = child.cancel.clone();
    let p = params(&row, "cancelled-read");
    let task = tokio::spawn(async move { delete(&child, p).await });
    tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert!(task.await.unwrap().is_err());
    assert_eq!(h.http.states[0].lock().unwrap().deletes, 0);
    let gate = Arc::new(Gate::default());
    h.http.states[0].lock().unwrap().hold_delete = Some(gate.clone());
    let mut child = h.ctx[0].clone();
    child.cancel = h.ctx[0].cancel.child_token();
    let cancel = child.cancel.clone();
    let p = params(&row, "late");
    let task = tokio::spawn(async move { delete(&child, p).await });
    tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let account = h.ctx[0].account.as_str().to_owned();
    h.ctx[0]
        .core
        .call(move |store| {
            store.set_google_connection_state(
                &account,
                fm_core::live_profile::ConnectionState::Disconnected,
            )
        })
        .await
        .unwrap();
    gate.release.notify_one();
    assert_eq!(task.await.unwrap().unwrap()["state"], "deleted");
    assert_eq!(h.ctx[0].preparation.available_permits(), 1);
    h.close();
}

#[tokio::test]
async fn adopted_target_waits_for_edit_and_immediate_send_uses_original_container() {
    let h = Harness::new();
    let one = h.imported(0).await;
    let two = h.imported(1).await;
    let adopted = adopt(&h.ctx[0], params(&one, "adopt-one")).await.unwrap();
    assert_eq!(adopted["google"]["state"], "adopted");
    assert!(adopted["google"]["baseline"].is_null());
    assert!(matches!(
        drafts::step(&h.ctx[0]).await.unwrap(),
        Step::Idle(_)
    ));
    let account = h.ctx[0].account.as_str().to_owned();
    let mut save = adopted.clone();
    save["body"] = json!("New deliberate text");
    let request = json!({"draft_id":save["draft_id"],"expected_revision":save["revision"],"request_id":"edit","body":save["body"]});
    let local = crate::local_content::Runtime::new(h.ctx[0].core.clone());
    let saved = local
        .handle(
            CancellationToken::new(),
            "draft_save",
            Some(&account),
            request,
        )
        .await
        .unwrap();
    drop(local);
    assert_eq!(drafts::execute(&h.ctx[0],json!({"draft_id":saved["draft_id"],"expected_revision":saved["revision"],"request_id":"upload"})).await.unwrap()["state"],"synced");
    assert_eq!(h.http.states[0].lock().unwrap().uploads, 1);
    let adopted = adopt(&h.ctx[1], params(&two, "adopt-two")).await.unwrap();
    let sent=crate::google_sync::send::execute(&h.ctx[1],json!({"draft_id":adopted["draft_id"],"expected_revision":adopted["revision"],"expected_google_revision":adopted["google"]["revision"],"request_id":"send-adopted"})).await.unwrap();
    assert_eq!(sent["state"], "accepted");
    assert_eq!(sent["provider_draft_id"], "target");
    assert_eq!(h.http.states[1].lock().unwrap().uploads, 0);
    assert_eq!(h.http.states[1].lock().unwrap().sends, 1);
    h.close();
}

#[tokio::test]
async fn local_assessment_copy_and_replay_need_no_provider_owner_and_preserve_source() {
    let h = Harness::new();
    let original = h.imported(0).await;
    let before = h.http.states[0].lock().unwrap().reads;
    h.ctx[0].cancel.cancel();
    let account = h.ctx[0].account.as_str().to_owned();
    let runtime = crate::local_content::Runtime::new(h.ctx[0].core.clone());
    let assessment = runtime.handle(CancellationToken::new(), "google_draft_authoring", Some(&account), json!({"draft_id":original["draft_id"],"source_revision":original["remote_source"]["source_revision"]})).await.unwrap();
    assert_eq!(assessment["copy"]["allowed"], true);
    let refusal=runtime.handle(CancellationToken::new(),"google_draft_copy",Some(&account),json!({"draft_id":original["draft_id"],"source_revision":original["remote_source"]["source_revision"],"request_id":"bad-copy","from_policy":"invented","content_policy":"plain_text","inline_policy":"attachments","thread_policy":"omit"})).await.unwrap_err();
    assert_eq!(
        refusal.details["draft_authoring_invocation_finished_without_commit"],
        true
    );
    let mut request = params(&original, "copy-one");
    for (key, value) in [
        ("from_policy", "primary"),
        ("content_policy", "plain_text"),
        ("inline_policy", "attachments"),
        ("thread_policy", "omit"),
    ] {
        request[key] = json!(value);
    }
    let copied = runtime
        .handle(
            CancellationToken::new(),
            "google_draft_copy",
            Some(&account),
            request.clone(),
        )
        .await
        .unwrap();
    assert_eq!(copied["origin"], "local");
    assert_eq!(copied["body"], "Retained text 0/0");
    assert_eq!(copied["bcc_recipients"][0]["email"], "blind@example.test");
    assert_ne!(copied["draft_id"], original["draft_id"]);
    assert!(copied["google"]["baseline"].is_null());
    assert_eq!(
        runtime
            .handle(
                CancellationToken::new(),
                "google_draft_copy",
                Some(&account),
                request
            )
            .await
            .unwrap(),
        copied
    );
    let id = original["draft_id"].clone();
    let retained = h.ctx[0]
        .core
        .call(move |store| store.handle("draft_get", Some(&account), json!({"draft_id":id})))
        .await
        .unwrap();
    assert_eq!(retained["origin"], "google_snapshot");
    assert_eq!(retained["body"], original["body"]);
    assert_eq!(h.http.states[0].lock().unwrap().reads, before);
    drop(runtime);
    h.close();
}
