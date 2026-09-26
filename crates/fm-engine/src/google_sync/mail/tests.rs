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
    /// What Gmail lists for `in:trash`.
    trash: Vec<String>,
    recent_query: Option<String>,
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
            trash: Vec::new(),
            recent_query: None,
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
            .filter(|url| url.contains(&format!("/messages/{id}?format=raw")))
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
                } else if suffix.starts_with("labels/TRASH?") {
                    (
                        200,
                        json!({"id":"TRASH","messagesTotal":state.trash.len(),"threadsTotal":state.trash.len()}),
                        None,
                        false,
                    )
                } else if suffix.starts_with("messages?") && suffix.contains("q=in%3Atrash") {
                    (
                        200,
                        json!({"messages":state.trash.iter().map(|id|json!({"id":id,"threadId":format!("thread-{id}")})).collect::<Vec<_>>(),"resultSizeEstimate":state.trash.len()}),
                        None,
                        false,
                    )
                } else if suffix.starts_with("messages?") {
                    assert!(suffix.contains("includeSpamTrash=true"));
                    let after = suffix
                        .split('&')
                        .find(|part| part.starts_with("q=after%3A"));
                    if state.recent_query.is_none() {
                        state.recent_query = after.map(str::to_owned);
                    }
                    let ids = if after.is_some() && state.recent_query.as_deref() == after {
                        &state.recent
                    } else {
                        &state.inventory
                    };
                    let page: usize = suffix
                        .split('&')
                        .find_map(|part| part.strip_prefix("pageToken=page-"))
                        .map(|page| page.parse().unwrap())
                        .unwrap_or(0);
                    // Pages scale with the requested size: the 500-ID count
                    // pages hold five inventory pages, as in Gmail.
                    let requested: usize = suffix
                        .split('&')
                        .find_map(|part| {
                            part.strip_prefix("messages?maxResults=")
                                .or_else(|| part.strip_prefix("maxResults="))
                        })
                        .map(|size| size.parse().unwrap())
                        .unwrap_or(100);
                    let size = (state.list_page_size.saturating_mul(requested) / 100).max(1);
                    let first = page.saturating_mul(size);
                    let end = first.saturating_add(size).min(ids.len());
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
                } else if let Some(id) = suffix.strip_prefix("messages/").and_then(|path| {
                    path.strip_suffix("?format=full&fields=id%2CthreadId%2ClabelIds%2ChistoryId")
                }) {
                    // Label state only, for a message whose source is kept.
                    match state.raw.get(id) {
                        Some(value) => (
                            200,
                            json!({"id":value["id"],"threadId":value["threadId"],"labelIds":value["labelIds"],"historyId":value["historyId"]}),
                            None,
                            false,
                        ),
                        None => (404, json!({}), None, false),
                    }
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
        Self::with_scope(
            count,
            recent,
            body_size,
            "https://www.googleapis.com/auth/gmail.modify",
        )
    }
    /// A harness whose account granted `scope` for mail.
    fn with_scope(count: usize, recent: usize, body_size: usize, scope: &str) -> Self {
        let scope = scope.to_owned();
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
                    &BTreeSet::from([scope]),
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

// While the inventory downloads, each of its steps also counts IDs for
// up to two seconds, so progress has a total long before the download ends.
#[tokio::test]
async fn inventory_count_gives_progress_a_total_before_the_download_ends() {
    let h = Harness::new(14, 0, 0);
    h.http.0.lock().unwrap().list_page_size = 2;
    let mut worker = MailWorker::new(h.account.clone());
    worker.step(&h.ctx, 0).await.unwrap();
    let before = h.coverage().await;
    assert_eq!((before.listed, before.listing), (None, None));
    let mut seen = Vec::new();
    while !h.coverage().await.inventory_complete {
        worker.step(&h.ctx, 0).await.unwrap();
        let coverage = h.coverage().await;
        seen.push((coverage.listing, coverage.listed, coverage.retained_sources));
    }
    // A fast list finishes the count within the first inventory step, while
    // most of the mail is still to download.
    let first_total = seen.iter().position(|s| s.1 == Some(14)).expect("counted");
    assert!(
        seen[first_total].2 <= 2 && seen.iter().all(|s| s.0.is_none()),
        "the whole count arrives in one step: {seen:?}"
    );
    let requests = h.http.0.lock().unwrap().requests.clone();
    // The count lists exactly what the inventory lists.
    let query = |url: &String| {
        url.split('&')
            .find(|part| part.starts_with("q="))
            .map(str::to_owned)
    };
    // The last 100-ID page is the inventory's (the first is the recent lane's).
    let inventory = requests
        .iter()
        .rev()
        .find(|url| url.contains("maxResults=100"))
        .and_then(query);
    assert!(inventory.is_some());
    assert!(requests
        .iter()
        .filter(|url| url.contains("maxResults=500"))
        .all(|url| url.contains("includeSpamTrash=true") && query(url) == inventory));
    // Complete counts are not repeated.
    let counts = requests
        .iter()
        .filter(|url| url.contains("maxResults=500"))
        .count();
    h.idle(&mut worker).await;
    assert_eq!(
        h.http
            .0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|url| url.contains("maxResults=500"))
            .count(),
        counts
    );
    assert_eq!(h.coverage().await.listed, Some(14));
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
    let requests = h.http.0.lock().unwrap().requests.clone();
    let recent_url = requests
        .iter()
        .find(|url| url.contains("q=after%3A"))
        .unwrap();
    assert_eq!(requests.iter().filter(|url| *url == recent_url).count(), 1);
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
    // The inventory's own pages; its count lists 500 IDs a page.
    let listed = |token: &str| {
        requests
            .iter()
            .filter(|url| url.contains(token) && url.contains("maxResults=100"))
            .count()
    };
    assert_eq!(listed("pageToken=page-1"), 1);
    assert_eq!(listed("pageToken=page-2"), 1);
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

// Each step says whether it downloads older mail, so the app can show only
// new-mail checks as syncing while a long download runs.
#[tokio::test]
async fn steps_report_whether_they_download_older_mail() {
    let h = Harness::new(60, 60, 0);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let mut worker = MailWorker::new(h.account.clone());
    worker.observe_lanes(move |backfill| record.lock().unwrap().push(backfill));
    // The baseline step picks no lane.
    worker.step(&h.ctx, 0).await.unwrap();
    for _ in 0..5 {
        worker.step(&h.ctx, 0).await.unwrap();
    }
    // Four recent-mail parts, then the pending history check.
    assert_eq!(*seen.lock().unwrap(), [true, true, true, true, false]);
    // A refresh during the download checks for new mail next.
    worker.step(&h.ctx, 0).await.unwrap();
    worker.request_refresh();
    worker.step(&h.ctx, 0).await.unwrap();
    assert_eq!(seen.lock().unwrap()[5..], [true, false]);
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
    // The inventory relisting and the missing recheck read label state only:
    // Gmail never changes a kept message's source.
    assert_eq!(h.http.raw_count("m000"), 1, "downloaded once, by recent");
    let retained = h
        .core
        .call(move |store| store.verified_blob(&old_source.0, old_source.1))
        .await
        .unwrap();
    assert!(retained.is_file());
    h.close();
}

// A label change on a kept message reads Gmail's label state, never the
// source again: Gmail never changes a message's source.
#[tokio::test]
async fn label_changes_read_label_state_without_downloading_again() {
    let h = Harness::new(1, 1, 0);
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    let reads = h.http.raw_count("m000");
    {
        let mut state = h.http.0.lock().unwrap();
        let message = state.raw.get_mut("m000").unwrap();
        message["labelIds"] = json!(["INBOX"]);
        message["historyId"] = json!("201");
        state.history_replies.push_back((200, json!({"historyId":"201","history":[{"id":"201","labelsRemoved":[{"message":{"id":"m000","threadId":"thread-m000"},"labelIds":["UNREAD"]}]}]})));
    }
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    assert_eq!(h.http.raw_count("m000"), reads, "no second download");
    assert!(h
        .http
        .0
        .lock()
        .unwrap()
        .requests
        .iter()
        .any(|url| url.contains("/messages/m000?format=full")));
    assert_eq!(
        h.sql_count("SELECT count(*) FROM google_message_state WHERE account=?1 AND provider_labels_json NOT LIKE '%UNREAD%'"),
        1
    );
    h.close();
}

// Once a history check has started it runs to the end, so new mail never
// waits behind the download of older mail.
#[tokio::test]
async fn a_started_history_check_finishes_before_listing_resumes() {
    let h = Harness::new(60, 60, 0);
    let added: Vec<_> = (40..60)
        .map(|n| json!({"message":{"id":format!("m{n:03}"),"threadId":format!("thread-m{n:03}")}}))
        .collect();
    h.http.0.lock().unwrap().history_replies.push_back((
        200,
        json!({"historyId":"101","history":[{"id":"101","messagesAdded":added}]}),
    ));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let mut worker = MailWorker::new(h.account.clone());
    worker.observe_lanes(move |backfill| record.lock().unwrap().push(backfill));
    for _ in 0..8 {
        worker.step(&h.ctx, 0).await.unwrap();
    }
    // Four recent parts, the history check's two parts back to back, then
    // the listing again.
    assert_eq!(
        *seen.lock().unwrap(),
        [true, true, true, true, false, false, true]
    );
    h.close();
}

// A history scan held up by a bulk change made elsewhere must not hold back
// new mail: once the next check is due, or on a refresh, the newest mail is
// listed and downloaded before the scan continues.
#[tokio::test]
async fn new_mail_arrives_while_a_history_scan_is_behind() {
    let h = Harness::new(1, 1, 0);
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    let arrived = |id: &str| {
        let mut state = h.http.0.lock().unwrap();
        state.inventory.insert(0, id.to_owned());
        state.raw.insert(id.to_owned(), raw(id, "300", 0));
    };
    arrived("new1");
    {
        let mut state = h.http.0.lock().unwrap();
        state.history = "300".into();
        state.history_replies.push_back((200, json!({"historyId":"300","nextPageToken":"more","history":[{"id":"201","labelsRemoved":[{"message":{"id":"m000","threadId":"thread-m000"},"labelIds":["UNREAD"]}]}]})));
        state.history_replies.push_back((200, json!({"historyId":"300","history":[{"id":"300","messagesAdded":[{"message":{"id":"new1","threadId":"thread-new1"}},{"message":{"id":"new2","threadId":"thread-new2"}}]}]})));
    }
    let earlier = h
        .http
        .0
        .lock()
        .unwrap()
        .requests
        .iter()
        .filter(|url| url.contains("/history?"))
        .count();
    let history_pages = || {
        h.http
            .0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|url| url.contains("/history?"))
            .count()
            - earlier
    };
    let stored = |id: &str| {
        h.sql_count(&format!(
            "SELECT count(*) FROM messages WHERE account=?1 AND remote_id='{id}'"
        ))
    };
    let synced = || h.sql_count("SELECT count(*) FROM meta WHERE key='last_mail_sync:'||?1");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let mut worker = MailWorker::new(h.account.clone());
    worker.observe_lanes(move |backfill| record.lock().unwrap().push(backfill));
    let interval = h
        .core
        .call(|store| store.google_sync_interval_ms())
        .await
        .unwrap();
    let before = synced();
    worker.step(&h.ctx, 0).await.unwrap(); // Start a scan.
    worker.step(&h.ctx, 0).await.unwrap(); // Its first history page.
    assert_eq!((history_pages(), stored("new1")), (1, 0));

    // The next check falls due with the scan still running.
    worker.step(&h.ctx, interval).await.unwrap();
    assert_eq!((history_pages(), stored("new1")), (1, 1));
    assert_eq!(synced(), before.max(1));

    // The Sync button does the same at once.
    arrived("new2");
    worker.request_refresh();
    worker.step(&h.ctx, interval).await.unwrap();
    assert_eq!((history_pages(), stored("new2")), (1, 1));

    // The scan then finishes in the background, reading label state only for
    // the mail it already has.
    h.idle(&mut worker).await;
    assert_eq!(history_pages(), 2);
    assert_eq!((h.http.raw_count("new1"), h.http.raw_count("new2")), (1, 1));
    // Only the new-mail work shows as syncing once the scan is behind.
    assert_eq!(
        seen.lock().unwrap()[..5],
        [false, false, false, false, true]
    );
    h.close();
}

