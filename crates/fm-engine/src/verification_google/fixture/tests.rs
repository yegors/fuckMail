use super::*;
use fm_google::{
    gmail_api::{GmailApi, GmailCall, GmailReply, RawOutcome},
    transport::{HttpMethod, HttpRequest, HttpTransport},
    AccountId,
};

fn setup() -> (tempfile::TempDir, Store, Fixture) {
    setup_mode(false)
}
fn setup_mode(send_enabled: bool) -> (tempfile::TempDir, Store, Fixture) {
    let directory = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
    claim_empty_profile(&profile).unwrap();
    let mut store = Store::open_live(&profile).unwrap();
    let fixture = Fixture::prepare(&profile, &mut store, send_enabled).unwrap();
    (directory, store, fixture)
}
fn api(fixture: &Fixture, index: usize) -> GmailApi<FixtureHttp> {
    GmailApi::new(
        AccountId::new(fixture.accounts[index].id.clone()).unwrap(),
        fixture.http.clone(),
    )
}
fn account(fixture: &Fixture, index: usize) -> AccountId {
    AccountId::new(fixture.accounts[index].id.clone()).unwrap()
}
fn limits() -> Value {
    json!({"gmail_project_units_per_minute":1_000_000,"gmail_account_units_per_minute":500_000,"calendar_project_units_per_minute":10_000,"calendar_account_units_per_minute":5_000})
}

#[tokio::test]
async fn actions_fixture_keeps_account_targets_sources_and_delete_certainty_distinct() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use fm_google::gmail_api::{DraftDeletionResult, GmailDeletion};
    let directory = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
    claim_empty_profile(&profile).unwrap();
    let mut store = Store::open_live(&profile).unwrap();
    let fixture = Fixture::prepare_with_actions(&profile, &mut store, false, false, true).unwrap();
    let cancel = CancellationToken::new();
    for index in 0..2 {
        let account = account(&fixture, index);
        let api = api(&fixture, index);
        let token = fixture.accounts[index].token();
        let GmailReply::Drafts(page) = api
            .execute(
                GmailCall::list_drafts(&account, None, 50).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("list")
        };
        assert_eq!(page.drafts.len(), 6);
        for source in 0..6 {
            let id = format!("ra{source:04}");
            let GmailReply::Draft(draft) = api
                .execute(
                    GmailCall::get_draft(&account, &id).unwrap(),
                    &token,
                    &cancel,
                )
                .await
                .unwrap()
            else {
                panic!("source")
            };
            assert_eq!(draft.message.id, format!("ram{source:04}v1"));
            assert_eq!(
                URL_SAFE_NO_PAD.decode(draft.message.raw.unwrap()).unwrap(),
                draft_actions::source(
                    fixture.accounts[index].key,
                    source,
                    1,
                    fixture.base_unix_secs
                )
            );
        }
        let mime=format!("From: {}\r\nTo: one@example.test\r\nBcc: blind@example.test\r\nMessage-ID: <action-{index}@example.test>\r\n\r\nNew explicit content",fixture.accounts[index].email);
        if index == 0 {
            let GmailReply::Draft(ack) = api
                .execute(
                    GmailCall::update_draft(&account, "ra0000", mime.as_bytes(), Some("rat0000"))
                        .unwrap(),
                    &token,
                    &cancel,
                )
                .await
                .unwrap()
            else {
                panic!("update")
            };
            let GmailReply::Draft(read) = api
                .execute(
                    GmailCall::get_draft(&account, "ra0000").unwrap(),
                    &token,
                    &cancel,
                )
                .await
                .unwrap()
            else {
                panic!("readback")
            };
            assert_eq!(ack.id, "ra0000");
            assert_eq!(ack.message.id, read.message.id);
            assert_eq!(
                URL_SAFE_NO_PAD.decode(read.message.raw.unwrap()).unwrap(),
                mime.as_bytes()
            );
        } else {
            assert!(matches!(
                api.execute(
                    GmailCall::send_draft(&account, "ra0000", mime.as_bytes(), Some("rat0000"))
                        .unwrap(),
                    &token,
                    &cancel
                )
                .await
                .unwrap(),
                GmailReply::Submission(_)
            ));
            assert!(matches!(
                api.execute(
                    GmailCall::get_draft(&account, "ra0000").unwrap(),
                    &token,
                    &cancel
                )
                .await,
                Err(fm_google::GoogleError::NotFound)
            ));
        }
        for (id, result) in [
            ("ra0004", DraftDeletionResult::NoDefinitiveResponse),
            ("ra0005", DraftDeletionResult::Deleted),
        ] {
            let GmailReply::DraftDeletion(GmailDeletion { result: actual, .. }) = api
                .execute(
                    GmailCall::delete_draft(&account, id).unwrap(),
                    &token,
                    &cancel,
                )
                .await
                .unwrap()
            else {
                panic!("delete")
            };
            assert_eq!(actual, result);
            assert!(matches!(
                api.execute(GmailCall::get_draft(&account, id).unwrap(), &token, &cancel)
                    .await,
                Err(fm_google::GoogleError::NotFound)
            ));
        }
    }
    fixture.action_version_two.store(true, Ordering::Release);
    let GmailReply::Draft(changed) = api(&fixture, 0)
        .execute(
            GmailCall::get_draft(&account(&fixture, 0), "ra0003").unwrap(),
            &fixture.accounts[0].token(),
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("changed")
    };
    assert_eq!(changed.message.id, "ram0003v2");
    let report = fixture.report(false);
    assert_eq!(report["errors"], json!([]));
    assert_eq!(report["remote_drafts_fixture"]["enabled"], false);
    assert_eq!(
        report["draft_actions_fixture"]["reads"]
            .as_array()
            .unwrap()
            .len(),
        14
    );
    assert_eq!(report["counters"]["draft_delete_attempts"], 4);
    assert_eq!(report["counters"]["draft_delete_unknown"], 2);
    assert_eq!(report["counters"]["draft_deleted"], 2);
    assert_eq!(
        report["send_fixture"]["publications"][0]["provider_draft_id"],
        "ra0000"
    );
}

