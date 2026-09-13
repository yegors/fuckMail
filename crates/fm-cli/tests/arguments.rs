use std::process::Command;

#[test]
fn google_preflight_does_not_start_storage_for_missing_scope_or_setup() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    for args in [
        vec!["google", "connect", "--client-config", "absent-client.json"],
        vec!["google", "disconnect"],
        vec!["google", "sync-stop"],
        vec!["google", "sync"],
        vec!["google", "sync", "--gmail-project-units-per-minute", "0"],
        vec!["google", "calendar-write", "--params-file", "absent.json"],
        vec!["google", "calendar-reconcile", "request"],
        vec!["google", "calendar-operation", "request"],
        vec!["mail", "delivery"],
        vec!["mail", "filing", "thread", "--action", "delete"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args(["--profile", absent.to_str().unwrap()])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!absent.exists());
    }
    for command in [
        "connect",
        "sync",
        "sync-status",
        "sync-stop",
        "calendar-write",
        "calendar-reconcile",
        "calendar-operation",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "google",
                command,
                "--help",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!absent.exists());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args([
            "--profile",
            absent.to_str().unwrap(),
            "--account",
            "example",
            "google",
            "calendar-write",
            "--params-file",
            root.path().join("missing.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        !absent.exists(),
        "missing Calendar input must fail before storage starts"
    );
}

#[test]
fn live_profile_initialization_requires_an_explicit_path() {
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args(["--json", "profile-init"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_request");
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("explicit --profile"));
}

#[test]
fn help_and_version_are_headless_and_do_not_start_a_profile() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    for argument in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args(["--profile", absent.to_str().unwrap(), argument])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!absent.exists(), "help must not start engine/storage");
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn scope_and_bound_errors_are_machine_readable_without_a_live_engine() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("not-created");
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "mail",
            "list",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["error"]["code"], "invalid_request");
    assert!(!profile.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--account",
            "windscribe",
            "mail",
            "list",
            "--limit",
            "1000000",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!profile.exists());
}

#[test]
fn composition_selectors_reject_ambiguity_before_starting_an_engine() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    for tail in [
        vec!["draft", "show"],
        vec![
            "draft",
            "show",
            "windscribe-thread-000",
            "--draft-id",
            "draft-1",
        ],
        vec![
            "draft",
            "capture",
            "windscribe-thread-000",
            "--draft-id",
            "draft-1",
        ],
        vec![
            "draft",
            "history",
            "--conversation-id",
            "windscribe-thread-000",
            "--draft-id",
            "draft-1",
        ],
        vec!["draft", "list", "--limit", "101"],
        vec!["draft", "create", "--mode", "send"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "--account",
                "windscribe",
            ])
            .args(&tail)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{tail:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !absent.exists(),
            "invalid composition arguments must not start storage"
        );
    }
}

#[test]
fn sender_commands_and_kinds_are_headless_and_reject_invalid_kinds() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    for command in [
        vec!["mail", "sender", "--help"],
        vec!["mail", "list", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args(["--profile", absent.to_str().unwrap()])
            .args(command)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!absent.exists());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args([
            "--profile",
            absent.to_str().unwrap(),
            "--account",
            "windscribe",
            "mail",
            "list",
            "--kind",
            "guessed",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!absent.exists());
}
