//! Spam and Trash actions against Gmail: their totals, marking a Spam thread
//! not spam, and deleting for good one conversation or a whole folder.
//! Deleting for good needs full mail access (`https://mail.google.com/`);
//! accounts connected with less are asked to reconnect. Spam is never
//! stored; a Trash conversation's local copies are removed once Gmail has
//! deleted them.
use super::remote::{invalid, object, provider, provider_id, require_mail_write, require_scope};
use super::spam::require_mail_read;
use super::{failure, SyncContext};
use fm_core::Result;
use fm_google::gmail_api::{GmailCall, GmailReply};
use serde_json::{json, Value};

/// Messages Google deletes in one call.
const DELETE_BATCH: usize = 1000;
/// Messages listed per page while emptying a folder.
const LIST_PAGE: u16 = 500;
/// An empty stops after this many messages; the person can empty again.
const MAX_EMPTY: usize = 10_000;

fn folder(fields: &serde_json::Map<String, Value>) -> Result<&'static str> {
    match fields.get("folder").and_then(Value::as_str) {
        Some("spam") => Ok("spam"),
        Some("trash") => Ok("trash"),
        _ => Err(failure("invalid_request", "Choose spam or trash")),
    }
}

fn label(folder: &str) -> &'static str {
    if folder == "spam" {
        "SPAM"
    } else {
        "TRASH"
    }
}

async fn require_mail_full(ctx: &SyncContext) -> Result<()> {
    require_scope(
        ctx,
        |scopes| scopes.mail_full(),
        "reconnect_required",
        "Reconnect this account to allow deleting mail for good",
    )
    .await
}

/// `{folder}`: Gmail's message and conversation totals for Spam or Trash.
pub(crate) async fn counts(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["folder"])?;
    let folder = folder(fields)?;
    require_mail_read(ctx).await?;
    let call = GmailCall::label(&ctx.account, label(folder)).map_err(provider)?;
    let GmailReply::LabelCounts(counts) = ctx.gmail(call).await.map_err(provider)? else {
        return Err(invalid());
    };
    Ok(json!({
        "folder": folder,
        "messages": counts.messages_total,
        "conversations": counts.threads_total,
    }))
}

/// The ids of a Gmail thread's messages carrying `label`.
async fn labelled(ctx: &SyncContext, thread: &str, label: &str) -> Result<Vec<String>> {
    let call = GmailCall::thread(&ctx.account, thread).map_err(provider)?;
    let messages = match ctx.gmail(call).await.map_err(provider)? {
        GmailReply::Thread(Some(messages)) => messages,
        GmailReply::Thread(None) => Vec::new(),
        _ => return Err(invalid()),
    };
    let ids: Vec<String> = messages
        .into_iter()
        .filter(|m| m.label_ids.iter().any(|l| l == label))
        .map(|m| m.id)
        .collect();
    if ids.is_empty() {
        let folder = if label == "SPAM" { "Spam" } else { "Trash" };
        return Err(failure(
            "not_found",
            &format!("This conversation is no longer in {folder}"),
        ));
    }
    Ok(ids)
}

/// `{thread_id}`: moves a Spam thread's spam messages to the Inbox, which
/// also tells Gmail they are not spam. Sync brings them in.
pub(crate) async fn not_spam(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["thread_id"])?;
    let thread = provider_id(fields.get("thread_id"), "Name the Spam conversation")?;
    require_mail_write(ctx).await?;
    let ids = labelled(ctx, &thread, "SPAM").await?;
    let add = ["INBOX".to_owned()];
    let remove = ["SPAM".to_owned()];
    for id in &ids {
        let call = GmailCall::modify_message(&ctx.account, id, &add, &remove).map_err(provider)?;
        match ctx.gmail(call).await.map_err(provider)? {
            GmailReply::Message(_) => {}
            _ => return Err(invalid()),
        }
    }
    Ok(json!({"thread_id": thread, "moved": ids.len()}))
}

/// Deletes the given messages for good, then forgets any local copies.
async fn delete(ctx: &SyncContext, ids: Vec<String>) -> Result<()> {
    for chunk in ids.chunks(DELETE_BATCH) {
        let call = GmailCall::batch_delete(&ctx.account, chunk).map_err(provider)?;
        match ctx.gmail(call).await.map_err(provider)? {
            GmailReply::Deleted => {}
            _ => return Err(invalid()),
        }
    }
    let account = ctx.account.as_str().to_owned();
    ctx.core_call(move |store| store.google_forget_deleted(&account, &ids))
        .await?;
    Ok(())
}

/// `{folder:"spam", thread_id}` or `{folder:"trash", conversation_id}`:
/// deletes that conversation's Spam or Trash messages for good.
pub(crate) async fn delete_forever(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["folder", "thread_id", "conversation_id"])?;
    let folder = folder(fields)?;
    require_mail_full(ctx).await?;
    let ids = if folder == "spam" {
        let thread = provider_id(fields.get("thread_id"), "Name the Spam conversation")?;
        labelled(ctx, &thread, "SPAM").await?
    } else {
        let conversation = fields
            .get("conversation_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or_else(|| failure("invalid_request", "Name the Trash conversation"))?
            .to_owned();
        let account = ctx.account.as_str().to_owned();
        ctx.core_call(move |store| store.google_trashed_in_conversation(&account, &conversation))
            .await?
    };
    let count = ids.len();
    delete(ctx, ids).await?;
    Ok(json!({"folder": folder, "deleted": count}))
}

/// `{folder}`: deletes everything in Spam or Trash for good, up to 10,000
/// messages at a time; `more` says whether some remain.
pub(crate) async fn empty(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["folder"])?;
    let folder = folder(fields)?;
    require_mail_full(ctx).await?;
    let query = format!("in:{folder}");
    let mut ids = Vec::new();
    let mut token: Option<String> = None;
    let more = loop {
        let call = GmailCall::list_messages(
            &ctx.account,
            token.as_deref(),
            Some(&query),
            true,
            LIST_PAGE,
        )
        .map_err(provider)?;
        let GmailReply::Messages(page) = ctx.gmail(call).await.map_err(provider)? else {
            return Err(invalid());
        };
        ids.extend(page.messages.into_iter().map(|m| m.id));
        token = page.next_page_token;
        if token.is_none() {
            break false;
        }
        if ids.len() >= MAX_EMPTY {
            break true;
        }
    };
    let count = ids.len();
    if count > 0 {
        delete(ctx, ids).await?;
    }
    Ok(json!({"folder": folder, "deleted": count, "more": more}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_spam_and_trash_are_emptied() {
        for (value, expected) in [
            (json!({"folder":"spam"}), Some("spam")),
            (json!({"folder":"trash"}), Some("trash")),
            (json!({"folder":"inbox"}), None),
            (json!({}), None),
        ] {
            let fields = value.as_object().unwrap();
            assert_eq!(folder(fields).ok(), expected);
        }
        assert_eq!(label("spam"), "SPAM");
        assert_eq!(label("trash"), "TRASH");
    }
}
