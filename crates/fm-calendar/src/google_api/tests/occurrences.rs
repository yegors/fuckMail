use super::*;
use crate::EventTime;

fn instance(id: &str, original: Value) -> Value {
    json!({"id":id,"recurringEventId":"master001","originalStartTime":original,
        "etag":"\"instance-1\"","status":"confirmed","summary":"Moved occurrence",
        "start":{"dateTime":"2026-12-12T10:00:00Z","timeZone":"UTC"},
        "futureMetadata":{"preserved":[1,2,3]}})
}

#[tokio::test]
async fn instances_terminal_and_empty_continuation_preserve_identity_without_actual_time_filters() {
    let source = instance(
        "actual-provider-id",
        json!({"dateTime":"2026-09-12T10:00:00-04:00","timeZone":"America/Toronto"}),
    );
    let http = FakeHttp::new(vec![
        (200, json!({"items":[],"nextPageToken":"second"})),
        (200, json!({"items":[source]})),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let original = EventTime::Zoned {
        instant: "2026-09-12T14:00:00Z".parse().unwrap(),
        time_zone: "UTC".into(),
    };
    let first = api
        .instances(
            "synthetic-access",
            "calendar@example.test",
            "master001",
            Some(&original),
            None,
            &cancel,
        )
        .await
        .unwrap();
    assert!(first.items.is_empty());
    assert_eq!(first.next_page_token.as_deref(), Some("second"));
    let second = api
        .instances(
            "synthetic-access",
            "calendar@example.test",
            "master001",
            Some(&original),
            first.next_page_token.as_deref(),
            &cancel,
        )
        .await
        .unwrap();
    assert!(second.next_page_token.is_none());
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].key.event, "actual-provider-id");
    assert_eq!(
        second.items[0].original_start.identity(),
        original.identity()
    );
    for request in http.requests.lock().unwrap().iter() {
        assert_eq!(request.method(), HttpMethod::Get);
        let q = query(request);
        assert_eq!(q["maxResults"], "250");
        assert_eq!(q["showDeleted"], "true");
        assert_eq!(q["originalStart"], "2026-09-12T14:00:00+00:00");
        assert!(q.keys().all(|k| matches!(
            k.as_str(),
            "maxResults" | "showDeleted" | "originalStart" | "pageToken"
        )));
    }
}

#[tokio::test]
async fn instances_refuse_repeated_tokens_malformed_identity_and_unbounded_pages() {
    let valid = instance("instance001", json!({"date":"2026-09-12"}));
    let mut wrong_parent = valid.clone();
    wrong_parent["recurringEventId"] = "another-series".into();
    let mut malformed_status = valid.clone();
    malformed_status["status"] = true.into();
    let mut malformed_zone = valid.clone();
    malformed_zone["originalStartTime"] = json!({"dateTime":"2026-09-12T14:00:00Z","timeZone":42});
    for page in [
        json!({"items":[],"nextPageToken":"same"}),
        json!({"items":[],"nextPageToken":"next","nextSyncToken":"sync"}),
        json!({"items":vec![valid;251]}),
        json!({"items":[wrong_parent]}),
        json!({"items":[malformed_status]}),
        json!({"items":[malformed_zone]}),
    ] {
        let api = api(FakeHttp::new(vec![(200, page)]));
        assert!(matches!(
            api.instances(
                "synthetic-access",
                "cal",
                "master001",
                None,
                Some("same"),
                &CancellationToken::new()
            )
            .await,
            Err(ApiError::InvalidResponse)
        ));
    }
    let http = FakeHttp::new(vec![]);
    let api = api(http.clone());
    let original = EventTime::Date {
        date: "2026-09-12".parse().unwrap(),
    };
    assert!(matches!(
        api.instances(
            "synthetic-access",
            "cal",
            "master001",
            Some(&original),
            None,
            &CancellationToken::new()
        )
        .await,
        Err(ApiError::InvalidRequest)
    ));
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn occurrence_get_retains_full_moved_source_and_sparse_cancellation_without_fabricating_etag()
{
    let full = instance("instance001", json!({"date":"2026-09-12"}));
    let sparse = json!({"id":"instance001","recurringEventId":"master001","originalStartTime":{"date":"2026-09-12"},"status":"cancelled"});
    let http = FakeHttp::new(vec![
        (200, full.clone()),
        (200, sparse.clone()),
        (200, sparse.clone()),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let moved = api
        .get_occurrence(
            "synthetic-access",
            "cal",
            "master001",
            "instance001",
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(moved.source, full);
    let cancelled = api
        .get_occurrence(
            "synthetic-access",
            "cal",
            "master001",
            "instance001",
            &cancel,
        )
        .await
        .unwrap();
    assert!(cancelled.identity.cancelled);
    assert!(cancelled.identity.etag.is_none());
    assert_eq!(cancelled.source, sparse);
    assert!(matches!(
        api.get_event("synthetic-access", "cal", "instance001", None, &cancel)
            .await,
        Err(ApiError::InvalidResponse)
    ));
    assert!(http
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|r| query(r).is_empty()));
}

#[tokio::test]
async fn occurrence_get_refuses_wrong_target_or_parent_and_non_cancelled_etag_absence() {
    let mut no_etag = instance("instance001", json!({"date":"2026-09-12"}));
    no_etag.as_object_mut().unwrap().remove("etag");
    for (source, parent, id) in [
        (no_etag, "master001", "instance001"),
        (
            instance("other", json!({"date":"2026-09-12"})),
            "master001",
            "instance001",
        ),
        (
            instance("instance001", json!({"date":"2026-09-12"})),
            "other",
            "instance001",
        ),
    ] {
        let api = api(FakeHttp::new(vec![(200, source)]));
        assert!(matches!(
            api.get_occurrence(
                "synthetic-access",
                "cal",
                parent,
                id,
                &CancellationToken::new()
            )
            .await,
            Err(ApiError::InvalidResponse)
        ));
    }
}