#[tokio::test]
async fn seeded_remote_drafts_have_separate_read_only_accounts_versions_and_bounded_pages() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let directory = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
    claim_empty_profile(&profile).unwrap();
    let mut store = Store::open_live(&profile).unwrap();
    let fixture = Fixture::prepare_with_drafts(&profile, &mut store, false, true).unwrap();
    let cancel = CancellationToken::new();
    for index in 0..2 {
        let owner = account(&fixture, index);
        let api = api(&fixture, index);
        let token = fixture.accounts[index].token();
        let GmailReply::Drafts(first) = api
            .execute(
                GmailCall::list_drafts(&owner, None, 50).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("page")
        };
        assert_eq!(first.drafts.len(), 50);
        assert_eq!(first.drafts[1].message.id, "rm0001v1");
        let GmailReply::Drafts(last) = api
            .execute(
                GmailCall::list_drafts(&owner, first.next_page_token.as_deref(), 50).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("page")
        };
        assert_eq!(last.drafts.len(), 2);
        assert!(last.next_page_token.is_none());
        let GmailReply::Draft(read) = api
            .execute(
                GmailCall::get_draft(&owner, "rd0001").unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("draft")
        };
        assert_eq!(read.message.id, "rm0001v2");
        assert_eq!(
            URL_SAFE_NO_PAD.decode(read.message.raw.unwrap()).unwrap(),
            remote_drafts::source(fixture.accounts[index].key, 1, 2, fixture.base_unix_secs)
        );
        assert!(matches!(
            api.execute(
                GmailCall::get_draft(&owner, "rd0002").unwrap(),
                &token,
                &cancel
            )
            .await,
            Err(fm_google::GoogleError::NotFound)
        ));
        let binding = store
            .google_account_bindings()
            .unwrap()
            .into_iter()
            .find(|binding| binding.account == owner.as_str())
            .unwrap();
        let scopes = fm_google::auth::ScopeSet::new(&binding.scopes);
        assert!(scopes.mail_read());
        assert!(!scopes.mail_write());
    }
    fixture.remote_version_two.store(true, Ordering::Release);
    let GmailReply::Draft(read) = api(&fixture, 0)
        .execute(
            GmailCall::get_draft(&account(&fixture, 0), "rd0000").unwrap(),
            &fixture.accounts[0].token(),
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("draft")
    };
    assert_eq!(read.message.id, "rm0000v2");
    assert_eq!(
        URL_SAFE_NO_PAD.decode(read.message.raw.unwrap()).unwrap(),
        remote_drafts::source("personal", 0, 2, fixture.base_unix_secs)
    );
    assert_eq!(fixture.report(false)["counters"]["draft_list"], 4);
    assert_eq!(fixture.report(false)["counters"]["draft_created"], 0);
    assert_eq!(fixture.report(false)["counters"]["send_completed"], 0);
    assert!(fixture.report(false)["errors"]
        .as_array()
        .unwrap()
        .is_empty());
    // The mode also refuses a write from an erroneously enabled consumer.
    assert!(api(&fixture, 0)
        .execute(
            GmailCall::create_draft(&account(&fixture, 0), b"Subject: no\r\n\r\nNo write", None)
                .unwrap(),
            &fixture.accounts[0].token(),
            &cancel
        )
        .await
        .is_err());
    assert_eq!(fixture.report(false)["counters"]["draft_created"], 0);
    assert_eq!(
        fixture.report(false)["errors"],
        json!(["remote_drafts_write_refused"])
    );
}

