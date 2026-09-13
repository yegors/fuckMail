use super::*;
use std::{collections::VecDeque, sync::Mutex};

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<HttpRequest>>,
    replies: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
}
impl Fake {
    fn respond(&self, status: u16, body: Value) {
        self.replies.lock().unwrap().push_back(Ok(HttpResponse {
            status,
            body: serde_json::to_vec(&body).unwrap(),
            retry_after_secs: Some(17),
            etag: None,
        }));
    }
    fn fail(&self, error: TransportError) {
        self.replies.lock().unwrap().push_back(Err(error));
    }
}
impl HttpTransport for Fake {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        _: &'a CancellationToken,
    ) -> crate::transport::HttpFuture<'a> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(request);
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected HTTP call")
        })
    }
}
fn account(id: &str) -> AccountId {
    AccountId::new(id).unwrap()
}
fn setup() -> (Arc<Fake>, GmailApi<Fake>) {
    let fake = Arc::new(Fake::default());
    let api = GmailApi::new(account("a"), fake.clone());
    (fake, api)
}
fn raw(id: &str, encoded: &str) -> Value {
    json!({"id":id,"threadId":"thread-a","historyId":"123","raw":encoded,"labelIds":["INBOX","UNREAD"],"internalDate":"1000","sizeEstimate":24})
}

#[test]
fn endpoint_inventory_has_documented_costs_and_fixed_mutation_classes() {
    let a = account("a");
    let mime = b"From: test@example.test\r\n\r\nHuman text";
    let calls = [
        (
            GmailCall::profile(&a).unwrap(),
            HttpMethod::Get,
            "/profile",
            1,
            WorkClass::UserRead,
        ),
        (
            GmailCall::list_messages(&a, None, None, true, 500).unwrap(),
            HttpMethod::Get,
            "/messages",
            5,
            WorkClass::UserRead,
        ),
        (
            GmailCall::history(&a, "42", None, 500).unwrap(),
            HttpMethod::Get,
            "/history",
            2,
            WorkClass::DeltaRead,
        ),
        (
            GmailCall::raw_message(&a, "m").unwrap(),
            HttpMethod::Get,
            "/messages/m",
            20,
            WorkClass::UserRead,
        ),
        (
            GmailCall::labels(&a).unwrap(),
            HttpMethod::Get,
            "/labels",
            1,
            WorkClass::UserRead,
        ),
        (
            GmailCall::modify_message(&a, "m", &["STARRED".into()], &["UNREAD".into()]).unwrap(),
            HttpMethod::Post,
            "/messages/m/modify",
            5,
            WorkClass::StateMutation,
        ),
        (
            GmailCall::trash_message(&a, "m", false).unwrap(),
            HttpMethod::Post,
            "/messages/m/trash",
            20,
            WorkClass::StateMutation,
        ),
        (
            GmailCall::trash_message(&a, "m", true).unwrap(),
            HttpMethod::Post,
            "/messages/m/untrash",
            5,
            WorkClass::StateMutation,
        ),
        (
            GmailCall::list_drafts(&a, None, 100).unwrap(),
            HttpMethod::Get,
            "/drafts",
            5,
            WorkClass::UserRead,
        ),
        (
            GmailCall::get_draft(&a, "d").unwrap(),
            HttpMethod::Get,
            "/drafts/d",
            20,
            WorkClass::UserRead,
        ),
        (
            GmailCall::create_draft(&a, mime, None).unwrap(),
            HttpMethod::Post,
            "/drafts",
            10,
            WorkClass::DraftMutation,
        ),
        (
            GmailCall::update_draft(&a, "d", mime, None).unwrap(),
            HttpMethod::Put,
            "/drafts/d",
            15,
            WorkClass::DraftMutation,
        ),
        (
            GmailCall::delete_draft(&a, "d").unwrap(),
            HttpMethod::Delete,
            "/drafts/d",
            10,
            WorkClass::DraftMutation,
        ),
        (
            GmailCall::send_message(&a, mime, None).unwrap(),
            HttpMethod::Post,
            "/messages/send",
            100,
            WorkClass::Submission,
        ),
        (
            GmailCall::send_draft(&a, "d", mime, None).unwrap(),
            HttpMethod::Post,
            "/drafts/send",
            100,
            WorkClass::Submission,
        ),
    ];
    for (call, verb, path, units, class) in calls {
        assert_eq!(call.request.method(), verb);
        let url = Url::parse(call.request.url()).unwrap();
        assert_eq!(url.path(), format!("/gmail/v1/users/me{path}"));
        let work = call.work();
        assert_eq!(
            (work.account, work.service, work.units, work.class),
            (a.clone(), Service::Gmail, units, class)
        );
        if !class.is_read() {
            assert!(call.with_read_class(WorkClass::BackfillRead).is_err());
        }
    }
}

