use crate::*;
use rusqlite::params;

const ACCOUNT: &str = "windscribe";
const THREAD: &str = "windscribe-thread-000";

#[test]
fn live_mail_pages_continue_during_flag_changes_without_losing_older_threads() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    for grouped in [true, false] {
        let mut query = json!({"folder":"all","grouped":grouped,"limit":7,"live_paging":true});
        let mut seen = std::collections::HashSet::new();
        let mut count = 0;
        loop {
            let page = store
                .handle("conversations", Some(ACCOUNT), query.clone())
                .unwrap();
            for row in page["conversations"].as_array().unwrap() {
                seen.insert(row["id"].as_str().unwrap().to_owned());
            }
            count += 1;
            assert!(
                count < 25,
                "pagination must make bounded progress during writes"
            );
            if page["next_cursor"].is_null() {
                break;
            }
            query["cursor"] = page["next_cursor"].clone();
            // Repeated same-value writes still invalidate strict cursors.
            let read = store.conversation(ACCOUNT, THREAD).unwrap();
            store.handle("flag",Some(ACCOUNT),json!({"id":THREAD,"message_ids":read["message_ids"],"expected_revision":read["revision"],"request_id":format!("live-{grouped}-{count}"),"starred":read["messages"][0]["starred"]})).unwrap();
        }
        assert_eq!(seen.len(), 110);
        let mut wrong = query.clone();
        wrong["folder"] = json!("sent");
        assert_eq!(
            store
                .handle("conversations", Some(ACCOUNT), wrong)
                .unwrap_err()
                .code,
            "stale_cursor"
        );
    }
}

#[test]
fn live_mail_pages_keep_the_order_after_head_insertion_and_deleted_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let insert = |store: &Store, id: &str, date: &str| {
        store.db.execute("INSERT INTO conversations(account,id,subject,revision) VALUES(?1,?2,'Paging test',1)",params![ACCOUNT,id]).unwrap();
        store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?2,?2,'Paging','paging@example.test','test@example.test','Paging test','Generated paging body',?3,1,0,1,0,'',1)",params![ACCOUNT,id,date]).unwrap();
    };
    for index in 1..=30 {
        insert(
            &store,
            &format!("paging-{index:02}"),
            &format!("2030-01-{index:02}T00:00:00Z"),
        );
    }
    let mut query = json!({"folder":"inbox","grouped":true,"live_paging":true,"limit":5});
    let first = store
        .handle("conversations", Some(ACCOUNT), query.clone())
        .unwrap();
    assert_eq!(first["conversations"][4]["id"], "paging-26");
    query["cursor"] = first["next_cursor"].clone();
    store
        .db
        .execute(
            "DELETE FROM messages WHERE account=?1 AND id='paging-26'",
            [ACCOUNT],
        )
        .unwrap();
    store
        .db
        .execute(
            "DELETE FROM conversations WHERE account=?1 AND id='paging-26'",
            [ACCOUNT],
        )
        .unwrap();
    insert(&store, "paging-new-head", "2031-01-01T00:00:00Z");
    insert(&store, "paging-new-older", "2029-01-01T00:00:00Z");
    let mut ids = Vec::new();
    for _ in 0..30 {
        let page = store
            .handle("conversations", Some(ACCOUNT), query.clone())
            .unwrap();
        ids.extend(
            page["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["id"].as_str().unwrap().to_owned()),
        );
        if page["next_cursor"].is_null() {
            break;
        }
        query["cursor"] = page["next_cursor"].clone();
    }
    let expected: Vec<_> = (1..=25)
        .rev()
        .map(|index| format!("paging-{index:02}"))
        .collect();
    assert_eq!(&ids[..25], expected.as_slice());
    assert_eq!(ids[25], "paging-new-older");
    assert!(!ids.iter().any(|id| id == "paging-new-head"));
    assert_eq!(
        ids.len(),
        ids.iter().collect::<std::collections::HashSet<_>>().len()
    );
}

