use super::*;
use crate::core_tasks::CoreBridge;
use fm_google::{
    transport::{CancellationToken, HttpFuture, HttpRequest, HttpResponse, HttpTransport},
    AccountId,
};
use serde_json::json;
use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
    sync::Mutex,
    time::Duration,
};
use tokio::sync::{mpsc, Semaphore};

struct Step {
    route: &'static str,
    status: u16,
    body: Value,
}
#[derive(Default)]
struct Script {
    steps: Mutex<VecDeque<Step>>,
    requests: Mutex<Vec<String>>,
}
impl Script {
    fn push(&self, route: &'static str, status: u16, body: Value) {
        self.steps.lock().unwrap().push_back(Step {
            route,
            status,
            body,
        });
    }
    fn consumed(&self) {
        assert!(self.steps.lock().unwrap().is_empty());
    }
}
impl HttpTransport for Script {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            assert_eq!(
                request.headers().get("authorization").unwrap(),
                "Bearer synthetic-token"
            );
            let step = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected provider read");
            assert!(
                request.url().contains(step.route),
                "unexpected synthetic route: {}",
                request.url()
            );
            self.requests.lock().unwrap().push(request.url().to_owned());
            Ok(HttpResponse {
                status: step.status,
                body: serde_json::to_vec(&step.body).unwrap(),
                retry_after_secs: None,
                etag: None,
            })
        })
    }
}
struct Harness {
    context: SyncContext,
    http: Arc<Script>,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new(path: &Path) -> Self {
        let path = path.to_owned();
        let (queue, mut receive) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let bindings = store.google_account_bindings().unwrap();
            let account = if bindings.is_empty() {
                let scopes: BTreeSet<String> = [
                    "openid",
                    "email",
                    "https://www.googleapis.com/auth/calendar.events",
                    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
                    "https://www.googleapis.com/auth/calendar.events.freebusy",
                ]
                .map(str::to_owned)
                .into();
                let account = store
                    .bind_google_account(
                        "calendar-worker-subject",
                        "human@example.test",
                        &scopes,
                        None,
                    )
                    .unwrap();
                store.handle("preferences_save", None, json!({"request_id":"zone", "expected_revision":0, "preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
                account
            } else {
                bindings[0].account.clone()
            };
            let generation = store.google_account_bindings().unwrap()[0].connection_generation;
            ready.send((account, generation)).unwrap();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected Core test work");
                }
            }
        });
        let (account, generation) = wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let http = Arc::new(Script::default());
        let context = SyncContext::for_test(
            CoreBridge::new(queue, Arc::new(Semaphore::new(8))),
            AccountId::new(account).unwrap(),
            generation,
            http.clone(),
            "synthetic-token".into(),
            CancellationToken::new(),
        );
        Self {
            context,
            http,
            writer,
        }
    }
    async fn view(&self) -> Value {
        self.context
            .core_call(|s| {
                s.handle(
                    "calendar_combined",
                    None,
                    json!({"from":"2026-09-12","to":"2026-09-13"}),
                )
            })
            .await
            .unwrap()
    }
    fn close(self) {
        self.http.consumed();
        drop(self.context);
        self.writer.join().unwrap();
    }
}
fn entry(id: &str, role: &str) -> Value {
    json!({"id":id,"summary":id,"timeZone":"UTC","accessRole":role})
}
fn event(id: &str, title: &str) -> Value {
    json!({"id":id,"etag":"synthetic-etag","summary":title,"start":{"dateTime":"2026-09-12T09:00:00Z"},"end":{"dateTime":"2026-09-12T10:00:00Z"}})
}
fn events(items: Vec<Value>, page: Option<&str>, token: Option<&str>) -> Value {
    let mut v = json!({"items":items,"timeZone":"UTC","accessRole":"owner","defaultReminders":[]});
    if let Some(page) = page {
        v["nextPageToken"] = page.into();
    }
    if let Some(token) = token {
        v["nextSyncToken"] = token.into();
    }
    v
}

