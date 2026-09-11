use std::process::Command;

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