#[test]
fn phase1_fixture_counts_filters_groups_and_cursor_agree() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let accounts = store.handle("accounts", None, json!({})).unwrap();
    assert_eq!(accounts["fixture_revision"], 3);
    assert_eq!(accounts["accounts"].as_array().unwrap().len(), 3);
    let account = &accounts["accounts"][0];
    assert_eq!(account["conversation_count"], 110);
    assert!(
        account["unread_count"].as_i64().unwrap()
            > account["unread_conversation_count"].as_i64().unwrap()
    );
    let first = store
        .handle(
            "conversations",
            Some(ACCOUNT),
            json!({"grouped":true,"folder":"all","limit":100}),
        )
        .unwrap();
    assert_eq!(first["filtered_conversation_count"], 110);
    let groups: Vec<_> = first["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| match r["group"].as_str().unwrap() {
            "review" => 0,
            "held" => 1,
            _ => 2,
        })
        .collect();
    assert!(groups.windows(2).all(|v| v[0] <= v[1]));
    let cursor = first["next_cursor"].clone();
    let next = store
        .handle(
            "conversations",
            Some(ACCOUNT),
            json!({"grouped":true,"folder":"all","limit":100,"cursor":cursor}),
        )
        .unwrap();
    assert_eq!(next["conversations"].as_array().unwrap().len(), 10);
    let selected=store.handle("conversations",Some(ACCOUNT),json!({"sender":"maya@windscribe.example","has_attachment":true,"after":"2026-09-11","before":"2026-09-12","status":"review"})).unwrap();
    assert_eq!(selected["filtered_conversation_count"], 1);
    assert_eq!(
        selected["conversations"][0]["matching_message_ids"],
        json!(["windscribe-message-000-002"])
    );
    assert_eq!(selected["account_counts"], first["account_counts"]);
    let read = store.conversation(ACCOUNT, THREAD).unwrap();
    store.handle("flag",Some(ACCOUNT),json!({"id":THREAD,"message_ids":read["message_ids"],"expected_revision":read["revision"],"request_id":"done","read":true,"starred":false})).unwrap();
    assert_eq!(
        store
            .handle(
                "conversations",
                Some(ACCOUNT),
                json!({"grouped":true,"folder":"all","limit":100,"cursor":cursor})
            )
            .unwrap_err()
            .code,
        "stale_cursor"
    );
    for invalid in [
        json!({"status":"snoozed"}),
        json!({"after":"2026-02-30"}),
        json!({"before":"2026-09-10","after":"2026-09-11"}),
        json!({"has_attachment":"yes"}),
        json!({"query":"from:maya"}),
    ] {
        assert_eq!(
            store
                .handle("conversations", Some(ACCOUNT), invalid)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}

#[test]
fn phase1_preferences_validate_replay_and_keep_none_distinct_from_all() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    assert!(store.preferences().unwrap()["preferences"]["selected_calendars"].is_null());
    let command = json!({"preferences":{"density":"compact","selected_calendars":[]},"expected_revision":0,"request_id":"compact"});
    let saved = store
        .handle("preferences_save", None, command.clone())
        .unwrap();
    assert_eq!(
        store
            .handle("preferences_save", None, command.clone())
            .unwrap(),
        saved
    );
    assert_eq!(store.handle("preferences_save",None,json!({"preferences":{"calendar_start_hour":22,"calendar_end_hour":7},"expected_revision":1,"request_id":"bad-hours"})).unwrap_err().code,"invalid_request");
    assert_eq!(
        store
            .handle(
                "calendar_combined",
                None,
                json!({"from":"2026-09-07","to":"2026-09-14"})
            )
            .unwrap()["events"],
        json!([])
    );
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.preferences().unwrap(), saved);
    assert_eq!(
        store.handle("preferences_save", None, command).unwrap(),
        saved
    );
}

