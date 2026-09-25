use super::*;
use crate::core_tasks::CoreBridge;
use fm_google::{
    sync::{prepare_full_page, ListPage, MessageContent, MessageRef, PageStore},
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
        TransportError,
    },
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, Semaphore};

#[derive(Clone, Copy)]
enum PostResult {
    Success,
    LostResponse,
    LostBeforeApply,
    Rejected,
    CancelAfterApply,
}
struct State {
    labels: BTreeSet<String>,
    history: u64,
    requests: Vec<(HttpMethod, String)>,
    post_result: PostResult,
}
struct Http(Mutex<State>);
impl HttpTransport for Http {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-access"
            );
            let suffix = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/messages/first")
                .expect("captured message only");
            let mut state = self.0.lock().unwrap();
            state
                .requests
                .push((request.method(), request.url().into()));
            if request.method() == HttpMethod::Get {
                assert!(suffix.contains("fields="));
                assert!(!suffix.contains("format=raw"));
                return Ok(HttpResponse{status:200,body:serde_json::to_vec(&json!({"id":"first","threadId":"thread","historyId":state.history.to_string(),"labelIds":state.labels})).unwrap(),retry_after_secs:None,etag:None});
            }
            assert_eq!(request.method(), HttpMethod::Post);
            if matches!(state.post_result, PostResult::LostBeforeApply) {
                state.post_result = PostResult::Success;
                return Err(TransportError::Network);
            }
            if matches!(state.post_result, PostResult::Rejected) {
                return Ok(HttpResponse {
                    status: 403,
                    body: br#"{"error":{"errors":[{"reason":"forbidden"}]}}"#.to_vec(),
                    retry_after_secs: None,
                    etag: None,
                });
            }
            match suffix {
                "/modify" => {
                    let body: serde_json::Value =
                        serde_json::from_slice(request.body_bytes()).unwrap();
                    for label in body["addLabelIds"].as_array().unwrap() {
                        state.labels.insert(label.as_str().unwrap().into());
                    }
                    for label in body["removeLabelIds"].as_array().unwrap() {
                        state.labels.remove(label.as_str().unwrap());
                    }
                }
                "/trash" => {
                    state.labels.insert("TRASH".into());
                }
                "/untrash" => {
                    state.labels.remove("TRASH");
                }
                _ => panic!("unexpected state mutation {suffix}"),
            }
            state.history += 1;
            let outcome = state.post_result;
            state.post_result = PostResult::Success;
            match outcome {
                PostResult::LostResponse => Err(TransportError::Network),
                PostResult::CancelAfterApply => {
                    cancel.cancel();
                    Err(TransportError::Cancelled)
                }
                _ => Ok(HttpResponse {
                    status: 200,
                    body: br#"{"id":"first","threadId":"thread"}"#.to_vec(),
                    retry_after_secs: None,
                    etag: None,
                }),
            }
        })
    }
}
struct Harness {
    ctx: SyncContext,
    core: CoreBridge,
    account: AccountId,
    conversation: String,
    message: String,
    http: Arc<Http>,
    directory: tempfile::TempDir,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new(outcome: PostResult) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile");
        let (sender, mut receiver) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let account = store
                .bind_google_account(
                    "mutation-worker-fixture",
                    "reader@example.test",
                    &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
                    None,
                )
                .unwrap();
            let cp = store.google_begin_full(&account, "100").unwrap();
            let plan = prepare_full_page(
                &cp,
                ListPage {
                    messages: vec![MessageRef {
                        id: "first".into(),
                        thread_id: "thread".into(),
                    }],
                    next_page_token: None,
                    result_size_estimate: Some(1),
                },
            )
            .unwrap();
            let mut adapter = fm_core::google_store::GooglePageStore::new(&mut store);
            adapter.stage_message(&cp,MessageContent{id:"first".into(),thread_id:"thread".into(),history_id:"100".into(),label_ids:vec!["INBOX".into(),"UNREAD".into(),"STARRED".into(),"Label_unrelated".into()],source_mime:b"From: Human <sender@example.test>\r\nTo: reader@example.test\r\nSubject: State fixture\r\nDate: Fri, 11 Sep 2026 15:00:00 +0000\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSynthetic retained body\r\n".to_vec()}).unwrap();
            adapter.commit_page(&plan).unwrap();
            let list = store
                .handle(
                    "conversations",
                    Some(&account),
                    json!({"folder":"all","limit":10}),
                )
                .unwrap();
            let conversation = list["conversations"][0]["id"].as_str().unwrap().to_owned();
            let detail = store
                .handle("conversation", Some(&account), json!({"id":conversation}))
                .unwrap();
            let message = detail["message_ids"][0].as_str().unwrap().to_owned();
            ready.send((account, conversation, message)).unwrap();
            while let Some(work) = receiver.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected writer job");
                }
            }
        });
        let (account, conversation, message) = wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let account = AccountId::new(account).unwrap();
        let core = CoreBridge::new(sender, Arc::new(Semaphore::new(8)));
        let http = Arc::new(Http(Mutex::new(State {
            labels: ["INBOX", "UNREAD", "STARRED", "Label_unrelated"]
                .into_iter()
                .map(Into::into)
                .collect(),
            history: 100,
            requests: vec![],
            post_result: outcome,
        })));
        let ctx = SyncContext::for_test(
            core.clone(),
            account.clone(),
            1,
            http.clone(),
            "synthetic-access".into(),
            CancellationToken::new(),
        );
        Self {
            ctx,
            core,
            account,
            conversation,
            message,
            http,
            directory,
            writer,
        }
    }
    async fn action(&self, request: &str, method: &str, extra: serde_json::Value) {
        let account = self.account.as_str().to_owned();
        let conversation = self.conversation.clone();
        let message = self.message.clone();
        let request = request.to_owned();
        let method = method.to_owned();
        self.core.call(move |store|{
            let detail=store.handle("conversation",Some(&account),json!({"id":conversation}))?;
            let mut p=json!({"id":conversation,"message_ids":[message],"request_id":request,"expected_revision":detail["revision"]});
            for (key,value) in extra.as_object().unwrap() {p[key]=value.clone();}
            store.handle(&method,Some(&account),p)
        }).await.unwrap();
    }
    async fn status(&self) -> serde_json::Value {
        let account = self.account.as_str().to_owned();
        self.core
            .call(move |store| store.google_mail_mutation_status(&account))
            .await
            .unwrap()
    }
    fn posts(&self) -> usize {
        self.http
            .0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(method, _)| *method == HttpMethod::Post)
            .count()
    }
    fn flags(&self) -> (bool, bool) {
        let db = rusqlite::Connection::open_with_flags(
            self.directory.path().join("profile/store.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        db.query_row(
            "SELECT unread,starred FROM messages WHERE account=?1",
            [self.account.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }
    fn close(self) {
        drop(self.ctx);
        drop(self.core);
        self.writer.join().unwrap();
    }
}

#[tokio::test]
async fn done_is_immediate_locally_and_one_message_post_preserves_other_labels() {
    let h = Harness::new(PostResult::Success);
    h.action("done", "flag", json!({"read":true,"starred":false}))
        .await;
    assert_eq!(h.flags(), (false, false));
    assert!(h.http.0.lock().unwrap().requests.is_empty());
    let mut worker = MailMutationWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(h.posts(), 1);
    assert_eq!(h.status().await["counts"]["reconcile"], 1);
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(h.status().await["counts"]["confirmed"], 1);
    assert_eq!(h.posts(), 1);
    assert_eq!(
        h.http.0.lock().unwrap().labels,
        BTreeSet::from(["INBOX".into(), "Label_unrelated".into()])
    );
    h.close();
}

#[tokio::test]
async fn lost_post_response_restarts_with_readback_and_never_repeats_applied_write() {
    let h = Harness::new(PostResult::LostResponse);
    h.action("done", "flag", json!({"read":true,"starred":false}))
        .await;
    let mut worker = MailMutationWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(h.posts(), 1);
    let requests = h.http.0.lock().unwrap().requests.len();
    assert!(matches!(
        worker.step(&h.ctx, 999).await.unwrap(),
        Step::Idle(_)
    ));
    assert_eq!(h.http.0.lock().unwrap().requests.len(), requests);
    let mut restarted = MailMutationWorker::new(h.account.clone());
    restarted.step(&h.ctx, 1000).await.unwrap();
    assert_eq!(h.posts(), 1);
    assert_eq!(h.status().await["counts"]["confirmed"], 1);
    h.close();
}

#[tokio::test]
async fn uncertain_unapplied_write_is_retried_only_after_a_fresh_readback() {
    let h = Harness::new(PostResult::LostBeforeApply);
    h.action("read", "flag", json!({"read":true})).await;
    let mut worker = MailMutationWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(h.posts(), 1);
    worker.step(&h.ctx, 1000).await.unwrap();
    assert_eq!(h.posts(), 2);
    let methods = h
        .http
        .0
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|(method, _)| *method)
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        vec![
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Get,
            HttpMethod::Post
        ]
    );
    assert!(matches!(
        worker.step(&h.ctx, 1999).await.unwrap(),
        Step::Idle(_)
    ));
    worker.step(&h.ctx, 2000).await.unwrap();
    assert_eq!(h.posts(), 2);
    assert_eq!(h.status().await["counts"]["confirmed"], 1);
    h.close();
}

