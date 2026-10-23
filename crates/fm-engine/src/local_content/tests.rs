//! Engine worker ownership, cancellation and offline content integration.
use super::*;
use crate::Work;
use std::{sync::Mutex, time::Duration};
use tokio::sync::mpsc;

fn writer(
    profile: &std::path::Path,
) -> (CoreBridge, mpsc::Sender<Work>, std::thread::JoinHandle<()>) {
    let mut store = fm_core::Store::open(profile).unwrap();
    let (queue, mut receive) = mpsc::channel(16);
    let worker = std::thread::spawn(move || {
        while let Some(work) = receive.blocking_recv() {
            match work {
                Work::Provider(call) => call.run(&mut store),
                Work::Request(work) => {
                    assert!(!work.cancelled.load(std::sync::atomic::Ordering::Acquire));
                    let request = work.request;
                    let reply = match store.handle(
                        &request.method,
                        request.account.as_deref(),
                        request.params,
                    ) {
                        Ok(value) => fm_ipc::response(&request.id, value),
                        Err(error) => fm_ipc::error(&request.id, &error.code, &error.message),
                    };
                    let _ = work.reply.send(reply);
                }
                _ => panic!("this test uses actual CoreBridge calls"),
            }
        }
    });
    (
        CoreBridge::new(queue.clone(), Arc::new(Semaphore::new(16))),
        queue,
        worker,
    )
}

