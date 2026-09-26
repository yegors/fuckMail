//! Search-planner regression tests plus opt-in performance measurements over
//! generated mail.
//!
//! The measurements are `#[ignore]`d; run one with
//! `cargo test -p fm-core --release <name> -- --ignored --nocapture`. Each
//! prints a JSON report and, when its `FM_*_REPORT` variable names a file,
//! writes it there. Every test builds a fresh temporary profile and never
//! reads a real one.
use crate::Store;
use rusqlite::params;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// The `quantile` (0 to 1) sample in milliseconds, rounding the rank up.
/// Sorts `samples` in place.
fn percentile(samples: &mut [Duration], quantile: f64) -> f64 {
    samples.sort_unstable();
    let index = ((samples.len() - 1) as f64 * quantile).ceil() as usize;
    samples[index].as_secs_f64() * 1000.0
}

/// Two accounts of generated mail with one rare token in the first account.
fn seed_planner_mail(store: &Store, accounts: [&str; 2]) {
    let tx = store.db.unchecked_transaction().unwrap();
    let mut conversation = tx
            .prepare("INSERT INTO conversations(account,id,subject,revision) VALUES(?1,?2,'Planner fixture',1)")
            .unwrap();
    let mut message = tx.prepare("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?3,?4,'Fixture','fixture@example.test','reader@example.test','Planner fixture',?5,'2026-09-11T12:00:00Z',1,0,1,0,'',1)").unwrap();
    let mut fts = tx
            .prepare("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,'Fixture fixture@example.test','Planner fixture',?2)")
            .unwrap();
    for index in 0..6_000 {
        let account = accounts[index % 2];
        let conversation_id = format!("planner-{account}-{index}");
        let message_id = format!("planner-message-{index}");
        let body = if index == 200 {
            "rareplannertoken"
        } else {
            "commonplannertoken"
        };
        conversation
            .execute(params![account, conversation_id])
            .unwrap();
        message
            .execute(params![
                account,
                message_id,
                conversation_id,
                message_id,
                body
            ])
            .unwrap();
        fts.execute(params![tx.last_insert_rowid(), body]).unwrap();
    }
    drop(fts);
    drop(message);
    drop(conversation);
    tx.commit().unwrap();
}

fn assert_search_starts_at_fts(store: &Store, account: &str) {
    let sql = format!(
        "EXPLAIN QUERY PLAN {}",
        crate::queries::search_matches_sql(true, &json!({"folder":"inbox"}))
    );
    let plan = store
        .db
        .prepare(&sql)
        .unwrap()
        .query_map(
            params![
                account,
                "inbox",
                "",
                "\"rareplannertoken\"*",
                20,
                0,
                "",
                "",
                None::<bool>,
                "",
                "",
                ""
            ],
            |r| r.get::<_, String>(3),
        )
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let first = plan
        .iter()
        .find(|step| step.contains("message_fts") || step.contains("messages"))
        .unwrap();
    assert!(
        first.contains("message_fts"),
        "search should start at FTS, got {plan:?}"
    );
}

#[test]
fn reopened_mailbox_builds_search_statistics_and_starts_at_fts() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("planner-profile");
    let store = Store::open(&profile).unwrap();
    seed_planner_mail(&store, ["windscribe", "personal"]);
    drop(store);

    let mut store = Store::open(&profile).unwrap();
    let stats: i64 = store
        .db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_stat1 WHERE tbl='messages'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        stats > 0,
        "opening a populated profile should collect planner statistics"
    );
    assert_search_starts_at_fts(&store, "windscribe");
    let result = store
        .handle(
            "conversations",
            Some("windscribe"),
            json!({"folder":"inbox","query":"rareplannertoken","limit":20}),
        )
        .unwrap();
    assert_eq!(result["conversations"].as_array().unwrap().len(), 1);
}

