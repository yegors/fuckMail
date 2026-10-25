use fm_ipc::{Client, Discovery};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader as StdBufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::io::{AsyncWriteExt, BufReader};

fn engine() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fm-engine"))
}
async fn client(profile: &Path) -> Client {
    Client::connect_demo(profile, Some(&engine()))
        .await
        .expect("start/connect actual engine")
}
fn ok(value: Value) -> Value {
    assert_eq!(value["ok"], true, "{value}");
    value["result"].clone()
}
async fn first(client: &mut Client, account: &str) -> Value {
    let list = ok(client
        .call("conversations", Some(account), json!({"limit":1}))
        .await
        .unwrap());
    ok(client
        .call(
            "conversation",
            Some(account),
            json!({"id":list["conversations"][0]["id"]}),
        )
        .await
        .unwrap())
}
async fn wait_stopped(profile: &Path) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while profile.join("engine.json").exists() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "engine kept running without an owner"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn older_profile_is_refused_at_once_with_its_reason() {
    let root = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&root.path().join("old-profile")).unwrap();
    rusqlite::Connection::open(profile.join("store.sqlite3"))
        .unwrap()
        .pragma_update(None, "user_version", 29)
        .unwrap();
    let started = std::time::Instant::now();
    let error = match Client::connect(&profile, Some(&engine())).await {
        Ok(_) => panic!("an older store must not open"),
        Err(error) => error,
    };
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "refusal waited {:?} instead of stopping",
        started.elapsed()
    );
    assert!(error.to_string().contains("older fuckMail"), "{error}");
    let refusal: Value =
        serde_json::from_slice(&std::fs::read(profile.join(fm_ipc::STARTUP_REFUSAL_FILE)).unwrap())
            .unwrap();
    assert_eq!(refusal["code"], "schema_mismatch");
    assert!(!profile.join("engine.json").exists());
    // The refusal is neither retained nor trusted once the store is replaced.
    std::fs::remove_file(profile.join("store.sqlite3")).unwrap();
    let mut client = Client::connect(&profile, Some(&engine())).await.unwrap();
    ok(client.call("status", None, json!({})).await.unwrap());
    assert!(!profile.join(fm_ipc::STARTUP_REFUSAL_FILE).exists());
}

#[tokio::test]
async fn live_profile_stays_empty_through_shared_clients_and_owner_restart() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("live-profile");
    let mut owner = Client::connect(&profile, Some(&engine())).await.unwrap();
    let status = ok(owner.call("status", None, json!({})).await.unwrap());
    assert_eq!(status["profile_kind"], "live");
    assert_eq!(status["fixture_revision"], 0);
    assert!(Client::connect_demo(&profile, Some(&engine()))
        .await
        .is_err());
    // This advertises the live interface. The empty account list below still
    // proves initialization grants no account a submission capability.
    assert_eq!(status["capabilities"]["send"], true);
    let mut observer = Client::connect(&profile, Some(&engine())).await.unwrap();
    assert_eq!(
        ok(observer.call("accounts", None, json!({})).await.unwrap())["accounts"],
        json!([])
    );
    drop(owner);
    assert_eq!(
        ok(observer.call("status", None, json!({})).await.unwrap())["synthetic"],
        false
    );
    drop(observer);
    wait_stopped(&profile).await;
    let mut reopened = Client::connect(&profile, Some(&engine())).await.unwrap();
    assert_eq!(
        ok(reopened.call("accounts", None, json!({})).await.unwrap())["accounts"],
        json!([])
    );
    assert_eq!(
        ok(reopened.call("status", None, json!({})).await.unwrap())["fixture_revision"],
        0
    );
    drop(reopened);
    wait_stopped(&profile).await;
    assert!(Client::connect_demo(&profile, Some(&engine()))
        .await
        .is_err());
}