async fn new_draft(core: &CoreBridge) -> Value {
    core.call(|store| {
        store.handle(
            "draft_create",
            Some("windscribe"),
            json!({"mode":"new","request_id":"local-content-fixture-draft"}),
        )
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn received_attachment_extraction_leaves_writer_free_and_recreates_evicted_path() {
    use fm_google::sync::{prepare_full_page, ListPage, MessageContent, MessageRef, PageStore};
    let directory = tempfile::tempdir().unwrap();
    let (core, queue, writer) = writer(directory.path());
    drop(queue);
    let attachment = core
        .call(|store| {
            let cp = store.google_begin_full("windscribe", "100")?;
            fm_core::google_store::GooglePageStore::new(store)
                .stage_message(
                    &cp,
                    MessageContent {
                        id: "received-worker-file".into(),
                        thread_id: "received-worker-thread".into(),
                        label_ids: vec!["INBOX".into()],
                        history_id: "110".into(),
                        source_mime: include_bytes!(
                            "../../../../fixtures/messages/inline-and-attachment.eml"
                        )
                        .to_vec(),
                    },
                )
                .unwrap();
            let page = prepare_full_page(
                &cp,
                ListPage {
                    messages: vec![MessageRef {
                        id: "received-worker-file".into(),
                        thread_id: "received-worker-thread".into(),
                    }],
                    next_page_token: None,
                    result_size_estimate: Some(1),
                },
            )
            .unwrap();
            fm_core::google_store::GooglePageStore::new(store)
                .commit_page(&page)
                .unwrap();
            let rows = store.handle(
                "conversations",
                Some("windscribe"),
                json!({"folder":"all","query":"subject:Deployment"}),
            )?;
            let thread = store.handle(
                "conversation",
                Some("windscribe"),
                json!({"id":rows["conversations"][0]["id"]}),
            )?;
            Ok(thread["messages"][0]["attachments"][0]["id"]
                .as_str()
                .unwrap()
                .to_owned())
        })
        .await
        .unwrap();
    let (entered, mut entering) = mpsc::unbounded_channel();
    let (release, released) = std::sync::mpsc::channel();
    let released = Mutex::new(Some(released));
    let mut runtime = Runtime::new(core.clone());
    runtime.before_prepare = Some(Arc::new(move |_| {
        if let Some(receiver) = released.lock().unwrap().take() {
            entered.send(()).unwrap();
            receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Ok(())
    }));
    let runtime = Arc::new(runtime);
    let operation = runtime.clone();
    let params = json!({"id":attachment});
    let first_params = params.clone();
    let job = tokio::spawn(async move {
        operation
            .handle(
                CancellationToken::new(),
                "attachment",
                Some("windscribe"),
                first_params,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entering.recv())
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        core.call(|store| store.handle("accounts", None, json!({}))),
    )
    .await
    .unwrap()
    .unwrap();
    release.send(()).unwrap();
    let first = job.await.unwrap().unwrap();
    let bytes = std::fs::read(first["path"].as_str().unwrap()).unwrap();
    assert!(!bytes.is_empty());
    assert!(!directory
        .path()
        .join("blobs")
        .join(first["sha256"].as_str().unwrap())
        .exists());
    std::fs::remove_file(first["path"].as_str().unwrap()).unwrap();
    let second = runtime
        .handle(
            CancellationToken::new(),
            "attachment",
            Some("windscribe"),
            params,
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(second["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    drop(runtime);
    drop(core);
    tokio::task::spawn_blocking(move || writer.join().unwrap())
        .await
        .unwrap();
}

async fn draft(core: &CoreBridge, id: String) -> Value {
    core.call(move |store| store.handle("draft_get", Some("windscribe"), json!({"draft_id":id})))
        .await
        .unwrap()
}

#[tokio::test]
async fn actual_import_preparation_leaves_writer_free_and_cancellation_preserves_draft() {
    for cancelled in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("profile");
        let path = directory.path().join("generated-attachment.bin");
        let bytes = b"Generated local attachment\0\x01\xff\n";
        std::fs::write(&path, bytes).unwrap();
        let (core, queue, writer) = writer(&profile);
        drop(queue);
        let original = new_draft(&core).await;
        let draft_id = original["draft_id"].as_str().unwrap().to_owned();
        let (entered, mut entering) = mpsc::unbounded_channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Mutex::new(Some(released));
        let mut runtime = Runtime::new(core.clone());
        runtime.before_prepare = Some(Arc::new(move |_| {
            entered.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(())
        }));
        let runtime = Arc::new(runtime);
        let owner = CancellationToken::new();
        let operation = runtime.clone();
        let operation_owner = owner.clone();
        let params = json!({"draft_id":draft_id,"expected_revision":original["revision"],
            "request_id":"local-content-import","path":path});
        let job = tokio::spawn(async move {
            operation
                .handle(
                    operation_owner,
                    "draft_attachment_import",
                    Some("windscribe"),
                    params,
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(3), entering.recv())
            .await
            .unwrap()
            .unwrap();
        if cancelled {
            owner.cancel();
        }
        // The production handle path has completed its SQL-only preflight and
        // is held on its blocking worker. Independent cached queries still run.
        let current = tokio::time::timeout(Duration::from_secs(1), draft(&core, draft_id.clone()))
            .await
            .expect("held content preparation occupied the Store writer");
        assert_eq!(current, original);
        assert!(!job.is_finished());
        assert_eq!(runtime.preparation.available_permits(), 1);
        release.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(3), job)
            .await
            .unwrap()
            .unwrap();
        let saved = draft(&core, draft_id).await;
        if cancelled {
            assert_eq!(result.unwrap_err().code, "cancelled");
            assert_eq!(saved, original);
        } else {
            let result = result.unwrap();
            assert_eq!(result["draft"], saved);
            assert_eq!(
                saved["revision"].as_i64(),
                original["revision"].as_i64().map(|v| v + 1)
            );
            assert_eq!(saved["attachment_ids"].as_array().unwrap().len(), 1);
            assert_eq!(result["attachment"]["message_id"], Value::Null);
            let hash = result["attachment"]["sha256"].as_str().unwrap();
            assert_eq!(
                std::fs::read(profile.join("blobs").join(hash)).unwrap(),
                bytes
            );
        }
        assert_eq!(runtime.preparation.available_permits(), 2);
        drop(runtime);
        drop(core);
        tokio::task::spawn_blocking(move || writer.join().unwrap())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn cancelled_blocking_workers_keep_both_slots_until_they_really_complete() {
    let (queue, _receive) = mpsc::channel(1);
    let runtime = Arc::new(Runtime::new(CoreBridge::new(
        queue,
        Arc::new(Semaphore::new(1)),
    )));
    let owner = CancellationToken::new();
    let (entered, mut entering) = mpsc::unbounded_channel();
    let (release, released) = std::sync::mpsc::channel();
    let released = Arc::new(Mutex::new(released));
    let mut workers = Vec::new();
    for index in 0..2 {
        let worker = runtime.clone();
        let cancel = owner.clone();
        let entered = entered.clone();
        let released = released.clone();
        workers.push(tokio::spawn(async move {
            worker
                .prepare(cancel, move |cancel| {
                    entered.send(index).unwrap();
                    released
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    assert!(cancel.is_cancelled());
                    Ok(index)
                })
                .await
        }));
    }
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(3), entering.recv())
            .await
            .unwrap()
            .unwrap();
    }
    owner.cancel();
    assert_eq!(runtime.preparation.available_permits(), 0);
    assert!(workers.iter().all(|worker| !worker.is_finished()));
    let waiting_runtime = runtime.clone();
    let waiting = tokio::spawn(async move {
        waiting_runtime
            .prepare(CancellationToken::new(), move |_| {
                entered.send(2).unwrap();
                Ok(2)
            })
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), entering.recv())
            .await
            .is_err()
    );
    release.send(()).unwrap();
    release.send(()).unwrap();
    for worker in workers {
        worker.await.unwrap().unwrap();
    }
    assert_eq!(waiting.await.unwrap().unwrap(), 2);
    assert_eq!(runtime.preparation.available_permits(), 2);
}

async fn exchange(
    client: &mut tokio::io::BufReader<tokio::io::DuplexStream>,
    request: Value,
) -> Value {
    fm_ipc::write_frame(client.get_mut(), &request)
        .await
        .unwrap();
    serde_json::from_slice(
        &tokio::time::timeout(Duration::from_secs(3), fm_ipc::read_frame(client))
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn authenticated_import_sessions_drain_preparation_on_client_and_engine_close() {
    async fn query_released(foreground: &Semaphore) {
        // The held import owns one seat. A query reply can arrive before its
        // writer work item drops the other seat; acquire that seat to observe
        // the actual release before asserting the import's retained ownership.
        let permit = tokio::time::timeout(Duration::from_secs(3), foreground.acquire())
            .await
            .expect("completed query did not release its foreground permit")
            .unwrap();
        drop(permit);
    }

    for engine_close in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("profile");
        let path = directory.path().join("session-attachment.bin");
        std::fs::write(&path, b"owned synthetic session content").unwrap();
        let (core, queue, writer) = writer(&profile);
        let original = new_draft(&core).await;
        let draft_id = original["draft_id"].as_str().unwrap().to_owned();
        let parameters = json!({"draft_id":draft_id,"expected_revision":original["revision"],
            "request_id":"session-content-import","path":path});
        let (entered, mut entering) = mpsc::unbounded_channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Mutex::new(Some(released));
        let mut runtime = Runtime::new(core.clone());
        runtime.before_prepare = Some(Arc::new(move |cancel| {
            entered.send(cancel.clone()).unwrap();
            released
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(())
        }));
        let runtime = Arc::new(runtime);
        let services = crate::SessionServices {
            google: None,
            content: runtime.clone(),
            assistance: Arc::new(crate::local_model::Runtime::new(CoreBridge::new(
                queue.clone(),
                Arc::new(Semaphore::new(16)),
            ))),
            remote: Arc::new(crate::remote_content::Runtime::new(
                CoreBridge::new(queue.clone(), Arc::new(Semaphore::new(4))),
                &std::env::temp_dir(),
                None,
            )),
            keyring: Arc::new(crate::pgp::Runtime::new(
                CoreBridge::new(queue.clone(), Arc::new(Semaphore::new(4))),
                Arc::new(fm_core::pgp::MemoryKek::new()),
            )),
        };
        let foreground = Arc::new(Semaphore::new(2));
        let engine = CancellationToken::new();
        let mut clients = Vec::new();
        let mut sessions = Vec::new();
        for _ in 0..2 {
            let (client, server) = tokio::io::duplex(fm_ipc::MAX_FRAME * 2);
            let session = tokio::spawn(crate::session(
                Box::new(server),
                Arc::new("content-session-token".into()),
                queue.clone(),
                Arc::new(json!({"protocol":1})),
                foreground.clone(),
                services.clone(),
                engine.clone(),
            ));
            let mut client = tokio::io::BufReader::new(client);
            assert_eq!(
                exchange(
                    &mut client,
                    json!({"version":1,"id":"hello","method":"hello",
                "params":{"token":"content-session-token"}})
                )
                .await["ok"],
                true
            );
            clients.push(client);
            sessions.push(session);
        }
        let mut querying = clients.pop().unwrap();
        let mut importing = clients.pop().unwrap();
        let querying_session = sessions.pop().unwrap();
        let importing_session = sessions.pop().unwrap();
        fm_ipc::write_frame(
            importing.get_mut(),
            &json!({"version":1,"id":"import",
            "method":"draft_attachment_import","account":"windscribe","params":parameters}),
        )
        .await
        .unwrap();
        let worker_owner = tokio::time::timeout(Duration::from_secs(3), entering.recv())
            .await
            .unwrap()
            .unwrap();
        let query = json!({"version":1,"id":"query","method":"draft_get","account":"windscribe",
            "params":{"draft_id":draft_id}});
        let reply = exchange(&mut querying, query.clone()).await;
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["result"], original);
        query_released(&foreground).await;
        let mut importing = Some(importing);
        if engine_close {
            engine.cancel();
        } else {
            drop(importing.take());
        }
        tokio::time::timeout(Duration::from_secs(1), worker_owner.cancelled())
            .await
            .unwrap();
        assert!(
            !importing_session.is_finished(),
            "session abandoned its running preparation"
        );
        assert_eq!(foreground.available_permits(), 1);
        assert_eq!(runtime.preparation.available_permits(), 1);
        if !engine_close {
            // Closing this owner cannot cancel another authenticated client's cached query.
            assert_eq!(exchange(&mut querying, query).await["result"], original);
            query_released(&foreground).await;
        }
        release.send(()).unwrap();
        // Cancellation may make the final socket write return Interrupted; all owned work must drain.
        let _ = tokio::time::timeout(Duration::from_secs(3), importing_session)
            .await
            .unwrap()
            .unwrap();
        drop(importing);
        assert_eq!(foreground.available_permits(), 2);
        assert_eq!(runtime.preparation.available_permits(), 2);
        assert_eq!(draft(&core, draft_id).await, original);
        core.call(move |store| {
            assert!(
                matches!(
                    store.local_content_begin(
                        "draft_attachment_import",
                        "windscribe",
                        parameters
                    )?,
                    LocalContentStep::Prepare(_)
                ),
                "cancelled preparation wrote an import receipt"
            );
            Ok(())
        })
        .await
        .unwrap();
        drop(querying);
        let _ = tokio::time::timeout(Duration::from_secs(3), querying_session)
            .await
            .unwrap()
            .unwrap();
        drop(services);
        drop(runtime);
        drop(core);
        drop(queue);
        tokio::task::spawn_blocking(move || writer.join().unwrap())
            .await
            .unwrap();
    }
}