#[test]
fn fresh_profile_first_sync_refreshes_search_statistics_without_restart() {
    let temp = tempfile::tempdir().unwrap();
    // Open-time statistics are collected while this live profile is empty.
    let store = Store::open_live(&temp.path().join("fresh-planner")).unwrap();
    store
        .db
        .execute_batch(
            "INSERT INTO accounts(id,name,email,provider) VALUES('first','First','first@example.test','google'),('second','Second','second@example.test','google');",
        )
        .unwrap();
    seed_planner_mail(&store, ["first", "second"]);
    store.refresh_planner_statistics();
    assert_search_starts_at_fts(&store, "first");
}

#[test]
#[ignore = "Opt-in 100k search experiment: cargo test -p fm-core --release search_100k -- --ignored --nocapture"]
fn search_100k() {
    let iterations = std::env::var("FM_BENCHMARK_ITERATIONS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|v| (1..=1000).contains(v))
        .unwrap_or(100);
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("disposable-search-benchmark");
    let store = Store::open(&profile).unwrap();
    let source = store.stage_blob(b"From: Synthetic <benchmark@fixture.example>\r\nSubject: Search benchmark fixture\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSynthetic source; SQL metadata is generated for an index-density experiment.\r\n").unwrap();
    let seed_start = Instant::now();
    {
        let tx = store.db.unchecked_transaction().unwrap();
        let mut conversation = tx
            .prepare("INSERT INTO conversations(account,id,subject,revision) VALUES(?1,?2,?3,1)")
            .unwrap();
        let mut message = tx.prepare("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1,0,1,0,?11,1)").unwrap();
        let mut fts = tx
            .prepare("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)")
            .unwrap();
        for index in 0..100_000 {
            let account = if index % 2 == 0 {
                "windscribe"
            } else {
                "personal"
            };
            let conversation_id = format!("benchmark-{account}-thread-{index:06}");
            let message_id = format!("benchmark-{account}-message-{index:06}");
            let subject = if index % 10_000 == 0 {
                "Benchmark needlequarter planning"
            } else {
                "Benchmark weekly planning"
            };
            let (name, email) = if account == "windscribe" {
                ("Alex O’Hara", "alex.o'hara+build@windscribe.example")
            } else {
                ("Zoë Chen", "zoe.chen+home@personal.example")
            };
            let body = if index % 997 == 0 {
                "Search benchmark: ultraquartz deployment detail. Café review 日本語."
            } else {
                "Search benchmark: ordinary deployment detail. Café review 日本語."
            };
            let sent_at = format!(
                "2026-09-{:02}T{:02}:{:02}:{:02}Z",
                1 + index % 11,
                index % 24,
                index % 60,
                (index / 60) % 60
            );
            conversation
                .execute(params![account, conversation_id, subject])
                .unwrap();
            message
                .execute(params![
                    account,
                    message_id,
                    conversation_id,
                    format!("bench-remote-{index:06}"),
                    name,
                    email,
                    format!("you@{account}.example"),
                    subject,
                    body,
                    sent_at,
                    source
                ])
                .unwrap();
            fts.execute(params![
                tx.last_insert_rowid(),
                format!("{name} {email}"),
                subject,
                body
            ])
            .unwrap();
        }
        drop(fts);
        drop(message);
        drop(conversation);
        tx.commit().unwrap();
    }
    store
        .db
        .execute_batch("ANALYZE; PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    let seed_seconds = seed_start.elapsed().as_secs_f64();
    let database_bytes = std::fs::metadata(profile.join("store.sqlite3"))
        .unwrap()
        .len();
    let message_count: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    drop(store);

    let queries = [
        (
            "sender punctuation",
            "windscribe",
            "sender",
            "o'hara+build@windscribe.example",
            20,
        ),
        (
            "sender account isolation",
            "personal",
            "sender",
            "o'hara+build@windscribe.example",
            0,
        ),
        ("rare subject", "windscribe", "subject", "needlequarter", 10),
        ("rare body", "windscribe", "body", "ultraquartz", 20),
        ("common body", "personal", "body", "deployment", 20),
        ("unicode body", "personal", "body", "cafe 日本語", 20),
    ];
    let mut store = Store::open(&profile).unwrap();
    let mut measurements = Vec::new();
    for (name, account, field, query, expected) in queries {
        let request = json!({"query":query,"field":field,"folder":"all","limit":20});
        let started = Instant::now();
        let initial = store
            .handle("conversations", Some(account), request.clone())
            .unwrap();
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            initial["conversations"].as_array().unwrap().len(),
            expected,
            "query={name}"
        );
        for row in initial["conversations"].as_array().unwrap() {
            assert!(row["id"]
                .as_str()
                .unwrap()
                .starts_with(&format!("benchmark-{account}-")));
            assert_eq!(row["matching_message_ids"].as_array().unwrap().len(), 1);
        }
        let mut samples = Vec::new();
        for _ in 0..iterations {
            let started = Instant::now();
            let result = store
                .handle("conversations", Some(account), request.clone())
                .unwrap();
            samples.push(started.elapsed());
            assert_eq!(
                result["conversations"], initial["conversations"],
                "unstable results for {name}"
            );
        }
        measurements.push(json!({"query":name,"field":field,"account":account,"terms":query,"returned":expected,
            "first_query_ms":first_ms,"warm_iterations":samples.len(),"warm_p50_ms":percentile(&mut samples,0.50),
            "warm_p95_ms":percentile(&mut samples,0.95),"warm_p99_ms":percentile(&mut samples,0.99)}));
    }
    for (name, request) in [
        (
            "grouped first50",
            json!({"grouped":true,"folder":"all","limit":50}),
        ),
        (
            "combined filters first50",
            json!({"query":"deployment","field":"body","grouped":true,"folder":"all","limit":50,"sender":"alex.o'hara+build@windscribe.example","status":"review","has_attachment":false,"after":"2026-09-01","before":"2026-09-12"}),
        ),
    ] {
        let started = Instant::now();
        let initial = store
            .handle("conversations", Some("windscribe"), request.clone())
            .unwrap();
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(initial["conversations"].as_array().unwrap().len(), 50);
        let mut samples = Vec::new();
        for _ in 0..iterations {
            let started = Instant::now();
            let result = store
                .handle("conversations", Some("windscribe"), request.clone())
                .unwrap();
            samples.push(started.elapsed());
            assert_eq!(result["conversations"], initial["conversations"]);
        }
        measurements.push(json!({"query":name,"account":"windscribe","request":request,"returned":50,"first_query_ms":first_ms,"warm_iterations":samples.len(),"warm_p50_ms":percentile(&mut samples,0.50),"warm_p95_ms":percentile(&mut samples,0.95),"warm_p99_ms":percentile(&mut samples,0.99)}));
    }
    let concurrent_ingestion = measure_queued_ingestion(&mut store, &source, iterations);
    let plan_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        crate::queries::search_matches_sql(
            true,
            &json!({"folder":"all","sender":"alex","status":"review","has_attachment":false,"after":"2026-09-01","before":"2026-09-12"})
        )
    );
    let mut statement = store.db.prepare(&plan_sql).unwrap();
    let plan = statement
        .query_map(
            params![
                "windscribe",
                "all",
                "",
                "body:(\"deployment\"*)",
                51,
                0,
                "%alex%",
                "review",
                Some(false),
                "2026-09-01",
                "2026-09-12",
                ""
            ],
            |r| r.get::<_, String>(3),
        )
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let mut report: Value = json!({"experiment":"100k generated SQLite/FTS5 message rows through Store.handle",
        "fixture_revision":crate::FIXTURE_REVISION,"generated_messages":100000,"observations_per_class":iterations,"total_messages_including_small_fixture":message_count,
        "sqlite_version":rusqlite::version(),"target_os":std::env::consts::OS,"target_arch":std::env::consts::ARCH,
        "seed_seconds":seed_seconds,"database_bytes":database_bytes,"cache_context":"One reopened SQLite connection after bulk seed/checkpoint; OS filesystem cache was not cleared. Per-query warm observation counts recorded in each row. Static samples have no ingestion/inference. Concurrent-ingestion block uses independent producers and one serialized Store writer with bounded queue.",
        "coverage_limits":"Index density fixture with shared retained synthetic source; not 25 GB mailbox, attachment-throughput, 1m/3m, OS-cold, real provider ingestion or power-loss evidence.","query_plan":plan,"queries":measurements,"concurrent_ingestion":concurrent_ingestion});
    if std::env::var("FM_BENCHMARK_KEEP_PROFILE").as_deref() == Ok("1") {
        // Only keep the newly generated tempfile; never overwrite an input profile.
        let _retained_root = temp.keep();
        report["retained_profile"] = json!(profile.to_string_lossy());
    }
    let encoded = serde_json::to_string_pretty(&report).unwrap();
    if let Some(path) = std::env::var_os("FM_BENCHMARK_REPORT") {
        std::fs::write(path, &encoded).unwrap();
    }
    println!("{encoded}");
}