#[tokio::test]
async fn cancelled_after_apply_persists_attempt_outcome_and_reconnect_readback_confirms() {
    let mut h = Harness::new(PostResult::CancelAfterApply);
    h.action("read", "flag", json!({"read":true})).await;
    let mut worker = MailMutationWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    assert!(h.ctx.cancel.is_cancelled());
    assert_eq!(h.status().await["counts"]["reconcile"], 1);
    h.ctx = SyncContext::for_test(
        h.core.clone(),
        h.account.clone(),
        1,
        h.http.clone(),
        "synthetic-access".into(),
        CancellationToken::new(),
    );
    worker.step(&h.ctx, 1000).await.unwrap();
    assert_eq!(h.posts(), 1);
    assert_eq!(h.status().await["counts"]["confirmed"], 1);
    h.close();
}

#[tokio::test]
async fn permission_failure_stays_visible_without_retrying_or_reverting_local_intent() {
    let h = Harness::new(PostResult::Rejected);
    h.action("read", "flag", json!({"read":true})).await;
    let mut worker = MailMutationWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(h.status().await["counts"]["failed"], 1);
    assert_eq!(
        h.status().await["failures"][0]["error_code"],
        "google_permission_denied"
    );
    assert!(!h.flags().0);
    worker.step(&h.ctx, 999999).await.unwrap();
    assert_eq!(h.posts(), 1);
    h.close();
}

