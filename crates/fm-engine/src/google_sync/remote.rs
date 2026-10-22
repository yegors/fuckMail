//! Mail read live from Google on request and not kept: remote search results,
//! a thread opened from them, its attachments, and the unsubscribe email a
//! list's `List-Unsubscribe: <mailto:…>` asks for.
use super::spam::{mailbox, require_mail_read, unescape};
use super::{failure, SyncContext};
use fm_core::{CoreError, Result};
use fm_google::{
    gmail_api::{GmailCall, GmailMethod, GmailReply, GmailSubmission, RawOutcome},
    send::SubmissionResult,
    GoogleError,
};
use fm_mail::unsubscribe::Unsubscribe;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Messages listed per search; results are grouped into threads.
const SEARCH_PAGE: u16 = 50;
/// Remote-only threads returned per search.
const MAX_THREADS: usize = 25;
/// Metadata reads in flight at once while building search rows.
const CONCURRENT_READS: usize = 8;
/// The newest messages of a long thread are shown.
const MAX_THREAD_MESSAGES: usize = 50;
/// Per-message caps, in bytes, on text and HTML in a remote thread.
const MAX_TEXT: usize = 64 * 1024;
const MAX_HTML: usize = 256 * 1024;
/// Leave room for the IPC envelope.
const MAX_REPLY: usize = 900 * 1024;
/// Id prefixes that mark conversations, messages and attachments read live
/// from Gmail, so they are never confused with stored ones.
pub(crate) const THREAD_PREFIX: &str = "remote:";
const MESSAGE_PREFIX: &str = "remote-";
const ATTACHMENT_PREFIX: &str = "remote-att:";

/// `{query, folder}`: threads matching on the server that have nothing on
/// this machine, newest first, as list rows marked `remote`.
pub(crate) async fn search(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["query", "folder"])?;
    let query = fields
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty() && q.len() <= 512)
        .ok_or_else(|| failure("invalid_request", "Type something to search for"))?;
    let folder = fields
        .get("folder")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let (q, include_trash) = gmail_query(query, folder).ok_or_else(|| {
        failure(
            "invalid_request",
            "This folder cannot be searched on the server",
        )
    })?;
    require_mail_read(ctx).await?;
    let call = GmailCall::list_messages(&ctx.account, None, Some(&q), include_trash, SEARCH_PAGE)
        .map_err(provider)?;
    let GmailReply::Messages(page) = ctx.gmail(call).await.map_err(provider)? else {
        return Err(invalid());
    };
    // Threads in listing order (newest first) with how many messages matched.
    let mut threads: Vec<(String, String)> = Vec::new();
    let mut matched: HashMap<String, usize> = HashMap::new();
    for message in page.messages {
        let count = matched.entry(message.thread_id.clone()).or_insert(0);
        if *count == 0 {
            threads.push((message.thread_id, message.id));
        }
        *count += 1;
    }
    let ids: Vec<String> = threads.iter().map(|(thread, _)| thread.clone()).collect();
    let account = ctx.account.as_str().to_owned();
    let present = ctx
        .core_call(move |store| store.google_threads_present(&account, &ids))
        .await?;
    threads.retain(|(thread, _)| !present.contains(thread));
    threads.truncate(MAX_THREADS);
    let mut rows = vec![Value::Null; threads.len()];
    for (chunk_index, chunk) in threads.chunks(CONCURRENT_READS).enumerate() {
        let mut reads = tokio::task::JoinSet::new();
        for (offset, (_, id)) in chunk.iter().enumerate() {
            let ctx = ctx.clone();
            let id = id.clone();
            reads.spawn(async move {
                let call = GmailCall::message_metadata(&ctx.account, &id)?;
                Ok::<_, GoogleError>((offset, ctx.gmail(call).await?))
            });
        }
        while let Some(read) = reads.join_next().await {
            let (offset, reply) = read
                .map_err(|_| failure("engine_stopped", "Remote search stopped"))?
                .map_err(provider)?;
            let index = chunk_index * CONCURRENT_READS + offset;
            match reply {
                GmailReply::MessageMetadata(Some(message)) => {
                    let (name, email) = mailbox(message.header("From").unwrap_or(""));
                    let thread = &threads[index].0;
                    let subject = message.header("Subject").unwrap_or("").trim();
                    rows[index] = json!({
                        "id": format!("{THREAD_PREFIX}{thread}"),
                        "remote": true,
                        "remote_thread_id": thread,
                        "subject": if subject.is_empty() { "(No subject)" } else { subject },
                        "sender_name": name,
                        "sender_email": email,
                        "recipient": "",
                        "snippet": unescape(&message.snippet),
                        "latest_at": message.internal_date.as_deref().and_then(|d| d.parse::<i64>().ok()).and_then(rfc3339),
                        "message_count": matched.get(thread).copied().unwrap_or(1),
                        "attachment_count": 0,
                        "unread": false,
                        "starred": false,
                        "revision": 0,
                        "categories": [],
                        "message_ids": [],
                        "matching_message_ids": [],
                        "others": 0,
                        "other_names": [],
                        "summary": null,
                        "kind": "unknown",
                        "group": "done",
                    });
                }
                GmailReply::MessageMetadata(None) => {}
                _ => return Err(invalid()),
            }
        }
    }
    rows.retain(|row| !row.is_null() && !row["latest_at"].is_null());
    Ok(json!({"query": query, "folder": folder, "conversations": rows}))
}