// Independent synthetic insertion and search producers share one bounded worker
// queue, matching Core's single-writer scheduling without bypassing writer locks.
// SQL insertion is a normalized-row density diagnostic, not a provider importer.
fn measure_queued_ingestion(store: &mut Store, source: &str, iterations: usize) -> Value {
    enum Work {
        Insert(usize),
        Query(Instant, std::sync::mpsc::SyncSender<(Duration, Duration)>),
    }
    let mut search_samples = Vec::new();
    let mut queued_samples = Vec::new();
    let mut ingest_samples = Vec::new();
    std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::sync_channel::<Work>(2);
        let worker=scope.spawn(move||{
            let mut ingestion=Vec::new();
            while let Ok(work)=receive.recv(){match work {
                Work::Insert(batch)=>{
                    let started=Instant::now();let tx=store.db.unchecked_transaction().unwrap();
                    for index in 0..25 {
                        let id=format!("ingest-{batch:03}-{index:02}");
                        tx.execute("INSERT INTO conversations(account,id,subject,revision) VALUES('windscribe',?1,'Concurrent deployment',1)",[&id]).unwrap();
                        tx.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES('windscribe',?1,?1,?1,'Alex O’Hara','alex.o''hara+build@windscribe.example','you@windscribe.example','Concurrent deployment','New normalized deployment detail','2026-09-11T23:00:00Z',1,0,1,0,?2,1)",params![id,source]).unwrap();
                        tx.execute("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,'Alex O’Hara alex.o''hara+build@windscribe.example','Concurrent deployment','New normalized deployment detail')",[tx.last_insert_rowid()]).unwrap();
                    }
                    tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'",[]).unwrap();tx.commit().unwrap();ingestion.push(started.elapsed());
                },
                Work::Query(queued,reply)=>{
                    let started=Instant::now();
                    let result=store.handle("conversations",Some("windscribe"),json!({"query":"deployment","field":"body","grouped":true,"folder":"all","limit":50,"sender":"alex","status":"review","has_attachment":false,"after":"2026-09-01","before":"2026-09-12"})).unwrap();
                    assert_eq!(result["conversations"].as_array().unwrap().len(),50);
                    assert_eq!(result["coverage"]["text_index_complete"],true);
                    reply.send((started.elapsed(),queued.elapsed())).unwrap();
                }
            }}
            ingestion
        });
        let ingest_send = send.clone();
        let producer = scope.spawn(move || {
            for batch in 0..iterations {
                ingest_send.send(Work::Insert(batch)).unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        for _ in 0..iterations {
            let (reply, receive) = std::sync::mpsc::sync_channel(1);
            send.send(Work::Query(Instant::now(), reply)).unwrap();
            let (query, queue) = receive.recv().unwrap();
            search_samples.push(query);
            queued_samples.push(queue);
        }
        producer.join().unwrap();
        drop(send);
        ingest_samples = worker.join().unwrap();
    });
    assert_eq!(ingest_samples.len(), iterations);
    json!({"model":"independent search and normalized-insertion producers; one serialized Store writer; bounded queue capacity 2; no parallel SQLite writer or provider transport","inserted_messages":iterations*25,"batches":iterations,"messages_per_batch":25,"query_observations":search_samples.len(),"core_query_p50_ms":percentile(&mut search_samples,0.5),"core_query_p95_ms":percentile(&mut search_samples,0.95),"core_query_p99_ms":percentile(&mut search_samples,0.99),"queue_to_response_p50_ms":percentile(&mut queued_samples,0.5),"queue_to_response_p95_ms":percentile(&mut queued_samples,0.95),"queue_to_response_p99_ms":percentile(&mut queued_samples,0.99),"ingestion_batch_p50_ms":percentile(&mut ingest_samples,0.5),"ingestion_batch_p95_ms":percentile(&mut ingest_samples,0.95),"ingestion_batch_p99_ms":percentile(&mut ingest_samples,0.99)})
}

#[test]
#[ignore = "Scoped generated mail browsing latency measurement"]
fn mail_browse_20k() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("disposable-search-benchmark");
    let store = Store::open(&profile).unwrap();
    let source = store.stage_blob(b"From: Synthetic <benchmark@fixture.example>\r\nSubject: Search benchmark fixture\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSynthetic source; SQL metadata is generated for an index-density experiment.\r\n").unwrap();
    {
        let tx = store.db.unchecked_transaction().unwrap();
        let mut conversation = tx
            .prepare("INSERT INTO conversations(account,id,subject,revision) VALUES(?1,?2,?3,1)")
            .unwrap();
        let mut message = tx.prepare("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1,0,1,0,?11,1)").unwrap();
        let mut fts = tx
            .prepare("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)")
            .unwrap();
        for index in 0..20_000 {
            let account = if index % 2 == 0 {
                "windscribe"
            } else {
                "personal"
            };
            let conversation_id = format!("benchmark-{account}-thread-{index:06}");
            let message_id = format!("benchmark-{account}-message-{index:06}");
            let subject = if index % 10_000 == 0 {
                "Benchmark needlequarter planning"
            } else {
                "Benchmark weekly planning"
            };
            let (name, email) = if account == "windscribe" {
                ("Alex O’Hara", "alex.o'hara+build@windscribe.example")
            } else {
                ("Zoë Chen", "zoe.chen+home@personal.example")
            };
            let body = if index % 997 == 0 {
                "Search benchmark: ultraquartz deployment detail. Café review 日本語."
            } else {
                "Search benchmark: ordinary deployment detail. Café review 日本語."
            };
            let sent_at = format!(
                "2026-09-{:02}T{:02}:{:02}:{:02}Z",
                1 + index % 11,
                index % 24,
                index % 60,
                (index / 60) % 60
            );
            conversation
                .execute(params![account, conversation_id, subject])
                .unwrap();
            message
                .execute(params![
                    account,
                    message_id,
                    conversation_id,
                    format!("bench-remote-{index:06}"),
                    name,
                    email,
                    format!("you@{account}.example"),
                    subject,
                    body,
                    sent_at,
                    source
                ])
                .unwrap();
            fts.execute(params![
                tx.last_insert_rowid(),
                format!("{name} {email}"),
                subject,
                body
            ])
            .unwrap();
        }
        drop(fts);
        drop(message);
        drop(conversation);
        tx.commit().unwrap();
    }
    store
        .db
        .execute_batch("ANALYZE; PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();

    let mut store = store;
    let mut pages = Vec::new();
    let mut readers = Vec::new();
    let mut changes = Vec::new();
    let mut first = String::new();
    for index in 0..50 {
        let started = Instant::now();
        let page = store
            .handle(
                "conversations",
                Some("windscribe"),
                json!({"folder":"inbox","grouped":true,"limit":100,"offset":index*100}),
            )
            .unwrap();
        pages.push(started.elapsed());
        assert_eq!(page["conversations"].as_array().unwrap().len(), 100);
        let id = page["conversations"][0]["id"].as_str().unwrap();
        if index == 0 {
            first = id.into();
        }
        let started = Instant::now();
        let read = store
            .handle("conversation", Some("windscribe"), json!({"id":id}))
            .unwrap();
        readers.push(started.elapsed());
        let started = Instant::now();
        store.handle("flag",Some("windscribe"),json!({"id":id,"request_id":format!("browse-{index}"),"expected_revision":read["revision"],"message_ids":read["message_ids"],"read":true})).unwrap();
        store.handle("accounts", None, json!({})).unwrap();
        store
            .handle(
                "conversations",
                Some("windscribe"),
                json!({"folder":"inbox","grouped":true,"limit":100}),
            )
            .unwrap();
        changes.push(started.elapsed());
    }
    let contentless_fts_bytes: i64 = store
        .db
        .query_row(
            "SELECT COALESCE(sum(pgsize),0) FROM dbstat WHERE name LIKE 'message_fts_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    store
        .db
        .execute_batch("CREATE VIRTUAL TABLE full_fts USING fts5(sender,subject,body,tokenize='unicode61 remove_diacritics 2',prefix='2 3 4'); INSERT INTO full_fts(rowid,sender,subject,body) SELECT rowid,sender_name||' '||sender_email,subject,body FROM messages;")
        .unwrap();
    let full_fts_bytes: i64 = store
        .db
        .query_row(
            "SELECT COALESCE(sum(pgsize),0) FROM dbstat WHERE name LIKE 'full_fts_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    store.db.execute_batch("INSERT INTO categories(account,id,name) VALUES('windscribe','bench-common','Common'),('personal','bench-common','Common'),('windscribe','bench-selective','Selective'),('personal','bench-selective','Selective'),('windscribe','bench-write','Write probe'),('personal','bench-write','Write probe');").unwrap();
    {
        let tx = store.db.unchecked_transaction().unwrap();
        let mut category = tx
            .prepare(
                "INSERT INTO message_categories(account,message_id,category_id) VALUES(?1,?2,?3)",
            )
            .unwrap();
        for index in 0..20_000 {
            let account = if index % 2 == 0 {
                "windscribe"
            } else {
                "personal"
            };
            let message_id = format!("benchmark-{account}-message-{index:06}");
            category
                .execute(params![account, message_id, "bench-common"])
                .unwrap();
            if index % 101 == 0 {
                category
                    .execute(params![account, message_id, "bench-selective"])
                    .unwrap();
            }
        }
        drop(category);
        tx.commit().unwrap();
    }
    store
        .db
        .execute_batch("DROP INDEX IF EXISTS message_categories_by_category; ANALYZE;")
        .unwrap();
    let category_write_probe = |store: &Store| {
        let start = Instant::now();
        let tx = store.db.unchecked_transaction().unwrap();
        let mut insert = tx.prepare("INSERT INTO message_categories(account,message_id,category_id) VALUES(?1,?2,'bench-write')").unwrap();
        for index in 0..20_000 {
            let account = if index % 2 == 0 {
                "windscribe"
            } else {
                "personal"
            };
            let message_id = format!("benchmark-{account}-message-{index:06}");
            insert.execute(params![account, message_id]).unwrap();
        }
        drop(insert);
        tx.rollback().unwrap();
        start.elapsed().as_secs_f64() * 1000.0
    };
    let category_write_ms_before = category_write_probe(&store);
    let category_sql = "SELECT message_id FROM message_categories WHERE account='windscribe' AND category_id='bench-selective'";
    let category_probe = |store: &Store| {
        let plan: String = store
            .db
            .query_row(&format!("EXPLAIN QUERY PLAN {category_sql}"), [], |row| {
                row.get(3)
            })
            .unwrap();
        let mut timings = Vec::new();
        let mut found = 0;
        for _ in 0..100 {
            let start = Instant::now();
            let mut statement = store.db.prepare_cached(category_sql).unwrap();
            found = statement
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
                .len();
            timings.push(start.elapsed());
        }
        assert!(found > 0);
        (plan, percentile(&mut timings, 0.95))
    };
    let (category_plan_before, category_p95_before) = category_probe(&store);
    store.db.execute_batch("CREATE INDEX message_categories_by_category ON message_categories(account,category_id,message_id); ANALYZE;").unwrap();
    let category_write_ms_after = category_write_probe(&store);
    let (category_plan_after, category_p95_after) = category_probe(&store);
    let category_index_bytes: i64 = store.db.query_row("SELECT COALESCE(sum(pgsize),0) FROM dbstat WHERE name='message_categories_by_category'",[],|row|row.get(0)).unwrap();
    let report = json!({"generated_messages":20000,"observations":50,"profile":"fresh generated, warm OS cache, same build mode",
        "page_p95_ms":percentile(&mut pages,0.95),"reader_p95_ms":percentile(&mut readers,0.95),"review_refresh_p95_ms":percentile(&mut changes,0.95),"first_id":first,
        "fts_contentless_dbstat_bytes":contentless_fts_bytes,"fts_full_dbstat_bytes":full_fts_bytes,
        "category_plan_before":category_plan_before,"category_plan_after":category_plan_after,"category_p95_ms_before":category_p95_before,"category_p95_ms_after":category_p95_after,"category_index_bytes":category_index_bytes,"category_insert_20k_ms_before":category_write_ms_before,"category_insert_20k_ms_after":category_write_ms_after});
    println!("{report}");
    if let Some(path) = std::env::var_os("FM_BROWSE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

/// Pages SQLite had to fetch from the file since the last call (cache misses).
fn cache_misses(store: &Store) -> i64 {
    let (mut current, mut highest) = (0, 0);
    // SAFETY: the handle belongs to this open connection; reset=1 clears it.
    unsafe {
        rusqlite::ffi::sqlite3_db_status(
            store.db.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_MISS,
            &mut current,
            &mut highest,
            1,
        );
    }
    i64::from(current)
}

/// What a running sync does to the store every second, and what the GUI's
/// once-a-second refresh then reads: accounts plus the open Inbox page.
#[test]
#[ignore = "Scoped generated sync-and-refresh measurement"]
fn sync_poll_48k() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("disposable-sync-benchmark");
    let store = Store::open(&profile).unwrap();
    let source = store.stage_blob(b"From: Synthetic <benchmark@fixture.example>\r\nSubject: Sync benchmark fixture\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSynthetic source.\r\n").unwrap();
    // Sized like a large personal mailbox: 48k messages, bodies averaging ~2.2k chars.
    let filler = "Quarterly planning notes, deployment detail and a reply thread. ".repeat(34);
    {
        let tx = store.db.unchecked_transaction().unwrap();
        let mut conversation = tx
            .prepare("INSERT OR IGNORE INTO conversations(account,id,subject,revision) VALUES(?1,?2,?3,1)")
            .unwrap();
        let mut message = tx.prepare("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,1,0,?12,1)").unwrap();
        let mut fts = tx
            .prepare("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)")
            .unwrap();
        for index in 0..48_000 {
            let account = if index % 2 == 0 {
                "windscribe"
            } else {
                "personal"
            };
            let conversation_id = format!("sync-{account}-thread-{:06}", index / 6);
            let sent_at = format!(
                "2026-{:02}-{:02}T{:02}:{:02}:00Z",
                1 + (index / 5000) % 9,
                1 + index % 28,
                index % 24,
                index % 60
            );
            let body = format!("{index} {filler}");
            conversation
                .execute(params![account, conversation_id, "Sync benchmark thread"])
                .unwrap();
            message
                .execute(params![
                    account,
                    format!("sync-{account}-message-{index:06}"),
                    conversation_id,
                    format!("sync-remote-{index:06}"),
                    "Alex Example",
                    "alex@example.com",
                    format!("you@{account}.example"),
                    "Sync benchmark thread",
                    body,
                    sent_at,
                    index % 7 == 0,
                    source
                ])
                .unwrap();
            fts.execute(params![
                tx.last_insert_rowid(),
                "Alex Example alex@example.com",
                "Sync benchmark thread",
                body
            ])
            .unwrap();
        }
        drop((fts, message, conversation));
        tx.commit().unwrap();
    }
    store
        .db
        .execute_batch("ANALYZE; PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO people(email,name) VALUES('p@example.com','P');")
        .unwrap();
    let mut store = store;
    if std::env::var_os("FM_BENCH_NO_MMAP").is_some() {
        // Page counts then include every page read, not only cache misses.
        store.db.execute_batch("PRAGMA mmap_size=0;").unwrap();
    }
    let (mut accounts_ms, mut list_ms) = (Vec::new(), Vec::new());
    let mut refresh = |store: &mut Store| {
        let started = Instant::now();
        store.handle("accounts", None, json!({})).unwrap();
        accounts_ms.push(started.elapsed());
        let started = Instant::now();
        store
            .handle(
                "conversations",
                Some("windscribe"),
                json!({"folder":"inbox","grouped":true,"limit":100}),
            )
            .unwrap();
        list_ms.push(started.elapsed());
    };
    refresh(&mut store);
    let first: i64 = store
        .db
        .query_row(
            "SELECT min(rowid) FROM messages WHERE id LIKE 'sync-%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (mut commits, mut refreshes, mut misses) = (Vec::new(), Vec::new(), Vec::new());
    for round in 0..40_i64 {
        // A sync part: flag/label updates on known messages, their text index
        // rows, and the people/link indexer's WITHOUT ROWID rows.
        let started = Instant::now();
        let tx = store.db.unchecked_transaction().unwrap();
        for step in 0..10_i64 {
            let id = first + (round * 997 + step * 4_801) % 48_000;
            tx.execute(
                "UPDATE messages SET unread=1-unread,revision=revision+1 WHERE rowid=?1",
                [id],
            )
            .unwrap();
            if step < 5 {
                tx.execute("DELETE FROM message_fts WHERE rowid=?1", [id])
                    .unwrap();
                tx.execute("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,'Alex','Sync benchmark thread','updated')", [id]).unwrap();
                tx.execute(
                    "INSERT OR IGNORE INTO message_people(message_rowid,person) VALUES(?1,1)",
                    [id],
                )
                .unwrap();
            }
        }
        tx.commit().unwrap();
        commits.push(started.elapsed());
        cache_misses(&store);
        let started = Instant::now();
        refresh(&mut store);
        refreshes.push(started.elapsed());
        misses.push(cache_misses(&store));
    }
    misses.sort_unstable();
    let report = json!({"generated_messages":48000,"rounds":40,"profile":"fresh generated, warm OS cache, same build mode",
        "commit_p50_ms":percentile(&mut commits.clone(),0.5),"commit_p95_ms":percentile(&mut commits,0.95),
        "refresh_p50_ms":percentile(&mut refreshes.clone(),0.5),"refresh_p95_ms":percentile(&mut refreshes,0.95),
        "refresh_pages_read_p50":misses[misses.len()/2],"refresh_pages_read_max":misses[misses.len()-1],
        "accounts_p50_ms":percentile(&mut accounts_ms.clone(),0.5),"list_page_p50_ms":percentile(&mut list_ms.clone(),0.5),
        "mmap":std::env::var_os("FM_BENCH_NO_MMAP").is_none()});
    println!("{report}");
    if let Some(path) = std::env::var_os("FM_SYNC_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
