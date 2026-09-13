//! Actual-process isolation checks; every path is a newly allocated test profile.
use std::{
    fs,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

const MARKER: &str = "verification-google-profile.json";

struct OwnedEngine(Child);
impl OwnedEngine {
    fn wait_bounded(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait().expect("read owned child status") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "owned engine exceeded its watchdog"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for OwnedEngine {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn spawn(profile: &Path, extra: &[&str], log: &Path) -> OwnedEngine {
    let output = fs::File::create(log).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fm-engine"));
    command
        .arg("--profile")
        .arg(profile)
        .args(extra)
        .stdin(Stdio::null())
        .stdout(output.try_clone().unwrap())
        .stderr(output);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW for a test-owned helper.
    }
    OwnedEngine(command.spawn().expect("start actual engine binary"))
}

#[test]
fn marked_profile_is_refused_by_an_actual_ordinary_startup_without_store_mutation() {
    let directory = tempfile::tempdir().unwrap();
    for directory_marker in [false, true] {
        let profile = directory.path().join(if directory_marker {
            "directory-marker"
        } else {
            "file-marker"
        });
        fs::create_dir(&profile).unwrap();
        let marker = profile.join(MARKER);
        if directory_marker {
            fs::create_dir(&marker).unwrap();
        } else {
            fs::write(
                &marker,
                b"verification sentinel; never trusted as startup instructions",
            )
            .unwrap();
        }
        let log = directory.path().join(if directory_marker {
            "directory.log"
        } else {
            "file.log"
        });
        let mut child = spawn(&profile, &["--live-profile"], &log);
        let status = child.wait_bounded(Duration::from_secs(5));
        assert!(
            !status.success(),
            "marked profile was opened by ordinary startup"
        );
        let output = fs::read_to_string(log).unwrap();
        assert!(
            output.contains("verification") || output.contains("Verification"),
            "{output}"
        );
        assert!(!profile.join("store.sqlite3").exists());
        assert!(!profile.join("engine.json").exists());
        assert!(!profile.join("live-profile").exists());
        assert_eq!(fs::read_dir(&profile).unwrap().count(), 1);
        if !directory_marker {
            assert_eq!(
                fs::read(&marker).unwrap(),
                b"verification sentinel; never trusted as startup instructions"
            );
        }
    }
}

#[cfg(not(feature = "verification-google"))]
#[test]
fn ordinary_binary_has_no_verification_option_and_does_not_create_a_profile() {
    let directory = tempfile::tempdir().unwrap();
    for option in ["--verification-google", "--verification-google-send"] {
        let profile = directory.path().join(option.trim_start_matches('-'));
        let log = profile.with_extension("log");
        let mut child = spawn(&profile, &["--live-profile", option], &log);
        let status = child.wait_bounded(Duration::from_secs(5));
        assert_eq!(
            status.code(),
            Some(2),
            "default binary unexpectedly recognized the feature option"
        );
        let output = fs::read_to_string(log).unwrap();
        assert!(
            output.contains("unexpected argument") && output.contains(option),
            "{output}"
        );
        assert!(!profile.exists());
    }
}

#[cfg(feature = "verification-google")]
#[test]
fn isolated_binary_requires_both_explicit_fixture_mode_and_live_profile_before_creation() {
    let directory = tempfile::tempdir().unwrap();
    for (name, args) in [
        ("missing-mode", vec!["--live-profile"]),
        ("missing-live", vec!["--verification-google"]),
        (
            "send-missing-mode",
            vec!["--live-profile", "--verification-google-send"],
        ),
        (
            "send-missing-live",
            vec!["--verification-google", "--verification-google-send"],
        ),
    ] {
        let profile = directory.path().join(name);
        let log = directory.path().join(format!("{name}.log"));
        let mut child = spawn(&profile, &args, &log);
        assert!(!child.wait_bounded(Duration::from_secs(5)).success());
        assert!(
            !profile.exists(),
            "invalid isolated startup created its requested profile"
        );
        assert!(fs::metadata(log).unwrap().len() > 0);
    }
}

#[cfg(feature = "verification-google")]
#[tokio::test]
async fn explicit_empty_fixture_serves_cached_queries_without_acquisition_and_stays_fenced_after_exit(
) {
    use fm_ipc::Client;
    use serde_json::{json, Value};

    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("owned-empty-profile");
    let mut child = spawn(
        &profile,
        &["--live-profile", "--verification-google"],
        &directory.path().join("fixture.log"),
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if fm_ipc::read_discovery(&profile).is_ok_and(|discovery| discovery.pid == child.0.id()) {
            break;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "fixture engine exited before discovery"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "fixture startup exceeded its watchdog"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Discovery already identifies this process: this client must never need to
    // launch a replacement engine or infer authorization from retained data.
    let mut client = tokio::time::timeout(
        Duration::from_secs(3),
        Client::connect_live(&profile, Some(Path::new(env!("CARGO_BIN_EXE_fm-engine")))),
    )
    .await
    .unwrap()
    .unwrap();
    let status = tokio::time::timeout(
        Duration::from_secs(3),
        client.call("status", None, json!({})),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(status["ok"], true);
    assert_eq!(status["result"]["profile_kind"], "live");
    assert_eq!(status["result"]["fixture_revision"], 0);
    let accounts = tokio::time::timeout(
        Duration::from_secs(3),
        client.call("accounts", None, json!({})),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(accounts["ok"], true);
    assert_eq!(accounts["result"]["accounts"].as_array().unwrap().len(), 2);
    drop(client);
    // Keep the current-thread Tokio I/O driver running while the dropped
    // asynchronous pipe completes its close. The synchronous helper is only
    // suitable for tests that never owned an asynchronous client.
    let exit = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "owned engine exceeded its unchanged shutdown watchdog: {}",
            fs::read_to_string(directory.path().join("fixture.log")).unwrap_or_default()
        )
    });
    assert!(exit.success());
    assert!(!profile.join("engine.json").exists());
    let report: Value =
        serde_json::from_slice(&fs::read(profile.join("verification-google-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["engine_pid"], child.0.id());
    assert_eq!(report["status"], "complete");
    assert_eq!(report["transport"], "in_memory_only");
    assert_eq!(report["native_vault"], false);
    assert_eq!(report["counters"]["http_started"], 0);
    assert_eq!(report["counters"]["vault_loads"], 0);
    assert_eq!(report["counters"]["vault_stores"], 0);
    assert_eq!(report["publications"], json!([]));
    assert_eq!(report["errors"], json!([]));
    let db = rusqlite::Connection::open_with_flags(
        profile.join("store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM messages", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);

    let marker = fs::read(profile.join(MARKER)).unwrap();
    let database = fs::read(profile.join("store.sqlite3")).unwrap();
    let mut restarted = spawn(
        &profile,
        &["--live-profile", "--verification-google"],
        &directory.path().join("restart.log"),
    );
    assert!(!restarted.wait_bounded(Duration::from_secs(5)).success());
    assert!(!profile.join("engine.json").exists());
    assert_eq!(fs::read(profile.join(MARKER)).unwrap(), marker);
    assert_eq!(fs::read(profile.join("store.sqlite3")).unwrap(), database);
}
