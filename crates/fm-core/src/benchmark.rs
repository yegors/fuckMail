//! Opt-in 100k synthetic search experiment; never touches a real profile.
use crate::Store;
use rusqlite::params;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn percentile(samples: &mut [Duration], quantile: f64) -> f64 {
    samples.sort_unstable();
    let index = ((samples.len() - 1) as f64 * quantile).ceil() as usize;
    samples[index].as_secs_f64() * 1000.0
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
            "phase1 grouped first50",
            json!({"grouped":true,"folder":"all","limit":50}),
        ),
        (
            "phase1 combined filters first50",
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
    let report = json!({"generated_messages":20000,"observations":50,"profile":"fresh generated, warm OS cache, same build mode",
        "page_p95_ms":percentile(&mut pages,0.95),"reader_p95_ms":percentile(&mut readers,0.95),"review_refresh_p95_ms":percentile(&mut changes,0.95),"first_id":first});
    println!("{report}");
    if let Some(path) = std::env::var_os("FM_BROWSE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
