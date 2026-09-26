//! Request-level tests against the seeded demo profile: list paging, counts,
//! filters and search grammar, preferences, and draft signatures.
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
fn fixture_counts_filters_groups_and_cursor_agree() {
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
        json!({"query":"is:snoozed"}),
        json!({"query":"is:done","status":"review"}),
        json!({"query":"has:attachment","has_attachment":false}),
        json!({"query":"label:nosuchlabel"}),
        json!({"query":"subject:!!"}),
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
fn preferences_validate_replay_and_keep_none_distinct_from_all() {
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
    // Polling clients compare this before reloading preferences.
    assert_eq!(
        store.handle("status", None, json!({})).unwrap()["preferences_revision"],
        saved["revision"]
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
fn signature_snapshot_capture_discard_and_history_are_durable() {
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
    // A discarded reply reopens like a new one: blank lines, then the signature.
    assert_eq!(empty["body"], "\n\nNew signature must not enter old reply");
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
fn discarded_reply_restarts_with_current_identity_and_outgoing_reply_targets_recipient() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let first=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Old draft","expected_revision":0,"request_id":"old"})).unwrap();
    let discard=store.handle("draft_discard",Some(ACCOUNT),json!({"conversation_id":THREAD,"expected_revision":first["revision"],"request_id":"clear"})).unwrap();
    store.handle("account_update",Some(ACCOUNT),json!({"name":"New name","signature_text":"New signature","expected_revision":0,"request_id":"new-identity"})).unwrap();
    let restarted=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"New draft","expected_revision":discard["revision"],"request_id":"new"})).unwrap();
    assert_eq!(restarted["body"], "New draft");
    assert_eq!(restarted["signature_text"], "");
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
fn authored_signature_override_captures_once_after_preference_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let signature = "Personally edited signature — exact-marker";
    let saved=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Authored reply","signature_text":signature,"expected_revision":0,"request_id":"signature-override"})).unwrap();
    store.handle("account_update",Some(ACCOUNT),json!({"name":"New name","signature_text":"Changed default","expected_revision":0,"request_id":"default-change"})).unwrap();
    // The override reads back as body text; saving that text keeps it once.
    assert_eq!(saved["body"], format!("Authored reply\n\n{signature}"));
    assert_eq!(saved["signature_text"], "");
    let updated=store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":format!("Edited authored reply\n\n{signature}"),"expected_revision":saved["revision"],"request_id":"body-only"})).unwrap();
    assert_eq!(updated["signature_text"], "");
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
fn cached_counts_invalidate_after_source_and_flag_changes() {
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
fn status_filters_use_entire_conversation_and_source_filters_share_one_member() {
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

// Words typed together are one phrase that must appear as written,
// from the start of a word; the last word may be partial.
#[test]
fn search_phrases_match_as_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let picked: Vec<(i64, String)> = store
        .db
        .prepare("SELECT rowid,conversation_id FROM messages WHERE account=?1 AND inbox=1 AND sent=0 AND trashed=0 AND conversation_id IN (SELECT conversation_id FROM messages WHERE account=?1 GROUP BY conversation_id HAVING COUNT(*)=1) ORDER BY id LIMIT 4")
        .unwrap()
        .query_map([ACCOUNT], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(picked.len(), 4);
    let long = format!(
        "Cover note. {}The C-22 draft is attached.",
        "Some words here. ".repeat(30)
    );
    for ((rowid, _), (subject, body)) in picked.iter().zip([
        ("Bill C-22 Option Illustrations", long.as_str()),
        (
            "Invitation: Standup @ Tue Sep 22, 2026",
            "Calendar call details",
        ),
        ("Weekly", "Thanks for the Bill\r\nC-22 update, Café Élan"),
        ("Re: c22", "Nothing here"),
    ]) {
        store.db.execute("UPDATE messages SET subject=?2,body=?3,sender_name='Sender',sender_email='sender@fixture.example' WHERE rowid=?1", params![rowid, subject, body]).unwrap();
        store
            .db
            .execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,'Sender sender@fixture.example',?2,?3)",
                params![rowid, subject, body],
            )
            .unwrap();
    }
    let [a, b, c, d] = [&picked[0].1, &picked[1].1, &picked[2].1, &picked[3].1].map(String::clone);
    let ids = |store: &mut Store, query: &str| -> Vec<String> {
        let result = store
            .handle(
                "conversations",
                Some(ACCOUNT),
                json!({"query":query,"folder":"inbox","limit":100}),
            )
            .unwrap();
        let mut ids: Vec<String> = result["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        assert_eq!(result["filtered_conversation_count"], ids.len(), "{query}");
        ids
    };
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    // "c-22" is not "c…" and "22" anywhere: Sep 22 and c22 stay out.
    assert_eq!(ids(&mut store, "c-22"), sorted(vec![a.clone(), c.clone()]));
    assert_eq!(ids(&mut store, "C-2"), sorted(vec![a.clone(), c.clone()]));
    assert_eq!(ids(&mut store, "subject:c-22"), vec![a.clone()]);
    assert_eq!(
        ids(&mut store, "body:c-22"),
        sorted(vec![a.clone(), c.clone()])
    );
    assert_eq!(ids(&mut store, "body:bill c-22"), vec![c.clone()]);
    // Case, accents and line breaks do not matter; order and punctuation do.
    assert_eq!(
        ids(&mut store, "bill c-22"),
        sorted(vec![a.clone(), c.clone()])
    );
    assert_eq!(
        ids(&mut store, "\"BILL C-22\""),
        sorted(vec![a.clone(), c.clone()])
    );
    assert_eq!(ids(&mut store, "cafe elan"), vec![c.clone()]);
    assert!(ids(&mut store, "c-22 bill").is_empty());
    assert!(ids(&mut store, "c 22").is_empty());
    assert!(ids(&mut store, "ill c-22").is_empty());
    assert_eq!(ids(&mut store, "c22"), vec![d.clone()]);
    assert_eq!(ids(&mut store, "sep 22"), vec![b.clone()]);
    // One word needs only its own start; the fixture's "Member 22 of 32" also matches.
    let twenty_two = ids(&mut store, "22");
    assert!([&a, &b, &c].iter().all(|id| twenty_two.contains(id)) && !twenty_two.contains(&d));
    // Separate phrases each need to appear.
    assert_eq!(
        ids(&mut store, "c-22 from:sender@fixture.example"),
        sorted(vec![a.clone(), c.clone()])
    );
    assert_eq!(ids(&mut store, "c-22 subject:weekly"), vec![c.clone()]);
    assert!(ids(&mut store, "bill option subject:weekly").is_empty());
    // The preview starts near the phrase, not at the first word starting with c.
    let result = store
        .handle(
            "conversations",
            Some(ACCOUNT),
            json!({"query":"c-22","folder":"inbox","limit":100}),
        )
        .unwrap();
    let row = result["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == a.as_str())
        .unwrap();
    let snippet = row["snippet"].as_str().unwrap();
    assert!(
        snippet.starts_with("… ") && snippet.contains("The C-22 draft"),
        "{snippet}"
    );
    assert_eq!(
        result["search_terms"]["phrases"],
        json!([{"text":"c-22","field":"any"}])
    );
}

// The search box grammar that the GUI and CLI share.
#[test]
fn search_grammar_scopes_fields_labels_state_and_recipients() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    // Three single-message Inbox conversations with distinctive text.
    let picked: Vec<(i64, String)> = store
        .db
        .prepare("SELECT rowid,conversation_id FROM messages WHERE account=?1 AND inbox=1 AND sent=0 AND trashed=0 AND conversation_id IN (SELECT conversation_id FROM messages WHERE account=?1 GROUP BY conversation_id HAVING COUNT(*)=1) ORDER BY id LIMIT 3")
        .unwrap()
        .query_map([ACCOUNT], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(picked.len(), 3);
    for ((rowid, _), (name, email, subject, body)) in picked.iter().zip([
        (
            "Quentin Ruiz",
            "quentin@fixture.example",
            "Zeppelin plans",
            "Orchid budget notes",
        ),
        (
            "Orchid Team",
            "team@fixture.example",
            "Weekly",
            "Nothing new",
        ),
        (
            "Pat Moss",
            "pat@fixture.example",
            "Other",
            "zeppelin orchid",
        ),
    ]) {
        store.db.execute("UPDATE messages SET sender_name=?2,sender_email=?3,subject=?4,body=?5 WHERE rowid=?1", params![rowid, name, email, subject, body]).unwrap();
        store
            .db
            .execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)",
                params![rowid, format!("{name} {email}"), subject, body],
            )
            .unwrap();
    }
    let ids = |store: &mut Store, query: &str, folder: &str| -> Vec<String> {
        let result = store
            .handle(
                "conversations",
                Some(ACCOUNT),
                json!({"query":query,"folder":folder,"limit":100}),
            )
            .unwrap();
        let mut ids: Vec<String> = result["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    };
    let [a, b, c] = [&picked[0].1, &picked[1].1, &picked[2].1].map(String::clone);
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    // Bare words search subject or text, never the sender.
    assert_eq!(
        ids(&mut store, "orchid", "inbox"),
        sorted(vec![a.clone(), c.clone()])
    );
    assert_eq!(ids(&mut store, "from:orchid", "inbox"), vec![b.clone()]);
    assert_eq!(
        ids(&mut store, "subject:zeppelin", "inbox"),
        vec![a.clone()]
    );
    assert_eq!(ids(&mut store, "body:zeppelin", "inbox"), vec![c.clone()]);
    // Words typed together are one phrase.
    assert_eq!(ids(&mut store, "zeppelin orchid", "inbox"), vec![c.clone()]);
    assert!(ids(&mut store, "orchid zeppelin", "inbox").is_empty());
    assert_eq!(
        ids(&mut store, "QUENTIN@fixture.example", "inbox"),
        vec![a.clone()]
    );
    assert_eq!(
        ids(
            &mut store,
            "subject:\"zeppelin plans\" from:quentin",
            "inbox"
        ),
        vec![a.clone()]
    );
    assert_eq!(ids(&mut store, "from:", "inbox").len(), 85);
    // label:, is: and has: agree with the typed filters they stand for.
    let count = |store: &mut Store, p: Value| {
        store.handle("conversations", Some(ACCOUNT), p).unwrap()["filtered_conversation_count"]
            .clone()
    };
    assert_eq!(
        count(&mut store, json!({"query":"label:INF"})),
        count(&mut store, json!({"category":"infra"}))
    );
    assert_eq!(
        count(&mut store, json!({"query":"is:delayed","folder":"all"})),
        count(&mut store, json!({"status":"held","folder":"all"}))
    );
    assert_eq!(
        count(&mut store, json!({"query":"has:attachment","folder":"all"})),
        count(&mut store, json!({"has_attachment":true,"folder":"all"}))
    );
    assert_eq!(
        count(
            &mut store,
            json!({"query":"is:done","status":"done","folder":"all"})
        ),
        count(&mut store, json!({"status":"done","folder":"all"}))
    );
    let terms = store
        .handle(
            "conversations",
            Some(ACCOUNT),
            json!({"query":"relay subject:board deck is:held label:inf"}),
        )
        .unwrap()["search_terms"]
        .clone();
    assert_eq!(terms["words"], "relay");
    assert_eq!(terms["subject"], json!(["board deck"]));
    assert_eq!(terms["is"], "delayed");
    assert_eq!(terms["label"], "Infra");
    assert!(store
        .handle("conversations", Some(ACCOUNT), json!({}))
        .unwrap()["search_terms"]
        .is_null());
    // to: names a recipient in every folder; from: is always the sender; a
    // bare address is either.
    let sent: (i64, String) = store.db.query_row("SELECT rowid,conversation_id FROM messages WHERE account=?1 AND sent=1 AND trashed=0 ORDER BY id LIMIT 1", [ACCOUNT], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    store
        .db
        .execute(
            "UPDATE messages SET recipient='zed@fixture.example' WHERE rowid=?1",
            [sent.0],
        )
        .unwrap();
    assert_eq!(ids(&mut store, "to:zed", "sent"), vec![sent.1.clone()]);
    assert!(ids(&mut store, "from:zed", "sent").is_empty());
    assert_eq!(ids(&mut store, "zed@fixture.example", "sent"), vec![sent.1]);
    assert!(ids(&mut store, "to:zed", "inbox").is_empty());
    // Cc people come from the link index's people rows; the sender is not a recipient.
    let (row, conversation, sender): (i64, String, String) = store.db.query_row("SELECT rowid,conversation_id,sender_email FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY rowid LIMIT 1", rusqlite::params![ACCOUNT, b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    for (email, name) in [
        ("yara@fixture.example", "Yara Cc"),
        (sender.to_lowercase().as_str(), ""),
    ] {
        store
            .db
            .execute(
                "INSERT INTO people(email,name) VALUES(?1,?2) ON CONFLICT(email) DO NOTHING",
                [email, name],
            )
            .unwrap();
        store.db.execute("INSERT OR IGNORE INTO message_people(message_rowid,person) SELECT ?1,id FROM people WHERE email=?2", rusqlite::params![row, email]).unwrap();
    }
    assert_eq!(
        ids(&mut store, "to:yara@fixture.example", "inbox"),
        vec![conversation.clone()]
    );
    assert_eq!(
        ids(&mut store, "to:\"Yara Cc\"", "inbox"),
        vec![conversation.clone()]
    );
    assert!(ids(&mut store, "from:yara", "inbox").is_empty());
    assert_eq!(
        ids(&mut store, "yara@fixture.example", "inbox"),
        vec![conversation.clone()]
    );
    assert!(!ids(&mut store, &format!("to:{sender}"), "inbox").contains(&conversation));
    let terms = store
        .handle(
            "conversations",
            Some(ACCOUNT),
            json!({"query":"zed@fixture.example to:yara from:orchid"}),
        )
        .unwrap()["search_terms"]
        .clone();
    assert_eq!(terms["to"], json!(["yara"]));
    assert_eq!(terms["from"], json!(["orchid"]));
    assert_eq!(terms["people"], json!(["zed@fixture.example"]));
}

// Done is presented by month; counts cover the filtered set and a collapsed
// month's rows leave paging.
#[test]
fn done_months_count_in_the_client_zone_and_collapse_independently() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let request =
        json!({"grouped":true,"folder":"inbox","limit":100,"month_zone":"America/Toronto"});
    // Spread three Done conversations over other months. 02:00 UTC on
    // 1 August is still July in Toronto.
    let done: Vec<String> = store
        .handle("conversations", Some(ACCOUNT), request.clone())
        .unwrap()["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["group"] == "done")
        .take(3)
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    for (conversation, at) in done.iter().zip([
        "2026-07-15T12:00:00-04:00",
        "2025-12-03T12:00:00-05:00",
        "2026-08-01T02:00:00Z",
    ]) {
        store
            .db
            .execute(
                "UPDATE messages SET sent_at=?3 WHERE account=?1 AND conversation_id=?2",
                params![ACCOUNT, conversation, at],
            )
            .unwrap();
    }
    let full = store
        .handle("conversations", Some(ACCOUNT), request.clone())
        .unwrap();
    let months = full["done_months"].as_array().unwrap().clone();
    assert_eq!(
        months
            .iter()
            .map(|m| m["month"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2026-08", "2026-07", "2025-12"]
    );
    assert_eq!(months[1]["count"], 2);
    let total: i64 = months.iter().map(|m| m["count"].as_i64().unwrap()).sum();
    assert_eq!(
        total,
        full["filtered_group_counts"]["done"].as_i64().unwrap()
    );
    let names: Vec<&str> = months
        .iter()
        .map(|m| m["month"].as_str().unwrap())
        .collect();
    let mut newest_first = names.clone();
    newest_first.sort_by(|a, b| b.cmp(a));
    assert_eq!(names, newest_first);
    let zone: fm_calendar::Tz = "America/Toronto".parse().unwrap();
    let month_of = |row: &Value| {
        chrono::DateTime::parse_from_rfc3339(row["latest_at"].as_str().unwrap())
            .unwrap()
            .with_timezone(&zone)
            .format("%Y-%m")
            .to_string()
    };
    for month in &months {
        let key = month["month"].as_str().unwrap();
        let rows = full["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["group"] == "done" && month_of(r) == key)
            .count();
        assert_eq!(rows as i64, month["count"].as_i64().unwrap(), "{key}");
    }
    let collapsed = names[0];
    let mut folded = request.clone();
    folded["collapsed_months"] = json!([collapsed]);
    let result = store
        .handle("conversations", Some(ACCOUNT), folded)
        .unwrap();
    assert_eq!(result["done_months"], full["done_months"]);
    assert_eq!(
        result["expanded_conversation_count"].as_i64().unwrap(),
        full["expanded_conversation_count"].as_i64().unwrap()
            - months[0]["count"].as_i64().unwrap()
    );
    assert!(result["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["group"] != "done" || month_of(r) != collapsed));
    assert!(store
        .handle("conversations", Some(ACCOUNT), json!({"grouped":true}))
        .unwrap()["done_months"]
        .is_null());
    for bad in [
        json!({"collapsed_months":["2026-09"]}),
        json!({"month_zone":"Mars/Olympus"}),
        json!({"month_zone":"America/Toronto","collapsed_months":["2026-13"]}),
        json!({"month_zone":"America/Toronto","collapsed_months":["2026-09","2026-09"]}),
    ] {
        assert_eq!(
            store
                .handle("conversations", Some(ACCOUNT), bad)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    // Each month's state is saved with the account's view, as are recent searches.
    let saved = store.handle("view_save", Some(ACCOUNT), json!({"view":{"collapsedSections":["review","done:2026-08"],"recentSearches":["from:maya","subject:board deck"]},"expected_revision":0,"request_id":"sections"})).unwrap();
    assert_eq!(
        saved["view"]["collapsedSections"],
        json!(["review", "done:2026-08"])
    );
    for bad in [
        json!({"collapsedSections":["done:2026-8"]}),
        json!({"recentSearches":["a","b","c","d","e","f","g"]}),
        json!({"recentSearches":["two\nlines"]}),
    ] {
        assert_eq!(
            store
                .handle(
                    "view_save",
                    Some(ACCOUNT),
                    json!({"view":bad,"expected_revision":1,"request_id":"bad-view"})
                )
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}
