//! Live acquisition, crash recovery and retained source fixtures.
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_google::{
    gmail_api::{RawItem, RawOutcome},
    sync::{
        prepare_full_page, prepare_history_page, HistoryPage, ListPage, MessageRef, RawMessage,
    },
    transport::CancellationToken,
};
use serde_json::json;

fn setup() -> (tempfile::TempDir, Store, String) {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(directory.path()).unwrap();
    let account = store
        .bind_google_account(
            "synthetic-live-mail-subject",
            "reader@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    store
        .google_live_begin(&account, "100", 1700000000)
        .unwrap();
    (directory, store, account)
}

#[test]
fn staged_attachment_resumes_from_original_without_decoded_blob() {
    let (directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["attached"],
        None,
    );
    let raw=b"From: sender@example.test\r\nTo: reader@example.test\r\nSubject: File\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nhello\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=file.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAH/\r\n--x--\r\n";
    let item = prepare_google_item(
        &store.google_blob_store(),
        &page,
        "attached",
        Some(raw_bytes(
            "attached",
            "110",
            raw,
            &["INBOX"],
            "1789210800123",
        )),
        &CancellationToken::new(),
    )
    .unwrap();
    let attachment = item.manifest.message.as_ref().unwrap().attachments[0].clone();
    assert!(!directory
        .path()
        .join("blobs")
        .join(&attachment.hash)
        .exists());
    store.google_live_stage_prepared(&page, &item).unwrap();
    drop(store);
    let mut store = Store::open_live(directory.path()).unwrap();
    let staged = store
        .google_live_pending(&page, 1)
        .unwrap()
        .remove(0)
        .staged
        .unwrap();
    let verified = verify_staged_google_item(
        &store.google_blob_store(),
        &page,
        staged,
        &CancellationToken::new(),
    )
    .unwrap();
    publish(&mut store, &page, verified);
    store.google_live_finish_page(&page).unwrap();
    let aid: String = store
        .db
        .query_row(
            "SELECT id FROM attachments WHERE account=?1",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    let result = store
        .attachment_begin(&account, &json!({"id":aid}))
        .unwrap()
        .export()
        .unwrap();
    assert_eq!(
        std::fs::read(result["path"].as_str().unwrap()).unwrap(),
        b"\0\x01\xff"
    );
}
fn checkpoint(store: &Store, account: &str, lane: MailLane) -> Checkpoint {
    store
        .google_live_lanes(account)
        .unwrap()
        .into_iter()
        .find(|s| s.lane == lane)
        .unwrap()
        .checkpoint
}
fn listing(ids: &[&str], next: Option<&str>) -> ListPage {
    ListPage {
        messages: ids
            .iter()
            .map(|id| MessageRef {
                id: (*id).into(),
                thread_id: format!("thread-{id}"),
            })
            .collect(),
        next_page_token: next.map(Into::into),
        result_size_estimate: Some(999999),
    }
}
fn begin_list(
    store: &mut Store,
    account: &str,
    lane: MailLane,
    ids: &[&str],
    next: Option<&str>,
) -> GooglePageIdentity {
    let cp = checkpoint(store, account, lane);
    let history = match &cp.phase {
        Phase::Full {
            baseline_history_id,
            ..
        } => baseline_history_id.clone(),
        _ => panic!("expected inventory"),
    };
    let plan = prepare_full_page(&cp, listing(ids, next)).unwrap();
    store.google_live_begin_page(lane, &plan, &history).unwrap()
}
fn source(body: &str) -> Vec<u8> {
    format!("From: Human <sender@example.test>\r\nTo: reader@example.test\r\nSubject: retained {body}\r\nDate: Fri, 11 Sep 2020 01:02:03 +0000\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n").into_bytes()
}
fn raw_bytes(id: &str, history: &str, bytes: &[u8], labels: &[&str], date: &str) -> RawOutcome {
    RawOutcome::Available(RawItem {
        message: RawMessage {
            id: id.into(),
            thread_id: format!("thread-{id}"),
            label_ids: labels.iter().map(|s| (*s).into()).collect(),
            history_id: history.into(),
            raw: URL_SAFE_NO_PAD.encode(bytes),
        },
        internal_date: Some(date.into()),
        size_estimate: Some(bytes.len() as u64),
    })
}
fn raw(id: &str, history: &str, body: &str) -> RawOutcome {
    raw_bytes(
        id,
        history,
        &source(body),
        &["INBOX", "UNREAD"],
        "1789210800123",
    )
}

#[test]
fn retention_filters_inventory_and_old_delta_without_storing_content() {
    let (directory, mut store, account) = setup();
    let since = store.google_mail_since(&account).unwrap().unwrap();
    let lanes = store.google_live_lanes(&account).unwrap();
    assert_eq!(
        lanes
            .iter()
            .find(|l| l.lane == MailLane::Inventory)
            .unwrap()
            .query,
        Some(format!("after:{since}"))
    );
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["old"], None);
    let blobs = store.google_blob_store();
    let item = prepare_google_item_since(
        &blobs,
        &page,
        "old",
        Some(raw_bytes(
            "old",
            "101",
            &source("old private body"),
            &["INBOX"],
            &((since - 1) * 1000).to_string(),
        )),
        &CancellationToken::new(),
        Some(since),
    )
    .unwrap();
    assert!(item.manifest.source.is_none());
    assert!(!directory.path().join("blobs").exists());
    store.google_live_stage_prepared(&page, &item).unwrap();
    drop(store);
    let mut store = Store::open_live(directory.path()).unwrap();
    assert_eq!(store.google_mail_since(&account).unwrap(), Some(since));
    let staged = store
        .google_live_pending(&page, 1)
        .unwrap()
        .remove(0)
        .staged
        .unwrap();
    let item = verify_staged_google_item(
        &store.google_blob_store(),
        &page,
        staged,
        &CancellationToken::new(),
    )
    .unwrap();
    store.google_live_commit_part(&page, &[item]).unwrap();
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT count(*) FROM messages WHERE account=?1",
                [&account],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT count(*) FROM google_mail_coverage WHERE account=?1",
                [&account],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
fn prepared(
    store: &Store,
    page: &GooglePageIdentity,
    id: &str,
    outcome: RawOutcome,
) -> PreparedGoogleItem {
    prepare_google_item(
        &store.google_blob_store(),
        page,
        id,
        Some(outcome),
        &CancellationToken::new(),
    )
    .unwrap()
}
fn publish(
    store: &mut Store,
    page: &GooglePageIdentity,
    item: PreparedGoogleItem,
) -> GooglePartReceipt {
    store.google_live_stage_prepared(page, &item).unwrap();
    store.google_live_commit_part(page, &[item]).unwrap()
}
fn count(store: &Store, account: &str) -> u64 {
    store
        .db
        .query_row(
            "SELECT count(*) FROM messages WHERE account=?1",
            [account],
            |r| r.get(0),
        )
        .unwrap()
}
fn catch_up(
    store: &mut Store,
    account: &str,
    history: &str,
    added: &[&str],
    deleted: &[&str],
) -> GooglePageIdentity {
    let mut cp = checkpoint(store, account, MailLane::Delta);
    if cp.phase == Phase::Idle {
        let plan = cp.begin_delta().unwrap();
        let page = store
            .google_live_begin_page(
                MailLane::Delta,
                &plan,
                cp.committed_history_id.as_deref().unwrap(),
            )
            .unwrap();
        cp = store.google_live_finish_page(&page).unwrap();
    }
    let page:HistoryPage=serde_json::from_value(json!({"historyId":history,"history":[{"id":history,
        "messagesAdded":added.iter().map(|id|json!({"message":{"id":id,"threadId":format!("thread-{id}")}})).collect::<Vec<_>>(),
        "messagesDeleted":deleted.iter().map(|id|json!({"message":{"id":id,"threadId":format!("thread-{id}")}})).collect::<Vec<_>>() }]})).unwrap();
    let plan = prepare_history_page(&cp, page).unwrap();
    store
        .google_live_begin_page(MailLane::Delta, &plan, history)
        .unwrap()
}

#[test]
fn visible_parts_replay_resume_and_page_cursor_wait_for_every_outcome() {
    let (directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["a", "b"],
        Some("second-page"),
    );
    let first = prepared(&store, &page, "a", raw("a", "110", "alpha"));
    store.google_live_stage_prepared(&page, &first).unwrap();
    assert_eq!(count(&store, &account), 0);
    let receipt = store
        .google_live_commit_part(&page, std::slice::from_ref(&first))
        .unwrap();
    assert_eq!(count(&store, &account), 1);
    assert_eq!(
        store.google_live_commit_part(&page, &[first]).unwrap(),
        receipt
    );
    assert_eq!(
        checkpoint(&store, &account, MailLane::Inventory).revision,
        0
    );
    assert_eq!(
        store.google_live_finish_page(&page).unwrap_err().code,
        "google_page_pending"
    );
    let second = prepared(&store, &page, "b", raw("b", "110", "bravo"));
    store.google_live_stage_prepared(&page, &second).unwrap();
    drop(store);
    let mut store = Store::open_live(directory.path()).unwrap();
    let saved_page = store
        .google_live_resume_page(&account, MailLane::Inventory)
        .unwrap()
        .unwrap();
    assert_eq!(saved_page, page);
    let page = saved_page;
    let mut pending = store.google_live_pending(&page, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let recovered = verify_staged_google_item(
        &store.google_blob_store(),
        &page,
        pending.remove(0).staged.unwrap(),
        &CancellationToken::new(),
    )
    .unwrap();
    store.google_live_commit_part(&page, &[recovered]).unwrap();
    let completed = store.google_live_finish_page(&page).unwrap();
    assert!(matches!(completed.phase,Phase::Full{page_token:Some(ref t),..} if t=="second-page"));
    assert_eq!(store.google_live_finish_page(&page).unwrap(), completed);
    let retained_payloads: u64 = store.db.query_row("SELECT count(*) FROM google_live_page_items WHERE account=?1 AND manifest_json IS NOT NULL",[&account],|r|r.get(0)).unwrap();
    assert_eq!(retained_payloads, 0);
    let replay = prepared(&store, &page, "a", raw("a", "110", "alpha"));
    assert_eq!(
        store
            .google_live_commit_part(&page, std::slice::from_ref(&replay))
            .unwrap(),
        receipt
    );
    assert_eq!(count(&store, &account), 2);
    let fts: u64 = store
        .db
        .query_row(
            "SELECT count(*) FROM message_fts WHERE message_fts MATCH 'alpha OR bravo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts, 2);
}

#[test]
fn oversized_and_malformed_items_do_not_poison_page_or_claim_offline_coverage() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["good", "huge", "invalid", "mime"],
        None,
    );
    let outcomes = [
        ("good", raw("good", "110", "good")),
        ("huge", RawOutcome::TooLarge),
        ("invalid", RawOutcome::Malformed),
        (
            "mime",
            raw_bytes("mime", "110", b"", &["INBOX"], "1789210800123"),
        ),
    ];
    for (id, outcome) in outcomes {
        let item = prepared(&store, &page, id, outcome);
        publish(&mut store, &page, item);
    }
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 1);
    let coverage = store.google_live_coverage(&account).unwrap();
    assert!(coverage.inventory_complete);
    assert!(!coverage.offline_complete);
    assert_eq!((coverage.observed, coverage.unavailable), (4, 3));
    // Only the two whose source never arrived will not download; the
    // unparsable one is kept.
    assert_eq!((coverage.retained_sources, coverage.undownloadable), (2, 2));
    let gaps = store.google_live_gaps(&account, None, 2).unwrap();
    assert_eq!(
        gaps.iter()
            .map(|g| g.remote_id.as_str())
            .collect::<Vec<_>>(),
        ["huge", "invalid"]
    );
    let next = store
        .google_live_gaps(&account, Some("invalid"), 2)
        .unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].issue, "malformed_mime");
    assert!(next[0].source_retained);
    let mime:(String,String,String)=store.db.query_row("SELECT source_state,index_state,issue FROM google_mail_coverage WHERE account=?1 AND remote_id='mime'",[&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(
        mime,
        (
            "retained".into(),
            "unindexed".into(),
            "malformed_mime".into()
        )
    );
    assert!(store.google_live_pending(&page, 10).unwrap().is_empty());
}

#[test]
fn a_source_blocked_on_disk_is_recorded_and_the_page_still_completes() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["good", "blocked"],
        None,
    );
    // Antivirus that alters or removes the file as it is written leaves the
    // same name holding other bytes, which staging then cannot verify.
    let probe = prepared(&store, &page, "blocked", raw("blocked", "110", "phish"));
    let source = probe.manifest.source.as_ref().unwrap();
    let path = store.verified_blob(&source.hash, source.size).unwrap();
    std::fs::write(&path, b"quarantined").unwrap();
    let item = prepared(&store, &page, "blocked", raw("blocked", "110", "phish"));
    assert!(item.manifest.source.is_none());
    publish(&mut store, &page, item);
    let item = prepared(&store, &page, "good", raw("good", "110", "good"));
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 1);
    let blocked:(String,String)=store.db.query_row("SELECT source_state,issue FROM google_mail_coverage WHERE account=?1 AND remote_id='blocked'",[&account],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(blocked, ("unavailable".into(), "source_blocked".into()));
    assert!(store.google_live_pending(&page, 10).unwrap().is_empty());
}