/// `{thread_id}`: the thread's newest messages read live, shaped like a
/// stored conversation. Nothing is kept.
pub(crate) async fn thread(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["thread_id"])?;
    let thread = provider_id(fields.get("thread_id"), "Open a remote thread by its id")?;
    require_mail_read(ctx).await?;
    let call = GmailCall::thread(&ctx.account, &thread).map_err(provider)?;
    let ids = match ctx.gmail(call).await.map_err(provider)? {
        GmailReply::Thread(Some(messages)) if !messages.is_empty() => {
            messages.into_iter().map(|m| m.id).collect::<Vec<_>>()
        }
        GmailReply::Thread(_) => {
            return Err(failure(
                "not_found",
                "This conversation is no longer in Gmail",
            ))
        }
        _ => return Err(invalid()),
    };
    let skip = ids.len().saturating_sub(MAX_THREAD_MESSAGES);
    let ids: Vec<String> = ids.into_iter().skip(skip).collect();
    let last = ids.len() - 1;
    let mut messages = Vec::with_capacity(ids.len());
    for (index, id) in ids.into_iter().enumerate() {
        let Some(item) = raw(ctx, &id).await? else {
            continue;
        };
        let (content, permit) = item;
        let message = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            project(&content, index == last)
        })
        .await
        .map_err(|_| failure("engine_stopped", "Remote thread reading stopped"))?;
        if let Some(message) = message {
            messages.push(message);
        }
    }
    if messages.is_empty() {
        return Err(failure(
            "not_found",
            "This conversation is no longer in Gmail",
        ));
    }
    let subject = messages[0]["subject"].clone();
    let mut result = json!({
        "id": format!("{THREAD_PREFIX}{thread}"),
        "remote": true,
        "remote_thread_id": thread,
        "kind": "unknown",
        "subject": subject,
        "revision": 0,
        "message_ids": [],
        "categories": [],
        "skipped_messages": skip,
        "messages": messages,
    });
    fit(&mut result);
    Ok(result)
}