#[tokio::test]
async fn live_initialization_refuses_a_running_synthetic_profile_without_conversion() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("demo");
    let mut owner = client(&profile).await;
    let before = ok(owner.call("accounts", None, json!({})).await.unwrap());
    let error = match Client::connect_live(&profile, Some(&engine())).await {
        Ok(_) => panic!("demo accepted as live"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        ok(owner.call("accounts", None, json!({})).await.unwrap()),
        before
    );
    drop(owner);
    wait_stopped(&profile).await;
    assert!(Client::connect_live(&profile, Some(&engine()))
        .await
        .is_err());
    let mut reopened = client(&profile).await;
    assert_eq!(
        ok(reopened.call("accounts", None, json!({})).await.unwrap()),
        before
    );
    drop(reopened);
    wait_stopped(&profile).await;
}

// Own the exact process handle used by kill tests; never kill a PID merely
// because a profile/discovery file names it, and clean up on assertion failure.
struct OwnedEngine(Child);
impl Drop for OwnedEngine {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn owned_engine(profile: &Path) -> OwnedEngine {
    let mut process = OwnedEngine(
        Command::new(engine())
            .arg("--demo-profile")
            .arg("--profile")
            .arg(profile)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = tokio::time::Instant::now() + fm_ipc::STARTUP_TIMEOUT;
    loop {
        if fm_ipc::read_discovery(profile).is_ok_and(|d| d.pid == process.0.id()) {
            break;
        }
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "owned engine exited during startup"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "owned engine startup timed out"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    process
}
async fn raw_owner(d: &Discovery) -> BufReader<fm_ipc::AnyStream> {
    let mut stream = BufReader::new(fm_ipc::connect(d).await.unwrap());
    fm_ipc::write_frame(
        stream.get_mut(),
        &json!({"version":1,"id":"hello","method":"hello","params":{"token":d.token}}),
    )
    .await
    .unwrap();
    let hello: Value =
        serde_json::from_slice(&fm_ipc::read_frame(&mut stream).await.unwrap().unwrap()).unwrap();
    assert_eq!(hello["ok"], true);
    stream
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn killed_engine_recovers_an_unread_draft_receipt_without_repeating_it() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let mut process = owned_engine(&profile).await;
    let mut observer = client(&profile).await;
    let discovery = fm_ipc::read_discovery(&profile).unwrap();
    let mut sender = raw_owner(&discovery).await;
    let params = json!({"conversation_id":"windscribe-thread-000", "body":"Human reply after restart — 日本語", "expected_revision":0, "request_id":"lost-draft-receipt"});
    fm_ipc::write_frame(sender.get_mut(), &json!({"version":1,"id":"lost","method":"draft_save","account":"windscribe","params":params})).await.unwrap();
    // Do not read the sender's reply. An independent owner observes the durable
    // effect before the OS terminates the process, so replay must find its receipt.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let saved = loop {
        let draft = ok(observer
            .call(
                "draft_get",
                Some("windscribe"),
                json!({"conversation_id":"windscribe-thread-000"}),
            )
            .await
            .unwrap());
        if draft["revision"] == 1 {
            break draft;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "draft did not commit"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    process.0.kill().unwrap();
    assert!(!process.0.wait().unwrap().success());
    drop(sender);
    drop(observer);
    let mut reopened = client(&profile).await;
    assert_eq!(
        ok(reopened
            .call("draft_save", Some("windscribe"), params.clone())
            .await
            .unwrap()),
        saved
    );
    let newer = ok(reopened.call("draft_save", Some("windscribe"), json!({"conversation_id":"windscribe-thread-000", "body":"Later author edit", "expected_revision":1, "request_id":"later-edit"})).await.unwrap());
    assert_eq!(newer["revision"], 2);
    assert_eq!(
        ok(reopened
            .call("draft_save", Some("windscribe"), params)
            .await
            .unwrap()),
        saved
    );
    assert_eq!(
        ok(reopened
            .call(
                "draft_get",
                Some("windscribe"),
                json!({"conversation_id":"windscribe-thread-000"})
            )
            .await
            .unwrap()),
        newer
    );
    drop(reopened);
    wait_stopped(&profile).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_frame_owner_does_not_stall_another_client_or_keep_engine_after_disconnect() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let mut other = client(&profile).await;
    let discovery = fm_ipc::read_discovery(&profile).unwrap();
    let mut slow = raw_owner(&discovery).await;
    slow.get_mut().write_all(b"{\"version\":").await.unwrap();
    let status = tokio::time::timeout(
        Duration::from_secs(3),
        other.call("status", None, json!({})),
    )
    .await
    .unwrap()
    .unwrap();
    ok(status);
    drop(other);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(fm_ipc::read_discovery(&profile).unwrap().pid, discovery.pid);
    drop(slow);
    wait_stopped(&profile).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paired_clients_share_durable_state_and_explicit_lifetimes() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let mut gui = client(&profile).await;
    let discovery = fm_ipc::read_discovery(&profile).unwrap();
    let mut cli = client(&profile).await;
    assert_eq!(fm_ipc::read_discovery(&profile).unwrap().pid, discovery.pid);
    let accounts = ok(cli.call("accounts", None, json!({})).await.unwrap());
    let mut account_ids: Vec<_> = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|account| account["id"].as_str().unwrap())
        .collect();
    account_ids.sort_unstable();
    assert_eq!(account_ids, ["personal", "sideproject", "windscribe"]);
    let original = first(&mut gui, "windscribe").await;
    let id = original["id"].as_str().unwrap();
    let params = json!({"id":id,"message_ids":original["message_ids"],"expected_revision":original["revision"],"request_id":"test-flag-once","starred":true});
    let changed = ok(cli
        .call("flag", Some("windscribe"), params.clone())
        .await
        .unwrap());
    let seen = ok(gui
        .call("conversation", Some("windscribe"), json!({"id":id}))
        .await
        .unwrap());
    let mut reader_shape = changed.clone();
    reader_shape.as_object_mut().unwrap().remove("mail_undo");
    assert_eq!(seen, reader_shape);
    assert_eq!(
        seen["messages"][0]["unread"], original["messages"][0]["unread"],
        "star must not review mail"
    );
    assert_eq!(
        ok(cli.call("flag", Some("windscribe"), params).await.unwrap()),
        changed,
        "request retry must reuse recorded outcome"
    );
    let cross = cli
        .call("conversation", Some("personal"), json!({"id":id}))
        .await
        .unwrap();
    assert_eq!(
        cross["ok"], false,
        "cross-account local identity must not resolve"
    );
    let draft = ok(gui
        .call(
            "draft_get",
            Some("windscribe"),
            json!({"conversation_id":id}),
        )
        .await
        .unwrap());
    let text = "A human reply.\nΚαλημέρα — こんにちは\nNo automated text generation.";
    ok(gui.call("draft_save",Some("windscribe"),json!({"conversation_id":id,"expected_revision":draft["revision"],"request_id":"draft-once","body":text})).await.unwrap());
    drop(gui);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        fm_ipc::read_discovery(&profile).unwrap().pid,
        discovery.pid,
        "explicit CLI owns the surviving engine"
    );
    assert_eq!(
        ok(cli
            .call(
                "draft_get",
                Some("windscribe"),
                json!({"conversation_id":id})
            )
            .await
            .unwrap())["body"],
        text
    );
    drop(cli);
    wait_stopped(&profile).await;
    let mut reopened = client(&profile).await;
    assert_eq!(
        ok(reopened
            .call(
                "draft_get",
                Some("windscribe"),
                json!({"conversation_id":id})
            )
            .await
            .unwrap())["body"],
        text
    );
    drop(reopened);
    wait_stopped(&profile).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_recovers_when_a_previous_writer_releases_its_lock() {
    for live in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let profile = fm_ipc::private_profile(&temp.path().join("profile")).unwrap();
        // Model the real shutdown interval after discovery disappears but before
        // SQLite closes and releases the previous engine's exclusive writer lock.
        let previous_writer = if live {
            fm_core::Store::open_live(&profile).unwrap()
        } else {
            fm_core::Store::open(&profile).unwrap()
        };
        let reconnect_profile = profile.clone();
        let reconnect = tokio::spawn(async move {
            if live {
                Client::connect_live(&reconnect_profile, Some(&engine()))
                    .await
                    .unwrap()
            } else {
                client(&reconnect_profile).await
            }
        });
        tokio::time::sleep(Duration::from_secs(2)).await;
        drop(previous_writer);
        let mut next = reconnect.await.unwrap();
        assert_eq!(
            ok(next.call("status", None, json!({})).await.unwrap())["synthetic"],
            !live
        );
        drop(next);
        wait_stopped(&profile).await;
    }
}

// CTest supplies the built native GUI; plain cargo runs have no Qt build.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires the actual native GUI; invoked by CTest gui-startup-recovery"]
async fn gui_startup_recovers_after_writer_closes() {
    let executable = std::env::var_os("FM_GUI_TEST_EXECUTABLE")
        .expect("CTest must provide the built native GUI executable");
    let temp = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&temp.path().join("profile")).unwrap();
    let previous_writer = fm_core::Store::open(&profile).unwrap();
    let mut gui = OwnedEngine(
        Command::new(executable)
            .args(["--smoke", "--engine"])
            .arg(engine())
            .arg("--profile")
            .arg(&profile)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("QT_QUICK_BACKEND", "software")
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env("FM_GUI_TEST_TRACE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stderr = gui.0.stderr.take().unwrap();
    let (retry_seen, mut retries) = tokio::sync::mpsc::unbounded_channel();
    let log_reader = std::thread::spawn(move || {
        for line in StdBufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("{line}");
            if line.contains("GUI engine launch attempt 2") {
                let _ = retry_seen.send(());
            }
        }
    });
    // Observe a real second launch while the lock remains held. A slow Windows
    // Qt load cannot pass by starting only after a fixed sleep has expired.
    tokio::time::timeout(Duration::from_secs(35), retries.recv())
        .await
        .expect("GUI never retried unavailable startup while the writer lock was held")
        .expect("GUI exited before retrying startup");
    drop(previous_writer);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(35);
    let status = loop {
        if let Some(status) = gui.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "GUI did not recover after releasing the writer lock"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    log_reader.join().unwrap();
    assert!(
        status.success(),
        "GUI failed after unavailable startup: {status}"
    );
    wait_stopped(&profile).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_launches_elect_one_writer() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let (mut a, mut b, mut c) = tokio::join!(client(&profile), client(&profile), client(&profile));
    // Refresh generations keep moving while startup work finishes, so the
    // clients compare the store's epoch rather than those counters.
    let status = |mut value: Value| {
        let epoch = value["ui_generations"]["epoch"].take();
        assert!(epoch.is_string());
        value["ui_generations"] = json!({ "epoch": epoch });
        value
    };
    let astatus = status(ok(a.call("status", None, json!({})).await.unwrap()));
    let bstatus = status(ok(b.call("status", None, json!({})).await.unwrap()));
    let cstatus = status(ok(c.call("status", None, json!({})).await.unwrap()));
    assert_eq!(astatus, bstatus);
    assert_eq!(bstatus, cstatus);
    drop(a);
    drop(b);
    tokio::time::sleep(Duration::from_millis(500)).await;
    ok(c.call("status", None, json!({})).await.unwrap());
    drop(c);
    wait_stopped(&profile).await;
}

async fn handshake(d: &Discovery, token: &str, version: u32) -> Value {
    let mut stream = BufReader::new(fm_ipc::connect(d).await.unwrap());
    fm_ipc::write_frame(
        stream.get_mut(),
        &json!({"version":version,"id":"probe","method":"hello","params":{"token":token}}),
    )
    .await
    .unwrap();
    serde_json::from_slice(&fm_ipc::read_frame(&mut stream).await.unwrap().unwrap()).unwrap()
}

#[tokio::test]
async fn private_handshake_and_version_are_enforced_before_queries() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let mut owner = client(&profile).await;
    let d = fm_ipc::read_discovery(&profile).unwrap();
    assert_eq!(
        handshake(&d, "wrong", 1).await["error"]["code"],
        "unauthorized"
    );
    assert_eq!(
        handshake(&d, &d.token, 999).await["error"]["code"],
        "protocol_mismatch"
    );
    assert_eq!(
        owner
            .call("conversation", None, json!({"id":"anything"}))
            .await
            .unwrap()["ok"],
        false
    );
    assert_eq!(
        owner.call("not-a-method", None, json!({})).await.unwrap()["ok"],
        false
    );
    ok(owner.call("status", None, json!({})).await.unwrap());
    drop(owner);
    wait_stopped(&profile).await;
}

#[cfg(unix)]
#[tokio::test]
async fn shared_profile_is_rejected_and_stale_owned_endpoint_recovers() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    std::fs::create_dir(&profile).unwrap();
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Client::connect(&profile, Some(&engine())).await.is_err());
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700)).unwrap();
    let old = std::os::unix::net::UnixListener::bind(profile.join("engine.sock")).unwrap();
    drop(old);
    let mut live = client(&profile).await;
    ok(live.call("status", None, json!({})).await.unwrap());
    drop(live);
    wait_stopped(&profile).await;
}