#[tokio::test]
async fn catalog_pagination_calendar_fairness_and_saved_event_cursor_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path());
    h.http.push(
        "calendarList?",
        200,
        json!({"items":[],"nextPageToken":"catalog-next"}),
    );
    h.http.push(
        "pageToken=catalog-next",
        200,
        json!({"items":[entry("a","owner"),entry("b","owner")],"nextSyncToken":"catalog-final"}),
    );
    h.http.push("calendarList/a", 200, entry("a", "owner"));
    h.http.push(
        "calendars/a/events?",
        200,
        events(
            vec![event("a-first", "Pending calendar")],
            Some("a-next"),
            None,
        ),
    );
    h.http.push("calendarList/b", 200, entry("b", "owner"));
    h.http.push(
        "calendars/b/events?",
        200,
        events(
            vec![event("b-only", "Small calendar ready")],
            None,
            Some("b-final"),
        ),
    );
    let pass = run_pass(&h.context, true).await.unwrap();
    assert!(pass.more);
    assert_eq!(pass.pages, 4);
    assert_eq!(pass.failures, 0);
    let view = h.view().await;
    assert_eq!(view["total"], 1);
    assert!(view.to_string().contains("Small calendar ready"));
    assert!(!view.to_string().contains("Pending calendar"));
    h.close();
    let h = Harness::new(directory.path());
    h.http.push(
        "pageToken=a-next",
        200,
        events(
            vec![event("a-last", "Large calendar finished")],
            None,
            Some("a-final"),
        ),
    );
    let pass = run_pass(&h.context, false).await.unwrap();
    assert!(!pass.more);
    assert_eq!(pass.pages, 1);
    assert_eq!(pass.failures, 0);
    assert_eq!(h.view().await["total"], 3);
    h.close();
}

async fn initial_inventory(h: &Harness) {
    h.http.push(
        "calendarList?",
        200,
        json!({"items":[entry("a","owner")],"nextSyncToken":"catalog-old"}),
    );
    h.http.push("calendarList/a", 200, entry("a", "owner"));
    h.http.push(
        "calendars/a/events?",
        200,
        events(
            vec![event("old", "Retained old event")],
            None,
            Some("events-old"),
        ),
    );
    assert_eq!(run_pass(&h.context, true).await.unwrap().failures, 0);
}

#[tokio::test]
async fn expired_catalog_and_event_tokens_replace_only_after_complete_publication() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path());
    initial_inventory(&h).await;
    h.http.push("syncToken=catalog-old", 410, json!({}));
    h.http.push(
        "calendarList?",
        200,
        json!({"items":[entry("a","owner")],"nextSyncToken":"catalog-new"}),
    );
    h.http.push("calendarList/a", 200, entry("a", "owner"));
    h.http.push("syncToken=events-old", 410, json!({}));
    h.http.push(
        "calendars/a/events?",
        200,
        events(
            vec![event("fresh", "Fresh inventory")],
            None,
            Some("events-new"),
        ),
    );
    let pass = run_pass(&h.context, true).await.unwrap();
    assert_eq!(pass.failures, 0);
    assert!(!pass.more);
    let urls = h.http.requests.lock().unwrap().clone();
    assert!(!urls[4].contains("syncToken="));
    assert!(!urls[7].contains("syncToken="));
    let view = h.view().await;
    assert_eq!(view["total"], 1);
    assert!(view.to_string().contains("Fresh inventory"));
    assert!(!view.to_string().contains("Retained old event"));
    h.close();
}

#[tokio::test]
async fn metadata_permission_loss_redacts_and_free_busy_errors_never_become_free_time() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path());
    initial_inventory(&h).await;
    h.http.push(
        "syncToken=catalog-old",
        200,
        json!({"items":[],"nextSyncToken":"catalog-current"}),
    );
    h.http
        .push("calendarList/a", 200, entry("a", "freeBusyReader"));
    let pass = run_pass(&h.context, true).await.unwrap();
    assert_eq!(pass.failures, 0);
    assert_eq!(h.view().await["total"], 0);
    h.http.push("/freeBusy",200,json!({"timeMin":"2026-09-12T00:00:00Z","timeMax":"2026-09-13T00:00:00Z","calendars":{"a":{"errors":[{"reason":"forbidden"}]}}}));
    let window = Window::from_dates(
        "2026-09-12".parse().unwrap(),
        "2026-09-13".parse().unwrap(),
        "UTC",
    )
    .unwrap();
    assert_eq!(refresh_busy(&h.context, window).await.unwrap(), 1);
    let view = h.view().await;
    assert_eq!(view["total"], 0);
    assert_eq!(view["coverage"]["complete"], false);
    assert!(!view.to_string().contains("Retained old event"));
    h.close();
}