#[test]
fn recovery_rehash_rejects_corruption_and_preparation_is_store_independent() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let blobs = store.google_blob_store();
    let worker_page = page.clone();
    let worker = std::thread::spawn(move || {
        prepare_google_item(
            &blobs,
            &worker_page,
            "a",
            Some(raw("a", "110", "threaded")),
            &CancellationToken::new(),
        )
        .unwrap()
    });
    // Store is free to answer while parsing/filesystem work owns only its handle.
    assert_eq!(count(&store, &account), 0);
    let item = worker.join().unwrap();
    store.google_live_stage_prepared(&page, &item).unwrap();
    let source = item.manifest.source.as_ref().unwrap();
    let path = store.verified_blob(&source.hash, source.size).unwrap();
    std::fs::write(&path, b"corrupt").unwrap();
    let staged = store
        .google_live_pending(&page, 1)
        .unwrap()
        .remove(0)
        .staged
        .unwrap();
    assert!(verify_staged_google_item(
        &store.google_blob_store(),
        &page,
        staged,
        &CancellationToken::new()
    )
    .is_err());
    assert_eq!(count(&store, &account), 0);
    assert_eq!(
        checkpoint(&store, &account, MailLane::Inventory).revision,
        0
    );
}

#[test]
fn transaction_failure_rolls_back_fts_coverage_receipt_and_messages() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a", "b"], None);
    let items = [
        prepared(&store, &page, "a", raw("a", "110", "alpha")),
        prepared(&store, &page, "b", raw("b", "110", "bravo")),
    ];
    for item in &items {
        store.google_live_stage_prepared(&page, item).unwrap();
    }
    store.db.execute_batch("CREATE TRIGGER fail_live_coverage BEFORE INSERT ON google_mail_coverage WHEN NEW.remote_id='b' BEGIN SELECT RAISE(ABORT,'synthetic disk failure'); END;").unwrap();
    assert!(store.google_live_commit_part(&page, &items).is_err());
    assert_eq!(count(&store, &account), 0);
    assert_eq!(store.google_live_coverage(&account).unwrap().observed, 0);
    let indexed: u64 = store
        .db
        .query_row("SELECT count(*) FROM message_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(indexed, 0);
    store
        .db
        .execute_batch("DROP TRIGGER fail_live_coverage;")
        .unwrap();
    store.google_live_commit_part(&page, &items).unwrap();
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 2);
}

