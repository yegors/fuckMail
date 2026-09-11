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
        for _ in 0..40 {
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
    let plan_sql = format!(
        "EXPLAIN QUERY PLAN {}",
        crate::queries::search_page_sql(true)
    );
    let mut statement = store.db.prepare(&plan_sql).unwrap();
    let plan = statement
        .query_map(
            params!["windscribe", "all", "", "body:(\"ultraquartz\"*)", 21, 0],
            |r| r.get::<_, String>(3),
        )
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let report: Value = json!({"experiment":"100k generated SQLite/FTS5 message rows through Store.handle",
        "fixture_revision":crate::FIXTURE_REVISION,"generated_messages":100000,"total_messages_including_small_fixture":message_count,
        "sqlite_version":rusqlite::version(),"target_os":std::env::consts::OS,"target_arch":std::env::consts::ARCH,
        "seed_seconds":seed_seconds,"database_bytes":database_bytes,"cache_context":"One reopened SQLite connection after bulk seed/checkpoint; OS filesystem cache was not cleared. Per-query first sample followed by 40 warm samples. No ingestion or inference during measurement.",
        "coverage_limits":"Index density fixture with shared retained synthetic source; not 25 GB mailbox, attachment-throughput, 1m/3m, OS-cold, concurrent-ingestion or power-loss evidence.","query_plan":plan,"queries":measurements});
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
