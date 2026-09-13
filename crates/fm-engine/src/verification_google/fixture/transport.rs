//! Fixed Gmail-shaped responses. There is deliberately no network implementation.
use super::{Account, Evidence, BACKFILL_MESSAGES, RAW_DELAY_MS, RECENT_MESSAGES};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_google::{
    auth::{CredentialVault, StoredToken},
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
        TransportError,
    },
    AccountId, GoogleError,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const MAX_REQUESTS: u64 = 60_000;
const ORIGIN: &str = "https://gmail.googleapis.com/gmail/v1/users/me/";
const MAX_DRAFT_BYTES: usize = 2 * 1024 * 1024;
const MAX_DRAFTS: usize = 16;
const MAX_DRAFT_PUBLICATIONS: usize = 128;

pub(super) struct FixtureVault {
    accounts: Arc<Vec<Account>>,
    expires_at_secs: u64,
    evidence: Arc<Mutex<Evidence>>,
}
impl FixtureVault {
    pub(super) fn new(
        accounts: Arc<Vec<Account>>,
        expires_at_secs: u64,
        evidence: Arc<Mutex<Evidence>>,
    ) -> Self {
        Self {
            accounts,
            expires_at_secs,
            evidence,
        }
    }
}
impl CredentialVault for FixtureVault {
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        let mut evidence = self.evidence.lock().unwrap();
        evidence.counters.vault_loads += 1;
        let Some(account) = self.accounts.iter().find(|row| row.id == account.as_str()) else {
            evidence.error("unknown_synthetic_vault_account");
            return Err(GoogleError::StoreUnavailable);
        };
        Ok(Some(StoredToken {
            access_token: account.token(),
            refresh_token: "synthetic-refresh-never-used".into(),
            expires_at_secs: self.expires_at_secs,
        }))
    }
    fn store(&self, _: &AccountId, _: StoredToken) -> Result<(), GoogleError> {
        let mut evidence = self.evidence.lock().unwrap();
        evidence.counters.vault_stores += 1;
        evidence.error("synthetic_vault_write_refused");
        Err(GoogleError::StoreUnavailable)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Message {
    pub(super) recent: bool,
    pub(super) index: usize,
}
impl Message {
    fn parse(id: &str) -> Option<Self> {
        let (recent, digits) = if let Some(value) = id.strip_prefix("vr") {
            (true, value)
        } else {
            (false, id.strip_prefix("vb")?)
        };
        if digits.len() != 4 || !digits.bytes().all(|value| value.is_ascii_digit()) {
            return None;
        }
        let index = digits.parse().ok()?;
        (index
            < if recent {
                RECENT_MESSAGES
            } else {
                BACKFILL_MESSAGES
            })
        .then_some(Self { recent, index })
    }
    fn at(ordinal: usize) -> Self {
        if ordinal < RECENT_MESSAGES {
            Self {
                recent: true,
                index: ordinal,
            }
        } else {
            Self {
                recent: false,
                index: ordinal - RECENT_MESSAGES,
            }
        }
    }
    pub(super) fn id(self) -> String {
        format!("v{}{:04}", if self.recent { "r" } else { "b" }, self.index)
    }
    fn thread_index(self) -> usize {
        if self.recent {
            self.index / 2
        } else {
            self.index
        }
    }
    pub(super) fn thread(self) -> String {
        format!(
            "t{}{:04}",
            if self.recent { "r" } else { "b" },
            self.thread_index()
        )
    }
    fn kind(self) -> &'static str {
        if self.recent {
            "recent"
        } else {
            "backfill"
        }
    }
    fn sent_secs(self, base: u64) -> i64 {
        if self.recent {
            base as i64 - 86_400 + self.index as i64 * 60
        } else {
            base as i64 - 90 * 86_400 - self.index as i64 * 60
        }
    }
    pub(super) fn body(self, account: &Account) -> String {
        format!("Generated verification mail for {}/{}/{:04}. This fixed source exists only in the disposable acquisition fixture.{}", account.key, self.kind(), self.index, " Cached mail must stay responsive.".repeat(32))
    }
    pub(super) fn mime(self, account: &Account, base: u64) -> String {
        let sent = chrono::DateTime::from_timestamp(self.sent_secs(base), 0).unwrap();
        let date = sent.format("%a, %d %b %Y %H:%M:%S +0000");
        format!("From: Verification Sender <sender@example.test>\r\nTo: {}\r\nSubject: Verification {} {} {:04}\r\nDate: {date}\r\nMessage-ID: <v1-{}-{}-{:04}@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{}",
            account.email, account.key, self.kind(), self.thread_index(), account.key, self.kind(), self.index, self.body(account))
    }
}

