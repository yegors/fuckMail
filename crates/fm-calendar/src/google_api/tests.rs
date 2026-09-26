//! Calendar API client tests against generated responses; no accounts or network.
use super::*;
use fm_google::transport::HttpFuture;
use std::{collections::VecDeque, sync::Mutex};

mod occurrences;

struct FakeHttp {
    responses: Mutex<VecDeque<std::result::Result<HttpResponse, TransportError>>>,
    requests: Mutex<Vec<HttpRequest>>,
}
impl FakeHttp {
    fn new(values: Vec<(u16, Value)>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(
                values
                    .into_iter()
                    .map(|(status, body)| {
                        Ok(HttpResponse {
                            status,
                            body: serde_json::to_vec(&body).unwrap(),
                            retry_after_secs: Some(17),
                            etag: None,
                        })
                    })
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        })
    }
}
impl HttpTransport for FakeHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            self.requests.lock().unwrap().push(request);
            self.responses.lock().unwrap().pop_front().unwrap()
        })
    }
}
fn api(http: Arc<FakeHttp>) -> GoogleCalendarApi<FakeHttp> {
    GoogleCalendarApi::new(AccountId::new("fixture-account").unwrap(), http)
}
fn query(request: &HttpRequest) -> BTreeMap<String, String> {
    Url::parse(request.url())
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}
fn event() -> Value {
    json!({"id":"event001","etag":"\"event-etag-1\"","summary":"Synthetic event","visibility":"private",
        "start":{"dateTime":"2026-09-12T10:00:00-04:00","timeZone":"America/Toronto"},
        "end":{"dateTime":"2026-09-12T11:00:00-04:00","timeZone":"America/Toronto"},
        "recurrence":["RRULE:FREQ=WEEKLY;COUNT=3"],"conferenceData":{"conferenceId":"synthetic-meet","entryPoints":[{"entryPointType":"video","uri":"https://meet.google.com/synthetic-only"}]},
        "attachments":[{"fileId":"synthetic-attachment","fileUrl":"https://example.invalid/file"}],
        "extendedProperties":{"private":{"synthetic":"retained"}},"unknownFutureProperty":{"nested":[1,2,3]}})
}
fn event_page(items: Vec<Value>, token_key: &str, token: &str) -> Value {
    let mut page = json!({"items":items,"timeZone":"America/Toronto","accessRole":"writer","etag":"\"list-etag\"","defaultReminders":[{"method":"popup","minutes":10}]});
    page[token_key] = token.into();
    page
}

