//! Costed, account-bound Gmail calls. No store, retry loop or implicit submission.
use crate::scheduler::{Service, Work, WorkClass};
use crate::send::SubmissionResult;
use crate::sync::{HistoryPage, ListPage, MessageRef, RawMessage};
use crate::transport::{
    CancellationToken, HttpMethod, HttpRequest, HttpResponse, HttpTransport, TransportError,
};
use crate::{decode_json, AccountId, GoogleError};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use oauth2::url::Url;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fmt, sync::Arc};

const MAX_JSON: usize = 4 * 1024 * 1024;
pub const MAX_RAW_MIME: usize = 40 * 1024 * 1024;
const MAX_RAW_JSON: usize = MAX_RAW_MIME.div_ceil(3) * 4 + 64 * 1024;
pub const MAX_SUBMITTED_MIME: usize = 35_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GmailMethod {
    Profile,
    MessagesList,
    HistoryList,
    MessageRaw,
    MessageState,
    MessageMetadata,
    LabelsList,
    LabelCreate,
    MessageModify,
    MessageTrash,
    MessageRestore,
    DraftsList,
    DraftGet,
    DraftCreate,
    DraftUpdate,
    DraftDelete,
    MessageSend,
    DraftSend,
}
impl GmailMethod {
    /// Google's method-unit table checked 2026-09-13; project limits are separate.
    pub fn quota_units(self) -> u32 {
        match self {
            Self::Profile | Self::LabelsList => 1,
            Self::HistoryList => 2,
            Self::MessagesList
            | Self::MessageModify
            | Self::MessageRestore
            | Self::DraftsList
            | Self::LabelCreate
            | Self::MessageMetadata => 5,
            Self::DraftCreate | Self::DraftDelete => 10,
            Self::DraftUpdate => 15,
            Self::MessageRaw | Self::MessageState | Self::MessageTrash | Self::DraftGet => 20,
            Self::MessageSend | Self::DraftSend => 100,
        }
    }
    fn class(self) -> WorkClass {
        match self {
            Self::MessageModify | Self::MessageTrash | Self::MessageRestore | Self::LabelCreate => {
                WorkClass::StateMutation
            }
            Self::DraftCreate | Self::DraftUpdate | Self::DraftDelete => WorkClass::DraftMutation,
            Self::MessageSend | Self::DraftSend => WorkClass::Submission,
            _ => WorkClass::UserRead,
        }
    }
    fn is_send(self) -> bool {
        matches!(self, Self::MessageSend | Self::DraftSend)
    }
}

