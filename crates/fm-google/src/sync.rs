use crate::{AccountId, GoogleError};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_PAGE_MESSAGES: usize = 500;
pub const MAX_PAGES_PER_SCAN: u32 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRef {
    pub id: String,
    pub thread_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    pub id: String,
    pub thread_id: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
    pub history_id: String,
    pub raw: String,
}

/// Source MIME goes to Core's existing parser/content store; no provider-side schema.
pub struct MessageContent {
    pub id: String,
    pub thread_id: String,
    pub label_ids: Vec<String>,
    pub history_id: String,
    pub source_mime: Vec<u8>,
}

pub fn decode_raw_message(
    message: RawMessage,
    expected_id: &str,
) -> Result<MessageContent, GoogleError> {
    const MAX_MIME: usize = 40 * 1024 * 1024;
    if message.id != expected_id
        || !valid_id(&message.id)
        || !valid_id(&message.thread_id)
        || message.label_ids.len() > 1000
        || message.label_ids.iter().any(|label| !valid_id(label))
        || message.raw.len() > MAX_MIME / 3 * 4 + 4
    {
        return Err(GoogleError::InvalidResponse(
            "invalid full message identity or size",
        ));
    }
    history_number(&message.history_id)?;
    let source = URL_SAFE_NO_PAD
        .decode(message.raw.trim_end_matches('='))
        .map_err(|_| GoogleError::InvalidResponse("invalid message base64url"))?;
    if source.len() > MAX_MIME {
        return Err(GoogleError::InvalidResponse("message exceeds byte limit"));
    }
    Ok(MessageContent {
        id: message.id,
        thread_id: message.thread_id,
        label_ids: message.label_ids,
        history_id: message.history_id,
        source_mime: source,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPage {
    #[serde(default)]
    pub messages: Vec<MessageRef>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    #[serde(default)]
    pub history: Vec<HistoryRecord>,
    pub next_page_token: Option<String>,
    pub history_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: String,
    #[serde(default)]
    pub messages_added: Vec<MessageChange>,
    #[serde(default)]
    pub messages_deleted: Vec<MessageChange>,
    #[serde(default)]
    pub labels_added: Vec<LabelChange>,
    #[serde(default)]
    pub labels_removed: Vec<LabelChange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageChange {
    pub message: MessageRef,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelChange {
    pub message: MessageRef,
    pub label_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Full {
        baseline_history_id: String,
        page_token: Option<String>,
    },
    CatchUp {
        start_history_id: String,
        page_token: Option<String>,
    },
    Delta {
        start_history_id: String,
        page_token: Option<String>,
    },
    Idle,
    NeedsFullSync,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub account: AccountId,
    pub generation: u64,
    pub revision: u64,
    pub committed_history_id: Option<String>,
    pub page_count: u32,
    pub phase: Phase,
}

impl Checkpoint {
    pub fn begin_full(
        account: AccountId,
        generation: u64,
        baseline_history_id: String,
    ) -> Result<Self, GoogleError> {
        history_number(&baseline_history_id)?;
        Ok(Self {
            account,
            generation,
            revision: 0,
            committed_history_id: None,
            page_count: 0,
            phase: Phase::Full {
                baseline_history_id,
                page_token: None,
            },
        })
    }

    pub fn begin_delta(&self) -> Result<PagePlan, GoogleError> {
        if self.phase != Phase::Idle {
            return Err(GoogleError::StaleCheckpoint);
        }
        let history = self
            .committed_history_id
            .clone()
            .ok_or(GoogleError::StaleCheckpoint)?;
        Ok(PagePlan {
            expected: self.clone(),
            next: Self {
                revision: self
                    .revision
                    .checked_add(1)
                    .ok_or(GoogleError::StaleCheckpoint)?,
                phase: Phase::Delta {
                    start_history_id: history,
                    page_token: None,
                },
                page_count: 0,
                ..self.clone()
            },
            observed_message_ids: BTreeSet::new(),
            fetch_message_ids: BTreeSet::new(),
            explicitly_deleted_ids: BTreeSet::new(),
            inventory_finished: false,
            caught_up: false,
        })
    }

    pub fn restart_full(&self, baseline_history_id: String) -> Result<PagePlan, GoogleError> {
        if self.phase != Phase::NeedsFullSync {
            return Err(GoogleError::StaleCheckpoint);
        }
        history_number(&baseline_history_id)?;
        Ok(PagePlan {
            expected: self.clone(),
            next: Self {
                generation: self
                    .generation
                    .checked_add(1)
                    .ok_or(GoogleError::StaleCheckpoint)?,
                revision: self
                    .revision
                    .checked_add(1)
                    .ok_or(GoogleError::StaleCheckpoint)?,
                phase: Phase::Full {
                    baseline_history_id,
                    page_token: None,
                },
                page_count: 0,
                ..self.clone()
            },
            observed_message_ids: BTreeSet::new(),
            fetch_message_ids: BTreeSet::new(),
            explicitly_deleted_ids: BTreeSet::new(),
            inventory_finished: false,
            caught_up: false,
        })
    }
}

/// Core persists these instructions and the next checkpoint in one writer transaction.
/// `fetch_message_ids` names full messages to obtain before applying the plan. A current
/// messages.get result reconciles labels without racing historical add/remove sequences.
/// Source bytes and draft/pending-operation state remain owned by Core, not this crate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagePlan {
    pub expected: Checkpoint,
    pub next: Checkpoint,
    pub observed_message_ids: BTreeSet<String>,
    pub fetch_message_ids: BTreeSet<String>,
    pub explicitly_deleted_ids: BTreeSet<String>,
    pub inventory_finished: bool,
    pub caught_up: bool,
}

/// A real Core adapter must atomically apply fetched content, explicit provider removals
/// and this checkpoint, preserving local drafts/pending user intent. The trait supplies
/// that integration seam; the deterministic fake does not prove disk atomicity.
pub trait PageStore {
    /// Stage one verified source before page commit. Must be idempotent for this
    /// account/generation/ID and invisible to inventory removal until commit.
    fn stage_message(
        &mut self,
        expected: &Checkpoint,
        message: MessageContent,
    ) -> Result<(), GoogleError>;
    fn commit_page(&mut self, plan: &PagePlan) -> Result<(), GoogleError>;
}

pub trait GmailTransport {
    fn list(
        &mut self,
        account: &AccountId,
        page_token: Option<&str>,
        include_spam_trash: bool,
    ) -> Result<ListPage, GoogleError>;
    fn history(
        &mut self,
        account: &AccountId,
        start_history_id: &str,
        page_token: Option<&str>,
    ) -> Result<HistoryPage, GoogleError>;
    fn get_raw(&mut self, account: &AccountId, message_id: &str)
        -> Result<RawMessage, GoogleError>;
}

pub fn advance<T: GmailTransport, S: PageStore>(
    checkpoint: &mut Checkpoint,
    transport: &mut T,
    store: &mut S,
) -> Result<PagePlan, GoogleError> {
    let mut plan = match &checkpoint.phase {
        Phase::Full { page_token, .. } => prepare_full_page(
            checkpoint,
            transport.list(&checkpoint.account, page_token.as_deref(), true)?,
        )?,
        Phase::CatchUp {
            start_history_id,
            page_token,
        }
        | Phase::Delta {
            start_history_id,
            page_token,
        } => {
            match transport.history(&checkpoint.account, start_history_id, page_token.as_deref()) {
                Ok(page) => prepare_history_page(checkpoint, page)?,
                Err(GoogleError::HistoryExpired) => prepare_expiry(checkpoint)?,
                Err(error) => return Err(error),
            }
        }
        _ => return Err(GoogleError::StaleCheckpoint),
    };
    // No store transaction spans transport calls. Partial staging is safe to replay.
    // A listed message can disappear during the scan; only an explicit 404 tombstones it.
    for id in plan.fetch_message_ids.clone() {
        match transport.get_raw(&checkpoint.account, &id) {
            Ok(message) => store.stage_message(checkpoint, decode_raw_message(message, &id)?)?,
            Err(GoogleError::NotFound) => {
                plan.fetch_message_ids.remove(&id);
                plan.explicitly_deleted_ids.insert(id);
            }
            Err(error) => return Err(error),
        }
    }
    store.commit_page(&plan)?;
    *checkpoint = plan.next.clone();
    Ok(plan)
}

pub fn prepare_full_page(checkpoint: &Checkpoint, page: ListPage) -> Result<PagePlan, GoogleError> {
    let Phase::Full {
        baseline_history_id,
        page_token,
    } = &checkpoint.phase
    else {
        return Err(GoogleError::StaleCheckpoint);
    };
    validate_paging(
        checkpoint.page_count,
        page_token.as_deref(),
        page.next_page_token.as_deref(),
    )?;
    if page.messages.len() > MAX_PAGE_MESSAGES {
        return Err(GoogleError::InvalidResponse("too many listed messages"));
    }
    let mut ids = BTreeSet::new();
    for message in &page.messages {
        validate_message(message)?;
        ids.insert(message.id.clone());
    }
    let finished = page.next_page_token.is_none();
    let next_phase = if finished {
        Phase::CatchUp {
            start_history_id: baseline_history_id.clone(),
            page_token: None,
        }
    } else {
        Phase::Full {
            baseline_history_id: baseline_history_id.clone(),
            page_token: page.next_page_token,
        }
    };
    Ok(PagePlan {
        expected: checkpoint.clone(),
        next: Checkpoint {
            revision: checkpoint
                .revision
                .checked_add(1)
                .ok_or(GoogleError::StaleCheckpoint)?,
            page_count: if finished {
                0
            } else {
                checkpoint.page_count + 1
            },
            phase: next_phase,
            ..checkpoint.clone()
        },
        observed_message_ids: ids.clone(),
        fetch_message_ids: ids,
        explicitly_deleted_ids: BTreeSet::new(),
        inventory_finished: finished,
        caught_up: false,
    })
}

pub fn prepare_history_page(
    checkpoint: &Checkpoint,
    page: HistoryPage,
) -> Result<PagePlan, GoogleError> {
    let (start, token, catching_up) = match &checkpoint.phase {
        Phase::CatchUp {
            start_history_id,
            page_token,
        } => (start_history_id, page_token, true),
        Phase::Delta {
            start_history_id,
            page_token,
        } => (start_history_id, page_token, false),
        _ => return Err(GoogleError::StaleCheckpoint),
    };
    validate_paging(
        checkpoint.page_count,
        token.as_deref(),
        page.next_page_token.as_deref(),
    )?;
    let end_history = history_number(&page.history_id)?;
    let start_history = history_number(start)?;
    if end_history < start_history || page.history.len() > 500 {
        return Err(GoogleError::InvalidResponse("invalid history range"));
    }
    let mut fetch = BTreeSet::new();
    let mut removed = BTreeSet::new();
    let mut previous_history = start_history;
    let mut changes = 0;
    for history in page.history {
        let id = history_number(&history.id)?;
        if id < previous_history || id > end_history {
            return Err(GoogleError::InvalidResponse("unordered history"));
        }
        previous_history = id;
        changes += history.messages_added.len()
            + history.messages_deleted.len()
            + history.labels_added.len()
            + history.labels_removed.len();
        if changes > 5000 {
            return Err(GoogleError::InvalidResponse("too many history changes"));
        }
        for change in history.messages_added {
            validate_message(&change.message)?;
            removed.remove(&change.message.id);
            fetch.insert(change.message.id);
        }
        for change in history
            .labels_added
            .into_iter()
            .chain(history.labels_removed)
        {
            validate_message(&change.message)?;
            if change.label_ids.len() > 1000
                || change.label_ids.iter().any(|label| !valid_id(label))
            {
                return Err(GoogleError::InvalidResponse("invalid label list"));
            }
            removed.remove(&change.message.id);
            fetch.insert(change.message.id);
        }
        for change in history.messages_deleted {
            validate_message(&change.message)?;
            fetch.remove(&change.message.id);
            removed.insert(change.message.id);
        }
    }
    let finished = page.next_page_token.is_none();
    let next_phase = if finished {
        Phase::Idle
    } else if catching_up {
        Phase::CatchUp {
            start_history_id: start.clone(),
            page_token: page.next_page_token,
        }
    } else {
        Phase::Delta {
            start_history_id: start.clone(),
            page_token: page.next_page_token,
        }
    };
    Ok(PagePlan {
        expected: checkpoint.clone(),
        next: Checkpoint {
            revision: checkpoint
                .revision
                .checked_add(1)
                .ok_or(GoogleError::StaleCheckpoint)?,
            committed_history_id: if finished {
                Some(page.history_id)
            } else {
                checkpoint.committed_history_id.clone()
            },
            page_count: if finished {
                0
            } else {
                checkpoint.page_count + 1
            },
            phase: next_phase,
            ..checkpoint.clone()
        },
        observed_message_ids: fetch.clone(),
        fetch_message_ids: fetch,
        explicitly_deleted_ids: removed,
        inventory_finished: false,
        caught_up: finished,
    })
}

pub fn prepare_expiry(checkpoint: &Checkpoint) -> Result<PagePlan, GoogleError> {
    Ok(PagePlan {
        expected: checkpoint.clone(),
        next: Checkpoint {
            revision: checkpoint
                .revision
                .checked_add(1)
                .ok_or(GoogleError::StaleCheckpoint)?,
            phase: Phase::NeedsFullSync,
            page_count: 0,
            ..checkpoint.clone()
        },
        observed_message_ids: BTreeSet::new(),
        fetch_message_ids: BTreeSet::new(),
        explicitly_deleted_ids: BTreeSet::new(),
        inventory_finished: false,
        caught_up: false,
    })
}

fn validate_paging(
    count: u32,
    previous: Option<&str>,
    next: Option<&str>,
) -> Result<(), GoogleError> {
    if count >= MAX_PAGES_PER_SCAN
        || next
            .is_some_and(|token| token.is_empty() || token.len() > 4096 || Some(token) == previous)
    {
        return Err(GoogleError::InvalidResponse(
            "invalid or excessive pagination",
        ));
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}
fn validate_message(message: &MessageRef) -> Result<(), GoogleError> {
    if !valid_id(&message.id) || !valid_id(&message.thread_id) {
        return Err(GoogleError::InvalidResponse("invalid message identity"));
    }
    Ok(())
}
fn history_number(id: &str) -> Result<u64, GoogleError> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(GoogleError::InvalidResponse("invalid history cursor"));
    }
    id.parse()
        .map_err(|_| GoogleError::InvalidResponse("invalid history cursor"))
}
