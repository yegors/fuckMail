use super::*;
use crate::core_tasks::CoreBridge;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_core::{google_live_mail::GoogleMailCoverage, live_profile::ConnectionState};
use fm_google::transport::{
    CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
    TransportError,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, Notify, Semaphore};

#[derive(Default)]
struct Gate {
    arrived: Notify,
    release: Notify,
}
struct FixtureState {
    history: String,
    recent: Vec<String>,
    inventory: Vec<String>,
    list_page_size: usize,
    raw: BTreeMap<String, Value>,
    history_replies: VecDeque<(u16, Value)>,
    requests: Vec<String>,
    fail_once: Option<String>,
    cancel_once: Option<String>,
    pause_once: Option<(String, Arc<Gate>)>,
}
struct FixtureHttp(Mutex<FixtureState>);
impl FixtureHttp {
    fn new(count: usize, recent_count: usize, body_size: usize) -> Self {
        let ids: Vec<String> = (0..count).map(|n| format!("m{n:03}")).collect();
        Self(Mutex::new(FixtureState {
            history: "100".into(),
            recent: ids.iter().take(recent_count).cloned().collect(),
            inventory: ids.clone(),
            list_page_size: usize::MAX,
            raw: ids
                .iter()
                .map(|id| (id.clone(), raw(id, "100", body_size)))
                .collect(),
            history_replies: VecDeque::new(),
            requests: Vec::new(),
            fail_once: None,
            cancel_once: None,
            pause_once: None,
        }))
    }
    fn raw_count(&self, id: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|url| url.contains(&format!("/messages/{id}?")))
            .count()
    }
}
impl HttpTransport for FixtureHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(
                request.method(),
                HttpMethod::Get,
                "read worker must never mutate"
            );
            assert!(request.body_bytes().is_empty());
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-access"
            );
            let suffix = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/")
                .expect("fixed Gmail origin");
            let (status, response, gate, cancel_after) = {
                let mut state = self.0.lock().unwrap();
                state.requests.push(request.url().to_owned());
                if suffix == "profile" {
                    (
                        200,
                        json!({"emailAddress":"reader@example.test","messagesTotal":999999,"threadsTotal":999999,"historyId":state.history}),
                        None,
                        false,
                    )
                } else if suffix.starts_with("messages?") {
                    assert!(suffix.contains("includeSpamTrash=true"));
                    let ids = if suffix.contains("q=after%3A") {
                        &state.recent
                    } else {
                        assert!(!suffix.contains("&q="));
                        &state.inventory
                    };
                    let page: usize = suffix
                        .split('&')
                        .find_map(|part| part.strip_prefix("pageToken=page-"))
                        .map(|page| page.parse().unwrap())
                        .unwrap_or(0);
                    let first = page.saturating_mul(state.list_page_size);
                    let end = first.saturating_add(state.list_page_size).min(ids.len());
                    let next = (end < ids.len()).then(|| format!("page-{}", page + 1));
                    (
                        200,
                        json!({"messages":ids[first..end].iter().map(|id|json!({"id":id,"threadId":format!("thread-{id}")})).collect::<Vec<_>>(),"resultSizeEstimate":999999,"nextPageToken":next}),
                        None,
                        false,
                    )
                } else if suffix.starts_with("history?") {
                    let (status, value) = state
                        .history_replies
                        .pop_front()
                        .unwrap_or_else(|| (200, json!({"historyId":state.history,"history":[]})));
                    (status, value, None, false)
                } else if let Some(raw_path) = suffix.strip_prefix("messages/") {
                    let id = raw_path.strip_suffix("?format=raw").unwrap();
                    if state.fail_once.as_deref() == Some(id) {
                        state.fail_once = None;
                        return Err(TransportError::Network);
                    }
                    let cancel_after = state.cancel_once.as_deref() == Some(id);
                    if cancel_after {
                        state.cancel_once = None;
                    }
                    let gate = if state
                        .pause_once
                        .as_ref()
                        .is_some_and(|(paused, _)| paused == id)
                    {
                        state.pause_once.take().map(|(_, gate)| gate)
                    } else {
                        None
                    };
                    match state.raw.get(id) {
                        Some(value) => (200, value.clone(), gate, cancel_after),
                        None => (404, json!({}), gate, cancel_after),
                    }
                } else {
                    panic!("unexpected read URL: {suffix}");
                }
            };
            if let Some(gate) = gate {
                gate.arrived.notify_one();
                gate.release.notified().await;
            }
            if cancel_after {
                cancel.cancel();
            }
            Ok(HttpResponse {
                status,
                body: serde_json::to_vec(&response).unwrap(),
                retry_after_secs: None,
                etag: None,
            })
        })
    }
}