#[tokio::test]
async fn catalog_pages_keep_hidden_deleted_and_limited_writer_without_acl_assumptions() {
    let http = FakeHttp::new(vec![
        (200, json!({"items":[],"nextPageToken":"catalog-next"})),
        (
            200,
            json!({"items":[{"id":"shared@example.invalid","accessRole":"writerWithoutPrivateAccess","timeZone":"Asia/Tokyo","hidden":true},
            {"id":"gone@example.invalid","deleted":true},{"id":"unknown@example.invalid","accessRole":"future-admin"}],"nextSyncToken":"catalog-final"}),
        ),
        (
            200,
            json!({"id":"shared@example.invalid","accessRole":"reader","timeZone":"Asia/Tokyo","summary":"Updated metadata"}),
        ),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let first = api
        .calendar_list("synthetic-access", SyncCursor::default(), &cancel)
        .await
        .unwrap();
    assert!(first.items.is_empty());
    assert!(first.next_sync_token.is_none());
    let second = api
        .calendar_list(
            "synthetic-access",
            SyncCursor {
                sync_token: Some("catalog-old"),
                page_token: first.next_page_token.as_deref(),
            },
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(second.account.as_str(), "fixture-account");
    assert_eq!(
        second.items[0].role,
        CalendarRole::WriterWithoutPrivateAccess
    );
    assert_eq!(second.items[0].role.event_access(true), Access::FreeBusy);
    assert_eq!(
        second.items[0].role.event_access(false),
        Access::WriterWithoutPrivateAccess
    );
    assert_eq!(second.items[1].role, CalendarRole::None);
    assert!(second.items[1].deleted);
    assert_eq!(second.items[2].role, CalendarRole::None);
    let refreshed = api
        .calendar_entry("synthetic-access", "shared@example.invalid", &cancel)
        .await
        .unwrap();
    assert_eq!(refreshed.role, CalendarRole::Reader);
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests[0].method(), HttpMethod::Get);
    assert!(requests[0]
        .url()
        .starts_with("https://www.googleapis.com/calendar/v3/users/me/calendarList?"));
    for request in &requests[..2] {
        let q = query(request);
        assert_eq!(q["showHidden"], "true");
        assert_eq!(q["showDeleted"], "true");
        assert!(!q.contains_key("minAccessRole"));
    }
    assert_eq!(query(&requests[1])["syncToken"], "catalog-old");
    assert!(!requests[0].url().contains("synthetic-access"));
}

#[tokio::test]
async fn event_sync_keeps_source_and_fixed_full_delta_query_with_empty_intermediate_page() {
    let source = event();
    let http = FakeHttp::new(vec![
        (
            200,
            event_page(vec![source.clone()], "nextPageToken", "event-next"),
        ),
        (200, event_page(vec![], "nextSyncToken", "event-final")),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let first = api
        .events(
            "synthetic-access",
            "shared/+?@example.invalid",
            SyncCursor::default(),
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(first.items[0], source);
    assert_eq!(first.time_zone, "America/Toronto");
    let second = api
        .events(
            "synthetic-access",
            "shared/+?@example.invalid",
            SyncCursor {
                sync_token: Some("previous/sync+?"),
                page_token: first.next_page_token.as_deref(),
            },
            &cancel,
        )
        .await
        .unwrap();
    assert!(second.items.is_empty());
    assert_eq!(second.next_sync_token.as_deref(), Some("event-final"));
    assert_eq!(second.calendar, "shared/+?@example.invalid");
    let requests = http.requests.lock().unwrap();
    let first = query(&requests[0]);
    let mut second = query(&requests[1]);
    assert_eq!(
        second.remove("syncToken").as_deref(),
        Some("previous/sync+?")
    );
    second.remove("pageToken");
    assert_eq!(first, second);
    assert_eq!(first["singleEvents"], "false");
    assert_eq!(first["showDeleted"], "true");
    for forbidden in [
        "timeMin",
        "timeMax",
        "updatedMin",
        "orderBy",
        "fields",
        "maxAttendees",
        "q",
    ] {
        assert!(!first.contains_key(forbidden));
    }
    assert!(requests[0].url().contains("shared%2F+%3F@example.invalid"));
    assert_eq!(
        requests[0].headers()["authorization"],
        "Bearer synthetic-access"
    );
}

#[tokio::test]
async fn page_failures_expiry_policy_quota_and_cancel_are_explicit_and_never_retried() {
    for (status, body, expected) in [
        (410, json!({}), ApiError::SyncTokenExpired),
        (
            403,
            json!({"error":{"message":"must never escape","errors":[{"reason":"forbidden"}]}}),
            ApiError::Provider(GoogleError::PermissionDenied),
        ),
        (
            403,
            json!({"error":{"errors":[{"reason":"userRateLimitExceeded"}]}}),
            ApiError::Provider(GoogleError::RateLimited {
                retry_after_secs: Some(17),
            }),
        ),
        (
            429,
            json!({}),
            ApiError::Provider(GoogleError::RateLimited {
                retry_after_secs: Some(17),
            }),
        ),
        (
            401,
            json!({}),
            ApiError::Provider(GoogleError::ReauthorizationRequired),
        ),
        (
            200,
            json!({"nextPageToken":"same","nextSyncToken":"both"}),
            ApiError::InvalidResponse,
        ),
        (
            200,
            json!({"nextPageToken":"same"}),
            ApiError::InvalidResponse,
        ),
        (
            200,
            json!({"items":"bad","nextSyncToken":"final"}),
            ApiError::InvalidResponse,
        ),
    ] {
        let http = FakeHttp::new(vec![(status, body)]);
        let api = api(http.clone());
        let result = api
            .calendar_list(
                "synthetic-access",
                SyncCursor {
                    sync_token: Some("old"),
                    page_token: Some("same"),
                },
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(result, Err(error) if error == expected));
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        assert!(!format!("{expected:?}").contains("must never escape"));
    }
    let http = FakeHttp::new(vec![]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        api.calendar_list("synthetic-access", SyncCursor::default(), &cancel)
            .await,
        Err(ApiError::Transport(TransportError::Cancelled))
    ));
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn conditional_event_read_validates_identity_and_preserves_etag_and_fields() {
    let original = event();
    let http = FakeHttp::new(vec![
        (304, json!(null)),
        (200, original.clone()),
        (200, json!({"id":"wrong","etag":"wrong"})),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    assert!(matches!(
        api.get_event(
            "synthetic-access",
            "calendar",
            "event001",
            Some("\"event-etag-1\""),
            &cancel
        )
        .await
        .unwrap(),
        EventFetch::NotModified
    ));
    let EventFetch::Found(resource) = api
        .get_event("synthetic-access", "calendar", "event001", None, &cancel)
        .await
        .unwrap()
    else {
        panic!("expected source");
    };
    assert_eq!(resource.source, original);
    assert_eq!(resource.etag, "\"event-etag-1\"");
    assert!(matches!(
        api.get_event("synthetic-access", "calendar", "event001", None, &cancel)
            .await,
        Err(ApiError::InvalidResponse)
    ));
    assert_eq!(
        http.requests.lock().unwrap()[0].headers()["if-none-match"],
        "\"event-etag-1\""
    );
    assert!(response_etag(&event(), Some("\"different\"".into())).is_err());
}

fn window() -> Window {
    Window::from_dates(
        chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
        "UTC",
    )
    .unwrap()
}
#[tokio::test]
async fn free_busy_has_exact_bounded_calendar_coverage_and_no_false_free_time() {
    let window = window();
    let response = json!({"timeMin":window.from.to_rfc3339(),"timeMax":window.to.to_rfc3339(),"calendars":{
        "busy":{"busy":[{"start":"2026-09-12T10:00:00Z","end":"2026-09-12T11:00:00Z"}]},
        "empty":{"busy":[]}, "denied":{"errors":[{"reason":"notFound"}],"busy":[]},
        "unknown":{"errors":[{"reason":"newProviderError"}]}, "invalid":{"busy":[{"start":"bad","end":"bad"}]},
        "unsolicited":{"busy":[]}}});
    let http = FakeHttp::new(vec![(200, response)]);
    let api = api(http.clone());
    let ids: Vec<String> = ["busy", "empty", "denied", "unknown", "invalid", "missing"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let snapshot = api
        .free_busy("synthetic-access", &ids, &window, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(snapshot.calendars.len(), ids.len());
    assert!(!snapshot.calendars.contains_key("unsolicited"));
    assert!(matches!(&snapshot.calendars["busy"], CalendarBusy::Available(v) if v.len() == 1));
    assert!(matches!(&snapshot.calendars["empty"], CalendarBusy::Available(v) if v.is_empty()));
    assert!(
        matches!(&snapshot.calendars["denied"], CalendarBusy::Unavailable(v) if v == &[BusyError::NotFound])
    );
    assert!(
        matches!(&snapshot.calendars["unknown"], CalendarBusy::Unavailable(v) if v == &[BusyError::Unknown])
    );
    assert!(
        matches!(&snapshot.calendars["invalid"], CalendarBusy::Unavailable(v) if v == &[BusyError::InvalidResponse])
    );
    assert!(
        matches!(&snapshot.calendars["missing"], CalendarBusy::Unavailable(v) if v == &[BusyError::Missing])
    );
    {
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests[0].method(), HttpMethod::Post);
        assert!(requests[0].url().ends_with("/freeBusy"));
        let body: Value = serde_json::from_slice(requests[0].body_bytes()).unwrap();
        assert_eq!(body["calendarExpansionMax"], 50);
        assert_eq!(body["items"].as_array().unwrap().len(), ids.len());
    }
    assert!(matches!(
        api.free_busy(
            "synthetic-access",
            &vec!["calendar".into(); 51],
            &window,
            &CancellationToken::new()
        )
        .await,
        Err(ApiError::InvalidRequest)
    ));
    assert!(matches!(
        api.free_busy(
            "synthetic-access",
            &["same".into(), "same".into()],
            &window,
            &CancellationToken::new()
        )
        .await,
        Err(ApiError::InvalidRequest)
    ));
}

fn write() -> GoogleWrite {
    GoogleWrite {
        target: EventKey {
            account: "fixture-account".into(),
            calendar: "calendar".into(),
            event: "event001".into(),
        },
        if_match: "\"event-etag-1\"".into(),
        body: json!({"summary":"Edited synthetic title"}),
        send_updates: "all",
        notification_owner: "google_calendar_api",
    }
}
#[tokio::test]
async fn retained_event_label_requires_version_without_changing_legacy_color_requests() {
    let http = FakeHttp::new(vec![(200, event()), (200, event())]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let mut labeled = write();
    labeled.body["eventLabelId"] = "fixture-label".into();
    labeled.body["colorId"] = "3".into();
    api.apply_write("synthetic-access", &labeled, &cancel)
        .await
        .unwrap();
    let mut legacy = write();
    legacy.body["colorId"] = "3".into();
    api.apply_write("synthetic-access", &legacy, &cancel)
        .await
        .unwrap();
    for invalid in [json!(""), json!(null), json!(1)] {
        labeled.body["eventLabelId"] = invalid;
        assert!(matches!(
            api.apply_write("synthetic-access", &labeled, &cancel).await,
            Err(ApiError::InvalidRequest)
        ));
    }
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let parameters = query(&requests[0]);
    assert_eq!(parameters["eventLabelVersion"], "1");
    assert_eq!(parameters["conferenceDataVersion"], "1");
    assert_eq!(parameters["supportsAttachments"], "true");
    assert_eq!(parameters["sendUpdates"], "all");
    assert!(!query(&requests[1]).contains_key("eventLabelVersion"));
    let body: Value = serde_json::from_slice(requests[0].body_bytes()).unwrap();
    assert_eq!(body["eventLabelId"], "fixture-label");
    assert_eq!(body["colorId"], "3");
    assert!(body.get("conferenceData").is_none());
    assert!(body.get("attachments").is_none());
}
#[tokio::test]
async fn patch_is_conditional_preserves_provider_fields_and_does_not_retry_conflicts_or_disconnect()
{
    let mut updated = event();
    updated["summary"] = "Edited synthetic title".into();
    updated["etag"] = "\"event-etag-2\"".into();
    let http = FakeHttp::new(vec![
        (200, updated.clone()),
        (412, json!({"error":{"message":"private provider detail"}})),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let resource = api
        .apply_write("synthetic-access", &write(), &cancel)
        .await
        .unwrap();
    assert_eq!(resource.source, updated);
    assert!(matches!(
        api.apply_write("synthetic-access", &write(), &cancel).await,
        Err(ApiError::PreconditionFailed)
    ));
    http.responses
        .lock()
        .unwrap()
        .push_back(Err(TransportError::Network));
    assert!(matches!(
        api.apply_write("synthetic-access", &write(), &cancel).await,
        Err(ApiError::Transport(TransportError::Network))
    ));
    {
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].method(), HttpMethod::Patch);
        assert_eq!(requests[0].headers()["if-match"], "\"event-etag-1\"");
        let q = query(&requests[0]);
        assert_eq!(q["conferenceDataVersion"], "1");
        assert_eq!(q["supportsAttachments"], "true");
        assert_eq!(q["sendUpdates"], "all");
        let body: Value = serde_json::from_slice(requests[0].body_bytes()).unwrap();
        assert_eq!(body, write().body);
        assert!(body.get("conferenceData").is_none());
    }
    let mut wrong = write();
    wrong.target.account = "another-account".into();
    assert!(matches!(
        api.apply_write("synthetic-access", &wrong, &cancel).await,
        Err(ApiError::InvalidRequest)
    ));
}

#[tokio::test]
async fn insert_keeps_durable_id_and_delete_keeps_precondition_and_explicit_notifications() {
    let mut created = event();
    created["id"] = "abcde12345".into();
    let http = FakeHttp::new(vec![
        (200, created),
        (409, json!({})),
        (204, json!(null)),
        (404, json!({})),
    ]);
    let api = api(http.clone());
    let cancel = CancellationToken::new();
    let body = json!({"summary":"Synthetic no-attendee canary","start":{"date":"2026-09-12"},"end":{"date":"2026-09-13"}});
    let created = api
        .insert_event(
            "synthetic-access",
            "calendar",
            "abcde12345",
            &body,
            NotificationPolicy::None,
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(created.key.event, "abcde12345");
    assert!(matches!(
        api.insert_event(
            "synthetic-access",
            "calendar",
            "abcde12345",
            &body,
            NotificationPolicy::None,
            &cancel
        )
        .await,
        Err(ApiError::Conflict)
    ));
    api.delete_event(
        "synthetic-access",
        "calendar",
        "abcde12345",
        "\"event-etag-1\"",
        NotificationPolicy::All,
        &cancel,
    )
    .await
    .unwrap();
    assert!(matches!(
        api.delete_event(
            "synthetic-access",
            "calendar",
            "abcde12345",
            "\"event-etag-1\"",
            NotificationPolicy::All,
            &cancel
        )
        .await,
        Err(ApiError::Provider(GoogleError::NotFound))
    ));
    let requests = http.requests.lock().unwrap();
    let posted: Value = serde_json::from_slice(requests[0].body_bytes()).unwrap();
    assert_eq!(posted["id"], "abcde12345");
    assert_eq!(requests[2].method(), HttpMethod::Delete);
    assert_eq!(requests[2].headers()["if-match"], "\"event-etag-1\"");
    assert_eq!(query(&requests[2])["sendUpdates"], "all");
}