#[test]
fn independent_delta_wins_over_older_inventory_and_preserves_manual_intent() {
    let (_directory, mut store, account) = setup();
    let inventory = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let stale_item = prepared(&store, &inventory, "a", raw("a", "110", "original"));
    store
        .google_live_stage_prepared(&inventory, &stale_item)
        .unwrap();
    let delta = catch_up(&mut store, &account, "200", &["a"], &[]);
    let recent = prepared(
        &store,
        &delta,
        "a",
        raw_bytes(
            "a",
            "200",
            &source("original"),
            &["INBOX", "STARRED"],
            "1789210800123",
        ),
    );
    publish(&mut store, &delta, recent);
    store.google_live_finish_page(&delta).unwrap();
    let id: String = store
        .db
        .query_row(
            "SELECT id FROM messages WHERE account=?1 AND remote_id='a'",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    store.db.execute("INSERT INTO google_manual_intents(account,message_id,field,category_id,value,revision) VALUES(?1,?2,'read','',1,1)",params![account,id]).unwrap();
    store
        .google_live_commit_part(&inventory, &[stale_item])
        .unwrap();
    store.google_live_finish_page(&inventory).unwrap();
    let flags:(bool,bool,String)=store.db.query_row("SELECT m.unread,m.starred,s.history_id FROM messages m JOIN google_message_state s ON s.account=m.account AND s.message_id=m.id WHERE m.account=?1",[&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(flags, (false, true, "200".into()));
    assert_eq!(
        checkpoint(&store, &account, MailLane::Delta)
            .committed_history_id
            .as_deref(),
        Some("200")
    );
    // Completing inventory invalidates an earlier catch-up, but never its cursor.
    assert!(!store.google_live_coverage(&account).unwrap().delta_current);
    let delta = catch_up(&mut store, &account, "210", &[], &[]);
    store.google_live_finish_page(&delta).unwrap();
    assert!(
        store
            .google_live_coverage(&account)
            .unwrap()
            .offline_complete
    );
    store
        .bind_google_account(
            "synthetic-live-mail-subject",
            "reader@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            Some(&account),
        )
        .unwrap();
    // A new authorization epoch needs fresh catch-up, even when state is connected.
    assert!(!store.google_live_coverage(&account).unwrap().delta_current);
    let delta = catch_up(&mut store, &account, "220", &[], &[]);
    store.google_live_finish_page(&delta).unwrap();
    assert!(
        store
            .google_live_coverage(&account)
            .unwrap()
            .offline_complete
    );
}

#[test]
fn date_order_uses_internal_date_and_original_rfc_date_is_retained() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a", "b"], None);
    for (id, date) in [("a", "1789210800999"), ("b", "1789210800001")] {
        let item = prepared(
            &store,
            &page,
            id,
            raw_bytes(id, "110", &source(id), &["INBOX"], date),
        );
        publish(&mut store, &page, item);
    }
    let ids = store
        .db
        .prepare("SELECT remote_id FROM messages WHERE account=?1 ORDER BY sent_at DESC")
        .unwrap()
        .query_map([&account], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(ids, vec!["a", "b"]);
    let (date,hash,size):(i64,String,u64)=store.db.query_row("SELECT internal_date_ms,source_hash,source_size FROM google_mail_coverage WHERE account=?1 AND remote_id='a'",[&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(date, 1789210800999);
    assert_eq!(
        std::fs::read(store.verified_blob(&hash, size).unwrap()).unwrap(),
        source("a")
    );
}

#[test]
fn generation_connection_and_profile_fences_reject_old_preparation() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let item = prepared(&store, &page, "a", raw("a", "110", "old"));
    store.google_live_stage_prepared(&page, &item).unwrap();
    store
        .set_google_connection_state(&account, crate::live_profile::ConnectionState::Disconnected)
        .unwrap();
    assert!(store
        .google_live_commit_part(&page, std::slice::from_ref(&item))
        .is_err());
    store
        .bind_google_account(
            "synthetic-live-mail-subject",
            "reader@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            Some(&account),
        )
        .unwrap();
    assert!(store
        .google_live_commit_part(&page, std::slice::from_ref(&item))
        .is_err());
    store
        .google_live_resync(&account, 1, "300", 1800000000)
        .unwrap();
    assert!(store.google_live_commit_part(&page, &[item]).is_err());
    assert_eq!(count(&store, &account), 0);
    let (_other, other_store, _) = setup();
    let next = begin_list(&mut store, &account, MailLane::Inventory, &["b"], None);
    let foreign = prepared(&other_store, &next, "b", raw("b", "310", "foreign"));
    assert!(store.google_live_stage_prepared(&next, &foreign).is_err());
}

#[test]
fn expired_history_absence_needs_reconciliation_and_never_erases_authored_work() {
    let (_directory, mut store, account) = setup();
    let draft = store
        .handle(
            "draft_create",
            Some(&account),
            json!({"mode":"new","request_id":"keep-draft"}),
        )
        .unwrap();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let item = prepared(&store, &page, "a", raw("a", "110", "retained"));
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    store
        .google_live_resync(&account, 1, "300", 1800000000)
        .unwrap();
    let empty = begin_list(&mut store, &account, MailLane::Inventory, &[], None);
    store.google_live_finish_page(&empty).unwrap();
    let delta = catch_up(&mut store, &account, "310", &[], &[]);
    store.google_live_finish_page(&delta).unwrap();
    assert!(
        !store
            .google_live_coverage(&account)
            .unwrap()
            .offline_complete
    );
    assert_eq!(count(&store, &account), 1);
    let plan = store
        .google_live_reconciliation_page(&account)
        .unwrap()
        .unwrap();
    assert!(plan.fetch_message_ids.contains("a"));
    let page = store
        .google_live_begin_page(MailLane::Inventory, &plan, "310")
        .unwrap();
    let missing = prepared(&store, &page, "a", RawOutcome::Missing);
    publish(&mut store, &page, missing);
    store.google_live_finish_page(&page).unwrap();
    let retained: (bool, String) = store
        .db
        .query_row(
            "SELECT m.inbox,m.source_hash FROM messages m WHERE account=?1 AND remote_id='a'",
            [&account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(!retained.0);
    assert!(!retained.1.is_empty());
    let draft_count: u64 = store
        .db
        .query_row(
            "SELECT count(*) FROM drafts WHERE account=?1",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(draft_count, 1);
    assert!(!draft.is_null());
    assert!(store
        .google_live_reconciliation_page(&account)
        .unwrap()
        .is_none());
}

#[test]
fn part_item_and_normalized_byte_limits_are_enforced_before_any_publication() {
    let (_directory, mut store, account) = setup();
    let ids = (0..11).map(|i| format!("m{i}")).collect::<Vec<_>>();
    let refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &refs, None);
    let mut items = Vec::new();
    for id in &ids {
        let item = prepared(&store, &page, id, raw(id, "110", "small"));
        store.google_live_stage_prepared(&page, &item).unwrap();
        items.push(item);
    }
    assert_eq!(
        store
            .google_live_commit_part(&page, &items)
            .unwrap_err()
            .code,
        "google_part_limit"
    );
    let body = "z".repeat(200000);
    let mut big = Vec::new();
    for id in &ids[..3] {
        let bytes = format!(
            "From: sender@example.test\r\nSubject: large\r\nContent-Type: text/plain\r\n\r\n{body}"
        )
        .into_bytes();
        let item = prepared(
            &store,
            &page,
            id,
            raw_bytes(id, "110", &bytes, &["INBOX"], "1789210800123"),
        );
        assert!(item.issue().is_none());
        store.google_live_stage_prepared(&page, &item).unwrap();
        big.push(item);
    }
    assert_eq!(
        store.google_live_commit_part(&page, &big).unwrap_err().code,
        "google_part_limit"
    );
    assert_eq!(count(&store, &account), 0);
}

#[test]
fn cancellation_leaves_no_staged_work_and_same_source_label_refresh_skips_fts() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(prepare_google_item(
        &store.google_blob_store(),
        &page,
        "a",
        Some(raw("a", "110", "keepfts")),
        &cancel
    )
    .is_err());
    let item = prepared(&store, &page, "a", raw("a", "110", "keepfts"));
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    // SQLite's FTS shadow-table trigger independently detects an unnecessary reindex.
    store.db.execute_batch("CREATE TRIGGER forbid_fts_delete BEFORE DELETE ON message_fts_data BEGIN SELECT RAISE(ABORT,'unexpected FTS rewrite'); END;").unwrap();
    let page = catch_up(&mut store, &account, "200", &["a"], &[]);
    let item = prepared(
        &store,
        &page,
        "a",
        raw_bytes(
            "a",
            "200",
            &source("keepfts"),
            &["INBOX", "STARRED"],
            "1789210800123",
        ),
    );
    publish(&mut store, &page, item);
    let starred: bool = store
        .db
        .query_row(
            "SELECT starred FROM messages WHERE account=?1",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    assert!(starred);
}

fn assert_metrics(store: &Store, account: &str) {
    let coverage = store.google_live_coverage(account).unwrap();
    let actual:(u64,u64,u64,u64,u64)=store.db.query_row("SELECT count(*),coalesce(sum(source_state='retained'),0),coalesce(sum(index_state='indexed'),0),coalesce(sum(attachment_state='complete'),0),coalesce(sum(source_state!='retained' OR index_state!='indexed' OR attachment_state!='complete' OR observed_generation!=?2),0) FROM google_mail_coverage WHERE account=?1 AND deleted=0",params![account,coverage.generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    assert_eq!(
        (
            coverage.observed,
            coverage.retained_sources,
            coverage.indexed,
            coverage.complete_attachments,
            coverage.unavailable
        ),
        actual
    );
}

#[test]
fn explicit_retry_repairs_older_message_history_and_counters_survive_resync_deletion() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let item = prepared(&store, &page, "a", RawOutcome::TooLarge);
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    let history: Option<String> = store
        .db
        .query_row(
            "SELECT history_id FROM google_mail_coverage WHERE account=?1 AND remote_id='a'",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    assert!(history.is_none());
    assert_metrics(&store, &account);
    let plan = store
        .google_live_retry_page(&account, &["a".into()])
        .unwrap();
    let page = store
        .google_live_begin_page(MailLane::Inventory, &plan, "200")
        .unwrap();
    // Message history is older than the page watermark; a previous failure is
    // not a newer message observation and must not prevent this repair.
    let item = prepared(&store, &page, "a", raw("a", "110", "repaired"));
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 1);
    assert_eq!(store.google_live_coverage(&account).unwrap().unavailable, 0);
    assert_metrics(&store, &account);
    store
        .google_live_resync(&account, 1, "300", 1800000000)
        .unwrap();
    assert_metrics(&store, &account);
    let page = catch_up(&mut store, &account, "310", &[], &["a"]);
    let item = prepared(&store, &page, "a", RawOutcome::Missing);
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    assert_metrics(&store, &account);
    assert_eq!(store.google_live_coverage(&account).unwrap().observed, 0);
    assert_eq!(count(&store, &account), 1);
    store
        .set_google_connection_state(&account, crate::live_profile::ConnectionState::Disconnected)
        .unwrap();
    let coverage = store.google_live_coverage(&account).unwrap();
    assert!(!coverage.delta_current);
    assert!(!coverage.offline_complete);
}

#[test]
fn bounded_preparation_retains_attachments_and_remote_drafts_without_publishing_drafts() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["a", "draft"],
        None,
    );
    let mime=b"From: sender@example.test\r\nSubject: binary\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nHuman body\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=sample.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAECA/8=\r\n--x--\r\n";
    let item = prepared(
        &store,
        &page,
        "a",
        raw_bytes("a", "110", mime, &["INBOX"], "1789210800123"),
    );
    assert!(item.issue().is_none());
    publish(&mut store, &page, item);
    let (hash, size, id): (String, u64, String) = store
        .db
        .query_row(
            "SELECT blob_hash,size,id FROM attachments WHERE account=?1",
            [&account],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        crate::attachment_content::read(
            &store.google_blob_store(),
            &hash,
            size,
            store.attachment_source(&account, &id).unwrap().as_ref()
        )
        .unwrap(),
        [0, 1, 2, 3, 255]
    );
    let draft = prepared(
        &store,
        &page,
        "draft",
        raw_bytes(
            "draft",
            "110",
            &source("unsent remote"),
            &["DRAFT"],
            "1789210800123",
        ),
    );
    assert_eq!(draft.issue(), Some(ItemIssue::RemoteDraft));
    publish(&mut store, &page, draft);
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 1);
    assert_eq!(store.google_live_coverage(&account).unwrap().unavailable, 1);
    assert_metrics(&store, &account);
}

#[test]
fn newer_deletion_cannot_be_resurrected_by_an_older_inventory_result() {
    let (_directory, mut store, account) = setup();
    let inventory = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let older = prepared(&store, &inventory, "a", raw("a", "110", "older"));
    store
        .google_live_stage_prepared(&inventory, &older)
        .unwrap();
    let deletion = catch_up(&mut store, &account, "200", &[], &["a"]);
    let missing = prepared(&store, &deletion, "a", RawOutcome::Missing);
    publish(&mut store, &deletion, missing);
    store.google_live_finish_page(&deletion).unwrap();
    store.google_live_commit_part(&inventory, &[older]).unwrap();
    store.google_live_finish_page(&inventory).unwrap();
    assert_eq!(count(&store, &account), 0);
    let tombstone:(bool,String)=store.db.query_row("SELECT deleted,history_id FROM google_mail_coverage WHERE account=?1 AND remote_id='a'",[&account],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(tombstone, (true, "200".into()));
    assert_metrics(&store, &account);
}

#[test]
fn parallel_content_publication_reuses_one_verified_blob() {
    let (_directory, store, _account) = setup();
    let blobs = store.google_blob_store();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let blobs = blobs.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let bytes = vec![73; 1024 * 1024];
            barrier.wait();
            blobs.stage_blob(&bytes).unwrap()
        }));
    }
    let hashes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(hashes[0], hashes[1]);
    let path = blobs.verified_blob(&hashes[0], 1024 * 1024).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), vec![73; 1024 * 1024]);
}

