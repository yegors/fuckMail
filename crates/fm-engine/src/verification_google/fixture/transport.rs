//! Fixed Gmail-shaped responses. There is deliberately no network implementation.
use super::{
    calendar_invitations::CalendarFixture, draft_actions, remote_drafts, Account, Evidence,
    BACKFILL_MESSAGES, RAW_DELAY_MS, RECENT_MESSAGES,
};
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
const MAX_SENDS: usize = 16;

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
    draft_sequence: u64,
    sent: BTreeMap<String, FixtureSent>,
}
struct FixtureSent {
    thread_id: String,
    raw: Vec<u8>,
    rfc_message_id: String,
    history: u64,
}
impl FixtureSent {
    fn value(&self, id: &str, include_raw: bool, base: u64) -> Value {
        let mut value = json!({"id":id,"threadId":self.thread_id,"labelIds":["SENT"],"historyId":self.history.to_string()});
        if include_raw {
            value["raw"] = json!(URL_SAFE_NO_PAD.encode(&self.raw));
            value["internalDate"] = json!((base * 1000).to_string());
        }
        value
    }
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
            draft_sequence: 0,
            sent: BTreeMap::new(),
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
    send_enabled: bool,
    remote_version_two: Option<Arc<AtomicBool>>,
    action_version_two: Option<Arc<AtomicBool>>,
    calendar: Option<CalendarFixture>,
    account_mode: bool,
}
impl FixtureHttp {
    pub(super) fn new(
        accounts: Arc<Vec<Account>>,
        base: u64,
        evidence: Arc<Mutex<Evidence>>,
        hold_raw: Arc<AtomicBool>,
        send_enabled: bool,
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
            send_enabled,
            remote_version_two: None,
            action_version_two: None,
            calendar: None,
            account_mode: false,
        }
    }
    pub(super) fn enable_calendar(&mut self) {
        self.calendar = Some(CalendarFixture::new(
            self.accounts.clone(),
            self.base,
            self.evidence.clone(),
        ));
    }
    pub(super) fn enable_occurrences(&mut self) {
        self.calendar = Some(CalendarFixture::occurrences(
            self.accounts.clone(),
            self.base,
            self.evidence.clone(),
        ));
    }
    pub(super) fn enable_accounts(&mut self) {
        self.account_mode = true;
        self.calendar = Some(CalendarFixture::with_count(
            self.accounts.clone(),
            self.base,
            self.evidence.clone(),
            1,
        ));
    }
    pub(super) fn enable_remote_drafts(&mut self, version_two: Arc<AtomicBool>) {
        self.remote_version_two = Some(version_two);
        for (account, state) in self.accounts.iter().zip(&mut self.states) {
            let state = state.get_mut().unwrap();
            for index in 0..remote_drafts::COUNT {
                let version = if index == 1 { 2 } else { 1 };
                state.drafts.insert(
                    format!("rd{index:04}"),
                    FixtureDraft {
                        message_id: format!("rm{index:04}v{version}"),
                        thread_id: format!("rt{index:04}"),
                        raw: remote_drafts::source(account.key, index, version, self.base),
                    },
                );
            }
        }
    }
    pub(super) fn enable_draft_actions(&mut self, version_two: Arc<AtomicBool>) {
        self.action_version_two = Some(version_two);
        for (account, state) in self.accounts.iter().zip(&mut self.states) {
            let state = state.get_mut().unwrap();
            for index in 0..draft_actions::COUNT {
                state.drafts.insert(
                    format!("ra{index:04}"),
                    FixtureDraft {
                        message_id: format!("ram{index:04}v1"),
                        thread_id: format!("rat{index:04}"),
                        raw: draft_actions::source(account.key, index, 1, self.base),
                    },
                );
            }
        }
    }
    fn writable_draft_id(&self, id: &str) -> bool {
        fixture_draft_id(id)
            || (self.action_version_two.is_some() && draft_actions::index(id).is_some())
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
        if self.remote_version_two.is_some() && request.method() != HttpMethod::Get {
            return Err(self.reject("remote_drafts_write_refused"));
        }
        if request.method() == HttpMethod::Get && request.body_bytes().is_empty() {
            match path {
                "profile" if params.is_empty() => return Ok((account, Route::Profile)),
                "labels" if params.is_empty() => return Ok((account, Route::Labels)),
                "drafts" => {
                    if params.get("maxResults") != Some(&"50")
                        || params
                            .keys()
                            .any(|key| !["maxResults", "pageToken"].contains(key))
                    {
                        return Err(self.reject("unexpected_synthetic_draft_list_query"));
                    }
                    let count = self.states[account].lock().unwrap().drafts.len();
                    let offset = match params.get("pageToken") {
                        None => 0,
                        Some(value) => value
                            .strip_prefix("drafts-")
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|offset| *offset > 0 && *offset < count && offset % 50 == 0)
                            .ok_or_else(|| self.reject("unexpected_synthetic_draft_cursor"))?,
                    };
                    return Ok((account, Route::DraftList(offset)));
                }
                "messages" => {
                    if self.send_enabled
                        && params.get("maxResults") == Some(&"10")
                        && params.get("includeSpamTrash") == Some(&"false")
                        && params.len() == 3
                    {
                        let query = params
                            .get("q")
                            .and_then(|query| decoded_query(query))
                            .and_then(|query| {
                                query
                                    .strip_prefix("in:sent rfc822msgid:")
                                    .map(str::to_owned)
                            })
                            .filter(|id| {
                                !id.is_empty()
                                    && id.len() <= 512
                                    && !id.chars().any(char::is_control)
                            })
                            .ok_or_else(|| self.reject("unexpected_synthetic_sent_query"))?;
                        return Ok((account, Route::SentList(query)));
                    }
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
                    } + self.states[account].lock().unwrap().sent.len();
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
                if self.send_enabled && fixture_sent_id(id) {
                    if params.len() == 1 && params.get("format") == Some(&"raw") {
                        return Ok((
                            account,
                            Route::SentGet {
                                id: id.into(),
                                raw: true,
                            },
                        ));
                    }
                    if params.len() == 2
                        && params.get("format") == Some(&"full")
                        && params.get("fields") == Some(&"id%2CthreadId%2ClabelIds%2ChistoryId")
                    {
                        return Ok((
                            account,
                            Route::SentGet {
                                id: id.into(),
                                raw: false,
                            },
                        ));
                    }
                }
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
                if (self.writable_draft_id(id)
                    || (self.remote_version_two.is_some() && remote_drafts::index(id).is_some()))
                    && params.len() == 1
                    && params.get("format") == Some(&"raw")
                {
                    return Ok((account, Route::DraftGet(id.into())));
                }
            }
        }
        if self.action_version_two.is_some()
            && request.method() == HttpMethod::Delete
            && params.is_empty()
            && request.body_bytes().is_empty()
        {
            if let Some(id) = path
                .strip_prefix("drafts/")
                .filter(|id| self.writable_draft_id(id))
            {
                return Ok((account, Route::DraftDelete(id.into())));
            }
        }
        if params.is_empty()
            && matches!(request.method(), HttpMethod::Post | HttpMethod::Put)
            && request.body_bytes().len() <= MAX_DRAFT_BYTES.div_ceil(3) * 4 + 1024
        {
            if self.send_enabled
                && request.method() == HttpMethod::Post
                && matches!(path, "messages/send" | "drafts/send")
            {
                let body: Value = serde_json::from_slice(request.body_bytes())
                    .map_err(|_| self.reject("invalid_synthetic_send_body"))?;
                let object = body
                    .as_object()
                    .ok_or_else(|| self.reject("invalid_synthetic_send_body"))?;
                let (id, message) = if path == "drafts/send" {
                    if object.len() != 2 || object.keys().any(|key| key != "id" && key != "message")
                    {
                        return Err(self.reject("unexpected_synthetic_send_fields"));
                    }
                    let id = body["id"]
                        .as_str()
                        .filter(|id| self.writable_draft_id(id))
                        .ok_or_else(|| self.reject("invalid_synthetic_send_target"))?;
                    (Some(id.to_owned()), &body["message"])
                } else {
                    (None, &body)
                };
                let (raw, thread) = authored_message(message)
                    .ok_or_else(|| self.reject("invalid_synthetic_send_mime"))?;
                let rfc_message_id = rfc_message_id(&raw)
                    .ok_or_else(|| self.reject("invalid_synthetic_send_message_id"))?;
                return Ok((
                    account,
                    Route::Send {
                        id,
                        raw,
                        thread,
                        rfc_message_id,
                    },
                ));
            }
            let draft_id = if path == "drafts" && request.method() == HttpMethod::Post {
                Some(None)
            } else {
                path.strip_prefix("drafts/")
                    .filter(|id| self.writable_draft_id(id) && request.method() == HttpMethod::Put)
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
                    json!({"emailAddress":owner.email,"messagesTotal":if self.account_mode {2}else{RECENT_MESSAGES+BACKFILL_MESSAGES+state.sent.len()},"threadsTotal":if self.account_mode {1}else{RECENT_MESSAGES/2+BACKFILL_MESSAGES+state.sent.len()},"historyId":state.history.to_string()})
                }
                Route::Labels => {
                    json!({"labels":[{"id":"INBOX","name":"INBOX","type":"system"},{"id":"UNREAD","name":"UNREAD","type":"system"}]})
                }
                Route::List { recent, offset } => {
                    let base_count = if self.account_mode {
                        2
                    } else if recent {
                        RECENT_MESSAGES
                    } else {
                        RECENT_MESSAGES + BACKFILL_MESSAGES
                    };
                    let count = base_count + state.sent.len();
                    let end = (offset + 100).min(count);
                    json!({"messages":(offset..end).map(|ordinal|{
                        if ordinal < base_count { let message=Message::at(ordinal);json!({"id":message.id(),"threadId":message.thread()}) }
                        else { let (id,sent)=state.sent.iter().nth(ordinal-base_count).unwrap(); json!({"id":id,"threadId":sent.thread_id}) }
                    }).collect::<Vec<_>>(),"resultSizeEstimate":count,
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
                    let mut history: Vec<_> = changes.into_iter().map(|(at,index)|{
                    let message=Message{recent:true,index}; json!({"id":at.to_string(),"labelsRemoved":[{"message":{"id":message.id(),"threadId":message.thread()},"labelIds":["UNREAD"]}]})
                    }).collect();
                    history.extend(state.sent.iter().filter(|(_,sent)|sent.history>start).map(|(id,sent)|json!({"id":sent.history.to_string(),"messagesAdded":[{"message":{"id":id,"threadId":sent.thread_id,"labelIds":["SENT"]}}]})));
                    history
                        .sort_by_key(|item| item["id"].as_str().unwrap().parse::<u64>().unwrap());
                    json!({"historyId":state.history.to_string(),"history":history})
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
                    if self.remote_version_two.is_some() && id == "rd0002" {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated remote draft is absent"}}),
                        ));
                    }
                    if id == "rd0000"
                        && self
                            .remote_version_two
                            .as_ref()
                            .is_some_and(|flag| flag.load(Ordering::Acquire))
                    {
                        state.drafts.insert(
                            id.clone(),
                            FixtureDraft {
                                message_id: "rm0000v2".into(),
                                thread_id: "rt0000".into(),
                                raw: remote_drafts::source(owner.key, 0, 2, self.base),
                            },
                        );
                    }
                    if id == "ra0003"
                        && self
                            .action_version_two
                            .as_ref()
                            .is_some_and(|flag| flag.load(Ordering::Acquire))
                        && state
                            .drafts
                            .get(&id)
                            .is_some_and(|draft| draft.message_id == "ram0003v1")
                    {
                        state.drafts.insert(
                            id.clone(),
                            FixtureDraft {
                                message_id: "ram0003v2".into(),
                                thread_id: "rat0003".into(),
                                raw: draft_actions::source(owner.key, 3, 2, self.base),
                            },
                        );
                    }
                    let Some(draft) = state.drafts.get(&id) else {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated draft is absent"}}),
                        ));
                    };
                    if self.remote_version_two.is_some() {
                        let row = json!({"account":owner.id,"draft_id":id,"message_id":draft.message_id,"thread_id":draft.thread_id,"size":draft.raw.len(),"sha256":format!("{:x}", Sha256::digest(&draft.raw))});
                        let mut evidence = self.evidence.lock().unwrap();
                        if !evidence.remote_draft_reads.contains(&row) {
                            if evidence.remote_draft_reads.len() >= 2 * (remote_drafts::COUNT + 1) {
                                evidence.error("remote_draft_read_trace_limit");
                                return Err(TransportError::InvalidRequest);
                            }
                            evidence.remote_draft_reads.push(row);
                        }
                    }
                    if self.action_version_two.is_some() {
                        let row = json!({"account":owner.id,"draft_id":id,"message_id":draft.message_id,"thread_id":draft.thread_id,"size":draft.raw.len(),"sha256":format!("{:x}", Sha256::digest(&draft.raw))});
                        let mut evidence = self.evidence.lock().unwrap();
                        if !evidence.draft_action_reads.contains(&row) {
                            if evidence.draft_action_reads.len() >= MAX_DRAFT_PUBLICATIONS {
                                evidence.error("draft_action_read_trace_limit");
                                return Err(TransportError::InvalidRequest);
                            }
                            evidence.draft_action_reads.push(row);
                        }
                    }
                    draft.value(&id, true)
                }
                Route::DraftDelete(id) => {
                    let existed = state.drafts.remove(&id).is_some();
                    let mut evidence = self.evidence.lock().unwrap();
                    evidence.counters.draft_delete_attempts += 1;
                    if evidence.draft_deletions.len() >= MAX_DRAFT_PUBLICATIONS {
                        evidence.error("draft_delete_trace_limit");
                        return Err(TransportError::InvalidRequest);
                    }
                    let lost = id == "ra0004" && existed;
                    evidence.draft_deletions.push(json!({"account":owner.id,"draft_id":id,"existed":existed,"response":if lost {"lost_response"} else if existed {"acknowledged"} else {"absent"}}));
                    if lost {
                        evidence.counters.draft_delete_unknown += 1;
                        return Err(TransportError::Network);
                    }
                    if !existed {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated draft is absent"}}),
                        ));
                    }
                    evidence.counters.draft_deleted += 1;
                    return Ok((204, json!({})));
                }
                Route::DraftList(offset) => {
                    self.evidence.lock().unwrap().counters.draft_list += 1;
                    let count = state.drafts.len();
                    let end = (offset + 50).min(count);
                    let rows = state
                        .drafts
                        .iter()
                        .skip(offset)
                        .take(50)
                        .map(|(id, draft)| {
                            let mut row = draft.value(id, false);
                            // A list can legitimately race a subsequent replacement.
                            if self.remote_version_two.is_some() && id == "rd0001" {
                                row["message"]["id"] = json!("rm0001v1");
                            }
                            row
                        })
                        .collect::<Vec<_>>();
                    json!({"drafts":rows,"nextPageToken":(end<count).then(||format!("drafts-{end}")),"resultSizeEstimate":count})
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
                    let id = id.unwrap_or_else(|| {
                        state.draft_sequence += 1;
                        format!("vd{:04}", state.draft_sequence)
                    });
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
                Route::Send {
                    id,
                    raw,
                    thread,
                    rfc_message_id,
                } => {
                    if state.sent.len() >= MAX_SENDS {
                        return Err(self.reject("synthetic_send_count_limit"));
                    }
                    if id.as_ref().is_some_and(|id| !state.drafts.contains_key(id)) {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated draft is absent"}}),
                        ));
                    }
                    let message_id = format!("vms{:04}", state.sent.len() + 1);
                    let thread_id = thread
                        .or_else(|| {
                            id.as_ref()
                                .and_then(|id| state.drafts.get(id))
                                .map(|draft| draft.thread_id.clone())
                        })
                        .unwrap_or_else(|| format!("vst-{message_id}"));
                    state.history += 1;
                    let sent = FixtureSent {
                        raw,
                        thread_id,
                        rfc_message_id,
                        history: state.history,
                    };
                    let response = sent.value(&message_id, false, self.base);
                    let mut evidence = self.evidence.lock().unwrap();
                    evidence.counters.send_completed += 1;
                    evidence.send_publications.push(json!({"account":owner.id,"provider_draft_id":id,"message_id":message_id,
                        "thread_id":sent.thread_id,"rfc_message_id":sent.rfc_message_id,"size":sent.raw.len(),
                        "sha256":format!("{:x}",Sha256::digest(&sent.raw)),"kind":if id.is_some(){"draft_send"}else{"message_send"}}));
                    if let Some(id) = id {
                        state.drafts.remove(&id);
                    }
                    state.sent.insert(message_id, sent);
                    response
                }
                Route::SentList(query) => {
                    let matches: Vec<_> = state
                        .sent
                        .iter()
                        .filter(|(_, sent)| sent.rfc_message_id == query)
                        .map(|(id, sent)| json!({"id":id,"threadId":sent.thread_id}))
                        .collect();
                    json!({"messages":matches.iter().take(10).collect::<Vec<_>>(),"resultSizeEstimate":matches.len(),"nextPageToken":(matches.len()>10).then_some("sent-truncated")})
                }
                Route::SentGet { id, raw } => {
                    self.evidence.lock().unwrap().counters.sent_read += 1;
                    let Some(sent) = state.sent.get(&id) else {
                        return Ok((
                            404,
                            json!({"error":{"code":404,"message":"Generated sent message is absent"}}),
                        ));
                    };
                    sent.value(&id, raw, self.base)
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
    DraftDelete(String),
    DraftList(usize),
    DraftWrite {
        id: Option<String>,
        raw: Vec<u8>,
        thread: Option<String>,
    },
    Send {
        id: Option<String>,
        raw: Vec<u8>,
        thread: Option<String>,
        rfc_message_id: String,
    },
    SentList(String),
    SentGet {
        id: String,
        raw: bool,
    },
}
fn fixture_sent_id(id: &str) -> bool {
    id.strip_prefix("vms")
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}
fn authored_message(value: &Value) -> Option<(Vec<u8>, Option<String>)> {
    let message = value.as_object()?;
    if message.keys().any(|key| key != "raw" && key != "threadId") {
        return None;
    }
    let raw = URL_SAFE_NO_PAD.decode(message.get("raw")?.as_str()?).ok()?;
    if raw.is_empty() || raw.len() > MAX_DRAFT_BYTES {
        return None;
    }
    let thread = match message.get("threadId") {
        None => None,
        Some(Value::String(thread))
            if !thread.is_empty()
                && thread.len() <= 256
                && !thread.chars().any(char::is_control) =>
        {
            Some(thread.clone())
        }
        _ => return None,
    };
    Some((raw, thread))
}
fn rfc_message_id(raw: &[u8]) -> Option<String> {
    let end = raw.windows(4).position(|value| value == b"\r\n\r\n")?;
    if end > 64 * 1024 {
        return None;
    }
    let headers = std::str::from_utf8(&raw[..end]).ok()?;
    let mut found: Option<String> = None;
    let mut in_message_id = false;
    for line in headers.split("\r\n") {
        if line.starts_with([' ', '\t']) {
            if in_message_id {
                found.as_mut()?.push_str(line.trim());
            }
            continue;
        }
        let (key, value) = line.split_once(':')?;
        in_message_id = key.eq_ignore_ascii_case("message-id");
        if in_message_id {
            if found.is_some() {
                return None;
            }
            found = Some(value.trim().to_owned());
        }
    }
    let value = found?;
    let id = value.strip_prefix('<')?.strip_suffix('>')?;
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return None;
    }
    Some(id.into())
}
pub(super) fn decoded_query(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let digits = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
                decoded.push(u8::from_str_radix(digits, 16).ok()?);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            value => {
                decoded.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
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
            if let Some(calendar) = self.calendar.as_ref().filter(|_| {
                !self.account_mode
                    || request
                        .url()
                        .starts_with("https://www.googleapis.com/calendar/v3/")
            }) {
                let result = calendar.response(&request);
                active.finish(result.is_ok());
                return result;
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
                (if status == 204 {
                    Ok(Vec::new())
                } else {
                    serde_json::to_vec(&value)
                })
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