#[test]
fn ids_and_queries_are_encoded_and_boundaries_reject_invalid_calls() {
    let a = account("a");
    let call = GmailCall::raw_message(&a, "id/with?query#fragment").unwrap();
    assert!(call.request.url().contains("id%2Fwith%3Fquery%23fragment"));
    let call = GmailCall::list_messages(
        &a,
        Some("opaque&maxResults=999"),
        Some("in:sent rfc822msgid:<private@example.test>"),
        true,
        200,
    )
    .unwrap();
    let url = Url::parse(call.request.url()).unwrap();
    let pairs: Vec<_> = url.query_pairs().collect();
    assert_eq!(
        pairs.iter().filter(|(key, _)| key == "maxResults").count(),
        1
    );
    assert!(pairs.contains(&("pageToken".into(), "opaque&maxResults=999".into())));
    assert!(!format!("{call:?}").contains("private"));
    assert!(GmailCall::raw_message(&a, "..").is_err());
    assert!(GmailCall::list_messages(&a, None, None, true, 501).is_err());
    assert!(GmailCall::history(&a, "not-a-cursor", None, 100).is_err());
    assert!(GmailCall::modify_message(&a, "m", &["UNREAD".into()], &["UNREAD".into()]).is_err());
    assert!(GmailCall::send_message(&a, b"", None).is_err());
}