// Mail deleted in Gmail while its history was still being replayed stays
// in local Trash; emptying Trash removes those copies once Gmail's own Trash
// is empty, and sync never asks Gmail about them again.
#[tokio::test]
async fn emptying_trash_forgets_copies_of_mail_gmail_already_deleted() {
    let h = Harness::with_scope(3, 3, 0, "https://mail.google.com/");
    {
        let mut state = h.http.0.lock().unwrap();
        for id in ["m001", "m002"] {
            state.raw.get_mut(id).unwrap()["labelIds"] = json!(["TRASH"]);
        }
    }
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    let trashed = || h.sql_count("SELECT count(*) FROM messages WHERE account=?1 AND trashed=1");
    assert_eq!(trashed(), 2);
    // In Gmail, m002 was deleted for good and m001 moved out of Trash; sync
    // has not caught up with either.
    {
        let mut state = h.http.0.lock().unwrap();
        state.inventory.retain(|id| id != "m002");
        state.raw.remove("m002");
    }
    let counts = crate::google_sync::bin::counts(&h.ctx, json!({"folder":"trash"}))
        .await
        .unwrap();
    assert_eq!(
        (counts["messages"].as_u64(), counts["stored"].as_u64()),
        (Some(0), Some(2))
    );

    let result = crate::google_sync::bin::empty(&h.ctx, json!({"folder":"trash"}))
        .await
        .unwrap();
    assert_eq!(result["deleted"], 1);
    let stored = |id: &str| {
        h.sql_count(&format!(
            "SELECT count(*) FROM messages WHERE account=?1 AND remote_id='{id}'"
        ))
    };
    assert_eq!((stored("m001"), stored("m002")), (1, 0));
    assert_eq!(h.sql_count("SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND remote_id='m002' AND deleted=1"), 1);
    assert_eq!(
        h.coverage().await.observed,
        h.sql_count("SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0")
    );

    // History then names it again; no request goes to Gmail for it.
    let before = h.http.0.lock().unwrap().requests.len();
    h.http.0.lock().unwrap().history_replies.push_back((200, json!({"historyId":"201","history":[{"id":"201","labelsAdded":[{"message":{"id":"m002","threadId":"thread-m002"},"labelIds":["TRASH"]}]}]})));
    h.idle(&mut MailWorker::new(h.account.clone())).await;
    assert!(h.http.0.lock().unwrap().requests[before..]
        .iter()
        .all(|url| !url.contains("/messages/m002")));
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