/// `{id}` from a remote thread's attachment list: the file, written to the
/// view cache, in the shape of the local `attachment` reply.
///
/// With `preview: true` an image instead gains `preview_path`, `width` and
/// `height`, a PNG the engine decoded and re-encoded, or `preview_error`, as
/// a stored attachment's preview does.
pub(crate) async fn attachment(ctx: &SyncContext, params: Value) -> Result<Value> {
    let fields = object(&params, &["id", "preview"])?;
    let preview = match fields.get("preview") {
        None => false,
        Some(Value::Bool(preview)) => *preview,
        Some(_) => return Err(failure("invalid_request", "preview must be true or false")),
    };
    let full = fields
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| id.len() <= 300)
        .ok_or_else(|| failure("invalid_request", "Name a remote attachment"))?
        .to_owned();
    let (message, index) = full
        .strip_prefix(ATTACHMENT_PREFIX)
        .and_then(|rest| rest.rsplit_once(':'))
        .and_then(|(message, index)| Some((message.to_owned(), index.parse::<usize>().ok()?)))
        .filter(|(message, _)| valid_provider_id(message))
        .ok_or_else(|| failure("invalid_request", "Name a remote attachment"))?;
    require_mail_read(ctx).await?;
    let (content, permit) = raw(ctx, &message)
        .await?
        .ok_or_else(|| failure("not_found", "This message is no longer in Gmail"))?;
    let cache = ctx.core_call(|store| Ok(store.view_cache())).await?;
    let account = ctx.account.as_str().to_owned();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed = mail_parser::MessageParser::default()
            .parse(&content.source_mime)
            .ok_or_else(|| failure("invalid_google_response", "This message could not be read"))?;
        use mail_parser::MimeHeaders;
        let part = parsed
            .attachments()
            .nth(index)
            .ok_or_else(|| failure("not_found", "This attachment is not in the message"))?;
        let name = part
            .attachment_name()
            .unwrap_or("attachment.bin")
            .to_owned();
        let bytes = part.contents();
        if preview {
            let mut reply = json!({"account": account, "id": full, "name": name,
                "mime_type": mime_type(part), "size": bytes.len()});
            let key = fm_core::view_cache::key(&["remote-attachment-preview", &full]);
            let shown = match cache.cached_image(&account, &key) {
                Some(hit) => Some(hit),
                None => match fm_mail::image_view::normalize_image(bytes) {
                    Ok(image) => Some((
                        cache.store_image(&account, &key, &image.png)?,
                        image.width,
                        image.height,
                    )),
                    Err(error) => {
                        reply["preview_error"] = json!(error.code());
                        None
                    }
                },
            };
            if let Some((path, width, height)) = shown {
                reply["preview_path"] = json!(path.to_string_lossy());
                reply["width"] = json!(width);
                reply["height"] = json!(height);
            }
            return Ok(reply);
        }
        let (path, sha256) = fm_core::remote_mail::remote_attachment_file(&cache, &name, bytes)?;
        Ok(json!({
            "account": account,
            "id": full,
            "name": name,
            "mime_type": mime_type(part),
            "size": bytes.len(),
            "sha256": sha256,
            "path": path.to_string_lossy(),
        }))
    })
    .await
    .map_err(|_| failure("engine_stopped", "Remote attachment reading stopped"))?
}