#[tokio::test]
async fn account_mismatch_never_calls_transport_and_bearer_stays_out_of_url_debug() {
    let (fake, api) = setup();
    assert_eq!(
        api.execute(
            GmailCall::profile(&account("b")).unwrap(),
            "token-a",
            &CancellationToken::new()
        )
        .await
        .unwrap_err(),
        GoogleError::PermissionDenied
    );
    assert!(fake.calls.lock().unwrap().is_empty());
    fake.respond(200,json!({"emailAddress":"private@example.test","messagesTotal":5,"threadsTotal":3,"historyId":"100"}));
    let response = api
        .execute(
            GmailCall::profile(&account("a")).unwrap(),
            "private-token",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let GmailReply::Profile(profile) = response else {
        panic!("profile reply");
    };
    assert_eq!(profile.history_id, "100");
    let calls = fake.calls.lock().unwrap();
    let call = &calls[0];
    assert_eq!(
        call.headers().get("authorization").unwrap(),
        "Bearer private-token"
    );
    assert!(
        !call.url().contains("private-token") && !format!("{call:?}").contains("private-token")
    );
}

#[tokio::test]
async fn pages_preserve_opaque_tokens_estimates_and_history_checkpoint_boundary() {
    let (fake, api) = setup();
    let a = account("a");
    fake.respond(200,json!({"messages":[{"id":"m","threadId":"t"}],"nextPageToken":"next/+&","resultSizeEstimate":999}));
    let GmailReply::Messages(page) = api
        .execute(
            GmailCall::list_messages(&a, None, None, true, 500).unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("messages reply");
    };
    assert_eq!(page.next_page_token.as_deref(), Some("next/+&"));
    assert_eq!(page.result_size_estimate, Some(999));
    let checkpoint = crate::sync::Checkpoint::begin_full(a.clone(), 1, "42".into()).unwrap();
    let plan = crate::sync::prepare_full_page(&checkpoint, page).unwrap();
    assert!(!plan.inventory_finished && plan.next.committed_history_id.is_none());
    fake.respond(200, json!({"messages":[],"nextPageToken":"repeat"}));
    assert!(api
        .execute(
            GmailCall::list_messages(&a, Some("repeat"), None, true, 100).unwrap(),
            "token",
            &CancellationToken::new()
        )
        .await
        .is_err());
    fake.respond(404, json!({"error":{"message":"private details"}}));
    assert_eq!(
        api.execute(
            GmailCall::history(&a, "42", None, 100).unwrap(),
            "token",
            &CancellationToken::new()
        )
        .await
        .unwrap_err(),
        GoogleError::HistoryExpired
    );
    fake.respond(404, json!({}));
    assert_eq!(
        api.execute(
            GmailCall::get_draft(&a, "d").unwrap(),
            "token",
            &CancellationToken::new()
        )
        .await
        .unwrap_err(),
        GoogleError::NotFound
    );
}

#[tokio::test]
async fn raw_outcomes_isolate_missing_oversized_malformed_and_valid_source() {
    let (fake, api) = setup();
    let a = account("a");
    fake.respond(404, json!({}));
    fake.fail(TransportError::ResponseTooLarge);
    fake.respond(200, raw("wrong", "YWJj"));
    fake.respond(200, raw("m", "not*base64"));
    let source = b"From: test@example.test\r\n\r\nPreserved bytes\r\n";
    fake.respond(200, raw("m", &URL_SAFE_NO_PAD.encode(source)));
    for index in 0..5 {
        let GmailReply::Raw(outcome) = api
            .execute(
                GmailCall::raw_message(&a, "m").unwrap(),
                "token",
                &CancellationToken::new(),
            )
            .await
            .unwrap()
        else {
            panic!("raw reply");
        };
        match (index, outcome) {
            (0, RawOutcome::Missing)
            | (1, RawOutcome::TooLarge)
            | (2 | 3, RawOutcome::Malformed) => (),
            (4, RawOutcome::Available(item)) => {
                assert_eq!(item.internal_date.as_deref(), Some("1000"));
                let decoded = crate::sync::decode_raw_message(item.message, "m").unwrap();
                assert_eq!(decoded.source_mime, source);
            }
            _ => panic!("incorrect RAW outcome"),
        }
    }
    assert_eq!(fake.calls.lock().unwrap().len(), 5);
}

#[tokio::test]
async fn state_mutation_targets_one_message_and_checks_returned_identity() {
    let (fake, api) = setup();
    let a = account("a");
    fake.respond(200, json!({"id":"m-before-new-arrival","threadId":"t"}));
    api.execute(
        GmailCall::modify_message(
            &a,
            "m-before-new-arrival",
            &[],
            &["UNREAD".into(), "STARRED".into()],
        )
        .unwrap(),
        "token",
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    {
        let calls = fake.calls.lock().unwrap();
        let body: Value = serde_json::from_slice(calls[0].body_bytes()).unwrap();
        assert_eq!(
            body,
            json!({"addLabelIds":[],"removeLabelIds":["UNREAD","STARRED"]})
        );
        assert!(!calls[0].url().contains("threads"));
    }
    fake.respond(200, json!({"id":"new-arrival","threadId":"t"}));
    assert!(api
        .execute(
            GmailCall::trash_message(&a, "m-before-new-arrival", false).unwrap(),
            "token",
            &CancellationToken::new()
        )
        .await
        .is_err());
}

#[tokio::test]
async fn draft_container_identity_survives_message_replacement_and_frozen_send() {
    let (fake, api) = setup();
    let a = account("a");
    let mime = b"Subject: Human subject\r\n\r\nFinal authored bytes";
    fake.respond(
        200,
        json!({"id":"draft-stable","message":{"id":"content-before","threadId":"t"}}),
    );
    let GmailReply::Draft(created) = api
        .execute(
            GmailCall::create_draft(&a, mime, Some("t")).unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("draft");
    };
    fake.respond(
        200,
        json!({"id":"draft-stable","message":{"id":"content-after","threadId":"t"}}),
    );
    let GmailReply::Draft(updated) = api
        .execute(
            GmailCall::update_draft(&a, &created.id, mime, Some("t")).unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("draft");
    };
    assert_eq!(created.id, updated.id);
    assert_ne!(created.message.id, updated.message.id);
    fake.respond(200, json!({"id":"sent-new","threadId":"t"}));
    let GmailReply::Submission(GmailSubmission {
        result: SubmissionResult::Accepted { message_id, .. },
        error: None,
    }) = api
        .execute(
            GmailCall::send_draft(&a, &updated.id, mime, Some("t")).unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("accepted");
    };
    assert_eq!(message_id, "sent-new");
    let calls = fake.calls.lock().unwrap();
    let body: Value = serde_json::from_slice(calls[2].body_bytes()).unwrap();
    assert_eq!(body["id"], "draft-stable");
    assert_eq!(body["message"]["threadId"], "t");
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode(body["message"]["raw"].as_str().unwrap())
            .unwrap(),
        mime
    );
    assert!(calls[1].headers().get("if-match").is_none());
}

#[tokio::test]
async fn uncertain_send_is_single_attempt_and_rejection_is_distinct() {
    let (fake, api) = setup();
    let a = account("a");
    fake.fail(TransportError::Timeout);
    fake.fail(TransportError::Network);
    fake.fail(TransportError::Cancelled);
    fake.respond(500, json!({"error":{"message":"private provider text"}}));
    fake.respond(200, json!({"id":"receipt-missing-thread"}));
    fake.respond(403, json!({"error":{"message":"private policy text"}}));
    fake.respond(429, json!({}));
    for index in 0..7 {
        let GmailReply::Submission(outcome) = api
            .execute(
                GmailCall::send_message(&a, b"From: user@example.test\r\n\r\nHuman", None).unwrap(),
                "token",
                &CancellationToken::new(),
            )
            .await
            .unwrap()
        else {
            panic!("submission");
        };
        if index < 5 {
            assert!(matches!(
                outcome.result,
                SubmissionResult::NoDefinitiveResponse
            ));
            let expected = match index {
                0..=2 => GoogleError::TransportInterrupted,
                3 => GoogleError::ServerUnavailable,
                _ => GoogleError::InvalidResponse("invalid Google submission receipt"),
            };
            assert_eq!(outcome.error, Some(expected));
        } else {
            let SubmissionResult::Rejected { reason } = outcome.result else {
                panic!("rejected");
            };
            assert!(!reason.contains("private"));
            assert_eq!(
                outcome.error,
                Some(if index == 5 {
                    GoogleError::PermissionDenied
                } else {
                    GoogleError::RateLimited {
                        retry_after_secs: Some(17),
                    }
                })
            );
        }
        assert_eq!(fake.calls.lock().unwrap().len(), index + 1);
    }
}

#[tokio::test]
async fn rejected_submissions_retain_auth_policy_and_quota_metadata_without_retry() {
    let (fake, api) = setup();
    let a = account("a");
    fake.respond(429, json!({"error":{"message":"private quota diagnostic"}}));
    fake.respond(401, json!({"error":{"message":"private expired grant"}}));
    fake.respond(403, json!({"error":{"message":"private policy diagnostic","errors":[{"reason":"domainPolicy"}]}}));
    fake.respond(403, json!({"error":{"message":"private rate diagnostic","errors":[{"reason":"userRateLimitExceeded"}]}}));
    for (index, expected) in [
        GoogleError::RateLimited {
            retry_after_secs: Some(17),
        },
        GoogleError::ReauthorizationRequired,
        GoogleError::PermissionDenied,
        GoogleError::RateLimited {
            retry_after_secs: Some(17),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let GmailReply::Submission(outcome) = api
            .execute(
                GmailCall::send_message(&a, b"From: user@example.test\r\n\r\nHuman", None).unwrap(),
                "token",
                &CancellationToken::new(),
            )
            .await
            .unwrap()
        else {
            panic!("submission");
        };
        assert!(matches!(outcome.result, SubmissionResult::Rejected { .. }));
        assert_eq!(outcome.error, Some(expected.clone()));
        assert!(!format!("{:?}", outcome.error).contains("private"));
        assert_eq!(
            crate::scheduler::retry_delay(WorkClass::Submission, &expected, 0, 0),
            None
        );
        assert_eq!(fake.calls.lock().unwrap().len(), index + 1);
    }
}

#[tokio::test]
async fn api_quota_and_permission_errors_remain_sanitized_and_do_not_retry() {
    let (fake, api) = setup();
    let a = account("a");
    fake.respond(403,json!({"error":{"message":"private quota text","errors":[{"reason":"userRateLimitExceeded"}]}}));
    fake.respond(
        403,
        json!({"error":{"message":"private policy text","errors":[{"reason":"domainPolicy"}]}}),
    );
    fake.respond(401, json!({}));
    for expected in [
        GoogleError::RateLimited {
            retry_after_secs: Some(17),
        },
        GoogleError::PermissionDenied,
        GoogleError::ReauthorizationRequired,
    ] {
        let error = api
            .execute(
                GmailCall::labels(&a).unwrap(),
                "token",
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(!format!("{error:?}").contains("private"));
    }
    assert_eq!(fake.calls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn labels_and_remote_drafts_keep_resource_ids_and_full_raw() {
    let (fake, api) = setup();
    let a = account("a");
    fake.respond(200, json!({"labels":[{"id":"Label_1","name":"Human category","type":"user","messageListVisibility":"show"}]}));
    let GmailReply::Labels(labels) = api
        .execute(
            GmailCall::labels(&a).unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("labels");
    };
    assert_eq!(labels.labels[0].id, "Label_1");
    assert_eq!(labels.labels[0].name, "Human category");
    fake.respond(200, json!({"drafts":[{"id":"draft-1","message":{"id":"message-1","threadId":"thread-1"}}],"nextPageToken":"draft-page"}));
    let GmailReply::Drafts(page) = api
        .execute(
            GmailCall::list_drafts(&a, None, 100).unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("draft list");
    };
    assert_eq!(page.drafts[0].id, "draft-1");
    assert_eq!(page.next_page_token.as_deref(), Some("draft-page"));
    let encoded = URL_SAFE_NO_PAD.encode(b"Subject: Durable remote draft\r\n\r\nHuman text");
    fake.respond(
        200,
        json!({"id":"draft-1","message":{"id":"message-2","threadId":"thread-1","raw":encoded}}),
    );
    let GmailReply::Draft(draft) = api
        .execute(
            GmailCall::get_draft(&a, "draft-1").unwrap(),
            "token",
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    else {
        panic!("draft get");
    };
    assert_eq!(draft.message.raw.as_deref(), Some(encoded.as_str()));
    fake.respond(200,json!({"id":"wrong-draft","message":{"id":"message-2","threadId":"thread-1","raw":encoded}}));
    assert!(api
        .execute(
            GmailCall::get_draft(&a, "draft-1").unwrap(),
            "token",
            &CancellationToken::new()
        )
        .await
        .is_err());
    fake.replies.lock().unwrap().push_back(Ok(HttpResponse {
        status: 204,
        body: Vec::new(),
        retry_after_secs: None,
        etag: None,
    }));
    assert!(matches!(
        api.execute(
            GmailCall::delete_draft(&a, "draft-1").unwrap(),
            "token",
            &CancellationToken::new()
        )
        .await
        .unwrap(),
        GmailReply::DeletedDraft
    ));
}
