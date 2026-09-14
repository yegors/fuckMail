//! Calendar-only, bounded in-memory invitation provider. Never opens a socket.
use super::{calendar_occurrences, transport::decoded_query, Account, Evidence};
use fm_google::transport::{HttpMethod, HttpRequest, HttpResponse, TransportError};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

pub(super) const COUNT: usize = 7;
const MAX_RECORDS: usize = 256;
const ORIGIN: &str = "https://www.googleapis.com/calendar/v3/";

pub(super) fn calendar(key: &str) -> String {
    format!("calendar-{key}@example.test")
}
pub(super) fn catalog(key: &str) -> Value {
    json!({"id":calendar(key),"summary":format!("Invitations {key}"),"accessRole":"owner","timeZone":"UTC","primary":true,"selected":true,"defaultReminders":[]})
}
pub(super) fn source(key: &str, index: usize, base: u64) -> Value {
    let start =
        chrono::DateTime::from_timestamp((base + (index as u64 + 1) * 3600) as i64, 0).unwrap();
    let end = start + chrono::Duration::minutes(30);
    let id = format!("ci{index:04}");
    let email = format!("{key}@example.test");
    let mut value = json!({"id":id,"etag":format!("\"{id}-v1\""),"status":"confirmed","eventType":"default",
        "summary":format!("Invitation {key} {index:04}"),"description":"Preserved generated invitation details","location":"Fixture room",
        "start":{"dateTime":start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"},
        "end":{"dateTime":end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"},
        "organizer":{"email":email,"self":true},"creator":{"email":email,"self":true},
        "attendees":[{"email":email,"organizer":true,"self":true,"responseStatus":"accepted"},
            {"email":"alpha@example.test","displayName":"Alpha guest","responseStatus":"accepted","optional":true,"comment":"Preserved reply","additionalGuests":1},
            {"email":"beta@example.test","displayName":"Beta guest","responseStatus":"tentative","resource":false}],
        "reminders":{"useDefault":false,"overrides":[{"method":"popup","minutes":15}]},
        "extendedProperties":{"private":{"fixture":"retained"},"shared":{"invitation":"v1"}}});
    if index == 0 {
        value["eventLabelId"] = "fixture-label".into();
        value["colorId"] = "3".into();
        value["conferenceData"] = json!({"conferenceSolution":{"key":{"type":"hangoutsMeet"},"name":"Google Meet"},"conferenceId":"fixture-meet","signature":"retained-signature","entryPoints":[{"entryPointType":"video","uri":"https://meet.example.test/fixture"}]});
        value["attachments"] = json!([{"fileUrl":"https://drive.example.test/fixture","title":"Retained attachment","mimeType":"application/pdf","fileId":"fixture-file"}]);
    }
    if index == 2 {
        value["attendeesOmitted"] = true.into();
    }
    if index == 3 {
        value["attendees"][1]["asyncOperation"] = "inProgress".into();
    }
    if index == 6 {
        value["organizer"] = json!({"email":"other@example.test","self":false});
        value["creator"] = json!({"email":"other@example.test","self":false});
        value["attendees"][0] =
            json!({"email":email,"organizer":false,"self":true,"responseStatus":"needsAction"});
        value["attendees"].as_array_mut().unwrap().push(json!({"email":"other@example.test","organizer":true,"self":false,"responseStatus":"accepted"}));
    }
    value
}

struct State {
    events: BTreeMap<String, Value>,
    version: u64,
    lost: BTreeSet<String>,
    exceptions: BTreeSet<String>,
}
pub(super) struct CalendarFixture {
    accounts: Arc<Vec<Account>>,
    states: Vec<Mutex<State>>,
    evidence: Arc<Mutex<Evidence>>,
    occurrences: bool,
}
impl CalendarFixture {
    pub(super) fn new(
        accounts: Arc<Vec<Account>>,
        base: u64,
        evidence: Arc<Mutex<Evidence>>,
    ) -> Self {
        Self::with_count(accounts, base, evidence, COUNT)
    }
    pub(super) fn with_count(
        accounts: Arc<Vec<Account>>,
        base: u64,
        evidence: Arc<Mutex<Evidence>>,
        count: usize,
    ) -> Self {
        let states = accounts
            .iter()
            .map(|a| {
                Mutex::new(State {
                    events: (0..count)
                        .map(|i| (format!("ci{i:04}"), source(a.key, i, base)))
                        .collect(),
                    version: 1,
                    lost: BTreeSet::new(),
                    exceptions: BTreeSet::new(),
                })
            })
            .collect();
        Self {
            accounts,
            states,
            evidence,
            occurrences: false,
        }
    }
    pub(super) fn occurrences(
        accounts: Arc<Vec<Account>>,
        base: u64,
        evidence: Arc<Mutex<Evidence>>,
    ) -> Self {
        let states = accounts
            .iter()
            .map(|account| {
                Mutex::new(State {
                    events: calendar_occurrences::sources(account.key, base),
                    version: 1,
                    lost: BTreeSet::new(),
                    exceptions: BTreeSet::new(),
                })
            })
            .collect();
        Self {
            accounts,
            states,
            evidence,
            occurrences: true,
        }
    }
    fn reject(&self, code: &'static str) -> TransportError {
        self.evidence.lock().unwrap().error(code);
        TransportError::InvalidRequest
    }
    pub(super) fn response(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let token = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        let index = self
            .accounts
            .iter()
            .position(|a| token == Some(format!("Bearer {}", a.token()).as_str()))
            .ok_or_else(|| self.reject("unexpected_calendar_token"))?;
        let account = &self.accounts[index];
        let path = request
            .url()
            .strip_prefix(ORIGIN)
            .ok_or_else(|| self.reject("unexpected_calendar_origin"))?;
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let path = path
            .split('/')
            .map(decoded_query)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| self.reject("invalid_calendar_path"))?;
        let mut params = BTreeMap::new();
        for pair in query.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| self.reject("invalid_calendar_query"))?;
            let v = decoded_query(v).ok_or_else(|| self.reject("invalid_calendar_query"))?;
            if params.insert(k.to_owned(), v).is_some() {
                return Err(self.reject("duplicate_calendar_query"));
            }
        }
        let cal = calendar(account.key);
        let parts = path.iter().map(String::as_str).collect::<Vec<_>>();
        let mut state = self.states[index].lock().unwrap();
        let method = request.method();
        if method == HttpMethod::Get {
            if !request.body_bytes().is_empty() {
                return Err(self.reject("calendar_read_body"));
            }
            let (value, kind) = match parts.as_slice() {
                ["users", "me", "calendarList"] if valid_list(&params, true) => (
                    json!({"items":[catalog(account.key)],"nextSyncToken":"fixture-catalog-v1"}),
                    "catalog_list",
                ),
                ["users", "me", "calendarList", id] if *id == cal && params.is_empty() => {
                    (catalog(account.key), "catalog_get")
                }
                ["calendars", id, "events"] if *id == cal && valid_list(&params, false) => (
                    json!({"items":state.events.iter().filter(|(id,_)|!self.occurrences||calendar_occurrences::initially_visible(id)||state.exceptions.contains(*id)).map(|(_,source)|source).collect::<Vec<_>>(),"nextSyncToken":format!("fixture-events-v{}",state.version),"timeZone":"UTC","accessRole":"owner","defaultReminders":[]}),
                    "event_list",
                ),
                ["calendars", id, "events", series, "instances"]
                    if self.occurrences && *id == cal =>
                {
                    (self.instances(&state, series, &params)?, "instances")
                }
                ["calendars", id, "events", event] if *id == cal && params.is_empty() => {
                    let Some(value) = state.events.get(*event) else {
                        return reply(404, json!({"error":{"code":404}}));
                    };
                    (value.clone(), "event_get")
                }
                _ => return Err(self.reject("unexpected_calendar_read")),
            };
            let mut evidence = self.evidence.lock().unwrap();
            evidence.counters.calendar_reads += 1;
            if evidence.calendar_reads.len() >= MAX_RECORDS {
                evidence.error("calendar_read_trace_limit");
                return Err(TransportError::InvalidRequest);
            }
            evidence.calendar_reads.push(json!({"account":account.id,"calendar":cal,"kind":kind,"method":"GET","query":params,"response":value,"expected_quota_units":1}));
            return reply(200, value);
        }
        if self.evidence.lock().unwrap().calendar_publications.len() >= MAX_RECORDS {
            return Err(self.reject("calendar_publication_limit"));
        }
        let is_patch = method == HttpMethod::Patch;
        let is_create = method == HttpMethod::Post;
        let is_delete = method == HttpMethod::Delete;
        if !(is_patch || is_create || is_delete) {
            return Err(self.reject("unexpected_calendar_mutation_method"));
        }
        if params.get("sendUpdates").map(String::as_str) != Some("all") {
            return Err(self.reject("calendar_notification_policy"));
        }
        if !is_delete
            && (params.get("conferenceDataVersion").map(String::as_str) != Some("1")
                || params.get("supportsAttachments").map(String::as_str) != Some("true"))
        {
            return Err(self.reject("calendar_preservation_parameters"));
        }
        if params.keys().any(|k| {
            ![
                "sendUpdates",
                "conferenceDataVersion",
                "supportsAttachments",
                "eventLabelVersion",
            ]
            .contains(&k.as_str())
        }) {
            return Err(self.reject("unexpected_calendar_mutation_query"));
        }
        let body = if is_delete {
            if !request.body_bytes().is_empty() || params.len() != 1 {
                return Err(self.reject("calendar_delete_payload"));
            }
            Value::Null
        } else {
            serde_json::from_slice::<Value>(request.body_bytes())
                .ok()
                .filter(Value::is_object)
                .ok_or_else(|| self.reject("calendar_mutation_body"))?
        };
        let event = match parts.as_slice() {
            ["calendars", id, "events"] if *id == cal && is_create => body["id"]
                .as_str()
                .ok_or_else(|| self.reject("calendar_create_id"))?,
            ["calendars", id, "events", event] if *id == cal && !is_create => *event,
            _ => return Err(self.reject("unexpected_calendar_mutation_target")),
        }
        .to_owned();
        if self.occurrences
            && (is_create
                || !matches!(
                    event.as_str(),
                    "oi0000" | "oi0001" | "oi0004" | "oi0005" | "od0000" | "od0002"
                ))
        {
            return Err(self.reject("unexpected_occurrence_mutation_target"));
        }
        let before = state.events.get(&event).cloned().unwrap_or(Value::Null);
        self.evidence
            .lock()
            .unwrap()
            .counters
            .calendar_mutation_attempts += 1;
        if body["attendees"].as_array().is_some_and(|rows| {
            rows.iter().any(|row| {
                ["id", "self", "organizer", "asyncOperation"]
                    .iter()
                    .any(|key| row.get(key).is_some())
            })
        }) {
            return Err(self.reject("calendar_read_only_attendee_wire"));
        }
        let etag = request
            .headers()
            .get("if-match")
            .and_then(|v| v.to_str().ok());
        if !is_create && (before.is_null() || before["status"] == "cancelled") {
            return reply(404, json!({"error":{"code":404}}));
        }
        if is_create && !before.is_null() {
            return reply(409, json!({"error":{"code":409}}));
        }
        if !is_create && etag != before["etag"].as_str() {
            return reply(412, json!({"error":{"code":412}}));
        }
        if let Some(label) = before.get("eventLabelId") {
            if !is_delete
                && (body.get("eventLabelId") != Some(label)
                    || params.get("eventLabelVersion").map(String::as_str) != Some("1"))
            {
                return Err(self.reject("calendar_label_not_preserved"));
            }
        }
        let mut after = if is_create {
            body.clone()
        } else {
            before.clone()
        };
        if is_delete {
            after["status"] = "cancelled".into();
        } else if is_patch {
            for (key, value) in body.as_object().unwrap() {
                if key == "attendees" {
                    after[key] = provider_roster(&before, value, body["attendeesOmitted"] == true)?;
                } else if key != "attendeesOmitted" {
                    after[key] = value.clone();
                }
            }
        }
        state.version += 1;
        after["etag"] = format!("\"{event}-v{}\"", state.version).into();
        if is_create {
            after["organizer"] = json!({"email":account.email,"self":true});
            after["status"] = "confirmed".into();
        }
        if self.occurrences && is_delete {
            after = calendar_occurrences::cancelled(&before);
        }
        state.events.insert(event.clone(), after.clone());
        if self.occurrences {
            state.exceptions.insert(event.clone());
        }
        let lost = (is_patch && event == if self.occurrences { "oi0001" } else { "ci0001" }
            || is_delete && event == if self.occurrences { "oi0004" } else { "ci0004" })
            && state.lost.insert(event.clone());
        let mut evidence = self.evidence.lock().unwrap();
        evidence.counters.calendar_mutations += 1;
        if lost {
            evidence.counters.calendar_lost_responses += 1;
        } else {
            evidence.counters.calendar_acknowledged += 1;
        }
        evidence.calendar_publications.push(json!({"account":account.id,"calendar":cal,"event":event,"method":if is_create{"POST"}else if is_patch{"PATCH"}else{"DELETE"},"if_match":etag,"query":params,"body":body,"before":before,"after":after,"response":if lost{"lost_response"}else{"acknowledged"},"expected_quota_units":if is_patch{3}else{1}}));
        drop(evidence);
        if lost {
            Err(TransportError::Network)
        } else if is_delete {
            reply(204, Value::Null)
        } else {
            reply(200, after)
        }
    }
    fn instances(
        &self,
        state: &State,
        series: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<Value, TransportError> {
        if params.get("maxResults").map(String::as_str) != Some("250")
            || params.get("showDeleted").map(String::as_str) != Some("true")
            || params.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "maxResults" | "showDeleted" | "originalStart" | "pageToken"
                )
            })
            || !matches!(series, "co0000" | "co0001" | "co0002" | "co0003")
        {
            return Err(self.reject("unexpected_occurrence_instances_query"));
        }
        let original = params
            .get("originalStart")
            .map(|v| {
                chrono::DateTime::parse_from_rfc3339(v)
                    .map_err(|_| self.reject("invalid_occurrence_original_start"))
            })
            .transpose()?;
        if series == "co0001" && original.is_some() {
            return Err(self.reject("unverified_all_day_original_start"));
        }
        let rows = state
            .events
            .values()
            .filter(|source| source["recurringEventId"] == series)
            .filter(|source| {
                original.is_none_or(|original| {
                    source["originalStartTime"]["dateTime"]
                        .as_str()
                        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                        == Some(original)
                })
            })
            .collect::<Vec<_>>();
        let offset = match params.get("pageToken") {
            None => 0,
            Some(token) => token
                .strip_prefix(&format!("occurrence-{series}-"))
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n > 0 && *n < rows.len() && *n % calendar_occurrences::PAGE_SIZE == 0)
                .ok_or_else(|| self.reject("invalid_occurrence_page_token"))?,
        };
        let end = (offset + calendar_occurrences::PAGE_SIZE).min(rows.len());
        let mut page = json!({"items":rows[offset..end]});
        if end < rows.len() {
            page["nextPageToken"] = format!("occurrence-{series}-{end}").into();
        }
        Ok(page)
    }
}
// Google returns read-only identity fields even when PATCH correctly omits
// them. Partial self-RSVP updates only that participant; removed guests stay out.
fn provider_roster(
    before: &Value,
    submitted: &Value,
    partial: bool,
) -> Result<Value, TransportError> {
    let old = before["attendees"].as_array().cloned().unwrap_or_default();
    let rows = submitted.as_array().ok_or(TransportError::InvalidRequest)?;
    if partial {
        let mut result = old;
        for row in rows {
            let email = row["email"]
                .as_str()
                .ok_or(TransportError::InvalidRequest)?;
            let target = result
                .iter_mut()
                .find(|v| {
                    v["email"]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case(email))
                })
                .ok_or(TransportError::InvalidRequest)?;
            for (key, value) in row.as_object().ok_or(TransportError::InvalidRequest)? {
                target[key] = value.clone();
            }
        }
        return Ok(json!(result));
    }
    let mut result = Vec::new();
    for row in rows {
        let email = row["email"]
            .as_str()
            .ok_or(TransportError::InvalidRequest)?;
        let mut returned = row.clone();
        if let Some(previous) = old.iter().find(|v| {
            v["email"]
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case(email))
        }) {
            for key in ["id", "self", "organizer", "asyncOperation"] {
                if let Some(value) = previous.get(key) {
                    returned[key] = value.clone();
                }
            }
        }
        result.push(returned);
    }
    Ok(json!(result))
}
fn valid_list(params: &BTreeMap<String, String>, catalog: bool) -> bool {
    let fixed = if catalog {
        vec![
            ("maxResults", "250"),
            ("showDeleted", "true"),
            ("showHidden", "true"),
        ]
    } else {
        vec![
            ("maxResults", "250"),
            ("showDeleted", "true"),
            ("singleEvents", "false"),
            ("showHiddenInvitations", "true"),
        ]
    };
    fixed
        .iter()
        .all(|(k, v)| params.get(*k).map(String::as_str) == Some(*v))
        && params
            .keys()
            .all(|k| fixed.iter().any(|(key, _)| key == k) || k == "syncToken")
        && params.get("syncToken").is_none_or(|v| {
            if catalog {
                v == "fixture-catalog-v1"
            } else {
                v.starts_with("fixture-events-v") && v[16..].parse::<u64>().is_ok()
            }
        })
}
fn reply(status: u16, value: Value) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status,
        etag: None,
        retry_after_secs: None,
        body: if status == 204 {
            Vec::new()
        } else {
            serde_json::to_vec(&value).map_err(|_| TransportError::InvalidRequest)?
        },
    })
}