struct MailState {
    history: u64,
    reviewed_at: [u64; RECENT_MESSAGES],
    drafts: BTreeMap<String, FixtureDraft>,
    draft_message_sequence: u64,
}
struct FixtureDraft {
    message_id: String,
    thread_id: String,
    raw: Vec<u8>,
}
impl FixtureDraft {
    fn value(&self, id: &str, include_raw: bool) -> Value {
        let mut value = json!({"id":id,"message":{"id":self.message_id,"threadId":self.thread_id,"labelIds":["DRAFT"]}});
        if include_raw {
            value["message"]["raw"] = json!(URL_SAFE_NO_PAD.encode(&self.raw));
        }
        value
    }
}
impl Default for MailState {
    fn default() -> Self {
        Self {
            history: 100,
            reviewed_at: [0; RECENT_MESSAGES],
            drafts: BTreeMap::new(),
            draft_message_sequence: 0,
        }
    }
}
impl MailState {
    fn labels(&self, message: Message) -> Vec<&'static str> {
        if message.recent && self.reviewed_at[message.index] == 0 {
            vec!["INBOX", "UNREAD"]
        } else {
            vec!["INBOX"]
        }
    }
    fn state(&self, message: Message) -> Value {
        let history = if message.recent {
            self.reviewed_at[message.index].max(100)
        } else {
            100
        };
        json!({"id":message.id(),"threadId":message.thread(),"labelIds":self.labels(message),"historyId":history.to_string()})
    }
}