#[test]
fn phase1_signature_snapshot_capture_discard_and_history_are_durable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Human reply — こんにちは","body_html":"<p>Human reply — こんにちは</p>","attachment_ids":["windscribe-attachment-0"],"expected_revision":0,"request_id":"save"})).unwrap();
    let signature = draft["signature_text"].clone();
    store.handle("account_update",Some(ACCOUNT),json!({"name":"Changed display","signature_text":"New signature must not enter old reply","expected_revision":0,"request_id":"identity"})).unwrap();
    assert_eq!(store.draft(ACCOUNT, THREAD).unwrap(), draft);
    let captured=store.handle("draft_capture",Some(ACCOUNT),json!({"conversation_id":THREAD,"expected_revision":draft["revision"],"request_id":"capture"})).unwrap();
    let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    let mime = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    assert!(mime
        .body_text(0)
        .unwrap()
        .contains(signature.as_str().unwrap()));
    assert!(!mime.body_text(0).unwrap().contains("New signature"));
    let history = store
        .handle(
            "capture_history",
            Some(ACCOUNT),
            json!({"conversation_id":THREAD}),
        )
        .unwrap();
    assert_eq!(history["total"], 1);
    assert_eq!(history["captures"][0]["state"], "accepted");
    assert_eq!(history["captures"][0]["synthetic_outcome"], "accepted");
    assert_eq!(
        history["captures"][0]["subject"],
        store.conversation(ACCOUNT, THREAD).unwrap()["subject"]
    );
    assert_eq!(history["captures"][0]["provider_delivered"], false);
    assert_eq!(
        store
            .handle("capture_history", Some("personal"), json!({}))
            .unwrap()["total"],
        0
    );
    assert_eq!(
        store
            .handle(
                "draft_discard",
                Some(ACCOUNT),
                json!({"conversation_id":THREAD,"expected_revision":0,"request_id":"stale-discard"})
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    let discard = json!({"conversation_id":THREAD,"expected_revision":draft["revision"],"request_id":"discard"});
    let empty = store
        .handle("draft_discard", Some(ACCOUNT), discard.clone())
        .unwrap();
    assert_eq!(empty["body"], "");
    assert!(empty["body_html"].is_null());
    assert_eq!(empty["attachment_ids"], json!([]));
    assert_eq!(empty["signature_text"], "");
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .handle("draft_discard", Some(ACCOUNT), discard)
            .unwrap(),
        empty
    );
    assert_eq!(
        store
            .handle(
                "capture_history",
                Some(ACCOUNT),
                json!({"conversation_id":THREAD})
            )
            .unwrap(),
        history
    );
}

#[test]
fn phase1_discarded_reply_restarts_with_current_identity_and_outgoing_reply_targets_recipient() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let first=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Old draft","expected_revision":0,"request_id":"old"})).unwrap();
    let discard=store.handle("draft_discard",Some(ACCOUNT),json!({"conversation_id":THREAD,"expected_revision":first["revision"],"request_id":"clear"})).unwrap();
    store.handle("account_update",Some(ACCOUNT),json!({"name":"New name","signature_text":"New signature","expected_revision":0,"request_id":"new-identity"})).unwrap();
    let restarted=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"New draft","expected_revision":discard["revision"],"request_id":"new"})).unwrap();
    assert_eq!(restarted["signature_text"], "New signature");
    assert_eq!(restarted["from_name"], "New name");
    let outgoing = store.draft(ACCOUNT, "windscribe-thread-017").unwrap();
    assert_eq!(outgoing["to"], "maya@windscribe.example");
    assert_ne!(outgoing["to"], outgoing["from"]);
    store.db.execute("UPDATE messages SET sent_at='2026-09-10T23:30:00-04:00' WHERE account=?1 AND conversation_id=?2",params![ACCOUNT,"windscribe-thread-001"]).unwrap();
    let date = store
        .conversations(
            ACCOUNT,
            &json!({"query":"resubmitting","after":"2026-09-11","before":"2026-09-12"}),
        )
        .unwrap();
    assert_eq!(date["filtered_conversation_count"], 1);
    assert_eq!(
        store
            .conversations(
                ACCOUNT,
                &json!({"query":"resubmitting","before":"2026-09-11"})
            )
            .unwrap()["filtered_conversation_count"],
        0
    );
}

