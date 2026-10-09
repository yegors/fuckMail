//! Whole-conversation summary jobs through `assist_start`, against a fake
//! loopback model server and the synthetic store.
use super::*;
use crate::Work;
use fm_assist::ollama::{Model, Provider};
use tokio::sync::mpsc;

const A: &str = "windscribe";
/// Three messages; the second is the person's own reply.
const C: &str = "windscribe-thread-000";

/// A store-writer thread on a synthetic profile. One profile serves every
/// case: opening a synthetic store is heavy enough to slow the timed tests
/// running beside these.
async fn runtime(profile: &Path) -> Arc<Runtime> {
    let mut store = fm_core::Store::open_synthetic(profile).unwrap();
    let (queue, mut receive) = mpsc::channel(16);
    std::thread::spawn(move || {
        while let Some(work) = receive.blocking_recv() {
            match work {
                Work::Provider(call) => call.run(&mut store),
                _ => panic!("this test uses CoreBridge calls only"),
            }
        }
    });
    Arc::new(Runtime::new(CoreBridge::new(
        queue,
        Arc::new(Semaphore::new(16)),
    )))
}

/// Points the saved settings at an OpenAI-compatible server at `endpoint`.
async fn point(runtime: &Arc<Runtime>, endpoint: &str) {
    let settings = Settings {
        provider: Provider::OpenaiCompatible,
        endpoint: format!("{endpoint}/v1"),
        model: Some(Model {
            name: "local-model".into(),
            digest: String::new(),
        }),
        ..Settings::default()
    };
    runtime
        .core
        .call(move |s| {
            let revision = s.local_model_settings()?["revision"].clone();
            s.local_model_handle(
                "assist_settings_save",
                None,
                &json!({"expected_revision":revision,"settings":settings}),
            )
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn thread_jobs_check_their_request_run_fail_and_cancel() {
    let profile = tempfile::tempdir().unwrap();
    let runtime = runtime(profile.path()).await;
    thread_jobs_take_a_conversation_and_no_message(&runtime).await;
    a_failed_thread_job_reports_its_error_without_recording_a_failure(&runtime).await;
    a_thread_job_can_be_canceled_while_the_model_runs(&runtime).await;
    a_thread_job_summarizes_the_whole_conversation_and_saves_it(&runtime).await;
}

async fn start(runtime: &Arc<Runtime>, cancel: &CancellationToken, p: Value) -> Result<Value> {
    runtime
        .handle("owner", cancel.clone(), "assist_start", Some(A), p)
        .await
}

/// Polls `assist_job` until the job leaves queued and running.
async fn finished(runtime: &Arc<Runtime>, cancel: &CancellationToken, job: &Value) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = runtime
            .handle(
                "owner",
                cancel.clone(),
                "assist_job",
                None,
                json!({"job_id":job}),
            )
            .await
            .unwrap();
        if !matches!(state["status"].as_str(), Some("queued" | "running")) {
            return state;
        }
        assert!(Instant::now() < deadline, "the job never finished");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn results(runtime: &Arc<Runtime>) -> Value {
    runtime
        .core
        .call(|s| s.local_model_results(A, C))
        .await
        .unwrap()
}

async fn thread_jobs_take_a_conversation_and_no_message(runtime: &Arc<Runtime>) {
    point(runtime, "http://127.0.0.1:9").await;
    let cancel = CancellationToken::new();
    for (p, code, message) in [
        (
            json!({"task":"thread","conversation_id":C,"message_id":"windscribe-message-000-000"}),
            "invalid_request",
            "A thread summary covers the whole conversation; omit message_id",
        ),
        (
            json!({"task":"thread"}),
            "invalid_request",
            "Choose a conversation to summarize",
        ),
        (
            json!({"task":"thread","conversation_id":"windscribe-thread-001"}),
            "invalid_request",
            "A thread summary needs at least two messages",
        ),
        (
            json!({"task":"thread","conversation_id":"missing"}),
            "not_found",
            "Conversation not found in this account",
        ),
        (
            json!({"task":"conversation","conversation_id":C}),
            "invalid_request",
            "Unknown local model task",
        ),
    ] {
        let error = start(runtime, &cancel, p.clone()).await.unwrap_err();
        assert_eq!(
            (error.code.as_str(), error.message.as_str()),
            (code, message),
            "{p}"
        );
    }
    let error = runtime
        .handle(
            "owner",
            cancel.clone(),
            "assist_start",
            None,
            json!({"task":"thread","conversation_id":C}),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "account_required");
}

async fn a_thread_job_summarizes_the_whole_conversation_and_saves_it(runtime: &Arc<Runtime>) {
    let summary = "Maya asked you to choose between renewing Frankfurt or moving 40% of the pool. You owe Maya a reply before the 14:00 review.";
    let reply = json!({"model":"local-model","usage":{"completion_tokens":30},"choices":[{"index":0,"finish_reason":"stop",
        "message":{"role":"assistant","content":json!({"summary":summary}).to_string(),"tool_calls":null}}]});
    let models = json!({"data":[{"id":"local-model"}]});
    let (endpoint, server) = crate::openai_local::tests::server(vec![
        (200, models.to_string()),
        (200, reply.to_string()),
    ])
    .await;
    point(runtime, &endpoint).await;
    let cancel = CancellationToken::new();
    let started = start(
        runtime,
        &cancel,
        json!({"task":"thread","conversation_id":C}),
    )
    .await
    .unwrap();
    assert_eq!(started["status"], "queued");
    let state = finished(runtime, &cancel, &started["job_id"]).await;
    assert_eq!(state["status"], "completed", "{state}");
    assert_eq!(
        (&state["completed"], &state["total"]),
        (&json!(1), &json!(1))
    );
    let saved = &state["items"][0];
    assert_eq!(saved["task"], "thread");
    assert_eq!(saved["target"], C);
    assert_eq!(saved["items"][0]["output"]["summary"], summary);
    // The reader's results carry the summary beside the message results.
    let results = results(runtime).await;
    let item = results["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["task"] == "thread")
        .unwrap();
    assert_eq!(item["output"]["summary"], summary);
    assert_eq!(item["stale"], false);
    assert_eq!(item["partial"], false);
    assert_eq!(item["source_ids"].as_array().unwrap().len(), 3);
    assert!(results["failures"].as_array().unwrap().is_empty());
    // One model check, then one request with the thread prompts and schema.
    let requests = server.await.unwrap();
    assert_eq!(
        requests.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        [
            "GET /v1/models HTTP/1.1",
            "POST /v1/chat/completions HTTP/1.1"
        ]
    );
    let body = &requests[1].1;
    assert_eq!(
        body["messages"][0]["content"],
        fm_assist::ollama::THREAD_SYSTEM
    );
    assert_eq!(
        body["response_format"]["json_schema"]["schema"],
        fm_assist::ollama::thread_schema()
    );
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(user.contains("You <you@windscribe.example>"));
    assert!(user.contains("Need your call before the 14:00 review."));
}

async fn a_failed_thread_job_reports_its_error_without_recording_a_failure(runtime: &Arc<Runtime>) {
    let reply = json!({"model":"local-model","choices":[{"index":0,"finish_reason":"stop",
        "message":{"role":"assistant","content":"{\"summary\":\"- a list\"}"}}]});
    let models = json!({"data":[{"id":"local-model"}]});
    let (endpoint, _server) = crate::openai_local::tests::server(vec![
        (200, models.to_string()),
        (200, reply.to_string()),
    ])
    .await;
    point(runtime, &endpoint).await;
    let cancel = CancellationToken::new();
    let started = start(
        runtime,
        &cancel,
        json!({"task":"thread","conversation_id":C}),
    )
    .await
    .unwrap();
    let state = finished(runtime, &cancel, &started["job_id"]).await;
    assert_eq!(state["status"], "failed");
    assert_eq!(state["error"]["code"], "invalid_assistance_result");
    assert_eq!(state["items"], json!([]));
    let results = results(runtime).await;
    assert!(results["failures"].as_array().unwrap().is_empty());
    assert!(results["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["task"] != "thread"));
}

async fn a_thread_job_can_be_canceled_while_the_model_runs(runtime: &Arc<Runtime>) {
    // A server that accepts the connection and never answers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (accepted, mut accepting) = mpsc::unbounded_channel();
    let hold = tokio::spawn(async move {
        let mut sockets = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            sockets.push(socket);
            let _ = accepted.send(());
        }
    });
    point(runtime, &endpoint).await;
    let cancel = CancellationToken::new();
    let started = start(
        runtime,
        &cancel,
        json!({"task":"thread","conversation_id":C}),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), accepting.recv())
        .await
        .unwrap();
    let canceled = runtime
        .handle(
            "owner",
            cancel.clone(),
            "assist_cancel",
            None,
            json!({"job_id":started["job_id"]}),
        )
        .await
        .unwrap();
    assert_eq!(canceled["status"], "canceled");
    let state = finished(runtime, &cancel, &started["job_id"]).await;
    assert_eq!(state["status"], "canceled");
    assert!(results(runtime).await["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["task"] != "thread"));
    hold.abort();
}