pub(super) struct FixtureHttp {
    accounts: Arc<Vec<Account>>,
    base: u64,
    states: Vec<Mutex<MailState>>,
    evidence: Arc<Mutex<Evidence>>,
    hold_raw: Arc<AtomicBool>,
}
impl FixtureHttp {
    pub(super) fn new(
        accounts: Arc<Vec<Account>>,
        base: u64,
        evidence: Arc<Mutex<Evidence>>,
        hold_raw: Arc<AtomicBool>,
    ) -> Self {
        Self {
            states: accounts
                .iter()
                .map(|_| Mutex::new(MailState::default()))
                .collect(),
            accounts,
            base,
            evidence,
            hold_raw,
        }
    }
    fn reject(&self, code: &'static str) -> TransportError {
        self.evidence.lock().unwrap().error(code);
        TransportError::InvalidRequest
    }
    fn resolve(&self, request: &HttpRequest) -> Result<(usize, Route), TransportError> {
        let token = request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        let account = self
            .accounts
            .iter()
            .position(|account| token == Some(format!("Bearer {}", account.token()).as_str()))
            .ok_or_else(|| self.reject("unexpected_synthetic_token"))?;
        let path = request
            .url()
            .strip_prefix(ORIGIN)
            .ok_or_else(|| self.reject("unexpected_synthetic_origin"))?;
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let params = parameters(query).ok_or_else(|| self.reject("invalid_synthetic_query"))?;
        if request.method() == HttpMethod::Get && request.body_bytes().is_empty() {
            match path {
                "profile" if params.is_empty() => return Ok((account, Route::Profile)),
                "labels" if params.is_empty() => return Ok((account, Route::Labels)),
                "messages" => {
                    if params.get("maxResults") != Some(&"100")
                        || params.get("includeSpamTrash") != Some(&"true")
                        || params.keys().any(|key| {
                            !["maxResults", "includeSpamTrash", "q", "pageToken"].contains(key)
                        })
                    {
                        return Err(self.reject("unexpected_synthetic_list_query"));
                    }
                    let recent = match params.get("q") {
                        None => false,
                        Some(query)
                            if query.strip_prefix("after%3A").is_some_and(|value| {
                                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                            }) =>
                        {
                            true
                        }
                        _ => return Err(self.reject("unexpected_synthetic_recent_query")),
                    };
                    let count = if recent {
                        RECENT_MESSAGES
                    } else {
                        RECENT_MESSAGES + BACKFILL_MESSAGES
                    };
                    let offset = page_offset(
                        params.get("pageToken").copied(),
                        if recent { "recent-" } else { "inventory-" },
                        count,
                    )
                    .ok_or_else(|| self.reject("unexpected_synthetic_list_cursor"))?;
                    return Ok((account, Route::List { recent, offset }));
                }
                "history" => {
                    if params.get("maxResults") != Some(&"100") || params.len() != 2 {
                        return Err(self.reject("unexpected_synthetic_history_query"));
                    }
                    let start = params
                        .get("startHistoryId")
                        .and_then(|value| value.parse::<u64>().ok())
                        .filter(|value| *value >= 100)
                        .ok_or_else(|| self.reject("unexpected_synthetic_history_cursor"))?;
                    return Ok((account, Route::History(start)));
                }
                _ => {}
            }
            if let Some(id) = path.strip_prefix("messages/") {
                let message = Message::parse(id)
                    .ok_or_else(|| self.reject("unexpected_synthetic_message"))?;
                if params.len() == 1 && params.get("format") == Some(&"raw") {
                    return Ok((account, Route::Raw(message)));
                }
                if params.len() == 2
                    && params.get("format") == Some(&"full")
                    && params.get("fields") == Some(&"id%2CthreadId%2ClabelIds%2ChistoryId")
                {
                    return Ok((account, Route::State(message)));
                }
            }
            if let Some(id) = path.strip_prefix("drafts/") {
                if fixture_draft_id(id) && params.len() == 1 && params.get("format") == Some(&"raw")
                {
                    return Ok((account, Route::DraftGet(id.into())));
                }
            }
        }
        if params.is_empty()
            && matches!(request.method(), HttpMethod::Post | HttpMethod::Put)
            && request.body_bytes().len() <= MAX_DRAFT_BYTES.div_ceil(3) * 4 + 1024
        {
            let draft_id = if path == "drafts" && request.method() == HttpMethod::Post {
                Some(None)
            } else {
                path.strip_prefix("drafts/")
                    .filter(|id| fixture_draft_id(id) && request.method() == HttpMethod::Put)
                    .map(|id| Some(id.to_owned()))
            };
            if let Some(draft_id) = draft_id {
                let body: Value = serde_json::from_slice(request.body_bytes())
                    .map_err(|_| self.reject("invalid_synthetic_draft_body"))?;
                let object = body
                    .as_object()
                    .ok_or_else(|| self.reject("invalid_synthetic_draft_body"))?;
                if object.keys().any(|key| key != "message" && key != "id")
                    || object
                        .get("id")
                        .is_some_and(|id| draft_id.is_none() || id.as_str() != draft_id.as_deref())
                {
                    return Err(self.reject("unexpected_synthetic_draft_fields"));
                }
                let message = body["message"]
                    .as_object()
                    .ok_or_else(|| self.reject("invalid_synthetic_draft_message"))?;
                if message.keys().any(|key| key != "raw" && key != "threadId") {
                    return Err(self.reject("unexpected_synthetic_draft_message_fields"));
                }
                let raw = message
                    .get("raw")
                    .and_then(Value::as_str)
                    .and_then(|raw| URL_SAFE_NO_PAD.decode(raw).ok())
                    .filter(|raw| !raw.is_empty() && raw.len() <= MAX_DRAFT_BYTES)
                    .ok_or_else(|| self.reject("invalid_synthetic_draft_raw"))?;
                let thread = match message.get("threadId") {
                    None => None,
                    Some(Value::String(value))
                        if !value.is_empty()
                            && value.len() <= 256
                            && !value.chars().any(char::is_control) =>
                    {
                        Some(value.clone())
                    }
                    _ => return Err(self.reject("invalid_synthetic_draft_thread")),
                };
                return Ok((
                    account,
                    Route::DraftWrite {
                        id: draft_id,
                        raw,
                        thread,
                    },
                ));
            }
        }
        if request.method() == HttpMethod::Post
            && params.is_empty()
            && request.body_bytes().len() <= 256
        {
            if let Some(id) = path
                .strip_prefix("messages/")
                .and_then(|value| value.strip_suffix("/modify"))
            {
                let message = Message::parse(id)
                    .filter(|message| message.recent)
                    .ok_or_else(|| self.reject("unexpected_synthetic_modify_target"))?;
                let body: Value = serde_json::from_slice(request.body_bytes())
                    .map_err(|_| self.reject("invalid_synthetic_modify_body"))?;
                if body == json!({"addLabelIds":[],"removeLabelIds":["UNREAD"]}) {
                    return Ok((account, Route::Review(message)));
                }
            }
        }
        Err(self.reject("unexpected_synthetic_route"))
    }
    fn response(&self, account: usize, route: Route) -> Result<(u16, Value), TransportError> {
        let owner = &self.accounts[account];
        let mut state = self.states[account].lock().unwrap();
        Ok((
            200,
            match route {
                Route::Profile => {
                    json!({"emailAddress":owner.email,"messagesTotal":RECENT_MESSAGES+BACKFILL_MESSAGES,"threadsTotal":RECENT_MESSAGES/2+BACKFILL_MESSAGES,"historyId":state.history.to_string()})
                }
                Route::Labels => {
                    json!({"labels":[{"id":"INBOX","name":"INBOX","type":"system"},{"id":"UNREAD","name":"UNREAD","type":"system"}]})
                }
                Route::List { recent, offset } => {
                    let count = if recent {
                        RECENT_MESSAGES
                    } else {
                        RECENT_MESSAGES + BACKFILL_MESSAGES
                    };
                    let end = (offset + 100).min(count);
                    json!({"messages":(offset..end).map(|ordinal|{let message=Message::at(ordinal);json!({"id":message.id(),"threadId":message.thread()})}).collect::<Vec<_>>(),"resultSizeEstimate":count,
                    "nextPageToken":(end<count).then(||format!("{}-{end}", if recent {"recent"} else {"inventory"}))})
                }
                Route::History(start) => {
                    if start > state.history {
                        return Err(self.reject("future_synthetic_history_cursor"));
                    }
                    let mut changes: Vec<_> = state
                        .reviewed_at
                        .iter()
                        .enumerate()
                        .filter(|(_, at)| **at > start)
                        .map(|(index, at)| (*at, index))
                        .collect();
                    changes.sort_unstable();
                    json!({"historyId":state.history.to_string(),"history":changes.into_iter().map(|(at,index)|{
                    let message=Message{recent:true,index}; json!({"id":at.to_string(),"labelsRemoved":[{"message":{"id":message.id(),"threadId":message.thread()},"labelIds":["UNREAD"]}]})
                }).collect::<Vec<_>>()})
                }
                Route::State(message) => state.state(message),
                Route::Raw(message) => {
                    let mut result = state.state(message);
                    result["raw"] = json!(URL_SAFE_NO_PAD.encode(message.mime(owner, self.base)));
                    result["internalDate"] =
                        json!((message.sent_secs(self.base) * 1000).to_string());
                    result
                }
                Route::Review(message) => {
                    if state.reviewed_at[message.index] == 0 {
                        state.history += 1;
                        state.reviewed_at[message.index] = state.history;
                    }
                    self.evidence.lock().unwrap().counters.modify_completed += 1;
                    state.state(message)
                }
                Route::DraftGet(id) => {
                    self.evidence.lock().unwrap().counters.draft_read += 1;
                    let Some(draft) = state.drafts.get(&id) else {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated draft is absent"}}),
                        ));
                    };
                    draft.value(&id, true)
                }
                Route::DraftWrite { id, raw, thread } => {
                    let creating = id.is_none();
                    if creating && state.drafts.len() >= MAX_DRAFTS {
                        return Err(self.reject("synthetic_draft_count_limit"));
                    }
                    if id.as_ref().is_some_and(|id| !state.drafts.contains_key(id)) {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated draft is absent"}}),
                        ));
                    }
                    let mut evidence = self.evidence.lock().unwrap();
                    if evidence.draft_publications.len() >= MAX_DRAFT_PUBLICATIONS {
                        evidence.error("synthetic_draft_publication_limit");
                        return Err(TransportError::InvalidRequest);
                    }
                    let id = id.unwrap_or_else(|| format!("vd{:04}", state.drafts.len() + 1));
                    state.draft_message_sequence += 1;
                    let draft = FixtureDraft {
                        message_id: format!("vmd{:04}", state.draft_message_sequence),
                        thread_id: thread.unwrap_or_else(|| format!("vdt-{id}")),
                        raw,
                    };
                    let response = draft.value(&id, false);
                    evidence.draft_publications.push(json!({"account":owner.id,"draft_id":id,"message_id":draft.message_id,
                    "thread_id":draft.thread_id,"size":draft.raw.len(),"sha256":format!("{:x}",Sha256::digest(&draft.raw)),
                    "kind":if creating{"create"}else{"update"}}));
                    if creating {
                        evidence.counters.draft_created += 1;
                    } else {
                        evidence.counters.draft_updated += 1;
                    }
                    state.drafts.insert(id, draft);
                    response
                }
            },
        ))
    }
}