pub struct GmailCall {
    request: HttpRequest,
    work: Work,
    method: GmailMethod,
    expected_id: Option<String>,
    previous_page: Option<String>,
}
impl fmt::Debug for GmailCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GmailCall")
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}
impl GmailCall {
    pub fn create_label(account: &AccountId, name: &str) -> Result<Self, GoogleError> {
        checked(name, 256)?;
        Self::new(
            account,
            GmailMethod::LabelCreate,
            HttpMethod::Post,
            &["labels"],
            &[],
            Some(
                json!({"name":name,"labelListVisibility":"labelShow","messageListVisibility":"show"}),
            ),
        )
    }
    fn new(
        account: &AccountId,
        method: GmailMethod,
        verb: HttpMethod,
        segments: &[&str],
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Self, GoogleError> {
        let mut url = Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/").unwrap();
        {
            let mut path = url.path_segments_mut().map_err(|_| invalid())?;
            path.pop_if_empty();
            for segment in segments {
                checked(segment, 256)?;
                if matches!(*segment, "." | "..") {
                    return Err(invalid());
                }
                path.push(segment);
            }
        }
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        let cap = if matches!(method, GmailMethod::MessageRaw | GmailMethod::DraftGet) {
            MAX_RAW_JSON
        } else {
            MAX_JSON
        };
        let mut request = HttpRequest::new(verb, url.as_str())
            .map_err(|_| invalid())?
            .response_limit(cap);
        if let Some(body) = body {
            request = request
                .body(
                    serde_json::to_vec(&body).map_err(|_| invalid())?,
                    "application/json",
                )
                .map_err(|_| invalid())?;
        }
        Ok(Self {
            request,
            work: Work {
                account: account.clone(),
                service: Service::Gmail,
                class: method.class(),
                units: method.quota_units(),
            },
            method,
            expected_id: None,
            previous_page: None,
        })
    }
    pub fn work(&self) -> Work {
        self.work.clone()
    }
    pub fn method(&self) -> GmailMethod {
        self.method
    }
    /// Validated, percent-encoded API target, before authorization is attached.
    pub fn request_url(&self) -> &str {
        self.request.url()
    }
    pub fn with_read_class(mut self, class: WorkClass) -> Result<Self, GoogleError> {
        if !self.work.class.is_read() || !class.is_read() {
            return Err(invalid());
        }
        self.work.class = class;
        Ok(self)
    }
    pub fn profile(account: &AccountId) -> Result<Self, GoogleError> {
        Self::new(
            account,
            GmailMethod::Profile,
            HttpMethod::Get,
            &["profile"],
            &[],
            None,
        )
    }
    pub fn list_messages(
        account: &AccountId,
        page: Option<&str>,
        query: Option<&str>,
        include_spam_trash: bool,
        max_results: u16,
    ) -> Result<Self, GoogleError> {
        let size = page_size(max_results)?;
        let mut params = vec![
            ("maxResults", size.as_str()),
            (
                "includeSpamTrash",
                if include_spam_trash { "true" } else { "false" },
            ),
        ];
        optional_param(&mut params, "pageToken", page, 4096)?;
        optional_param(&mut params, "q", query, 4096)?;
        let mut call = Self::new(
            account,
            GmailMethod::MessagesList,
            HttpMethod::Get,
            &["messages"],
            &params,
            None,
        )?;
        call.previous_page = page.map(str::to_owned);
        Ok(call)
    }
    pub fn history(
        account: &AccountId,
        start: &str,
        page: Option<&str>,
        max_results: u16,
    ) -> Result<Self, GoogleError> {
        history_id(start)?;
        let size = page_size(max_results)?;
        let mut params = vec![("startHistoryId", start), ("maxResults", size.as_str())];
        optional_param(&mut params, "pageToken", page, 4096)?;
        let mut call = Self::new(
            account,
            GmailMethod::HistoryList,
            HttpMethod::Get,
            &["history"],
            &params,
            None,
        )?;
        call.previous_page = page.map(str::to_owned);
        call.work.class = WorkClass::DeltaRead;
        Ok(call)
    }
    pub fn raw_message(account: &AccountId, id: &str) -> Result<Self, GoogleError> {
        let mut call = Self::new(
            account,
            GmailMethod::MessageRaw,
            HttpMethod::Get,
            &["messages", id],
            &[("format", "raw")],
            None,
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    pub fn labels(account: &AccountId) -> Result<Self, GoogleError> {
        Self::new(
            account,
            GmailMethod::LabelsList,
            HttpMethod::Get,
            &["labels"],
            &[],
            None,
        )
    }
    /// Read only the versioned label projection; no MIME/body transfer is needed.
    /// Use a partial response on the full resource because `minimal` documents
    /// only ID/labels and does not guarantee the history/thread fence fields.
    pub fn message_state(account: &AccountId, id: &str) -> Result<Self, GoogleError> {
        let mut call = Self::new(
            account,
            GmailMethod::MessageState,
            HttpMethod::Get,
            &["messages", id],
            &[
                ("format", "full"),
                ("fields", "id,threadId,labelIds,historyId"),
            ],
            None,
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    /// Headers and Google's snippet for a live listing, such as Spam, that is
    /// shown without downloading or keeping the message.
    pub fn message_metadata(account: &AccountId, id: &str) -> Result<Self, GoogleError> {
        let mut call = Self::new(
            account,
            GmailMethod::MessageMetadata,
            HttpMethod::Get,
            &["messages", id],
            &[
                ("format", "metadata"),
                ("metadataHeaders", "From"),
                ("metadataHeaders", "Subject"),
                ("metadataHeaders", "Date"),
                (
                    "fields",
                    "id,threadId,labelIds,snippet,internalDate,payload/headers",
                ),
            ],
            None,
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    /// One explicit existing message. Never uses threads.modify, which could capture a new arrival.
    pub fn modify_message(
        account: &AccountId,
        id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<Self, GoogleError> {
        if add.len() > 100 || remove.len() > 100 || add.is_empty() && remove.is_empty() {
            return Err(invalid());
        }
        for label in add.iter().chain(remove) {
            checked(label, 256)?;
        }
        if add.iter().any(|label| remove.contains(label)) {
            return Err(invalid());
        }
        let mut call = Self::new(
            account,
            GmailMethod::MessageModify,
            HttpMethod::Post,
            &["messages", id, "modify"],
            &[],
            Some(json!({"addLabelIds": add, "removeLabelIds": remove})),
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    pub fn trash_message(
        account: &AccountId,
        id: &str,
        restore: bool,
    ) -> Result<Self, GoogleError> {
        let method = if restore {
            GmailMethod::MessageRestore
        } else {
            GmailMethod::MessageTrash
        };
        let action = if restore { "untrash" } else { "trash" };
        let mut call = Self::new(
            account,
            method,
            HttpMethod::Post,
            &["messages", id, action],
            &[],
            None,
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    pub fn list_drafts(
        account: &AccountId,
        page: Option<&str>,
        max_results: u16,
    ) -> Result<Self, GoogleError> {
        let size = page_size(max_results)?;
        let mut params = vec![("maxResults", size.as_str())];
        optional_param(&mut params, "pageToken", page, 4096)?;
        let mut call = Self::new(
            account,
            GmailMethod::DraftsList,
            HttpMethod::Get,
            &["drafts"],
            &params,
            None,
        )?;
        call.previous_page = page.map(str::to_owned);
        Ok(call)
    }
    pub fn get_draft(account: &AccountId, id: &str) -> Result<Self, GoogleError> {
        let mut call = Self::new(
            account,
            GmailMethod::DraftGet,
            HttpMethod::Get,
            &["drafts", id],
            &[("format", "raw")],
            None,
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    pub fn create_draft(
        account: &AccountId,
        mime: &[u8],
        thread: Option<&str>,
    ) -> Result<Self, GoogleError> {
        Self::new(
            account,
            GmailMethod::DraftCreate,
            HttpMethod::Post,
            &["drafts"],
            &[],
            Some(json!({"message": authored(mime, thread)?})),
        )
    }
    /// Caller has already checked/preserved local and fetched remote versions.
    /// Google documents no revision precondition here; this is not CAS.
    pub fn update_draft(
        account: &AccountId,
        id: &str,
        mime: &[u8],
        thread: Option<&str>,
    ) -> Result<Self, GoogleError> {
        let mut call = Self::new(
            account,
            GmailMethod::DraftUpdate,
            HttpMethod::Put,
            &["drafts", id],
            &[],
            Some(json!({"id": id, "message": authored(mime, thread)?})),
        )?;
        call.expected_id = Some(id.into());
        Ok(call)
    }
    pub fn delete_draft(account: &AccountId, id: &str) -> Result<Self, GoogleError> {
        Self::new(
            account,
            GmailMethod::DraftDelete,
            HttpMethod::Delete,
            &["drafts", id],
            &[],
            None,
        )
    }
    /// Caller must have persisted frozen MIME/account/recipients and InFlight.
    pub fn send_message(
        account: &AccountId,
        mime: &[u8],
        thread: Option<&str>,
    ) -> Result<Self, GoogleError> {
        Self::new(
            account,
            GmailMethod::MessageSend,
            HttpMethod::Post,
            &["messages", "send"],
            &[],
            Some(authored(mime, thread)?),
        )
    }
    /// Include frozen MIME rather than sending whichever content another client
    /// may have last stored under this draft ID.
    pub fn send_draft(
        account: &AccountId,
        id: &str,
        mime: &[u8],
        thread: Option<&str>,
    ) -> Result<Self, GoogleError> {
        checked(id, 256)?;
        Self::new(
            account,
            GmailMethod::DraftSend,
            HttpMethod::Post,
            &["drafts", "send"],
            &[],
            Some(json!({"id": id, "message": authored(mime, thread)?})),
        )
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailProfile {
    pub email_address: String,
    pub messages_total: u64,
    pub threads_total: u64,
    pub history_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: String,
    pub message_list_visibility: Option<String>,
    pub label_list_visibility: Option<String>,
}
#[derive(Deserialize)]
pub struct GmailLabels {
    #[serde(default)]
    pub labels: Vec<GmailLabel>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailDraftPage {
    #[serde(default)]
    pub drafts: Vec<GmailDraft>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u64>,
}
#[derive(Deserialize)]
pub struct GmailDraft {
    pub id: String,
    pub message: DraftMessage,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftMessage {
    pub id: String,
    pub thread_id: String,
    pub raw: Option<String>,
    pub history_id: Option<String>,
    #[serde(default)]
    pub label_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawItem {
    #[serde(flatten)]
    pub message: RawMessage,
    pub internal_date: Option<String>,
    pub size_estimate: Option<u64>,
}
pub enum RawOutcome {
    Available(RawItem),
    Missing,
    TooLarge,
    Malformed,
}
/// Submission certainty and provider scheduling/auth metadata are independent.
/// An error may pause work or require reconnect; it never authorizes a resend.
pub struct GmailSubmission {
    pub result: SubmissionResult,
    pub error: Option<GoogleError>,
}
/// An exact draft deletion has its own certainty model. Absence is read evidence,
/// never an acknowledgement of this particular request.
#[derive(Debug, PartialEq, Eq)]
pub enum DraftDeletionResult {
    Deleted,
    Absent,
    Rejected,
    NoDefinitiveResponse,
}
pub struct GmailDeletion {
    pub result: DraftDeletionResult,
    pub error: Option<GoogleError>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessageState {
    pub id: String,
    pub thread_id: String,
    pub history_id: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessageMetadata {
    pub id: String,
    pub thread_id: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub internal_date: Option<String>,
    #[serde(default)]
    pub payload: GmailMetadataPayload,
}
#[derive(Deserialize, Default)]
pub struct GmailMetadataPayload {
    #[serde(default)]
    pub headers: Vec<GmailHeader>,
}
#[derive(Deserialize)]
pub struct GmailHeader {
    pub name: String,
    pub value: String,
}
impl GmailMessageMetadata {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.payload
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str())
    }
}
pub enum GmailReply {
    Profile(GmailProfile),
    Messages(ListPage),
    History(HistoryPage),
    Raw(RawOutcome),
    MessageState(Option<GmailMessageState>),
    /// None when the message is gone.
    MessageMetadata(Option<GmailMessageMetadata>),
    Labels(GmailLabels),
    Label(GmailLabel),
    /// HTTP 400/409 rejected creation; a malformed success remains uncertain.
    LabelCreateRejected,
    Drafts(GmailDraftPage),
    /// A supplied draft inventory continuation was rejected. Restarting the
    /// read enumeration must preserve already retained snapshots.
    DraftPageExpired,
    Draft(GmailDraft),
    Message(MessageRef),
    DraftDeletion(GmailDeletion),
    Submission(GmailSubmission),
}
impl fmt::Debug for GmailReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GmailReply { private provider data }")
    }
}

pub struct GmailApi<T: HttpTransport> {
    account: AccountId,
    transport: Arc<T>,
}
impl<T: HttpTransport> GmailApi<T> {
    pub fn new(account: AccountId, transport: Arc<T>) -> Self {
        Self { account, transport }
    }
    /// Execute exactly once after quota admission. This never opens auth, changes
    /// local state or treats an accepted receipt as proof of eventual delivery.
    pub async fn execute(
        &self,
        call: GmailCall,
        access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<GmailReply, GoogleError> {
        if call.work.account != self.account {
            return Err(GoogleError::PermissionDenied);
        }
        checked(access_token, 8192)?;
        let request = call
            .request
            .header("authorization", &format!("Bearer {access_token}"))
            .map_err(|_| invalid())?;
        let response = self.transport.execute(request, cancel).await;
        if call.method.is_send() {
            return Ok(GmailReply::Submission(submission(response)));
        }
        if call.method == GmailMethod::DraftDelete {
            return Ok(GmailReply::DraftDeletion(deletion(response)));
        }
        let response = match response {
            Err(TransportError::Cooldown { retry_after_secs }) => {
                return Err(GoogleError::RateLimited {
                    retry_after_secs: Some(retry_after_secs),
                })
            }
            Err(TransportError::ResponseTooLarge) if call.method == GmailMethod::MessageRaw => {
                return Ok(GmailReply::Raw(RawOutcome::TooLarge))
            }
            Err(TransportError::ResponseTooLarge) => {
                return Err(GoogleError::InvalidResponse(
                    "provider response exceeds byte limit",
                ))
            }
            Err(_) => return Err(GoogleError::TransportInterrupted),
            Ok(response) => response,
        };
        if response.status == 404 && call.method == GmailMethod::MessageRaw {
            return Ok(GmailReply::Raw(RawOutcome::Missing));
        }
        if matches!(response.status, 400 | 409) && call.method == GmailMethod::LabelCreate {
            return Ok(GmailReply::LabelCreateRejected);
        }
        if response.status == 404 && call.method == GmailMethod::MessageState {
            return Ok(GmailReply::MessageState(None));
        }
        if response.status == 404 && call.method == GmailMethod::MessageMetadata {
            return Ok(GmailReply::MessageMetadata(None));
        }
        if response.status == 400
            && call.method == GmailMethod::DraftsList
            && call.previous_page.is_some()
        {
            return Ok(GmailReply::DraftPageExpired);
        }
        if !(200..300).contains(&response.status) {
            return Err(crate::classify_api_error(
                response.status,
                &response.body,
                response.retry_after_secs,
                call.method == GmailMethod::HistoryList,
            ));
        }
        let bytes = &response.body;
        match call.method {
            GmailMethod::Profile => {
                let value: GmailProfile = decode_json(bytes, MAX_JSON)?;
                checked(&value.email_address, 320)?;
                history_id(&value.history_id)?;
                if !value.email_address.contains('@') {
                    return Err(invalid());
                }
                Ok(GmailReply::Profile(value))
            }
            GmailMethod::MessagesList => {
                let value: ListPage = decode_json(bytes, MAX_JSON)?;
                page_token(
                    value.next_page_token.as_deref(),
                    call.previous_page.as_deref(),
                )?;
                if value.messages.len() > 500 {
                    return Err(invalid());
                }
                for message in &value.messages {
                    message_ref(message)?;
                }
                Ok(GmailReply::Messages(value))
            }
            GmailMethod::HistoryList => {
                let value: HistoryPage = decode_json(bytes, MAX_JSON)?;
                page_token(
                    value.next_page_token.as_deref(),
                    call.previous_page.as_deref(),
                )?;
                history_id(&value.history_id)?;
                if value.history.len() > 500 {
                    return Err(invalid());
                }
                // Existing prepare_history_page validates range/order/changes
                // against the owning checkpoint before any atomic publication.
                Ok(GmailReply::History(value))
            }
            GmailMethod::MessageRaw => Ok(GmailReply::Raw(raw_outcome(
                bytes,
                call.expected_id.as_deref().unwrap(),
            ))),
            GmailMethod::MessageState => {
                let value: GmailMessageState = decode_json(bytes, MAX_JSON)?;
                checked(&value.id, 256)?;
                checked(&value.thread_id, 256)?;
                history_id(&value.history_id)?;
                if Some(value.id.as_str()) != call.expected_id.as_deref()
                    || value.label_ids.len() > 10_000
                {
                    return Err(invalid());
                }
                for label in &value.label_ids {
                    checked(label, 256)?;
                }
                Ok(GmailReply::MessageState(Some(value)))
            }
            GmailMethod::MessageMetadata => {
                let value: GmailMessageMetadata = decode_json(bytes, MAX_JSON)?;
                checked(&value.id, 256)?;
                checked(&value.thread_id, 256)?;
                if Some(value.id.as_str()) != call.expected_id.as_deref()
                    || value.label_ids.len() > 10_000
                    || value.payload.headers.len() > 100
                    || value.snippet.len() > 16 * 1024
                    || value
                        .payload
                        .headers
                        .iter()
                        .any(|header| header.name.len() > 256 || header.value.len() > 64 * 1024)
                {
                    return Err(invalid());
                }
                Ok(GmailReply::MessageMetadata(Some(value)))
            }
            GmailMethod::LabelsList => {
                let value: GmailLabels = decode_json(bytes, MAX_JSON)?;
                if value.labels.len() > 10_000 {
                    return Err(invalid());
                }
                for label in &value.labels {
                    checked(&label.id, 256)?;
                    checked(&label.name, 4096)?;
                    if !matches!(label.label_type.as_str(), "system" | "user") {
                        return Err(invalid());
                    }
                }
                Ok(GmailReply::Labels(value))
            }
            GmailMethod::LabelCreate => {
                let label: GmailLabel = decode_json(bytes, MAX_JSON)?;
                checked(&label.id, 256)?;
                checked(&label.name, 256)?;
                if label.label_type != "user" {
                    return Err(invalid());
                }
                Ok(GmailReply::Label(label))
            }
            GmailMethod::DraftsList => {
                let value: GmailDraftPage = decode_json(bytes, MAX_JSON)?;
                page_token(
                    value.next_page_token.as_deref(),
                    call.previous_page.as_deref(),
                )?;
                if value.drafts.len() > 500 {
                    return Err(invalid());
                }
                for draft in &value.drafts {
                    validate_draft(draft, None, false)?;
                }
                Ok(GmailReply::Drafts(value))
            }
            GmailMethod::DraftGet | GmailMethod::DraftCreate | GmailMethod::DraftUpdate => {
                let value: GmailDraft = decode_json(
                    bytes,
                    if call.method == GmailMethod::DraftGet {
                        MAX_RAW_JSON
                    } else {
                        MAX_JSON
                    },
                )?;
                validate_draft(
                    &value,
                    call.expected_id.as_deref(),
                    call.method == GmailMethod::DraftGet,
                )?;
                Ok(GmailReply::Draft(value))
            }
            GmailMethod::DraftDelete => unreachable!(),
            GmailMethod::MessageModify
            | GmailMethod::MessageTrash
            | GmailMethod::MessageRestore => {
                let value: MessageRef = decode_json(bytes, MAX_JSON)?;
                message_ref(&value)?;
                if Some(value.id.as_str()) != call.expected_id.as_deref() {
                    return Err(invalid());
                }
                Ok(GmailReply::Message(value))
            }
            GmailMethod::MessageSend | GmailMethod::DraftSend => unreachable!(),
        }
    }
}

fn deletion(response: Result<HttpResponse, TransportError>) -> GmailDeletion {
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return GmailDeletion {
                result: DraftDeletionResult::NoDefinitiveResponse,
                error: Some(match error {
                    TransportError::Cooldown { retry_after_secs } => GoogleError::RateLimited {
                        retry_after_secs: Some(retry_after_secs),
                    },
                    _ => GoogleError::TransportInterrupted,
                }),
            }
        }
    };
    if (200..300).contains(&response.status) {
        let empty = (response.status == 204 && response.body.is_empty())
            || (response.status == 200
                && serde_json::from_slice::<Value>(&response.body)
                    .ok()
                    .is_some_and(|v| v.as_object().is_some_and(|v| v.is_empty())));
        return GmailDeletion {
            result: if empty {
                DraftDeletionResult::Deleted
            } else {
                DraftDeletionResult::NoDefinitiveResponse
            },
            error: (!empty).then_some(GoogleError::InvalidResponse(
                "invalid Google deletion receipt",
            )),
        };
    }
    GmailDeletion {
        result: match response.status {
            404 => DraftDeletionResult::Absent,
            400 | 401 | 403 | 413 | 429 => DraftDeletionResult::Rejected,
            _ => DraftDeletionResult::NoDefinitiveResponse,
        },
        error: Some(crate::classify_api_error(
            response.status,
            &response.body,
            response.retry_after_secs,
            false,
        )),
    }
}

fn submission(response: Result<HttpResponse, TransportError>) -> GmailSubmission {
    let Ok(response) = response else {
        return GmailSubmission {
            result: SubmissionResult::NoDefinitiveResponse,
            error: Some(GoogleError::TransportInterrupted),
        };
    };
    if (200..300).contains(&response.status) {
        if let Ok(message) = decode_json::<MessageRef>(&response.body, MAX_JSON) {
            if message_ref(&message).is_ok() {
                return GmailSubmission {
                    result: SubmissionResult::Accepted {
                        message_id: message.id,
                        thread_id: message.thread_id,
                    },
                    error: None,
                };
            }
        }
        return GmailSubmission {
            result: SubmissionResult::NoDefinitiveResponse,
            error: Some(GoogleError::InvalidResponse(
                "invalid Google submission receipt",
            )),
        };
    }
    // Explicit client/auth/quota rejection; never automatically retry even then.
    let result = match response.status {
        400 | 401 | 403 | 404 | 413 | 429 => SubmissionResult::Rejected {
            reason: "Google rejected submission",
        },
        _ => SubmissionResult::NoDefinitiveResponse,
    };
    GmailSubmission {
        result,
        error: Some(crate::classify_api_error(
            response.status,
            &response.body,
            response.retry_after_secs,
            false,
        )),
    }
}
fn raw_outcome(bytes: &[u8], expected: &str) -> RawOutcome {
    if bytes.len() > MAX_RAW_JSON {
        return RawOutcome::TooLarge;
    }
    let Ok(value) = decode_json::<RawItem>(bytes, MAX_RAW_JSON) else {
        return RawOutcome::Malformed;
    };
    if value.message.raw.len() > MAX_RAW_MIME.div_ceil(3) * 4 {
        return RawOutcome::TooLarge;
    }
    if value.message.id != expected
        || checked(&value.message.id, 256).is_err()
        || checked(&value.message.thread_id, 256).is_err()
        || history_id(&value.message.history_id).is_err()
        || value.message.label_ids.len() > 1000
        || value
            .message
            .label_ids
            .iter()
            .any(|v| checked(v, 256).is_err())
        || value
            .internal_date
            .as_ref()
            .is_some_and(|v| v.parse::<i64>().is_err())
        || !valid_raw_encoding(&value.message.raw)
    {
        return RawOutcome::Malformed;
    }
    RawOutcome::Available(value)
}
fn validate_draft(
    draft: &GmailDraft,
    expected: Option<&str>,
    raw_required: bool,
) -> Result<(), GoogleError> {
    checked(&draft.id, 256)?;
    checked(&draft.message.id, 256)?;
    checked(&draft.message.thread_id, 256)?;
    if expected.is_some_and(|id| id != draft.id)
        || raw_required && draft.message.raw.is_none()
        || draft
            .message
            .raw
            .as_ref()
            .is_some_and(|v| v.len() > MAX_RAW_MIME.div_ceil(3) * 4 || !valid_raw_encoding(v))
        || draft.message.label_ids.len() > 1000
        || draft
            .message
            .label_ids
            .iter()
            .any(|v| checked(v, 256).is_err())
        || draft
            .message
            .history_id
            .as_ref()
            .is_some_and(|v| history_id(v).is_err())
    {
        return Err(invalid());
    }
    Ok(())
}
fn authored(mime: &[u8], thread: Option<&str>) -> Result<Value, GoogleError> {
    if mime.is_empty() || mime.len() > MAX_SUBMITTED_MIME {
        return Err(GoogleError::InvalidResponse(
            "authored MIME exceeds submission boundary",
        ));
    }
    let mut value = json!({"raw": URL_SAFE_NO_PAD.encode(mime)});
    if let Some(thread) = thread {
        checked(thread, 256)?;
        value["threadId"] = json!(thread);
    }
    Ok(value)
}
fn valid_raw_encoding(value: &str) -> bool {
    let unpadded = value.trim_end_matches('=');
    if unpadded.is_empty() || value.len() - unpadded.len() > 2 {
        return false;
    }
    let mut buffer = [0_u8; 3072];
    unpadded
        .as_bytes()
        .chunks(4096)
        .all(|chunk| URL_SAFE_NO_PAD.decode_slice(chunk, &mut buffer).is_ok())
}
fn optional_param<'a>(
    values: &mut Vec<(&'a str, &'a str)>,
    key: &'a str,
    value: Option<&'a str>,
    max: usize,
) -> Result<(), GoogleError> {
    if let Some(value) = value {
        checked(value, max)?;
        values.push((key, value));
    }
    Ok(())
}
fn page_size(size: u16) -> Result<String, GoogleError> {
    if !(1..=500).contains(&size) {
        Err(invalid())
    } else {
        Ok(size.to_string())
    }
}
fn page_token(value: Option<&str>, previous: Option<&str>) -> Result<(), GoogleError> {
    if let Some(value) = value {
        checked(value, 4096)?;
        if Some(value) == previous {
            return Err(invalid());
        }
    }
    Ok(())
}
fn message_ref(value: &MessageRef) -> Result<(), GoogleError> {
    checked(&value.id, 256)?;
    checked(&value.thread_id, 256)
}
fn history_id(value: &str) -> Result<(), GoogleError> {
    if value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.parse::<u64>().is_err()
    {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn checked(value: &str, max: usize) -> Result<(), GoogleError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn invalid() -> GoogleError {
    GoogleError::InvalidResponse("invalid Gmail request or response")
}

#[cfg(test)]
mod tests;
