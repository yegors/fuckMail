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
        vec!["google", "calendar-resume", "request"],
        vec!["google", "draft-status", "--draft-id", "draft-1"],
        vec!["google", "draft-reconcile", "--draft-id", "draft-1"],
        vec!["google", "send-status", "--request-id", "send-1"],
        vec!["google", "send-reconcile", "--request-id", "send-1"],
        vec![
            "google",
            "send-abandon",
            "--request-id",
            "send-1",
            "--expected-revision",
            "1",
        ],
        vec![
            "google",
            "send",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "1",
            "--expected-google-revision",
            "0",
            "--request-id",
            "send-1",
        ],
        vec![
            "google",
            "draft-sync",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--request-id",
            "upload-1",
        ],
        vec![
            "google",
            "draft-replace",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--expected-google-revision",
            "2",
            "--request-id",
            "replace-1",
        ],
        vec![
            "google",
            "calendar-abandon",
            "request",
            "--expected-revision",
            "1",
        ],
        vec!["google", "labels"],
        vec!["google", "labels-refresh"],
        vec!["google", "drafts-refresh"],
        vec!["google", "drafts-status"],
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
        "calendar-resume",
        "calendar-abandon",
        "draft-sync",
        "draft-status",
        "draft-reconcile",
        "draft-replace",
        "send",
        "send-status",
        "send-reconcile",
        "send-abandon",
        "labels",
        "labels-refresh",
        "drafts-refresh",
        "drafts-status",
        "category-bind",
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
fn google_send_preflight_requires_exact_identity_and_revision_before_storage() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    let valid = vec![
        "send",
        "--draft-id",
        "draft-1",
        "--expected-revision",
        "1",
        "--expected-google-revision",
        "0",
        "--request-id",
        "send-1",
    ];
    let long_id = "x".repeat(257);
    let long_request = "x".repeat(129);
    let mut invalid = vec![
        vec!["send-status", "--request-id", ""],
        vec!["send-reconcile", "--request-id", "send\nother"],
        vec!["send-status", "--request-id", &long_request],
        vec![
            "send-abandon",
            "--request-id",
            "send-1",
            "--expected-revision",
            "0",
        ],
        vec!["send-abandon", "--request-id", "send-1"],
        vec![
            "send-reconcile",
            "--request-id",
            "send-1",
            "--expected-revision",
            "1",
        ],
    ];
    for (index, value) in [
        (2, ""),
        (2, "draft\nother"),
        (2, long_id.as_str()),
        (4, "0"),
        (4, "9223372036854775808"),
        (6, "-1"),
        (6, "9223372036854775808"),
        (8, ""),
        (8, long_request.as_str()),
    ] {
        let mut tail = valid.clone();
        tail[index] = value;
        invalid.push(tail);
    }
    for flag in [
        "--draft-id",
        "--expected-revision",
        "--expected-google-revision",
        "--request-id",
    ] {
        let mut tail = valid.clone();
        let index = tail.iter().position(|arg| *arg == flag).unwrap();
        tail.drain(index..index + 2);
        invalid.push(tail);
    }
    for tail in invalid {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "--account",
                "account",
                "google",
            ])
            .args(&tail)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{tail:?}");
        assert!(
            !absent.exists(),
            "invalid submission must not start storage"
        );
    }
    // A new Google twin has revision zero. Reach the explicit-profile guard,
    // proving Clap accepted zero rather than rejecting this valid revision.
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args(["--json", "--account", "account", "google"])
        .args(valid)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert!(reply["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--profile"));
    assert!(!absent.exists());
}

#[test]
fn google_draft_preflight_refuses_ambiguous_revisions_and_identities_before_storage() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    let long_id = "x".repeat(257);
    let long_request = "x".repeat(129);
    for tail in [
        vec!["draft-status", "--draft-id", ""],
        vec!["draft-reconcile", "--draft-id", "draft\nother"],
        vec!["draft-status", "--draft-id", &long_id],
        vec![
            "draft-sync",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "0",
            "--request-id",
            "upload-1",
        ],
        vec![
            "draft-sync",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "9223372036854775808",
            "--request-id",
            "upload-1",
        ],
        vec![
            "draft-sync",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--request-id",
            "",
        ],
        vec![
            "draft-sync",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--request-id",
            &long_request,
        ],
        vec![
            "draft-replace",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--request-id",
            "replace-1",
        ],
        vec![
            "draft-replace",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--expected-google-revision",
            "0",
            "--request-id",
            "replace-1",
        ],
        vec![
            "draft-replace",
            "--draft-id",
            "draft-1",
            "--expected-revision",
            "3",
            "--expected-google-revision",
            "2",
            "--request-id",
            "replace\nother",
        ],
        vec![
            "draft-reconcile",
            "--draft-id",
            "draft-1",
            "--request-id",
            "invented-retry",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "--account",
                "chosen-account",
                "google",
            ])
            .args(&tail)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "accepted invalid draft arguments: {tail:?}"
        );
        assert!(
            !absent.exists(),
            "invalid draft arguments started storage: {tail:?}"
        );
    }
}