#[tokio::test]
async fn draft_fixture_retains_exact_raw_and_replaces_message_identity_in_separate_accounts() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    let (_directory, _store, fixture) = setup();
    let cancel = CancellationToken::new();
    for index in 0..2 {
        let owner = account(&fixture, index);
        let token = fixture.accounts[index].token();
        let api = api(&fixture, index);
        let raw = format!("From: {}\r\nBcc: retained@example.test\r\nSubject: Draft {index}\r\n\r\nExact synthetic body\r\n", fixture.accounts[index].email);
        let GmailReply::Draft(created) = api
            .execute(
                GmailCall::create_draft(&owner, raw.as_bytes(), None).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("draft create reply");
        };
        assert_eq!(created.id, "vd0001");
        assert_eq!(created.message.id, "vmd0001");
        assert!(created.message.raw.is_none());
        let GmailReply::Draft(read) = api
            .execute(
                GmailCall::get_draft(&owner, &created.id).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("draft read reply");
        };
        assert_eq!(
            URL_SAFE_NO_PAD.decode(read.message.raw.unwrap()).unwrap(),
            raw.as_bytes()
        );
        let replacement = format!("{raw}Human replacement {index}\r\n");
        let GmailReply::Draft(updated) = api
            .execute(
                GmailCall::update_draft(
                    &owner,
                    &created.id,
                    replacement.as_bytes(),
                    Some(&created.message.thread_id),
                )
                .unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("draft update reply");
        };
        assert_eq!(updated.id, created.id);
        assert_ne!(updated.message.id, created.message.id);
        let GmailReply::Draft(read) = api
            .execute(
                GmailCall::get_draft(&owner, &created.id).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("updated draft read reply");
        };
        assert_eq!(read.message.id, updated.message.id);
        assert_eq!(
            URL_SAFE_NO_PAD.decode(read.message.raw.unwrap()).unwrap(),
            replacement.as_bytes()
        );
        let report = fixture.report(false);
        let publication = report["draft_fixture"]["publications"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        assert_eq!(publication["account"], owner.as_str());
        assert_eq!(
            publication["sha256"],
            format!("{:x}", Sha256::digest(replacement.as_bytes()))
        );
        assert_eq!(publication["size"], replacement.len());
    }
    // Re-read the first account after the second reused the same provider IDs.
    // A accidentally shared draft map must not satisfy either account's bytes.
    for index in 0..2 {
        let GmailReply::Draft(read) = api(&fixture, index)
            .execute(
                GmailCall::get_draft(&account(&fixture, index), "vd0001").unwrap(),
                &fixture.accounts[index].token(),
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("isolated draft read reply");
        };
        let expected = format!("From: {}\r\nBcc: retained@example.test\r\nSubject: Draft {index}\r\n\r\nExact synthetic body\r\nHuman replacement {index}\r\n",fixture.accounts[index].email);
        assert_eq!(
            URL_SAFE_NO_PAD.decode(read.message.raw.unwrap()).unwrap(),
            expected.as_bytes()
        );
    }
    let report = fixture.report(false);
    assert_eq!(report["counters"]["draft_created"], 2);
    assert_eq!(report["counters"]["draft_updated"], 2);
    assert_eq!(report["counters"]["draft_read"], 6);
    assert!(report["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn draft_fixture_bounds_retained_data_and_never_supports_send_or_delete() {
    let (_directory, _store, fixture) = setup();
    let owner = account(&fixture, 0);
    let token = fixture.accounts[0].token();
    let api = api(&fixture, 0);
    let cancel = CancellationToken::new();
    let raw = b"From: personal@example.test\r\n\r\nGenerated only";
    for _ in 0..16 {
        api.execute(
            GmailCall::create_draft(&owner, raw, None).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap();
    }
    assert!(api
        .execute(
            GmailCall::create_draft(&owner, raw, None).unwrap(),
            &token,
            &cancel
        )
        .await
        .is_err());
    assert!(matches!(
        api.execute(
            GmailCall::delete_draft(&owner, "vd0001").unwrap(),
            &token,
            &cancel
        )
        .await
        .unwrap(),
        GmailReply::DraftDeletion(fm_google::gmail_api::GmailDeletion {
            result: fm_google::gmail_api::DraftDeletionResult::NoDefinitiveResponse,
            ..
        })
    ));
    for endpoint in ["messages/send", "drafts/send"] {
        let request = HttpRequest::new(
            HttpMethod::Post,
            &format!("https://gmail.googleapis.com/gmail/v1/users/me/{endpoint}"),
        )
        .unwrap()
        .header("authorization", &format!("Bearer {token}"))
        .unwrap()
        .body(b"{}".to_vec(), "application/json")
        .unwrap();
        assert!(fixture.http.execute(request, &cancel).await.is_err());
    }
    let oversized = vec![b'x'; 2 * 1024 * 1024 + 1];
    assert!(api
        .execute(
            GmailCall::update_draft(&owner, "vd0001", &oversized, None).unwrap(),
            &token,
            &cancel
        )
        .await
        .is_err());
    let report = fixture.report(false);
    assert_eq!(report["counters"]["draft_created"], 16);
    assert_eq!(report["counters"]["draft_updated"], 0);
    assert_eq!(
        report["draft_fixture"]["publications"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
}

#[test]
fn empty_claim_and_bootstrap_never_adopt_existing_data_or_seed_mail() {
    let directory = tempfile::tempdir().unwrap();
    let occupied = fm_ipc::private_profile(&directory.path().join("occupied")).unwrap();
    std::fs::write(occupied.join("authored.txt"), b"retained").unwrap();
    assert!(claim_empty_profile(&occupied).is_err());
    assert!(!occupied.join(MARKER).exists());
    assert_eq!(
        std::fs::read(occupied.join("authored.txt")).unwrap(),
        b"retained"
    );
    let (_directory, mut store, fixture) = setup();
    assert!(claim_empty_profile(&fixture.profile).is_err());
    assert!(super::super::refuse_marked_profile(&fixture.profile).is_err());
    let status = store.handle("status", None, json!({})).unwrap();
    assert_eq!(status["fixture_revision"], 0);
    for owner in fixture.accounts.iter() {
        assert_eq!(
            store
                .handle("conversations", Some(&owner.id), json!({}))
                .unwrap()["conversations"],
            json!([])
        );
    }
    assert_eq!(fixture.report(false)["status"], "ready");
    assert_eq!(fixture.report(false)["counters"]["http_started"], 0);
    assert_eq!(fixture.report(false)["counters"]["vault_loads"], 0);
}

#[tokio::test]
async fn explicit_send_fixture_preserves_frozen_bytes_consumes_only_target_and_exposes_sent_evidence(
) {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let (_directory, _store, fixture) = setup_mode(true);
    let cancel = CancellationToken::new();
    for index in 0..2 {
        let owner = account(&fixture, index);
        let token = fixture.accounts[index].token();
        let api = api(&fixture, index);
        let draft_raw = b"From: personal@example.test\r\n\r\nOld saved content";
        for _ in 0..2 {
            api.execute(
                GmailCall::create_draft(&owner, draft_raw, None).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap();
        }
        for (number, target) in [(1, Some("vd0001")), (2, None)] {
            let rfc_id = format!("explicit-{index}-{number}@example.test");
            let raw = format!("From: {}\r\nTo: recipient@example.test\r\nBcc: private@example.test\r\nSubject: Explicit generated send\r\nMessage-ID: <{rfc_id}>\r\n\r\nExact submitted version {index}/{number}\r\n",fixture.accounts[index].email);
            let call = match target {
                Some(id) => {
                    GmailCall::send_draft(&owner, id, raw.as_bytes(), Some("frozen-thread"))
                        .unwrap()
                }
                None => GmailCall::send_message(&owner, raw.as_bytes(), None).unwrap(),
            };
            let GmailReply::Submission(submission) =
                api.execute(call, &token, &cancel).await.unwrap()
            else {
                panic!("submission reply")
            };
            assert!(matches!(
                submission.result,
                fm_google::send::SubmissionResult::Accepted { .. }
            ));
            let query = format!("in:sent rfc822msgid:{rfc_id}");
            let GmailReply::Messages(matches) = api
                .execute(
                    GmailCall::list_messages(&owner, None, Some(&query), false, 10).unwrap(),
                    &token,
                    &cancel,
                )
                .await
                .unwrap()
            else {
                panic!("Sent search")
            };
            assert_eq!(matches.messages.len(), 1);
            let id = &matches.messages[0].id;
            assert_eq!(id, &format!("vms{number:04}"));
            let GmailReply::Raw(RawOutcome::Available(read)) = api
                .execute(GmailCall::raw_message(&owner, id).unwrap(), &token, &cancel)
                .await
                .unwrap()
            else {
                panic!("Sent RAW")
            };
            assert_eq!(
                URL_SAFE_NO_PAD.decode(&read.message.raw).unwrap(),
                raw.as_bytes()
            );
            let GmailReply::MessageState(Some(read)) = api
                .execute(
                    GmailCall::message_state(&owner, id).unwrap(),
                    &token,
                    &cancel,
                )
                .await
                .unwrap()
            else {
                panic!("Sent metadata")
            };
            assert_eq!(read.id, *id);
            assert_eq!(read.label_ids, vec!["SENT"]);
            assert_eq!(read.thread_id, matches.messages[0].thread_id);
            assert_eq!(read.history_id, (100 + number).to_string());
        }
        assert!(api
            .execute(
                GmailCall::get_draft(&owner, "vd0001").unwrap(),
                &token,
                &cancel
            )
            .await
            .is_err());
        let GmailReply::Draft(retained) = api
            .execute(
                GmailCall::get_draft(&owner, "vd0002").unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("unrelated draft")
        };
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(retained.message.raw.unwrap())
                .unwrap(),
            draft_raw
        );
        let GmailReply::Draft(created) = api
            .execute(
                GmailCall::create_draft(&owner, draft_raw, None).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("new unrelated draft")
        };
        assert_eq!(created.id, "vd0003");
        let GmailReply::History(history) = api
            .execute(
                GmailCall::history(&owner, "100", None, 100).unwrap(),
                &token,
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("Sent history")
        };
        assert_eq!(history.history_id, "102");
        let report = fixture.report(false);
        let publications = report["send_fixture"]["publications"].as_array().unwrap();
        assert_eq!(publications.last().unwrap()["account"], owner.as_str());
        assert_eq!(publications.last().unwrap()["kind"], "message_send");
    }
    let report = fixture.report(false);
    assert_eq!(report["counters"]["send_completed"], 4);
    assert_eq!(
        report["send_fixture"]["publications"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(report["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn explicit_send_fixture_has_a_separate_opt_in_and_per_account_retention_bound() {
    let (_directory, _store, disabled) = setup();
    let cancel = CancellationToken::new();
    let raw=b"From: personal@example.test\r\nTo: recipient@example.test\r\nMessage-ID: <bounded@example.test>\r\n\r\nGenerated only";
    let _ = api(&disabled, 0)
        .execute(
            GmailCall::send_message(&account(&disabled, 0), raw, None).unwrap(),
            &disabled.accounts[0].token(),
            &cancel,
        )
        .await;
    assert_eq!(disabled.report(false)["counters"]["send_completed"], 0);
    assert_eq!(disabled.report(false)["counters"]["http_rejected"], 1);
    let (_directory, _store, fixture) = setup_mode(true);
    let owner = account(&fixture, 0);
    let token = fixture.accounts[0].token();
    let api = api(&fixture, 0);
    for _ in 0..16 {
        api.execute(
            GmailCall::send_message(&owner, raw, None).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap();
    }
    // The adapter deliberately maps transport uncertainty into a submission
    // result, so inspect the fixture counter rather than expecting an Err.
    let _ = api
        .execute(
            GmailCall::send_message(&owner, raw, None).unwrap(),
            &token,
            &cancel,
        )
        .await;
    assert_eq!(fixture.report(false)["counters"]["send_completed"], 16);
    assert_eq!(
        fixture.report(false)["send_fixture"]["publications"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    let GmailReply::Messages(matches) = api
        .execute(
            GmailCall::list_messages(
                &owner,
                None,
                Some("in:sent rfc822msgid:bounded@example.test"),
                false,
                10,
            )
            .unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("bounded Sent")
    };
    assert_eq!(matches.messages.len(), 10);
    assert!(matches.next_page_token.is_some());
}

#[tokio::test]
async fn actual_gmail_adapter_routes_keep_account_identity_pages_and_review_readback() {
    let (_directory, _store, fixture) = setup();
    let cancel = CancellationToken::new();
    let first = account(&fixture, 0);
    let token = fixture.accounts[0].token();
    let api = api(&fixture, 0);
    let GmailReply::Profile(profile) = api
        .execute(GmailCall::profile(&first).unwrap(), &token, &cancel)
        .await
        .unwrap()
    else {
        panic!("profile reply");
    };
    assert_eq!(profile.email_address, "personal@example.test");
    assert_eq!(profile.messages_total, 4156);
    let GmailReply::Messages(recent) = api
        .execute(
            GmailCall::list_messages(&first, None, Some("after:100"), true, 100).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("recent list");
    };
    assert_eq!(recent.messages.len(), 60);
    assert_eq!(recent.messages[0].id, "vr0000");
    assert_eq!(recent.messages[0].thread_id, recent.messages[1].thread_id);
    assert!(recent.next_page_token.is_none());
    let GmailReply::Messages(initial) = api
        .execute(
            GmailCall::list_messages(&first, None, None, true, 100).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("initial inventory");
    };
    assert_eq!(initial.messages.len(), 100);
    assert_eq!(initial.next_page_token.as_deref(), Some("inventory-100"));
    let GmailReply::Messages(next) = api
        .execute(
            GmailCall::list_messages(&first, initial.next_page_token.as_deref(), None, true, 100)
                .unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("next inventory");
    };
    assert_eq!(next.messages[0].id, "vb0040");
    let mut bodies = Vec::new();
    for index in 0..2 {
        let GmailReply::Raw(RawOutcome::Available(raw)) = super::tests::api(&fixture, index)
            .execute(
                GmailCall::raw_message(&account(&fixture, index), "vr0001").unwrap(),
                &fixture.accounts[index].token(),
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("RAW reply");
        };
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &raw.message.raw,
        )
        .unwrap();
        let source = String::from_utf8(bytes).unwrap();
        assert!(source.contains(&format!(
            "To: {}@example.test\r\n",
            fixture.accounts[index].key
        )));
        assert!(source.contains("\r\nSubject: Verification "));
        assert!(source.contains(
            "recent/0001. This fixed source exists only in the disposable acquisition fixture."
        ));
        assert_eq!(raw.message.id, "vr0001");
        assert_eq!(raw.message.thread_id, "tr0000");
        bodies.push(source);
    }
    assert_ne!(bodies[0], bodies[1]);
    api.execute(
        GmailCall::modify_message(&first, "vr0001", &[], &["UNREAD".into()]).unwrap(),
        &token,
        &cancel,
    )
    .await
    .unwrap();
    let GmailReply::MessageState(Some(reviewed)) = api
        .execute(
            GmailCall::message_state(&first, "vr0001").unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("state");
    };
    assert_eq!(reviewed.label_ids, vec!["INBOX"]);
    assert_eq!(reviewed.history_id, "101");
    let GmailReply::MessageState(Some(other)) = super::tests::api(&fixture, 1)
        .execute(
            GmailCall::message_state(&account(&fixture, 1), "vr0001").unwrap(),
            &fixture.accounts[1].token(),
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("other state");
    };
    assert!(other.label_ids.contains(&"UNREAD".into()));
    let GmailReply::History(history) = api
        .execute(
            GmailCall::history(&first, "100", None, 100).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("history");
    };
    assert_eq!(history.history.len(), 1);
    assert_eq!(history.history[0].labels_removed[0].message.id, "vr0001");
    assert!(fixture.evidence.lock().unwrap().errors.is_empty());
}

#[tokio::test]
async fn unknown_tokens_oauth_origins_and_unsupported_writes_fail_without_fallback() {
    let (_directory, _store, fixture) = setup();
    for (method,url,token,body) in [
        (HttpMethod::Get,"https://gmail.googleapis.com/gmail/v1/users/me/profile","unknown",None),
        (HttpMethod::Post,"https://oauth2.googleapis.com/token","known",Some(json!({}))),
        (HttpMethod::Post,"https://gmail.googleapis.com/gmail/v1/users/me/messages/vr0001/modify","known",Some(json!({"addLabelIds":["STARRED"],"removeLabelIds":[]}))),
        (HttpMethod::Get,"https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults=100&includeSpamTrash=true&pageToken=inventory-999999","known",None),
    ] {
        let token = if token=="known" {fixture.accounts[0].token()} else {token.into()};
        let mut request=HttpRequest::new(method,url).unwrap().header("authorization",&format!("Bearer {token}")).unwrap();
        if let Some(body)=body {request=request.body(serde_json::to_vec(&body).unwrap(),"application/json").unwrap();}
        assert!(fixture.http.execute(request,&CancellationToken::new()).await.is_err());
    }
    let report = fixture.report(false);
    assert_eq!(report["counters"]["http_rejected"], 4);
    assert_eq!(report["counters"]["http_completed"], 0);
    assert_eq!(report["counters"]["modify_completed"], 0);
    assert_eq!(report["transport"], "in_memory_only");
    assert!(!report.to_string().contains("Bearer"));
}

#[tokio::test]
async fn held_raw_cancellation_balances_activity_without_success_or_publication() {
    let (_directory, _store, fixture) = setup();
    fixture.hold_raw.store(true, Ordering::Release);
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task_fixture = fixture.clone();
    let task = tokio::spawn(async move {
        api(&task_fixture, 0)
            .execute(
                GmailCall::raw_message(&account(&task_fixture, 0), "vb0000").unwrap(),
                &task_fixture.accounts[0].token(),
                &task_cancel,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.evidence.lock().unwrap().counters.raw_held_active == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    assert!(task.await.unwrap().is_err());
    let report = fixture.report(false);
    assert_eq!(report["counters"]["raw_cancelled"], 1);
    assert_eq!(report["counters"]["http_cancelled"], 1);
    assert_eq!(report["counters"]["raw_active"], 0);
    assert_eq!(report["counters"]["raw_held_active"], 0);
    assert_eq!(report["counters"]["raw_completed"], 0);
    assert_eq!(report["publication_count"], 0);
    assert!(fixture.evidence.lock().unwrap().errors.is_empty());
}

#[tokio::test]
async fn actual_runtime_uses_cached_tokens_and_traces_only_committed_writer_parts() {
    for reject_commit in [false, true] {
        let (_directory, mut store, fixture) = setup();
        if reject_commit {
            let db = rusqlite::Connection::open(fixture.profile.join("store.sqlite3")).unwrap();
            db.execute_batch("CREATE TRIGGER refuse_verified_part BEFORE UPDATE OF applied_revision ON google_live_page_items WHEN NEW.applied_revision IS NOT NULL BEGIN SELECT RAISE(ABORT,'fixture commit refusal'); END").unwrap();
        }
        let (send, mut receive) = tokio::sync::mpsc::channel(32);
        let writer_fixture = fixture.clone();
        let writer = std::thread::spawn(move || {
            let _trace = writer_fixture.install_writer_trace();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("provider work only");
                }
            }
        });
        let core = CoreBridge::new(send, Arc::new(tokio::sync::Semaphore::new(16)));
        let cancel = CancellationToken::new();
        let auth = fixture.auth(core.clone(), cancel.clone());
        assert_eq!(
            auth.handle(
                "owner",
                CancellationToken::new(),
                "google_auth_begin",
                None,
                json!({})
            )
            .await
            .unwrap_err()
            .code,
            "verification_refused"
        );
        let selected = fixture.accounts[0].id.clone();
        assert_eq!(
            auth.access_token(&selected, &cancel).await.unwrap(),
            fixture.accounts[0].token()
        );
        let runtime = crate::google_sync::Runtime::new(core.clone(), auth.clone(), cancel.clone());
        runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_sync_start",
                Some(&selected),
                limits(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let status = runtime
                    .handle(
                        "owner",
                        CancellationToken::new(),
                        "google_sync_status",
                        Some(&selected),
                        json!({}),
                    )
                    .await
                    .unwrap();
                if reject_commit {
                    if status["accounts"][0]["progress"]["mail"] == "paused" {
                        break;
                    }
                } else if !fixture.evidence.lock().unwrap().publications.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        runtime.shutdown().await;
        drop(runtime);
        auth.shutdown().await;
        drop(auth);
        drop(core);
        writer.join().unwrap();
        fixture.finish(true).await.unwrap();
        let report = fixture.report(true);
        assert!(report["counters"]["vault_loads"].as_u64().unwrap() > 0);
        assert_eq!(report["counters"]["vault_stores"], 0);
        let db = rusqlite::Connection::open(fixture.profile.join("store.sqlite3")).unwrap();
        let mut statement=db.prepare("SELECT account,part_key,applied_revision,count(*) FROM google_live_page_items WHERE applied_revision IS NOT NULL GROUP BY account,part_key,applied_revision ORDER BY part_key").unwrap();
        let receipts = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u64>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut observed = report["publications"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["account"].as_str().unwrap().to_owned(),
                    row["receipt_key"].as_str().unwrap().to_owned(),
                    row["revision"].as_i64().unwrap(),
                    row["item_count"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        observed.sort_by(|left, right| left.1.cmp(&right.1));
        assert_eq!(observed, receipts);
        let count: i64 = db
            .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        if reject_commit {
            assert!(observed.is_empty());
            assert_eq!(count, 0);
        } else {
            assert!(!observed.is_empty());
            assert!(count >= 10);
        }
    }
}