#[test]
fn successful_history_timestamp_requires_full_application_and_survives_reopen() {
    let (directory, mut store, account) = setup();
    assert_eq!(store.google_last_mail_sync(&account).unwrap(), None);
    let inventory = begin_list(&mut store, &account, MailLane::Inventory, &[], None);
    store.google_live_finish_page(&inventory).unwrap();
    assert_eq!(store.google_last_mail_sync(&account).unwrap(), None);
    let page = catch_up(&mut store, &account, "200", &["a"], &[]);
    assert_eq!(
        store.google_live_finish_page(&page).unwrap_err().code,
        "google_page_pending"
    );
    assert_eq!(store.google_last_mail_sync(&account).unwrap(), None);
    let item = prepared(&store, &page, "a", raw("a", "200", "source"));
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    let stamp = store.google_last_mail_sync(&account).unwrap().unwrap();
    assert!(stamp > 0);
    // Exact receipt replay cannot relabel an old completion as fresh.
    store
        .db
        .execute(
            "UPDATE meta SET value=1 WHERE key=?1",
            [format!("last_mail_sync:{account}")],
        )
        .unwrap();
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(store.google_last_mail_sync(&account).unwrap(), Some(1));
    let page = catch_up(&mut store, &account, "210", &["b"], &[]);
    assert!(store.google_live_finish_page(&page).is_err());
    drop(store);
    let store = Store::open_live(directory.path()).unwrap();
    assert_eq!(store.google_last_mail_sync(&account).unwrap(), Some(1));
    assert_eq!(
        store.accounts().unwrap()["accounts"][0]["last_mail_sync_at"],
        1
    );
}

