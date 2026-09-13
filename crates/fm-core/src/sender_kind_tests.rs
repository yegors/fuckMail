use crate::*;
use rusqlite::params;

const ACCOUNT: &str = "windscribe";
fn call(store: &mut Store, method: &str, params: Value) -> Value {
    store.handle(method, Some(ACCOUNT), params).unwrap()
}
fn detail(store: &mut Store, cid: &str) -> Value {
    call(store, "conversation", json!({"id":cid}))
}
fn append(
    store: &mut Store,
    account: &str,
    cid: &str,
    mid: &str,
    email: &str,
    date: &str,
    flags: (bool, bool, bool),
) {
    let (read, starred, sent) = flags;
    let hash=store.stage_blob(format!("From: {email}\r\nTo: you@example.test\r\nSubject: Sender test\r\n\r\nIndependent source {mid}").as_bytes()).unwrap();
    store
        .db
        .execute(
            "INSERT OR IGNORE INTO conversations(account,id,subject) VALUES(?1,?2,'Sender test')",
            params![account, cid],
        )
        .unwrap();
    store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) VALUES(?1,?2,?3,?2,'Known display name',?4,'you@example.test','Sender test','Independent source',?5,?6,?7,1,?8,?9)",params![account,mid,cid,email,date,!read,starred,sent,hash]).unwrap();
    let rowid = store.db.last_insert_rowid();
    store.db.execute("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,'Sender test','Independent source')",params![rowid,email]).unwrap();
}
fn annotate(store: &Store, account: &str, mid: &str, kind: &str) {
    store.db.execute("INSERT INTO message_sender_kinds(account,message_id,source_hash,kind,provenance) SELECT account,id,source_hash,?3,'fixture' FROM messages WHERE account=?1 AND id=?2 ON CONFLICT(account,message_id) DO UPDATE SET source_hash=excluded.source_hash,kind=excluded.kind",params![account,mid,kind]).unwrap();
}

#[test]
fn explicit_fixture_roles_and_filtered_counts_are_independent_of_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), 3);
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM message_sender_kinds", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        429
    );
    for (kind, total) in [
        ("all", 110),
        ("people", 109),
        ("automated", 1),
        ("unknown", 0),
    ] {
        let response = call(
            &mut store,
            "conversations",
            json!({"folder":"all","kind":kind,"grouped":true,"limit":1}),
        );
        assert_eq!(response["filtered_conversation_count"], total);
        let counts = &response["filtered_group_counts"];
        assert_eq!(
            counts["review"].as_i64().unwrap()
                + counts["held"].as_i64().unwrap()
                + counts["done"].as_i64().unwrap(),
            total
        );
        assert_eq!(response["account_counts"]["conversation_count"], 110);
    }
    let all = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true,"limit":1}),
    );
    assert_eq!(
        all["filtered_group_counts"],
        json!({"review":25,"held":9,"done":76})
    );
    let human = detail(&mut store, "windscribe-thread-000");
    assert_eq!(human["kind"], "people");
    assert!(human["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["kind"] == "people"));
    let automated = detail(&mut store, "windscribe-thread-005");
    assert_eq!(automated["kind"], "automated");
    assert_eq!(automated["messages"][0]["kind"], "automated");
    assert_eq!(automated["messages"][0]["sender_name"], "Status Robot");
    assert_eq!(
        automated["messages"][0]["sender_email"],
        "status@service.example"
    );
    assert_eq!(
        automated["messages"][0]["subject"],
        "Weekly service availability"
    );
    assert_eq!(automated["messages"][0]["to"], "alex@windscribe.example");
    for account in ["windscribe", "personal", "sideproject"] {
        let summary = store
            .handle(
                "sender_summary",
                Some(account),
                json!({"email":"STATUS@SERVICE.EXAMPLE"}),
            )
            .unwrap();
        assert_eq!(summary["conversation_count"], 1);
        assert_eq!(
            summary["conversations"][0]["id"],
            format!("{account}-thread-005")
        );
    }
    let hash: String = store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE account=?1 AND id='windscribe-message-005-000'",
            [ACCOUNT],
            |r| r.get(0),
        )
        .unwrap();
    let source = include_bytes!("../../../fixtures/messages/html-only.eml");
    assert_eq!(
        std::fs::read(store.verified_blob(&hash, source.len() as u64).unwrap()).unwrap(),
        source
    );
}