#[test]
fn label_pagination_and_binding_require_explicit_unambiguous_choices_before_storage() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    for tail in [
        vec!["labels", "--after", "Label_1"],
        vec!["labels", "--limit", "101"],
        vec![
            "category-bind",
            "category",
            "Label_1",
            "--expected-catalog-revision",
            "1",
            "--request-id",
            "bind",
        ],
        vec![
            "category-bind",
            "category",
            "Label_1",
            "--expected-catalog-revision",
            "1",
            "--request-id",
            "bind",
            "--create-name",
            "Name",
            "--unmapped",
        ],
        vec![
            "category-bind",
            "category",
            "Label_1",
            "--expected-catalog-revision",
            "1",
            "--request-id",
            "bind",
            "--unmapped",
            "--expected-provider-label-id",
            "Old",
        ],
        vec!["calendar-abandon", "request"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "--account",
                "account",
                "google",
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
        assert!(!absent.exists());
    }
    for tail in [
        vec!["mail", "delivery", "--request-id", "request", "--help"],
        vec![
            "google",
            "category-bind",
            "category",
            "Label_1",
            "--expected-catalog-revision",
            "1",
            "--request-id",
            "bind",
            "--create-name",
            "Name",
            "--help",
        ],
        vec![
            "google",
            "category-bind",
            "category",
            "Label_1",
            "--expected-catalog-revision",
            "1",
            "--request-id",
            "bind",
            "--expected-provider-label-id",
            "Old",
            "--help",
        ],
        vec![
            "google",
            "category-bind",
            "category",
            "Label_1",
            "--expected-catalog-revision",
            "1",
            "--request-id",
            "bind",
            "--unmapped",
            "--help",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args(["--profile", absent.to_str().unwrap()])
            .args(tail)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!absent.exists());
    }
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
fn attachment_import_requires_scope_saved_draft_revision_and_stable_identity() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    let valid = vec![
        "draft",
        "attach",
        "--draft-id",
        "draft-1",
        "--file",
        "private-attachment-name.bin",
        "--expected-revision",
        "1",
        "--request-id",
        "attach-1",
    ];
    let mut invalid = Vec::new();
    for flag in [
        "--draft-id",
        "--file",
        "--expected-revision",
        "--request-id",
    ] {
        let mut tail = valid.clone();
        let index = tail.iter().position(|arg| *arg == flag).unwrap();
        tail.drain(index..index + 2);
        invalid.push(tail);
    }
    for revision in ["0", "-1", "9223372036854775808"] {
        let mut tail = valid.clone();
        tail[7] = revision;
        invalid.push(tail);
    }
    let mut legacy = valid.clone();
    legacy.push("legacy-conversation");
    invalid.push(legacy);
    for tail in invalid {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "--account",
                "account",
            ])
            .args(tail)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-attachment-name.bin"));
        assert!(!absent.exists(), "invalid import must not start storage");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args(["--profile", absent.to_str().unwrap(), "--json"])
        .args(valid)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_request");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-attachment-name.bin"));
    assert!(!absent.exists());
    let help = Command::new(env!("CARGO_BIN_EXE_fm"))
        .args([
            "--profile",
            absent.to_str().unwrap(),
            "draft",
            "attach",
            "--help",
        ])
        .output()
        .unwrap();
    assert!(help.status.success());
    for flag in [
        "--draft-id",
        "--file",
        "--expected-revision",
        "--request-id",
    ] {
        assert!(String::from_utf8_lossy(&help.stdout).contains(flag));
    }
    assert!(!absent.exists());
}

#[test]
fn remote_draft_sources_reject_invalid_identity_revision_and_paging_before_storage() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("not-created");
    let output_path = root.path().join("must-not-be-created.eml");
    for command in ["source", "sources"] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "draft",
                command,
                "--help",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!absent.exists());
    }
    let valid_source = vec![
        "draft",
        "source",
        "--draft-id",
        "remote-draft",
        "--source-revision",
        "1",
        "--output",
        output_path.to_str().unwrap(),
    ];
    let mut invalid = Vec::new();
    for revision in ["0", "-1", "9223372036854775808"] {
        let mut args = valid_source.clone();
        args[5] = revision;
        invalid.push(args);
    }
    for field in ["--draft-id", "--source-revision"] {
        let mut args = valid_source.clone();
        let index = args.iter().position(|arg| *arg == field).unwrap();
        args.drain(index..index + 2);
        invalid.push(args);
    }
    for (field, value) in [
        ("--limit", "0"),
        ("--limit", "101"),
        ("--offset", "10000001"),
        ("--offset", "-1"),
    ] {
        invalid.push(vec![
            "draft",
            "sources",
            "--draft-id",
            "remote-draft",
            field,
            value,
        ]);
    }
    let overlong = "x".repeat(257);
    for id in ["", "private\nidentity", overlong.as_str()] {
        let mut args = valid_source.clone();
        args[3] = id;
        invalid.push(args);
        let mut args = valid_source.clone();
        args.extend(["--attachment-id", id]);
        invalid.push(args);
        invalid.push(vec!["draft", "sources", "--draft-id", id]);
    }
    for args in invalid {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args([
                "--profile",
                absent.to_str().unwrap(),
                "--account",
                "account",
                "--json",
            ])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            !absent.exists(),
            "Invalid source request must not open storage"
        );
        assert!(
            !output_path.exists(),
            "Invalid source request must not create an export"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private\nidentity"));
    }
    for args in [
        valid_source,
        vec!["draft", "sources", "--draft-id", "remote-draft"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args(["--profile", absent.to_str().unwrap(), "--json"])
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!absent.exists());
        let output = Command::new(env!("CARGO_BIN_EXE_fm"))
            .args(["--account", "account", "--json"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(data["error"]["code"], "invalid_request");
        assert!(!output_path.exists());
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