enum Route {
    Profile,
    Labels,
    List {
        recent: bool,
        offset: usize,
    },
    History(u64),
    Raw(Message),
    State(Message),
    Review(Message),
    DraftGet(String),
    DraftWrite {
        id: Option<String>,
        raw: Vec<u8>,
        thread: Option<String>,
    },
}
fn fixture_draft_id(id: &str) -> bool {
    id.strip_prefix("vd")
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}
fn parameters(query: &str) -> Option<BTreeMap<&str, &str>> {
    if query.is_empty() {
        return Some(BTreeMap::new());
    }
    let mut result = BTreeMap::new();
    for part in query.split('&') {
        let (key, value) = part.split_once('=')?;
        if key.is_empty() || value.is_empty() || result.insert(key, value).is_some() {
            return None;
        }
    }
    Some(result)
}
fn page_offset(value: Option<&str>, prefix: &str, count: usize) -> Option<usize> {
    match value {
        None => Some(0),
        Some(value) => value
            .strip_prefix(prefix)?
            .parse::<usize>()
            .ok()
            .filter(|offset| *offset > 0 && *offset < count && offset % 100 == 0),
    }
}

struct ActiveRequest {
    evidence: Arc<Mutex<Evidence>>,
    raw: bool,
    held: bool,
    finished: bool,
}
impl ActiveRequest {
    fn new(evidence: Arc<Mutex<Evidence>>) -> Self {
        evidence.lock().unwrap().counters.http_started += 1;
        Self {
            evidence,
            raw: false,
            held: false,
            finished: false,
        }
    }
    fn raw(&mut self, held: bool) {
        let mut evidence = self.evidence.lock().unwrap();
        self.raw = true;
        self.held = held;
        evidence.counters.raw_started += 1;
        evidence.counters.raw_active += 1;
        if held {
            evidence.counters.raw_held_active += 1;
        }
    }
    fn finish(&mut self, succeeded: bool) {
        let mut evidence = self.evidence.lock().unwrap();
        if succeeded {
            evidence.counters.http_completed += 1;
            if self.raw {
                evidence.counters.raw_completed += 1;
            }
        } else {
            evidence.counters.http_rejected += 1;
        }
        self.finished = true;
    }
}
impl Drop for ActiveRequest {
    fn drop(&mut self) {
        let mut evidence = self.evidence.lock().unwrap();
        if !self.finished {
            evidence.counters.http_cancelled += 1;
            if self.raw {
                evidence.counters.raw_cancelled += 1;
            }
        }
        if self.raw {
            evidence.counters.raw_active -= 1;
        }
        if self.held {
            evidence.counters.raw_held_active -= 1;
        }
    }
}
impl HttpTransport for FixtureHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            let mut active = ActiveRequest::new(self.evidence.clone());
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            if self.evidence.lock().unwrap().counters.http_started > MAX_REQUESTS {
                active.finish(false);
                return Err(self.reject("synthetic_request_limit"));
            }
            let (account, route) = match self.resolve(&request) {
                Ok(value) => value,
                Err(error) => {
                    active.finish(false);
                    return Err(error);
                }
            };
            if matches!(route, Route::Raw(_)) {
                let held = self.hold_raw.load(Ordering::Acquire);
                active.raw(held);
                if held {
                    cancel.cancelled().await;
                    return Err(TransportError::Cancelled);
                }
                tokio::select! {
                    _=cancel.cancelled()=>return Err(TransportError::Cancelled),
                    _=tokio::time::sleep(Duration::from_millis(RAW_DELAY_MS))=>{},
                }
            }
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            let result = self.response(account, route).and_then(|(status, value)| {
                serde_json::to_vec(&value)
                    .map(|body| HttpResponse {
                        status,
                        body,
                        retry_after_secs: None,
                        etag: None,
                    })
                    .map_err(|_| self.reject("synthetic_serialization_failed"))
            });
            active.finish(result.is_ok());
            result
        })
    }
}
