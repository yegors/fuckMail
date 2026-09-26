//! Gmail Spam, read live on request. Sync never downloads spam, and nothing
//! read here is stored: a listing is headers and Google's snippet, and an
//! opened message is its text only.
use super::SyncContext;
use fm_core::{CoreError, Result};
use fm_google::{
    gmail_api::{GmailCall, GmailMessageMetadata, GmailReply, RawOutcome},
    GoogleError,
};
use serde_json::{json, Value};

/// Spam messages listed per page.
const PAGE_SIZE: u16 = 50;
/// Metadata reads in flight at once while building a page.
const CONCURRENT_READS: usize = 8;

/// `{page_token?}`: one page of Spam, newest first, as header rows with
/// Google's snippet, plus the next page token and Google's estimated total.
pub(crate) async fn list(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = params
        .as_object()
        .ok_or_else(|| failure("invalid_request", "Spam listing takes an object"))?;
    if fields.keys().any(|key| key != "page_token") {
        return Err(failure("invalid_request", "Unknown Spam listing field"));
    }
    let token = match fields.get("page_token") {
        None | Some(Value::Null) => None,
        Some(Value::String(token)) if !token.is_empty() && token.len() <= 4096 => {
            Some(token.as_str())
        }
        Some(_) => return Err(failure("invalid_request", "Spam page token must be text")),
    };
    require_mail_read(ctx).await?;
    let call = GmailCall::list_messages(&ctx.account, token, Some("in:spam"), true, PAGE_SIZE)
        .map_err(provider)?;
    let GmailReply::Messages(page) = ctx.gmail(call).await.map_err(provider)? else {
        return Err(invalid());
    };
    let ids: Vec<String> = page
        .messages
        .into_iter()
        .map(|message| message.id)
        .collect();
    let mut rows = vec![Value::Null; ids.len()];
    for (chunk_index, chunk) in ids.chunks(CONCURRENT_READS).enumerate() {
        let mut reads = tokio::task::JoinSet::new();
        for (offset, id) in chunk.iter().enumerate() {
            let ctx = ctx.clone();
            let id = id.clone();
            reads.spawn(async move {
                let call = GmailCall::message_metadata(&ctx.account, &id)?;
                Ok::<_, GoogleError>((offset, ctx.gmail(call).await?))
            });
        }
        while let Some(read) = reads.join_next().await {
            let (offset, reply) = read
                .map_err(|_| failure("engine_stopped", "Spam listing stopped"))?
                .map_err(provider)?;
            match reply {
                GmailReply::MessageMetadata(Some(message)) => {
                    rows[chunk_index * CONCURRENT_READS + offset] = row(&message)
                }
                // Removed from Spam between the listing and this read.
                GmailReply::MessageMetadata(None) => {}
                _ => return Err(invalid()),
            }
        }
    }
    rows.retain(|row| !row.is_null());
    Ok(json!({
        "messages": rows,
        "next_page_token": page.next_page_token,
        "estimated_total": page.result_size_estimate,
    }))
}

/// `{id}`: one Spam message's extracted plain text. Its HTML is never
/// returned, so nothing in it can load or run.
pub(crate) async fn message(ctx: &SyncContext, params: Value) -> Result<Value> {
    let id = params
        .as_object()
        .filter(|fields| fields.len() == 1)
        .and_then(|fields| fields.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .ok_or_else(|| failure("invalid_request", "Open a Spam message by its id"))?
        .to_owned();
    require_mail_read(ctx).await?;
    let call = GmailCall::raw_message(&ctx.account, &id).map_err(provider)?;
    let (reply, permit) = ctx.gmail_prepared(call).await.map_err(provider)?;
    let GmailReply::Raw(outcome) = reply else {
        return Err(invalid());
    };
    let item = match outcome {
        RawOutcome::Available(item) => item,
        RawOutcome::Missing => {
            return Err(failure("not_found", "This message is no longer in Spam"))
        }
        _ => {
            return Err(failure(
                "invalid_google_response",
                "This message could not be read",
            ))
        }
    };
    let expected = id.clone();
    let text = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let content = fm_google::sync::decode_raw_message(item.message, &expected).ok()?;
        fm_mail::extract_incoming_text(&content.source_mime).ok()
    })
    .await
    .map_err(|_| failure("engine_stopped", "Spam message reading stopped"))?
    .ok_or_else(|| failure("invalid_google_response", "This message could not be read"))?;
    Ok(json!({"id": id, "text": text}))
}

/// A Spam list row built from message metadata.
fn row(message: &GmailMessageMetadata) -> Value {
    let (name, email) = mailbox(message.header("From").unwrap_or(""));
    json!({
        "id": message.id,
        "thread_id": message.thread_id,
        "sender_name": name,
        "sender_email": email,
        "subject": message.header("Subject").unwrap_or(""),
        "snippet": unescape(&message.snippet),
        "internal_date_ms": message.internal_date.as_deref().and_then(|date| date.parse::<i64>().ok()),
        "unread": message.label_ids.iter().any(|label| label == "UNREAD"),
    })
}

/// `Name <address>` or a bare address, as Gmail reports From.
pub(super) fn mailbox(from: &str) -> (String, String) {
    match (from.rfind('<'), from.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            let name = from[..open].trim().trim_matches('"').trim();
            (name.to_owned(), from[open + 1..close].trim().to_owned())
        }
        _ => (String::new(), from.trim().to_owned()),
    }
}

/// Google's snippet is HTML-escaped text.
pub(super) fn unescape(snippet: &str) -> String {
    snippet
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// Requires the Gmail read grant (`capability_unavailable` otherwise).
pub(super) async fn require_mail_read(ctx: &SyncContext) -> Result<()> {
    let account = ctx.account.as_str().to_owned();
    ctx.core_call(move |store| {
        let allowed = store
            .google_account_bindings()?
            .into_iter()
            .find(|binding| binding.account == account)
            .is_some_and(|binding| fm_google::auth::ScopeSet::new(&binding.scopes).mail_read());
        if allowed {
            Ok(())
        } else {
            Err(failure(
                "capability_unavailable",
                "Google mail reading permission is required",
            ))
        }
    })
    .await
}

fn provider(error: GoogleError) -> CoreError {
    let code = match &error {
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::PermissionDenied => "google_permission_denied",
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        _ => "invalid_google_response",
    };
    let mut result = failure(code, "Spam could not be read from Google");
    if let GoogleError::RateLimited { retry_after_secs } = error {
        result.details = json!({"retry_after_secs": retry_after_secs});
    }
    result
}
fn invalid() -> CoreError {
    failure(
        "invalid_google_response",
        "Google returned an unexpected Spam response",
    )
}
fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_and_snippet_read_as_plain_text() {
        assert_eq!(
            mailbox("\"Prize Desk\" <win@example.test>"),
            ("Prize Desk".into(), "win@example.test".into())
        );
        assert_eq!(
            mailbox("win@example.test"),
            (String::new(), "win@example.test".into())
        );
        assert_eq!(
            unescape("You&#39;ve won &amp; &lt;more&gt;"),
            "You've won & <more>"
        );
    }
}