#[test]
fn latest_incoming_kind_is_source_bound_and_invalidates_cached_filters_and_cursors() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let cid = "mixed-sender-kind";
    append(
        &mut store,
        ACCOUNT,
        cid,
        "kind-a",
        "test@example.test",
        "2026-09-12T10:00:00Z",
        (false, false, false),
    );
    annotate(&store, ACCOUNT, "kind-a", "people");
    append(
        &mut store,
        ACCOUNT,
        cid,
        "kind-outgoing",
        "you@example.test",
        "2026-09-13T10:00:00Z",
        (true, false, true),
    );
    annotate(&store, ACCOUNT, "kind-outgoing", "automated");
    assert_eq!(detail(&mut store, cid)["kind"], "people");
    let page = call(
        &mut store,
        "conversations",
        json!({"folder":"all","kind":"people","limit":1}),
    );
    annotate(&store, ACCOUNT, "kind-a", "automated");
    assert_eq!(detail(&mut store, cid)["kind"], "automated");
    assert_eq!(
        store
            .handle(
                "conversations",
                Some(ACCOUNT),
                json!({"folder":"all","kind":"people","limit":1,"cursor":page["next_cursor"]})
            )
            .unwrap_err()
            .code,
        "stale_cursor"
    );
    // New incoming member wins a timestamp tie by ID, including append-cache updates.
    append(
        &mut store,
        ACCOUNT,
        cid,
        "kind-z",
        "test@example.test",
        "2026-09-12T06:00:00-04:00",
        (false, false, false),
    );
    assert_eq!(detail(&mut store, cid)["kind"], "unknown");
    annotate(&store, ACCOUNT, "kind-z", "people");
    assert_eq!(detail(&mut store, cid)["kind"], "people");
    let original: String = store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE account=?1 AND id='kind-z'",
            [ACCOUNT],
            |r| r.get(0),
        )
        .unwrap();
    let replacement = store.stage_blob(b"Replaced retained source bytes").unwrap();
    store
        .db
        .execute(
            "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id='kind-z'",
            params![replacement, ACCOUNT],
        )
        .unwrap();
    assert_eq!(detail(&mut store, cid)["kind"], "unknown");
    store
        .db
        .execute(
            "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id='kind-z'",
            params![original, ACCOUNT],
        )
        .unwrap();
    assert_eq!(detail(&mut store, cid)["kind"], "people");
    store.db.execute_batch("BEGIN; UPDATE message_sender_kinds SET kind='automated' WHERE message_id='kind-z'; ROLLBACK;").unwrap();
    assert_eq!(detail(&mut store, cid)["kind"], "people");
    store
        .db
        .execute(
            "DELETE FROM message_sender_kinds WHERE account=?1 AND message_id='kind-z'",
            [ACCOUNT],
        )
        .unwrap();
    assert_eq!(detail(&mut store, cid)["kind"], "unknown");
    // All-outgoing conversations deliberately fall back to their newest member.
    store
        .db
        .execute(
            "UPDATE messages SET sent=1 WHERE account=?1 AND conversation_id=?2",
            params![ACCOUNT, cid],
        )
        .unwrap();
    assert_eq!(detail(&mut store, cid)["kind"], "automated");
    append(
        &mut store,
        "personal",
        cid,
        "kind-z",
        "test@example.test",
        "2026-09-12T10:00:00Z",
        (false, false, false),
    );
    annotate(&store, "personal", "kind-z", "people");
    assert_eq!(
        store
            .handle("conversation", Some("personal"), json!({"id":cid}))
            .unwrap()["kind"],
        "people"
    );
    assert_eq!(detail(&mut store, cid)["kind"], "automated");
}