#[tokio::test]
async fn unusable_calendar_page_does_not_block_other_calendar_and_cancellation_stops_all_reads() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path());
    h.http.push(
        "calendarList?",
        200,
        json!({"items":[entry("a","owner"),entry("b","owner")],"nextSyncToken":"catalog"}),
    );
    h.http.push("calendarList/a", 200, entry("a", "owner"));
    h.http
        .push("calendars/a/events?", 200, json!({"unusable":"shape"}));
    h.http.push("calendarList/b", 200, entry("b", "owner"));
    h.http.push(
        "calendars/b/events?",
        200,
        events(
            vec![event("good", "Available calendar")],
            None,
            Some("b-final"),
        ),
    );
    let pass = run_pass(&h.context, true).await.unwrap();
    assert_eq!(pass.failures, 1);
    assert_eq!(
        pass.first_failure.as_deref(),
        Some("calendar_response_invalid")
    );
    assert_eq!(h.view().await["total"], 1);
    let reads = h.http.requests.lock().unwrap().len();
    h.context.cancel.cancel();
    assert_eq!(
        run_pass(&h.context, false).await.err().unwrap().code,
        "cancelled"
    );
    assert_eq!(h.http.requests.lock().unwrap().len(), reads);
    h.close();
}

#[tokio::test]
async fn changed_partial_page_is_expired_and_replaced_without_publishing_mixed_sources() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path());
    initial_inventory(&h).await;
    let account = h.context.account.as_str().to_owned();
    h.context
        .core_call(move |s| {
            let lease = s.calendar_sync_begin(&account, "a", CalendarSyncMode::Delta)?;
            let page = CalendarPage {
                input_cursor: None,
                items: (0..40)
                    .map(|i| event(&format!("partial-{i}"), "Uncommitted earlier response"))
                    .collect(),
                next_page_token: None,
                next_sync_token: Some("never-published".into()),
            };
            assert_eq!(s.calendar_stage_page_chunk(&lease, &page, 0)?, 32);
            Ok(())
        })
        .await
        .unwrap();
    h.http.push(
        "syncToken=events-old",
        200,
        events(
            vec![event("different", "Changed replay response")],
            None,
            Some("changed-token"),
        ),
    );
    h.http.push(
        "calendars/a/events?",
        200,
        events(
            vec![event("replacement", "Complete replacement")],
            None,
            Some("replacement-token"),
        ),
    );
    let pass = run_pass(&h.context, false).await.unwrap();
    assert_eq!(pass.failures, 0);
    let view = h.view().await;
    assert_eq!(view["total"], 1);
    assert!(view.to_string().contains("Complete replacement"));
    assert!(!view.to_string().contains("Uncommitted earlier response"));
    assert!(!view.to_string().contains("Changed replay response"));
    h.close();
}

#[tokio::test]
async fn a_reconnected_account_rejects_the_previous_worker_before_network_or_store_changes() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path());
    let account = h.context.account.as_str().to_owned();
    h.context
        .core_call(move |s| {
            let scopes = s
                .google_account_bindings()?
                .into_iter()
                .find(|b| b.account == account)
                .unwrap()
                .scopes;
            s.bind_google_account(
                "calendar-worker-subject",
                "human@example.test",
                &scopes,
                Some(&account),
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        run_pass(&h.context, true).await.err().unwrap().code,
        "stale_google_work"
    );
    assert!(h.http.requests.lock().unwrap().is_empty());
    h.close();
}