fn raw(id: &str, history: &str, body_size: usize) -> Value {
    let body = if body_size == 0 {
        id.to_owned()
    } else {
        "x".repeat(body_size)
    };
    let source = format!("From: Human <sender@example.test>\r\nTo: reader@example.test\r\nSubject: retained {id}\r\nDate: Fri, 11 Sep 2020 01:02:03 +0000\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n");
    json!({"id":id,"threadId":format!("thread-{id}"),"labelIds":["INBOX","UNREAD"],"historyId":history,
        "internalDate":"1789210800123","sizeEstimate":source.len(),"raw":URL_SAFE_NO_PAD.encode(source.as_bytes())})
}

struct Harness {
    ctx: SyncContext,
    core: CoreBridge,
    http: Arc<FixtureHttp>,
    account: AccountId,
    directory: tempfile::TempDir,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new(count: usize, recent: usize, body_size: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile");
        let (send, mut receive) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let account = store
                .bind_google_account(
                    "synthetic-worker-subject",
                    "reader@example.test",
                    &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
                    None,
                )
                .unwrap();
            ready.send(account).unwrap();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected test writer work");
                }
            }
        });
        let account = AccountId::new(wait.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
        let core = CoreBridge::new(send, Arc::new(Semaphore::new(8)));
        let http = Arc::new(FixtureHttp::new(count, recent, body_size));
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
            http,
            account,
            directory,
            writer,
        }
    }
    fn sql_count(&self, sql: &str) -> u64 {
        let db = rusqlite::Connection::open_with_flags(
            self.directory.path().join("profile/store.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        db.query_row(sql, [self.account.as_str()], |row| row.get(0))
            .unwrap()
    }
    async fn coverage(&self) -> GoogleMailCoverage {
        let account = self.account.as_str().to_owned();
        self.core
            .call(move |store| store.google_live_coverage(&account))
            .await
            .unwrap()
    }
    async fn idle(&self, worker: &mut MailWorker) {
        for _ in 0..100 {
            if matches!(worker.step(&self.ctx, 0).await.unwrap(), Step::Idle(_)) {
                return;
            }
        }
        panic!("fixture did not reach idle");
    }
    fn close(self) {
        drop(self.ctx);
        drop(self.core);
        self.writer.join().unwrap();
    }
}

#[tokio::test]
async fn recent_first_full_inventory_and_final_catchup_have_exact_coverage() {
    let h = Harness::new(14, 3, 0);
    let mut worker = MailWorker::new(h.account.clone());
    assert_eq!(worker.step(&h.ctx, 0).await.unwrap(), Step::Progress);
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        0
    );
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        3
    );
    assert!(!h.coverage().await.inventory_complete);
    h.idle(&mut worker).await;
    let coverage = h.coverage().await;
    assert!(coverage.offline_complete && coverage.delta_current && coverage.inventory_complete);
    assert_eq!(
        (
            coverage.observed,
            coverage.retained_sources,
            coverage.indexed,
            coverage.complete_attachments,
            coverage.unavailable
        ),
        (14, 14, 14, 14, 0)
    );
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        14
    );
    assert_eq!(
        h.sql_count("SELECT count(*) FROM google_message_state WHERE account=?1 AND deleted=0"),
        14
    );
    assert_eq!(h.sql_count("SELECT count(*) FROM google_live_page_items WHERE account=?1 AND applied_revision IS NULL"), 0);
    let requests = h.http.0.lock().unwrap().requests.clone();
    assert!(requests[0].ends_with("/profile"));
    assert!(requests[1].contains("q=after%3A"));
    assert_eq!(
        requests
            .iter()
            .filter(|url| url.ends_with("/profile"))
            .count(),
        1
    );
    let count = requests.len();
    assert_eq!(
        worker.step(&h.ctx, 1).await.unwrap(),
        Step::Idle(Duration::from_millis(59_999))
    );
    assert_eq!(
        h.http.0.lock().unwrap().requests.len(),
        count,
        "idle cache reads cannot trigger HTTP"
    );
    h.close();
}