#[test]
fn phase1_authored_signature_override_captures_once_after_preference_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let signature = "Personally edited signature — exact-marker";
    let saved=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Authored reply","signature_text":signature,"expected_revision":0,"request_id":"signature-override"})).unwrap();
    store.handle("account_update",Some(ACCOUNT),json!({"name":"New name","signature_text":"Changed default","expected_revision":0,"request_id":"default-change"})).unwrap();
    let updated=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Edited authored reply","expected_revision":saved["revision"],"request_id":"body-only"})).unwrap();
    assert_eq!(updated["signature_text"], signature);
    let capture=store.handle("draft_capture",Some(ACCOUNT),json!({"conversation_id":THREAD,"expected_revision":updated["revision"],"request_id":"signed-capture"})).unwrap();
    let bytes = std::fs::read(capture["path"].as_str().unwrap()).unwrap();
    let mime = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    let text = mime.body_text(0).unwrap();
    assert_eq!(text.matches(signature).count(), 1);
    assert!(!text.contains("Changed default"));
    assert_eq!(store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Unchanged on invalid signature","signature_text":"x".repeat(16*1024+1),"expected_revision":updated["revision"],"request_id":"oversized-signature"})).unwrap_err().code,"invalid_request");
    assert_eq!(store.draft(ACCOUNT, THREAD).unwrap(), updated);
}

#[test]
fn phase1_cached_counts_invalidate_after_source_and_flag_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let before = store
        .conversations(ACCOUNT, &json!({"grouped":true}))
        .unwrap();
    let ids = store.conversation(ACCOUNT, THREAD).unwrap();
    store.handle("flag",Some(ACCOUNT),json!({"id":THREAD,"message_ids":ids["message_ids"],"read":true,"starred":false,"expected_revision":ids["revision"],"request_id":"invalidate-count"})).unwrap();
    let after = store
        .conversations(ACCOUNT, &json!({"grouped":true}))
        .unwrap();
    assert_eq!(
        after["account_counts"]["unread_conversation_count"]
            .as_i64()
            .unwrap(),
        before["account_counts"]["unread_conversation_count"]
            .as_i64()
            .unwrap()
            - 1
    );
    store.db.execute("DELETE FROM message_fts WHERE rowid=(SELECT rowid FROM messages WHERE account=?1 LIMIT 1)",[ACCOUNT]).unwrap();
    let coverage = store.conversations(ACCOUNT, &json!({})).unwrap();
    assert_eq!(coverage["coverage"]["text_index_complete"], false);
    assert_eq!(
        coverage["coverage"]["indexed_messages"].as_i64().unwrap(),
        before["coverage"]["indexed_messages"].as_i64().unwrap() - 1
    );
    let prefs=store.handle("preferences_save",None,json!({"preferences":{"calendar_view":"agenda","calendar_date":"2026-12-31","timeline_date":"2026-09-10"},"expected_revision":0,"request_id":"navigation"})).unwrap();
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.preferences().unwrap(), prefs);
}

#[test]
fn phase1_status_filters_use_entire_conversation_and_source_filters_share_one_member() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    // Only the latest member has the attachment; an earlier own outgoing member
    // cannot satisfy the sender constraint on its behalf.
    let wrong = store
        .conversations(
            ACCOUNT,
            &json!({"sender":"you@windscribe.example","has_attachment":true,"folder":"all"}),
        )
        .unwrap();
    assert_eq!(wrong["filtered_conversation_count"], 0);
    let mixed = store
        .conversations(
            ACCOUNT,
            &json!({"status":"held","query":"infra","grouped":true}),
        )
        .unwrap();
    assert!(mixed["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == THREAD && row["group"] == "review"));
    let unknown = store
        .conversations(ACCOUNT, &json!({"category":"foreign-missing"}))
        .unwrap_err();
    assert_eq!(unknown.code, "not_found");
    assert!(store
        .conversations(ACCOUNT, &json!({"query":"review:"}))
        .is_ok());
    let all = store
        .conversations(ACCOUNT, &json!({"grouped":true,"offset":1000}))
        .unwrap();
    assert_eq!(all["conversations"], json!([]));
    assert_eq!(
        all["filtered_conversation_count"],
        all["account_counts"]["folder_conversation_counts"]["inbox"]
    );
    let snapshot = store.conversation(ACCOUNT, THREAD).unwrap();
    store.handle("flag",Some(ACCOUNT),json!({"id":THREAD,"message_ids":snapshot["message_ids"],"read":true,"starred":false,"expected_revision":snapshot["revision"],"request_id":"classified-done"})).unwrap();
    let done = store
        .conversations(
            ACCOUNT,
            &json!({"status":"done","query":"infra","grouped":true}),
        )
        .unwrap();
    assert!(done["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == THREAD && row["group"] == "done"));
}