#[test]
fn spam_is_never_kept_and_leaving_spam_brings_the_message_back() {
    let (directory, mut store, account) = setup();
    let delta = catch_up(&mut store, &account, "200", &["kept", "junk"], &[]);
    let kept = prepared(&store, &delta, "kept", raw("kept", "200", "wanted"));
    let junk = prepared(
        &store,
        &delta,
        "junk",
        raw_bytes(
            "junk",
            "200",
            &source("offer"),
            &["SPAM", "UNREAD"],
            "1789210800123",
        ),
    );
    store.google_live_stage_prepared(&delta, &kept).unwrap();
    store.google_live_stage_prepared(&delta, &junk).unwrap();
    store
        .google_live_commit_part(&delta, &[kept, junk])
        .unwrap();
    store.google_live_finish_page(&delta).unwrap();
    assert_eq!(count(&store, &account), 1);
    assert_metrics(&store, &account);
    // Marked spam in Gmail: the next change removes the local copy and its coverage.
    let delta = catch_up(&mut store, &account, "210", &["kept"], &[]);
    let moved = prepared(
        &store,
        &delta,
        "kept",
        raw_bytes("kept", "210", &source("wanted"), &["SPAM"], "1789210800123"),
    );
    publish(&mut store, &delta, moved);
    store.google_live_finish_page(&delta).unwrap();
    assert_eq!(count(&store, &account), 0);
    let coverage: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM google_mail_coverage WHERE account=?1",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(coverage, 0);
    assert_metrics(&store, &account);
    // Not spam again: it is fetched and kept like any other message.
    let delta = catch_up(&mut store, &account, "220", &["kept"], &[]);
    let back = prepared(&store, &delta, "kept", raw("kept", "220", "wanted"));
    publish(&mut store, &delta, back);
    store.google_live_finish_page(&delta).unwrap();
    assert_eq!(count(&store, &account), 1);
    // Spam stored by an earlier build goes the next time the profile opens.
    store.db.execute("UPDATE google_message_state SET provider_labels_json='[\"SPAM\",\"UNREAD\"]' WHERE account=?1", [&account]).unwrap();
    store
        .db
        .execute("DELETE FROM meta WHERE key='spam_forgotten'", [])
        .unwrap();
    drop(store);
    let store = Store::open_live(directory.path()).unwrap();
    assert_eq!(count(&store, &account), 0);
    assert_metrics(&store, &account);
}