#[tokio::test]
async fn interrupted_part_resumes_saved_ids_and_verified_manifests_without_relisting() {
    let h = Harness::new(12, 12, 0);
    h.http.0.lock().unwrap().fail_once = Some("m005".into());
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(
        worker.step(&h.ctx, 0).await.unwrap_err().code,
        "google_transport_interrupted"
    );
    assert_eq!(h.sql_count("SELECT count(*) FROM google_live_page_items WHERE account=?1 AND manifest_json IS NOT NULL"), 5);
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        0
    );
    let mut resumed = MailWorker::new(h.account.clone());
    resumed.step(&h.ctx, 0).await.unwrap();
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        10
    );
    assert_eq!(h.sql_count("SELECT count(*) FROM google_live_pages WHERE account=?1 AND lane='recent' AND complete=0"), 1);
    assert_eq!(
        h.http.raw_count("m000"),
        1,
        "staged content must be verified, not downloaded again"
    );
    assert_eq!(h.http.raw_count("m005"), 2);
    assert_eq!(
        h.http
            .0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|url| url.contains("q=after%3A"))
            .count(),
        1
    );
    h.idle(&mut resumed).await;
    assert!(h.coverage().await.offline_complete);
    assert!(h.sql_count("SELECT coalesce(max(n),0) FROM (SELECT count(*) n FROM google_live_page_items WHERE account=?1 AND part_key IS NOT NULL GROUP BY lane,generation,expected_revision,part_key)") <= 10);
    h.close();
}

#[tokio::test]
async fn normalized_byte_ceiling_splits_part_and_retains_overflow_manifest() {
    let h = Harness::new(4, 4, 240 * 1024);
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        2
    );
    assert_eq!(h.sql_count("SELECT count(*) FROM google_live_page_items WHERE account=?1 AND applied_revision IS NULL AND manifest_json IS NOT NULL"), 1);
    assert!(h.sql_count("SELECT coalesce(max(n),0) FROM (SELECT sum(length(manifest_json)) n FROM google_live_page_items WHERE account=?1 AND part_key IS NOT NULL GROUP BY lane,generation,expected_revision,part_key)") <= MAX_PART_BYTES as u64);
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        4
    );
    assert_eq!(h.http.raw_count("m002"), 1);
    h.close();
}

#[tokio::test]
async fn saved_inventory_continuation_advances_only_after_all_page_items_commit() {
    let h = Harness::new(9, 0, 0);
    {
        let mut state = h.http.0.lock().unwrap();
        state.list_page_size = 3;
        state.fail_once = Some("m004".into());
    }
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    worker.step(&h.ctx, 0).await.unwrap();
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        3
    );
    assert!(worker.step(&h.ctx, 0).await.is_err());
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        3
    );
    let mut resumed = MailWorker::new(h.account.clone());
    h.idle(&mut resumed).await;
    assert!(h.coverage().await.offline_complete);
    assert_eq!(h.coverage().await.observed, 9);
    let requests = h.http.0.lock().unwrap().requests.clone();
    assert_eq!(
        requests
            .iter()
            .filter(|url| url.contains("pageToken=page-1"))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|url| url.contains("pageToken=page-2"))
            .count(),
        1
    );
    assert_eq!(
        h.http.raw_count("m003"),
        1,
        "uncommitted saved manifest resumes without a second RAW get"
    );
    h.close();
}

#[tokio::test]
async fn pending_delta_gets_a_turn_after_four_recent_parts() {
    let h = Harness::new(60, 60, 0);
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    for _ in 0..4 {
        worker.step(&h.ctx, 0).await.unwrap();
    }
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        40
    );
    worker.step(&h.ctx, 0).await.unwrap();
    let requests = h.http.0.lock().unwrap().requests.clone();
    assert!(requests.last().unwrap().contains("/history?"));
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        40
    );
    assert_eq!(h.http.raw_count("m040"), 0);
    h.close();
}