/// `{id, source_version, address}` of a stored message whose list
/// unsubscribes by email: send that email from this account, once, and
/// remember it. `address` is the recipient the person was shown and
/// confirmed; nothing is sent to any other.
pub(crate) async fn unsubscribe_mail(ctx: &SyncContext, mut params: Value) -> Result<Value> {
    require_mail_write(ctx).await?;
    let fields = params.as_object_mut();
    let (fallback, confirmed) = match fields {
        Some(fields) => (fields.remove("fallback"), fields.remove("address")),
        None => (None, None),
    };
    // `fallback`: the list refused its one-click POST; use its address instead.
    let fallback = fallback.is_some_and(|value| value == true);
    let confirmed = confirmed.and_then(|value| value.as_str().map(str::to_owned));
    let account = ctx.account.as_str().to_owned();
    let plan = ctx
        .core_call(move |store| store.unsubscribe_begin(&account, &params))
        .await?;
    let (plan, found) = tokio::task::spawn_blocking(move || {
        let found = if fallback {
            fm_core::unsubscribe::resolve_mailto(&plan).and_then(|found| {
                found.ok_or_else(|| {
                    failure("invalid_request", "This list does not unsubscribe by email")
                })
            })
        } else {
            fm_core::unsubscribe::resolve(&plan)
        };
        (plan, found)
    })
    .await
    .map_err(|_| failure("engine_stopped", "Unsubscribe stopped"))?;
    let found = found?;
    if matches!(found, Unsubscribe::Mailto { .. }) {
        fm_core::unsubscribe::check_confirmed(&found, confirmed.as_deref())?;
    }
    let Unsubscribe::Mailto {
        address,
        subject,
        body,
    } = found
    else {
        return Err(failure(
            "invalid_request",
            "This list does not unsubscribe by email",
        ));
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let request = fm_mail::ComposeRequest {
        account_id: plan.account.clone(),
        to: vec![fm_mail::Address {
            name: None,
            email: address.clone(),
        }],
        cc: vec![],
        bcc: vec![],
        subject,
        body_text: if body.trim().is_empty() {
            "unsubscribe".into()
        } else {
            body
        },
        body_html: None,
        forwarded_html: None,
        message_id: format!(
            "<unsubscribe-{}@fuckmail.invalid>",
            &fm_ipc::random_token()[..32]
        ),
        reply_to_message_id: None,
        references: vec![],
        timestamp_secs: now,
    };
    let identity = fm_mail::AccountIdentity {
        account_id: plan.account.clone(),
        from: fm_mail::Address {
            name: Some(plan.from_name.clone()),
            email: plan.from_email.clone(),
        },
        signature_text: String::new(),
        signature_html: None,
    };
    let mail = fm_mail::build_mail(&request, &identity, &[]).map_err(|_| {
        failure(
            "invalid_request",
            "The unsubscribe email could not be written",
        )
    })?;
    // Sent at most once: a lost reply is reported as possibly sent, never
    // retried here, so a list cannot receive two unsubscribe emails.
    let (api, _permit, _dispatched) = ctx.gmail_send_request(GmailMethod::MessageSend).await?;
    let call = GmailCall::send_message(&ctx.account, &mail.raw, None).map_err(provider)?;
    let reply = api
        .execute(call, "engine-managed", &ctx.cancel)
        .await
        .map_err(provider)?;
    match reply {
        GmailReply::Submission(GmailSubmission {
            result: SubmissionResult::Accepted { .. },
            ..
        }) => {
            let recorded = ctx
                .core_call(move |store| store.unsubscribe_record(&plan, "mailto"))
                .await?;
            Ok(json!({"method":"mailto","address":address,"at":recorded["at"]}))
        }
        GmailReply::Submission(GmailSubmission {
            result: SubmissionResult::Rejected { .. },
            ..
        }) => Err(failure(
            "unsubscribe_failed",
            "Google did not send the unsubscribe email",
        )),
        GmailReply::Submission(_) => Err(failure(
            "send_unknown",
            "The unsubscribe email may have been sent. Check Sent before trying again.",
        )),
        _ => Err(invalid()),
    }
}

/// One message's MIME, or None when it is gone.
async fn raw(
    ctx: &SyncContext,
    id: &str,
) -> Result<
    Option<(
        fm_google::sync::MessageContent,
        tokio::sync::OwnedSemaphorePermit,
    )>,
> {
    let call = GmailCall::raw_message(&ctx.account, id).map_err(provider)?;
    let (reply, permit) = ctx.gmail_prepared(call).await.map_err(provider)?;
    let GmailReply::Raw(outcome) = reply else {
        return Err(invalid());
    };
    let item = match outcome {
        RawOutcome::Available(item) => item,
        RawOutcome::Missing => return Ok(None),
        RawOutcome::TooLarge => {
            return Err(failure(
                "too_large",
                "A message in this conversation is too large to read",
            ))
        }
        _ => return Err(invalid()),
    };
    let content = fm_google::sync::decode_raw_message(item.message, id).map_err(|_| invalid())?;
    Ok(Some((content, permit)))
}

/// A remote message in the reader's message shape. Only the newest carries
/// its HTML, from which the reader builds formatted text.
fn project(content: &fm_google::sync::MessageContent, newest: bool) -> Option<Value> {
    let parsed = mail_parser::MessageParser::default().parse(&content.source_mime)?;
    let sender = parsed.from().and_then(|list| list.first());
    let to: Vec<String> = parsed
        .to()
        .map(|list| {
            list.iter()
                .take(50)
                .map(|a| match (a.name(), a.address()) {
                    (Some(name), _) if !name.trim().is_empty() => name.trim().to_owned(),
                    (_, Some(address)) => address.to_owned(),
                    _ => String::new(),
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let text = fm_mail::extract_parsed_incoming_contribution(&parsed).unwrap_or_default();
    use mail_parser::MimeHeaders;
    // Images the HTML shows (cid:) are part of the body, not attachments.
    let inline = fm_mail::html_view::inline_content_ids(&parsed);
    let attachments: Vec<Value> = parsed
        .attachments()
        .take(fm_core::google_store::MAX_RECEIVED_ATTACHMENTS)
        .enumerate()
        .filter(|(_, part)| {
            !fm_mail::html_view::part_content_id(part).is_some_and(|id| inline.contains(&id))
        })
        .map(|(index, part)| {
            json!({
                "id": format!("{ATTACHMENT_PREFIX}{}:{index}", content.id),
                "name": part.attachment_name().unwrap_or("attachment.bin"),
                "mime_type": mime_type(part),
                "size": part.contents().len(),
                "state": "remote",
            })
        })
        .collect();
    let labels = &content.label_ids;
    let subject = parsed.subject().unwrap_or("").trim();
    let mut message = json!({
        "id": format!("{MESSAGE_PREFIX}{}", content.id),
        "remote_id": content.id,
        "sender_name": sender.and_then(|s| s.name()).unwrap_or(""),
        "sender_email": sender.and_then(|s| s.address()).unwrap_or(""),
        "to": to.join(", "),
        "subject": if subject.is_empty() { "(No subject)" } else { subject },
        "sent_at": parsed.date().and_then(|d| rfc3339(d.to_timestamp() * 1000)),
        "text": bounded(&text, MAX_TEXT),
        "unread": labels.iter().any(|l| l == "UNREAD"),
        "starred": labels.iter().any(|l| l == "STARRED"),
        "sent": labels.iter().any(|l| l == "SENT"),
        "kind": "unknown",
        "revision": 0,
        "remote": true,
        "source_available": false,
        "source_version": "",
        "attachments": attachments,
        "categories": [],
        "sender_check": fm_core::message_source::sender_check(&parsed),
    });
    if newest {
        if let Some(html) = fm_mail::html_view::html_part(&parsed) {
            message["body_html"] = bounded(html, MAX_HTML).into();
        }
    }
    Some(message)
}

/// Shrink the largest bodies until the reply fits one IPC frame.
fn fit(result: &mut Value) {
    while serde_json::to_vec(result).map(|v| v.len()).unwrap_or(0) > MAX_REPLY {
        let messages = result["messages"].as_array_mut().unwrap();
        let mut largest: Option<(usize, &'static str, usize)> = None;
        for (index, message) in messages.iter().enumerate() {
            for key in ["text", "body_html"] {
                let len = message[key].as_str().map(str::len).unwrap_or(0);
                if len > largest.map(|l| l.2).unwrap_or(0) {
                    largest = Some((index, key, len));
                }
            }
        }
        let Some((index, key, len)) = largest else {
            return;
        };
        if key == "body_html" || len < 64 {
            messages[index].as_object_mut().unwrap().remove(key);
            continue;
        }
        let value = messages[index][key].as_str().unwrap().to_owned();
        messages[index][key] = bounded(&value, len / 2).into();
        messages[index]["text_truncated"] = true.into();
    }
}

/// The folder's search in Gmail's syntax, and whether Trash is included.
/// `body:` has no Gmail operator, so its words are searched everywhere.
fn gmail_query(query: &str, folder: &str) -> Option<(String, bool)> {
    let scope = match folder {
        "inbox" => "in:inbox",
        "sent" => "in:sent",
        "trash" => "in:trash",
        "drafts" => "in:drafts",
        "all" | "" => "",
        _ => return None,
    };
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|word| match word.get(..5) {
            Some(prefix) if prefix.eq_ignore_ascii_case("body:") => word[5..].to_owned(),
            _ => word.to_owned(),
        })
        .filter(|word| !word.is_empty())
        .collect();
    if terms.is_empty() {
        return None;
    }
    let q = if scope.is_empty() {
        terms.join(" ")
    } else {
        format!("{scope} {}", terms.join(" "))
    };
    Some((q, folder == "trash"))
}

fn mime_type(part: &mail_parser::MessagePart<'_>) -> String {
    use mail_parser::MimeHeaders;
    part.content_type()
        .map(|ct| {
            format!(
                "{}/{}",
                ct.c_type,
                ct.c_subtype.as_deref().unwrap_or("octet-stream")
            )
        })
        .unwrap_or_else(|| "application/octet-stream".into())
}

fn rfc3339(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Truncates to at most `limit` bytes without splitting a character.
fn bounded(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

/// Gmail message and thread ids are short ASCII alphanumerics. Anything else
/// is refused before it can reach a request URL.
fn valid_provider_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Reads a Gmail id parameter, failing with `invalid_request` and `message`.
pub(super) fn provider_id(value: Option<&Value>, message: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .filter(|id| valid_provider_id(id))
        .map(str::to_owned)
        .ok_or_else(|| failure("invalid_request", message))
}

/// Returns the parameters as an object, refusing any field not in `allowed`.
/// Listed fields may be absent.
pub(super) fn object<'a>(
    params: &'a Value,
    allowed: &[&str],
) -> Result<&'a serde_json::Map<String, Value>> {
    let fields = params
        .as_object()
        .ok_or_else(|| failure("invalid_request", "Parameters must be an object"))?;
    if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(failure("invalid_request", "Unknown request field"));
    }
    Ok(fields)
}

/// Requires the Gmail modify/send grant (`capability_unavailable` otherwise).
pub(super) async fn require_mail_write(ctx: &SyncContext) -> Result<()> {
    require_scope(
        ctx,
        |scopes| scopes.mail_write(),
        "capability_unavailable",
        "Google mail sending permission is required",
    )
    .await
}

/// The account's granted Google scopes pass `allowed`, or `code` fails.
pub(super) async fn require_scope(
    ctx: &SyncContext,
    allowed: fn(&fm_google::auth::ScopeSet<'_>) -> bool,
    code: &'static str,
    message: &'static str,
) -> Result<()> {
    let account = ctx.account.as_str().to_owned();
    ctx.core_call(move |store| {
        let granted = store
            .google_account_bindings()?
            .into_iter()
            .find(|binding| binding.account == account)
            .is_some_and(|binding| allowed(&fm_google::auth::ScopeSet::new(&binding.scopes)));
        if granted {
            Ok(())
        } else {
            Err(failure(code, message))
        }
    })
    .await
}

/// Maps a Gmail error to an engine error code, keeping any retry-after hint.
pub(super) fn provider(error: GoogleError) -> CoreError {
    let code = match &error {
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::PermissionDenied => "google_permission_denied",
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        _ => "invalid_google_response",
    };
    let mut result = failure(code, "Gmail could not be reached");
    if let GoogleError::RateLimited { retry_after_secs } = error {
        result.details = json!({"retry_after_secs": retry_after_secs});
    }
    result
}

pub(super) fn invalid() -> CoreError {
    failure(
        "invalid_google_response",
        "Google returned an unexpected response",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_translate_to_gmail_scoped_to_the_folder() {
        assert_eq!(
            gmail_query("from:ann@x.test body:invoice  march", "inbox"),
            Some(("in:inbox from:ann@x.test invoice march".into(), false))
        );
        assert_eq!(
            gmail_query("receipt", "trash"),
            Some(("in:trash receipt".into(), true))
        );
        assert_eq!(
            gmail_query("receipt", "all"),
            Some(("receipt".into(), false))
        );
        assert_eq!(gmail_query("body:", "all"), None);
        assert_eq!(gmail_query("x", "spam"), None);
    }

    #[test]
    fn remote_messages_read_like_stored_ones() {
        let content = fm_google::sync::MessageContent {
            id: "18c2".into(),
            thread_id: "18c1".into(),
            label_ids: vec!["UNREAD".into(), "INBOX".into()],
            history_id: "1".into(),
            source_mime: b"From: Ann <ann@x.test>\r\nTo: Me <me@y.test>\r\nSubject: Plans\r\nDate: Tue, 3 Mar 2020 10:00:00 +0000\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nSee you.\r\n--b\r\nContent-Type: application/pdf; name=a.pdf\r\nContent-Disposition: attachment; filename=a.pdf\r\n\r\n%PDF\r\n--b--\r\n".to_vec(),
        };
        let message = project(&content, true).unwrap();
        assert_eq!(message["id"], "remote-18c2");
        assert_eq!(message["sender_email"], "ann@x.test");
        assert_eq!(message["to"], "Me");
        assert_eq!(message["sent_at"], "2020-03-03T10:00:00Z");
        assert_eq!(message["unread"], true);
        assert_eq!(message["attachments"][0]["id"], "remote-att:18c2:0");
        assert_eq!(message["attachments"][0]["name"], "a.pdf");
        assert!(message["text"].as_str().unwrap().contains("See you."));
    }

    #[test]
    fn oversized_threads_fit_one_frame() {
        let body = "x".repeat(200 * 1024);
        let mut result =
            json!({"messages": (0..10).map(|_| json!({"text": body})).collect::<Vec<_>>()});
        fit(&mut result);
        assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_REPLY);
        assert_eq!(result["messages"][0]["text_truncated"], true);
    }
}