#[test]
fn large_sources_and_many_attachments_are_published_whole() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["large", "many"],
        None,
    );
    let mime = |parts: &[(String, Vec<u8>)]| {
        let mut mime = b"From: sender@example.test\r\nTo: me@example.test\r\nSubject: Files\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=part\r\n\r\n--part\r\nContent-Type: text/plain\r\n\r\nFiles attached\r\n".to_vec();
        for (name, bytes) in parts {
            mime.extend_from_slice(format!("--part\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename={name}\r\nContent-Transfer-Encoding: base64\r\n\r\n").as_bytes());
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            for line in encoded.as_bytes().chunks(76) {
                mime.extend_from_slice(line);
                mime.extend_from_slice(b"\r\n");
            }
        }
        mime.extend_from_slice(b"--part--\r\n");
        mime
    };
    // Over the earlier 40 MiB bound once encoded.
    let big: Vec<u8> = (0..31 * 1024 * 1024)
        .map(|n: usize| (n % 251) as u8)
        .collect();
    let large = mime(&[("big.bin".into(), big.clone())]);
    assert!(large.len() > 40 * 1024 * 1024);
    // More than the earlier limit of 50 attachments.
    let files: Vec<(String, Vec<u8>)> = (0..60)
        .map(|n| (format!("file-{n}.bin"), vec![n as u8; 100]))
        .collect();
    let many = mime(&files);
    for (id, bytes) in [("large", &large), ("many", &many)] {
        let item = prepared(
            &store,
            &page,
            id,
            raw_bytes(id, "110", bytes, &["INBOX"], "1789210800123"),
        );
        publish(&mut store, &page, item);
    }
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 2);
    let coverage = store.google_live_coverage(&account).unwrap();
    assert_eq!(
        (
            coverage.retained_sources,
            coverage.undownloadable,
            coverage.unavailable
        ),
        (2, 0, 0)
    );
    let counts: Vec<(i64, i64)> = store
        .db
        .prepare("SELECT count(*),max(size) FROM attachments WHERE account=?1 GROUP BY message_id ORDER BY count(*)")
        .unwrap()
        .query_map([&account], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(counts, [(1, big.len() as i64), (60, 100)]);
}