#[tokio::test]
async fn expired_history_preserves_source_until_explicit_missing_reconciliation() {
    let h = Harness::new(1, 1, 0);
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    let old_source = {
        let db = rusqlite::Connection::open_with_flags(
            h.directory.path().join("profile/store.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        db.query_row("SELECT source_hash,source_size FROM google_mail_coverage WHERE account=?1 AND remote_id='m000'", [h.account.as_str()], |row|Ok((row.get::<_,String>(0)?,row.get::<_,u64>(1)?))).unwrap()
    };
    {
        let mut state = h.http.0.lock().unwrap();
        state.history = "200".into();
        state.recent = vec!["new".into()];
        state.inventory = vec!["new".into()];
        state.raw = BTreeMap::from([("new".into(), raw("new", "200", 0))]);
        state.history_replies.push_back((404, json!({})));
    }
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap(); // Start a fresh Delta cycle.
    worker.step(&h.ctx, 0).await.unwrap(); // 404, fresh profile baseline, resync.
    assert_eq!(h.coverage().await.generation, 2);
    assert!(!h.coverage().await.inventory_complete);
    assert_eq!(
        h.sql_count("SELECT count(*) FROM google_message_state WHERE account=?1 AND deleted=0"),
        1
    );
    h.idle(&mut worker).await;
    assert!(h.coverage().await.offline_complete);
    assert_eq!(h.sql_count("SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=1 AND remote_id='m000'"), 1);
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        2,
        "provider removal must retain original message/source rows"
    );
    assert_eq!(
        h.http.raw_count("m000"),
        3,
        "recent, initial inventory, then explicit missing recheck"
    );
    let retained = h
        .core
        .call(move |store| store.verified_blob(&old_source.0, old_source.1))
        .await
        .unwrap();
    assert!(retained.is_file());
    h.close();
}

#[tokio::test]
async fn authoritative_history_deletion_never_fetches_or_mutates_remote_mail() {
    let h = Harness::new(1, 1, 0);
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    let reads = h.http.raw_count("m000");
    h.http.0.lock().unwrap().history_replies.push_back((200, json!({"historyId":"201","history":[{"id":"201","messagesDeleted":[{"message":{"id":"m000","threadId":"thread-m000"}}]}]})));
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    assert_eq!(h.http.raw_count("m000"), reads);
    assert_eq!(
        h.sql_count("SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=1"),
        1
    );
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        1
    );
    assert_eq!(h.coverage().await.observed, 0);
    h.close();
}

#[tokio::test]
async fn cancellation_during_raw_response_never_publishes_or_advances_page() {
    let h = Harness::new(1, 1, 0);
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    h.http.0.lock().unwrap().cancel_once = Some("m000".into());
    assert!(worker.step(&h.ctx, 0).await.is_err());
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        0
    );
    assert_eq!(
        h.sql_count("SELECT count(*) FROM google_live_pages WHERE account=?1 AND complete=0"),
        1
    );
    h.close();
}

#[tokio::test]
async fn permanent_source_and_remote_draft_gaps_stay_incomplete_without_retry_loops() {
    let h = Harness::new(3, 3, 0);
    {
        let mut state = h.http.0.lock().unwrap();
        state.raw.get_mut("m000").unwrap()["internalDate"] = Value::Null;
        state.raw.get_mut("m001").unwrap()["labelIds"] = json!(["DRAFT"]);
        state.raw.get_mut("m002").unwrap()["raw"] = json!("invalid!base64");
    }
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    let coverage = h.coverage().await;
    assert!(coverage.inventory_complete && coverage.delta_current);
    assert!(!coverage.offline_complete);
    assert_eq!(
        (
            coverage.observed,
            coverage.retained_sources,
            coverage.indexed,
            coverage.unavailable
        ),
        (3, 2, 0, 3)
    );
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        0
    );
    let account = h.account.as_str().to_owned();
    let reasons = h
        .core
        .call(move |store| store.google_live_gaps(&account, None, 100))
        .await
        .unwrap()
        .into_iter()
        .map(|gap| gap.issue)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        reasons,
        BTreeSet::from([
            "invalid_internal_date".into(),
            "remote_draft_pending".into(),
            "malformed_response".into()
        ])
    );
    let before: Vec<usize> = (0..3)
        .map(|n| h.http.raw_count(&format!("m{n:03}")))
        .collect();
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    assert_eq!(
        (0..3)
            .map(|n| h.http.raw_count(&format!("m{n:03}")))
            .collect::<Vec<_>>(),
        before
    );
    h.close();
}

#[tokio::test]
async fn paused_http_leaves_writer_available_and_new_connection_fences_old_result() {
    let h = Harness::new(1, 1, 0);
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    let gate = Arc::new(Gate::default());
    h.http.0.lock().unwrap().pause_once = Some(("m000".into(), gate.clone()));
    let account = h.account.as_str().to_owned();
    let intervention = async {
        tokio::time::timeout(Duration::from_secs(5), gate.arrived.notified())
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(2),
            h.core.call(move |store| {
                store.set_google_connection_state(&account, ConnectionState::Disconnected)
            }),
        )
        .await
        .expect("writer must remain available while Google HTTP is paused")
        .unwrap();
        gate.release.notify_one();
    };
    let (result, ()) = tokio::join!(worker.step(&h.ctx, 0), intervention);
    assert!(result.is_err());
    assert_eq!(
        h.sql_count("SELECT count(*) FROM messages WHERE account=?1"),
        0
    );
    assert_eq!(
        h.sql_count("SELECT count(*) FROM google_live_pages WHERE account=?1 AND complete=0"),
        1
    );
    h.close();
}