#[tokio::test]
async fn message_invite_is_a_scoped_read_in_demo_profiles() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("demo");
    let mut c = client(&profile).await;
    let accounts = ok(c.call("accounts", None, json!({})).await.unwrap());
    let account = accounts["accounts"][0]["id"].as_str().unwrap().to_owned();
    let conversation = first(&mut c, &account).await;
    let message = &conversation["messages"][0];
    let request = json!({"id":message["id"],"source_version":message["source_version"]});
    let source = ok(c
        .call("message_source", Some(&account), request.clone())
        .await
        .unwrap());
    assert_eq!(source["has_invite"], false);
    assert_eq!(
        ok(c.call("message_invite", Some(&account), request.clone())
            .await
            .unwrap()),
        json!({"invite":null})
    );
    let stale = c
        .call(
            "message_invite",
            Some(&account),
            json!({"id":message["id"],"source_version":"changed"}),
        )
        .await
        .unwrap();
    assert_eq!(stale["error"]["code"], "stale_source", "{stale}");
    let unscoped = c
        .call("message_invite", None, request.clone())
        .await
        .unwrap();
    assert_eq!(unscoped["error"]["code"], "account_required", "{unscoped}");
    let mut extra = request;
    extra["write"] = json!(true);
    let refused = c
        .call("message_invite", Some(&account), extra)
        .await
        .unwrap();
    assert_eq!(refused["error"]["code"], "invalid_request", "{refused}");
    drop(c);
    wait_stopped(&profile).await;
}