#[test]
fn sender_history_uses_exact_identity_full_counts_and_actual_recent_instants_without_reviewing() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    for (index, date, read, star) in [
        (0, "2026-09-12T10:00:00Z", false, false),
        (1, "2026-09-12T07:00:00-04:00", true, true),
        (2, "2026-09-12T08:00:00-04:00", true, false),
        (3, "2026-09-12T09:00:00-04:00", false, true),
    ] {
        append(
            &mut store,
            ACCOUNT,
            &format!("sender-{index}"),
            &format!("sender-{index}"),
            "Alex.O'Hara+build@sender.example",
            date,
            (read, star, false),
        );
    }
    append(
        &mut store,
        ACCOUNT,
        "sender-0",
        "sender-extra",
        "alex.o'hara+build@sender.example",
        "2026-09-12T10:01:00Z",
        (false, false, false),
    );
    append(
        &mut store,
        ACCOUNT,
        "sender-prefix",
        "sender-prefix",
        "prefixalex.o'hara+build@sender.example",
        "2026-09-15T00:00:00Z",
        (false, false, false),
    );
    append(
        &mut store,
        "personal",
        "sender-foreign",
        "sender-foreign",
        "alex.o'hara+build@sender.example",
        "2026-09-16T00:00:00Z",
        (false, false, false),
    );
    let revision = store.revision().unwrap();
    let summary = call(
        &mut store,
        "sender_summary",
        json!({"email":"  ALEX.O'HARA+BUILD@SENDER.EXAMPLE  "}),
    );
    assert_eq!(summary["account"], ACCOUNT);
    assert_eq!(summary["email"], "alex.o'hara+build@sender.example");
    assert_eq!(summary["conversation_count"], 4);
    assert_eq!(
        summary["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["sender-3", "sender-2", "sender-1"]
    );
    let filtered = call(
        &mut store,
        "conversations",
        json!({"folder":"all","exact_sender":summary["email"],"kind":"unknown","limit":1}),
    );
    assert_eq!(filtered["filtered_conversation_count"], 4);
    assert_eq!(
        filtered["filtered_group_counts"],
        json!({"review":2,"held":1,"done":1})
    );
    assert_eq!(store.revision().unwrap(), revision);
    assert_eq!(
        detail(&mut store, "sender-0")["messages"][0]["unread"],
        true
    );
    assert_eq!(
        call(
            &mut store,
            "sender_summary",
            json!({"email":"missing@example.test"})
        )["conversation_count"],
        0
    );
    for p in [
        json!({"kind":"inferred"}),
        json!({"kind":null}),
        json!({"exact_sender":"bad"}),
        json!({"exact_sender":3}),
    ] {
        assert_eq!(
            store
                .handle("conversations", Some(ACCOUNT), p)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    for p in [
        json!({"email":""}),
        json!({"email":"test@example.test","limit":100}),
        json!({"email":"x".repeat(257)}),
        json!({"email":"a@example.test\r\nBcc: b@example.test"}),
    ] {
        assert_eq!(
            store
                .handle("sender_summary", Some(ACCOUNT), p)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    let saved = call(
        &mut store,
        "view_save",
        json!({"request_id":"sender-view","expected_revision":0,"view":{"filters":{"kind":"unknown","exact_sender":"alex.o'hara+build@sender.example"}}}),
    );
    assert_eq!(saved["view"]["filters"]["kind"], "unknown");
    // The pinned MIME mailbox parser accepts these domains. Exact identity
    // must work in both directions, including a Unicode uppercase letter whose
    // lowercase equivalent is ASCII (Kelvin sign), without changing the source.
    for (index, stored, query) in [
        (0, "Author@BÜCHER.example", "author@bücher.EXAMPLE"),
        (1, "Reader@K.example", "reader@k.example"),
    ] {
        fm_mail::validate_address(&fm_mail::Address {
            name: None,
            email: stored.into(),
        })
        .unwrap();
        append(
            &mut store,
            ACCOUNT,
            &format!("sender-unicode-{index}"),
            &format!("sender-unicode-{index}"),
            stored,
            "2026-09-12T20:00:00Z",
            (false, false, false),
        );
        let summary = call(&mut store, "sender_summary", json!({"email":query}));
        assert_eq!(summary["conversation_count"], 1);
        assert_eq!(
            summary["conversations"][0]["id"],
            format!("sender-unicode-{index}")
        );
        let draft = call(
            &mut store,
            "draft_create",
            json!({"mode":"new","request_id":format!("unicode-card-{index}")}),
        );
        let recipients = json!([{"email":stored,"name":"Unicode sender"}]);
        let saved = call(
            &mut store,
            "draft_save",
            json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"request_id":format!("unicode-card-save-{index}"),"body":"Human-written card reply","to_recipients":recipients}),
        );
        assert_eq!(saved["to_recipients"], recipients);
    }
}

#[test]
fn schema6_migration_keeps_old_profiles_receipts_and_sources_unclassified() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = Store::open(dir.path()).unwrap();
    let draft = call(
        &mut old,
        "draft_get",
        json!({"conversation_id":"windscribe-thread-000"}),
    );
    call(
        &mut old,
        "draft_save",
        json!({"conversation_id":"windscribe-thread-000","body":"Preserve my authored words","expected_revision":0,"request_id":"migration-save","source_message_id":draft["source_message_id"]}),
    );
    let capture_params = json!({"conversation_id":"windscribe-thread-000","expected_revision":1,"request_id":"migration-capture","outcome":"accepted"});
    let receipt = call(&mut old, "draft_capture", capture_params.clone());
    crate::test_profiles::restore_schema_eight(&old.db);
    // Reconstruct the actual historical schema, including removal of later
    // profile tables; merely lowering user_version is not a schema-6 fixture.
    old.db.execute_batch("DROP TABLE google_accounts; DROP TABLE profile_identity; DELETE FROM meta WHERE key='profile_kind'; DROP TABLE message_sender_kinds; UPDATE meta SET value=2 WHERE key='fixture_revision'; PRAGMA user_version=6;").unwrap();
    let snapshot = |db: &Connection| -> Vec<Vec<Vec<String>>> {
        [
            "accounts",
            "messages",
            "drafts",
            "operations",
            "capture_operations",
            "views",
            "meta",
        ]
        .iter()
        .map(|table| {
            let where_clause = if *table == "meta" {
                " WHERE key!='profile_kind'"
            } else {
                ""
            };
            let mut stmt = db
                .prepare(&format!(
                    "SELECT * FROM {table}{where_clause} ORDER BY rowid"
                ))
                .unwrap();
            let count = stmt.column_count();
            stmt.query_map([], |r| {
                Ok((0..count)
                    .map(|column| format!("{:?}", r.get_ref(column).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
        })
        .collect()
    };
    let before = snapshot(&old.db);
    drop(old);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), 2);
    assert_eq!(snapshot(&store.db), before);
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM message_sender_kinds", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let list = call(
        &mut store,
        "conversations",
        json!({"folder":"all","kind":"unknown","limit":1}),
    );
    assert_eq!(list["filtered_conversation_count"], 110);
    assert_eq!(call(&mut store, "draft_capture", capture_params), receipt);
    assert_eq!(
        detail(&mut store, "windscribe-thread-005")["kind"],
        "unknown"
    );
}

#[test]
fn collapsed_groups_page_expanded_rows_without_changing_full_filter_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let full = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true,"limit":1}),
    );
    let held = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true,"limit":1,"collapsed_groups":["review"]}),
    );
    assert_eq!(held["conversations"][0]["group"], "held");
    assert_eq!(held["filtered_conversation_count"], 110);
    assert_eq!(held["expanded_conversation_count"], 85);
    assert_eq!(held["filtered_group_counts"], full["filtered_group_counts"]);
    assert_eq!(held["next_offset"], 1);
    let next = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true,"limit":1,"collapsed_groups":["review"],"cursor":held["next_cursor"]}),
    );
    assert_ne!(
        next["conversations"][0]["id"],
        held["conversations"][0]["id"]
    );
    let done = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true,"limit":1,"collapsed_groups":["review","held"]}),
    );
    assert_eq!(done["conversations"][0]["group"], "done");
    assert_eq!(done["expanded_conversation_count"], 76);
    assert_eq!(done["filtered_group_counts"], full["filtered_group_counts"]);
    let all = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true,"limit":1,"collapsed_groups":["review","held","done"]}),
    );
    assert_eq!(all["conversations"], json!([]));
    assert_eq!(all["expanded_conversation_count"], 0);
    assert_eq!(all["filtered_conversation_count"], 110);
    assert_eq!(all["filtered_group_counts"], full["filtered_group_counts"]);
    assert!(all["next_cursor"].is_null());
    assert!(all["next_offset"].is_null());
    assert_eq!(store.handle("conversations",Some(ACCOUNT),json!({"folder":"all","grouped":true,"limit":1,"collapsed_groups":[],"cursor":held["next_cursor"]})).unwrap_err().code,"stale_cursor");
    for groups in [
        json!(null),
        json!(true),
        json!(["all"]),
        json!(["review", "review"]),
        json!(["review", "held", "done", "review"]),
    ] {
        assert_eq!(
            store
                .handle(
                    "conversations",
                    Some(ACCOUNT),
                    json!({"collapsed_groups":groups})
                )
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}

#[test]
fn calendar_generation_tracks_actual_calendar_writes_but_not_appearance_or_mail() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let generation = |store: &mut Store| {
        store.handle("status", None, json!({})).unwrap()["calendar_generation"]
            .as_u64()
            .unwrap()
    };
    assert_eq!(generation(&mut store), 0);
    store.handle("preferences_save",None,json!({"request_id":"generation-theme","expected_revision":0,"preferences":{"theme":"paper","text_size":"large"}})).unwrap();
    call(
        &mut store,
        "draft_save",
        json!({"request_id":"generation-draft","conversation_id":"windscribe-thread-000","expected_revision":0,"body":"Human-authored generation test"}),
    );
    let row = detail(&mut store, "windscribe-thread-001");
    call(
        &mut store,
        "flag",
        json!({"request_id":"generation-read","id":row["id"],"message_ids":row["message_ids"],"expected_revision":row["revision"],"read":true}),
    );
    assert_eq!(generation(&mut store), 0);
    call(
        &mut store,
        "calendar_write",
        json!({"request_id":"generation-calendar","kind":"create","calendar_id":"primary","fields":{"title":"Calendar generation test","start":"2026-09-14","end":"2026-09-15","all_day":true,"time_zone":"America/Toronto"}}),
    );
    let changed = generation(&mut store);
    assert!(changed > 0);
    store.handle("preferences_save",None,json!({"request_id":"generation-theme-again","expected_revision":1,"preferences":{"theme":"harbour"}})).unwrap();
    call(
        &mut store,
        "calendar",
        json!({"from":"2026-09-14","to":"2026-09-15"}),
    );
    assert_eq!(generation(&mut store), changed);
    drop(store);
    let mut reopened = Store::open(dir.path()).unwrap();
    assert_eq!(generation(&mut reopened), 0);
}