fn history_position(checkpoint: &Checkpoint) -> String {
    match &checkpoint.phase {
        Phase::Delta {
            start_history_id, ..
        }
        | Phase::CatchUp {
            start_history_id, ..
        } => start_history_id.clone(),
        _ => checkpoint.committed_history_id.clone().unwrap(),
    }
}

#[test]
fn widening_retention_keeps_history_and_lists_only_missing_mail() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a", "b"], None);
    for id in ["a", "b"] {
        let item = prepared(&store, &page, id, raw(id, "110", id));
        publish(&mut store, &page, item);
    }
    store.google_live_finish_page(&page).unwrap();
    let history = catch_up(&mut store, &account, "120", &[], &[]);
    store.google_live_finish_page(&history).unwrap();
    let delta = checkpoint(&store, &account, MailLane::Delta);
    let recent_query = store.google_live_lanes(&account).unwrap()[0].query.clone();
    let generation = store.google_live_coverage(&account).unwrap().generation;

    store.change_mail_retention(&account, None).unwrap();
    let coverage = store.google_live_coverage(&account).unwrap();
    assert_eq!(coverage.generation, generation + 1);
    // Kept mail stays seen: nothing to check, nothing unavailable.
    assert_eq!(
        (
            coverage.unchecked,
            coverage.unavailable,
            coverage.retained_sources
        ),
        (0, 0, 2)
    );
    // The history lane keeps its place; the listings start over from it.
    assert_eq!(
        checkpoint(&store, &account, MailLane::Delta),
        Checkpoint {
            generation: generation + 1,
            ..delta.clone()
        }
    );
    let lanes = store.google_live_lanes(&account).unwrap();
    for lane in [MailLane::Recent, MailLane::Inventory] {
        let state = lanes.iter().find(|l| l.lane == lane).unwrap();
        assert!(!state.complete);
        assert_eq!(
            state.checkpoint.phase,
            Phase::Full {
                baseline_history_id: history_position(&delta),
                page_token: None
            }
        );
    }
    assert_eq!(lanes[0].query, recent_query);
    assert_eq!(
        lanes
            .iter()
            .find(|l| l.lane == MailLane::Inventory)
            .unwrap()
            .query,
        None
    );

    // The wider listing settles kept mail without a read and fetches the rest.
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["a", "b", "c"],
        None,
    );
    assert_eq!(store.google_live_skip_current(&page).unwrap(), 2);
    assert_eq!(store.google_live_skip_current(&page).unwrap(), 0);
    let pending = store.google_live_pending(&page, 10).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|p| p.remote_id.as_str())
            .collect::<Vec<_>>(),
        ["c"]
    );
    let item = prepared(&store, &page, "c", raw("c", "125", "charlie"));
    publish(&mut store, &page, item);
    store.google_live_finish_page(&page).unwrap();
    assert_eq!(count(&store, &account), 3);
    let coverage = store.google_live_coverage(&account).unwrap();
    assert!(coverage.inventory_complete);
    assert_eq!((coverage.retained_sources, coverage.unavailable), (3, 0));
}

