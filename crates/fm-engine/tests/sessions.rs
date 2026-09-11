use fm_ipc::{Client, Discovery};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::BufReader;

fn engine() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fm-engine"))
}
async fn client(profile: &Path) -> Client {
    Client::connect(profile, Some(&engine()))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paired_clients_share_durable_state_and_explicit_lifetimes() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let mut gui = client(&profile).await;
    let discovery = fm_ipc::read_discovery(&profile).unwrap();
    let mut cli = client(&profile).await;
    assert_eq!(fm_ipc::read_discovery(&profile).unwrap().pid, discovery.pid);
    let accounts = ok(cli.call("accounts", None, json!({})).await.unwrap());
    assert_eq!(accounts["accounts"].as_array().unwrap().len(), 2);
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
    assert_eq!(seen, changed);
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_launches_elect_one_writer() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let (mut a, mut b, mut c) = tokio::join!(client(&profile), client(&profile), client(&profile));
    let astatus = ok(a.call("status", None, json!({})).await.unwrap());
    let bstatus = ok(b.call("status", None, json!({})).await.unwrap());
    let cstatus = ok(c.call("status", None, json!({})).await.unwrap());
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