#[tokio::test]
async fn html_reader_serves_sanitised_html_and_fetches_nothing_unasked() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("demo");
    let mut c = client(&profile).await;
    let accounts = ok(c.call("accounts", None, json!({})).await.unwrap());
    let account = accounts["accounts"][0]["id"].as_str().unwrap().to_owned();
    let list = ok(c
        .call(
            "conversations",
            Some(&account),
            json!({"limit":100,"folder":"all"}),
        )
        .await
        .unwrap());
    let row = list["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["subject"] == "Weekly service availability")
        .expect("the fixture HTML conversation")
        .clone();
    let conversation = ok(c
        .call("conversation", Some(&account), json!({"id":row["id"]}))
        .await
        .unwrap());
    let message = &conversation["messages"][0];
    let request = json!({"id":message["id"],"source_version":message["source_version"]});
    let view = ok(c
        .call("message_html", Some(&account), request.clone())
        .await
        .unwrap());
    assert_eq!(view["has_html"], true);
    let html = view["html"].as_str().unwrap();
    assert!(
        html.contains("Weekly availability")
            && !html.contains("SCRIPT_MUST_NOT_APPEAR")
            && !html.contains("invalid.example")
    );
    let remote: Vec<Value> = view["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "remote")
        .cloned()
        .collect();
    assert!(
        !remote.is_empty()
            && remote
                .iter()
                .all(|r| r["host"] == "invalid.example" && r.get("source").is_none())
    );
    assert!(view["links"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["host"] == "status.example"));
    // Opening shows cached images only; nothing is fetched.
    let mut image = request.clone();
    image["resource"] = remote[0]["id"].clone();
    image["cached_only"] = json!(true);
    assert_eq!(
        ok(c.call("message_image", Some(&account), image.clone())
            .await
            .unwrap())["state"],
        "not_cached"
    );
    // A blocked host is refused before any connection, even after Load images.
    let prefs = ok(c.call("preferences_get", None, json!({})).await.unwrap());
    ok(c.call(
        "preferences_save",
        None,
        json!({"preferences":{"blocked_hosts":[{"pattern":"*.invalid.example","source":"settings","added_at":1}]},
               "expected_revision":prefs["revision"],"request_id":"block-invalid"}),
    )
    .await
    .unwrap());
    image["cached_only"] = json!(false);
    let blocked = ok(c
        .call("message_image", Some(&account), image)
        .await
        .unwrap());
    assert_eq!(
        (blocked["state"].as_str(), blocked["by"].as_str()),
        (Some("blocked"), Some("*.invalid.example"))
    );
    let prefs = ok(c.call("preferences_get", None, json!({})).await.unwrap());
    let bad = c
        .call(
            "preferences_save",
            None,
            json!({"preferences":{"blocked_hosts":[{"pattern":"Not A Domain","source":"settings","added_at":1}]},
                   "expected_revision":prefs["revision"],"request_id":"block-bad"}),
        )
        .await
        .unwrap();
    assert_eq!(bad["ok"], false, "unnormalised patterns are refused");
    // The background index counts links across every account.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let summary = loop {
        let summary = ok(c
            .call(
                "link_host_summary",
                Some(&account),
                json!({"host":"www.status.example","conversation":row["id"]}),
            )
            .await
            .unwrap());
        if summary["complete"] == true || tokio::time::Instant::now() > deadline {
            break summary;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(
        summary["complete"], true,
        "the link index finishes: {summary}"
    );
    assert_eq!(summary["host"], "status.example");
    let counts = ok(c
        .call(
            "link_pattern_counts",
            None,
            json!({"patterns":["*.status.example","status.example"]}),
        )
        .await
        .unwrap());
    // The pattern counts include this conversation's message; the summary does not.
    assert!(
        counts["counts"]["status.example"].as_i64().unwrap()
            > summary["messages"].as_i64().unwrap()
    );
    assert_eq!(
        counts["counts"]["*.status.example"],
        counts["counts"]["status.example"]
    );
    let cache = ok(c
        .call("remote_image_cache", None, json!({"op":"stats"}))
        .await
        .unwrap());
    assert_eq!(cache["limit"], 256 * 1024 * 1024);
    // Sender pictures need no account; turned off, nothing is fetched.
    let bad = c
        .call(
            "sender_avatar",
            None,
            json!({"address":"nobody","kind":"people"}),
        )
        .await
        .unwrap();
    assert_eq!(bad["error"]["code"], "invalid_request", "{bad}");
    let prefs = ok(c.call("preferences_get", None, json!({})).await.unwrap());
    assert_eq!(prefs["preferences"]["sender_pictures"], true);
    ok(c.call(
        "preferences_save",
        None,
        json!({"preferences":{"sender_pictures":false},
               "expected_revision":prefs["revision"],"request_id":"pictures-off"}),
    )
    .await
    .unwrap());
    assert_eq!(
        ok(c.call(
            "sender_avatar",
            None,
            json!({"address":"billing@e.deel.com","kind":"automated"}),
        )
        .await
        .unwrap()),
        json!({"state":"none","label":"No photo \u{b7} initials","off":true})
    );
}

#[tokio::test]
async fn keyring_requests_run_through_the_engine_with_a_private_demo_key() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("demo");
    let mut c = client(&profile).await;
    let accounts = ok(c.call("accounts", None, json!({})).await.unwrap());
    let account = &accounts["accounts"][0];
    let id = account["id"].as_str().unwrap().to_owned();
    assert_eq!(account["pgp"], Value::Null);
    assert_eq!(
        ok(c.call("pgp_keys", None, json!({})).await.unwrap())["keys"],
        json!([])
    );
    let generated = ok(c
        .call(
            "pgp_key_generate",
            None,
            json!({"name":account["name"],"email":account["email"],"algorithm":"curve25519","expires_days":1095,"account":id,"request_id":"engine-generate"}),
        )
        .await
        .unwrap());
    let fingerprint = generated["fingerprint"].as_str().unwrap().to_owned();
    // The demo profile keeps the sealing key in a private file, never the keychain.
    let key_file = profile.join("pgp-demo-key");
    assert!(key_file.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&key_file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let accounts = ok(c.call("accounts", None, json!({})).await.unwrap());
    assert_eq!(accounts["accounts"][0]["pgp"]["fingerprint"], fingerprint);
    let exported = ok(c
        .call(
            "pgp_key_export",
            None,
            json!({"fingerprint":fingerprint,"kind":"public"}),
        )
        .await
        .unwrap());
    assert!(exported["armored"]
        .as_str()
        .unwrap()
        .starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    // Errors keep their details, such as which key needs a passphrase.
    let secret = ok(c
        .call(
            "pgp_key_export",
            None,
            json!({"fingerprint":fingerprint,"kind":"secret","passphrase":"engine test passphrase"}),
        )
        .await
        .unwrap());
    let refused = c
        .call(
            "pgp_key_import",
            None,
            json!({"text":secret["armored"],"request_id":"engine-import"}),
        )
        .await
        .unwrap();
    assert_eq!(refused["error"]["code"], "passphrase_required", "{refused}");
    assert_eq!(refused["error"]["details"]["fingerprint"], fingerprint);
    // Given a path, the engine writes the key to a new owner-only file and
    // the reply carries the path, not the key.
    let backup = root.path().join("backup.asc");
    let written = ok(c
        .call(
            "pgp_key_export",
            None,
            json!({"fingerprint":fingerprint,"kind":"secret","passphrase":"engine test passphrase","path":backup.to_str().unwrap()}),
        )
        .await
        .unwrap());
    assert!(written["armored"].is_null());
    assert_eq!(written["path"], backup.to_str().unwrap());
    assert!(std::fs::read_to_string(&backup)
        .unwrap()
        .starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let exists = c
        .call(
            "pgp_key_export",
            None,
            json!({"fingerprint":fingerprint,"kind":"public","path":backup.to_str().unwrap()}),
        )
        .await
        .unwrap();
    assert_eq!(exists["error"]["code"], "already_exists", "{exists}");
    ok(c.call(
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"public","path":backup.to_str().unwrap(),"replace":true}),
    )
    .await
    .unwrap());
    assert!(std::fs::read_to_string(&backup)
        .unwrap()
        .starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    // A key file from a message previews without changing anything.
    let preview = ok(c
        .call(
            "pgp_key_import",
            None,
            json!({"text":exported["armored"],"preview":true,"public_only":true}),
        )
        .await
        .unwrap());
    assert_eq!(preview["keys"][0]["action"], "update");
    let scoped = c.call("pgp_keys", Some(&id), json!({})).await.unwrap();
    assert_eq!(scoped["error"]["code"], "invalid_request", "{scoped}");
    let recipients = ok(c
        .call(
            "pgp_recipients",
            Some(&id),
            json!({"addresses":[account["email"]]}),
        )
        .await
        .unwrap());
    assert_eq!(recipients["recipients"][0]["status"], "ready");
    // A restarted engine reads the same sealing key back.
    drop(c);
    wait_stopped(&profile).await;
    let mut c = client(&profile).await;
    let revocation = ok(c
        .call(
            "pgp_key_export",
            None,
            json!({"fingerprint":fingerprint,"kind":"revocation"}),
        )
        .await
        .unwrap());
    assert!(revocation["armored"]
        .as_str()
        .unwrap()
        .contains("BEGIN PGP PUBLIC KEY BLOCK"));
    drop(c);
    wait_stopped(&profile).await;
}