#[test]
fn retention_without_a_history_position_starts_sync_over() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    let item = prepared(&store, &page, "a", raw("a", "110", "alpha"));
    publish(&mut store, &page, item);
    store
        .db
        .execute(
            "UPDATE google_live_lanes SET checkpoint_json=json_set(checkpoint_json,'$.phase','NeedsFullSync') WHERE account=?1 AND lane='delta'",
            [&account],
        )
        .unwrap();
    store.change_mail_retention(&account, None).unwrap();
    assert!(store.google_live_lanes(&account).unwrap().is_empty());
    assert_eq!(store.google_live_coverage(&account).unwrap().unavailable, 1);
}

#[test]
fn listings_settle_mail_already_kept_this_generation_but_history_never_does() {
    let (_directory, mut store, account) = setup();
    let recent = begin_list(&mut store, &account, MailLane::Recent, &["a"], None);
    let item = prepared(&store, &recent, "a", raw("a", "110", "alpha"));
    publish(&mut store, &recent, item);
    store.google_live_finish_page(&recent).unwrap();
    // The inventory lists the recent message again: nothing to read for it.
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a", "b"], None);
    assert_eq!(store.google_live_skip_current(&page).unwrap(), 1);
    let pending = store.google_live_pending(&page, 10).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|p| p.remote_id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    let last: Option<i64> = store
        .db
        .query_row(
            "SELECT last_inventory_generation FROM google_mail_coverage WHERE account=?1 AND remote_id='a'",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last, Some(1));
    // A history page names changed labels; it always reads them.
    let history = catch_up(&mut store, &account, "120", &["a"], &[]);
    assert_eq!(store.google_live_skip_current(&history).unwrap(), 0);
    assert_eq!(store.google_live_pending(&history, 10).unwrap().len(), 1);
}

#[test]
fn after_history_expires_kept_mail_is_unchecked_until_listed_again() {
    let (_directory, mut store, account) = setup();
    let page = begin_list(
        &mut store,
        &account,
        MailLane::Inventory,
        &["a", "b", "huge"],
        None,
    );
    for (id, outcome) in [
        ("a", raw("a", "110", "alpha")),
        ("b", raw("b", "110", "bravo")),
        ("huge", RawOutcome::TooLarge),
    ] {
        let item = prepared(&store, &page, id, outcome);
        publish(&mut store, &page, item);
    }
    assert_eq!(store.google_live_coverage(&account).unwrap().unchecked, 0);
    store
        .google_live_resync(&account, 1, "300", 1800000000)
        .unwrap();
    // The one with an issue waits for the end, not for the listing.
    assert_eq!(store.google_live_coverage(&account).unwrap().unchecked, 2);
    let page = begin_list(&mut store, &account, MailLane::Inventory, &["a"], None);
    // Its labels may have changed while history was lost, so it is read.
    assert_eq!(store.google_live_skip_current(&page).unwrap(), 0);
    let item = prepared(&store, &page, "a", raw("a", "310", "alpha"));
    publish(&mut store, &page, item);
    assert_eq!(store.google_live_coverage(&account).unwrap().unchecked, 1);
}

#[test]
fn one_spam_message_is_found_by_its_remote_id_not_by_scanning_the_account() {
    let (_dir, store, _account) = setup();
    store
        .db
        .execute_batch("CREATE TEMP TABLE IF NOT EXISTS retention_messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,remote_id TEXT NOT NULL,source_hash TEXT NOT NULL,source_rowid INTEGER NOT NULL)")
        .unwrap();
    let plan: Vec<String> = store
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {}", spam_selection(true)))
        .unwrap()
        .query_map(rusqlite::params!["account", "remote"], |r| {
            r.get::<_, String>(3)
        })
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|step| step.starts_with("SEARCH m ") && step.contains("remote_id=?")),
        "{plan:?}"
    );
}