#[tokio::test]
async fn trash_and_restore_use_distinct_endpoints_without_invented_inbox_changes() {
    let h = Harness::new(PostResult::Success);
    let mut worker = MailMutationWorker::new(h.account.clone());
    h.action("trash", "mail_filing", json!({"action":"trash"}))
        .await;
    worker.step(&h.ctx, 0).await.unwrap();
    worker.step(&h.ctx, 0).await.unwrap();
    h.action("restore", "mail_filing", json!({"action":"restore"}))
        .await;
    worker.step(&h.ctx, 0).await.unwrap();
    worker.step(&h.ctx, 0).await.unwrap();
    let requests = h.http.0.lock().unwrap().requests.clone();
    assert_eq!(h.posts(), 2);
    assert!(requests
        .iter()
        .any(|(method, url)| *method == HttpMethod::Post && url.ends_with("/trash")));
    assert!(requests
        .iter()
        .any(|(method, url)| *method == HttpMethod::Post && url.ends_with("/untrash")));
    assert_eq!(h.flags(), (true, true));
    assert_eq!(h.status().await["counts"]["confirmed"], 2);
    h.close();
}

#[tokio::test]
async fn model_spam_label_moves_the_message_to_gmail_spam_and_drops_the_local_copy() {
    let h = Harness::new(PostResult::Success);
    let account = h.account.as_str().to_owned();
    let conversation = h.conversation.clone();
    let message = h.message.clone();
    h.core
        .call(move |store| {
            let settings = json!({"endpoint":"http://127.0.0.1:11434",
                "model":{"name":"fixture","digest":"a".repeat(64)},"accounts":{}});
            store.local_model_handle(
                "assist_settings_save",
                None,
                &json!({"expected_revision":0,"settings":settings}),
            )?;
            store.label_edit(&account,"label_create",&json!({"request_id":"create-promotions","id":"promotions","name":"Promotions"}))?;
            store.label_edit(&account,"label_auto_spam",&json!({"request_id":"promotions-spam","id":"promotions","expected_auto_spam":false,"auto_spam":true}))?;
            let plan = store.local_model_prepare(&account, &conversation, &message)?;
            store.local_model_finish(
                &plan,
                json!({"summary":"A promotion.","sender_kind":"automated","category_ids":["promotions"]}),
            )
        })
        .await
        .unwrap();
    assert!(
        h.http.0.lock().unwrap().requests.is_empty(),
        "classification itself contacts no provider"
    );
    let mut worker = MailMutationWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(h.posts(), 1);
    let requests = h.http.0.lock().unwrap().requests.clone();
    assert!(requests
        .iter()
        .any(|(method, url)| *method == HttpMethod::Post && url.ends_with("/modify")));
    assert_eq!(
        h.http.0.lock().unwrap().labels,
        ["SPAM", "UNREAD", "STARRED", "Label_unrelated"]
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<String>>(),
        "SPAM added and INBOX removed; review state and other labels untouched"
    );
    let db = rusqlite::Connection::open_with_flags(
        h.directory.path().join("profile/store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let kept: i64 = db
        .query_row(
            "SELECT count(*) FROM messages WHERE account=?1",
            [h.account.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept, 0, "confirmed Spam is not kept locally");
    drop(db);
    h.close();
}
