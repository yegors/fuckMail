//! Account-scoped durable human composition, sharing legacy replies and capture.
use crate::{err, fields, id, string, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Read};

/// The largest draft body in bytes, for plain text and HTML alike. Forwarded
/// text counts toward it.
pub(crate) const MAX_BODY: usize = 256 * 1024;
const MAX_SUBJECT: usize = 8192;

/// Builds the MIME composition request and sender identity for a projected
/// draft, as capture, Google draft upload and send all need.
///
/// `message_id` becomes the Message-ID and `timestamp_secs` the Date. Fails
/// with `invalid_draft` while any recipient field still holds unparsed text,
/// so a half-typed address is never dropped silently, and with
/// `invalid_request` if the draft has no From address.
pub(crate) fn compose_request(
    account: &str,
    draft: &Value,
    message_id: String,
    timestamp_secs: u64,
) -> Result<(fm_mail::ComposeRequest, fm_mail::AccountIdentity)> {
    if draft["recipient_inputs"]
        .as_object()
        .is_some_and(|inputs| !inputs.is_empty())
    {
        return Err(err(
            "invalid_draft",
            "Finish the recipient addresses before capturing or sending this draft",
        ));
    }
    Ok((
        fm_mail::ComposeRequest {
            account_id: account.into(),
            to: recipients(&draft["to_recipients"])?,
            cc: recipients(&draft["cc_recipients"])?,
            bcc: recipients(&draft["bcc_recipients"])?,
            subject: draft["subject"].as_str().unwrap_or("").into(),
            body_text: draft["body"].as_str().unwrap_or("").into(),
            body_html: draft["body_html"].as_str().map(str::to_owned),
            message_id,
            reply_to_message_id: draft["reply_to_message_id"].as_str().map(str::to_owned),
            references: serde_json::from_value(draft["references"].clone())?,
            timestamp_secs,
        },
        fm_mail::AccountIdentity {
            account_id: account.into(),
            from: fm_mail::Address {
                name: Some(draft["from_name"].as_str().unwrap_or("").into()),
                email: draft["from"]
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "Draft From identity is missing"))?
                    .into(),
            },
            signature_text: draft["signature_text"].as_str().unwrap_or("").into(),
            signature_html: None,
        },
    ))
}

/// Reads the draft selector from `p`: a `draft_id`, or a `conversation_id`
/// naming that conversation's legacy reply draft. Returns the ID and whether
/// it is the legacy form; exactly one of the two keys must be present.
pub(crate) fn selector(p: &Value) -> Result<(&str, bool)> {
    match (p.get("draft_id"), p.get("conversation_id")) {
        (Some(_), None) => Ok((string(p, "draft_id")?, false)),
        (None, Some(_)) => Ok((string(p, "conversation_id")?, true)),
        _ => Err(err(
            "invalid_request",
            "Choose exactly one draft_id or conversation_id",
        )),
    }
}

/// The wall clock in Unix seconds, for draft timestamps.
pub(crate) fn now() -> Result<i64> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| err("invalid_request", "System clock cannot date a draft"))?
            .as_secs(),
    )
    .map_err(|_| err("invalid_request", "System clock cannot date a draft"))
}

fn subject(value: &Value) -> Result<String> {
    value
        .as_str()
        .filter(|s| s.len() <= MAX_SUBJECT && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| {
            err(
                "invalid_request",
                "Subject must be one line of at most 8192 bytes",
            )
        })
}

/// Parses an array of at most 100 `{name?, email}` objects into addresses,
/// checking each with the MIME builder's own `fm_mail::validate_address`.
pub(crate) fn recipients(value: &Value) -> Result<Vec<fm_mail::Address>> {
    let rows = value
        .as_array()
        .filter(|rows| rows.len() <= 100)
        .ok_or_else(|| {
            err(
                "invalid_request",
                "Recipients must be an array of at most 100 addresses",
            )
        })?;
    rows.iter()
        .map(|row| {
            fields(row, &["name", "email"])?;
            let email = row["email"]
                .as_str()
                .filter(|s| s.len() <= 320)
                .ok_or_else(|| err("invalid_request", "Recipient email exceeds its bound"))?
                .to_owned();
            let name = match row.get("name") {
                None | Some(Value::Null) => None,
                Some(Value::String(name)) if name.len() <= 4096 => Some(name.clone()),
                _ => {
                    return Err(err(
                        "invalid_request",
                        "Recipient display name exceeds its bound",
                    ))
                }
            };
            let address = fm_mail::Address { name, email };
            fm_mail::validate_address(&address)
                .map_err(|_| err("invalid_request", "Invalid recipient address or header"))?;
            Ok(address)
        })
        .collect()
}

/// Checks a draft's routing: a one-line subject, valid recipients, at most
/// 100 across To, Cc and Bcc, and at most 64 KiB encoded.
pub(crate) fn validate_routing(routing: &Value) -> Result<()> {
    subject(&routing["subject"])?;
    let count = ["to_recipients", "cc_recipients", "bcc_recipients"]
        .into_iter()
        .map(|key| recipients(&routing[key]).map(|v| v.len()))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum::<usize>();
    if count > 100 || serde_json::to_vec(routing)?.len() > 64 * 1024 {
        return Err(err(
            "invalid_request",
            "Draft routing exceeds 100 recipients or 64 KiB",
        ));
    }
    Ok(())
}

/// The account signature is ordinary draft text the author can edit or delete.
/// A new draft starts with two empty lines for the author, then the signature,
/// then any forwarded text. Nothing is added to it when the message is sent.
pub(crate) fn signed_body(body: &str, signature: &str) -> String {
    if signature.trim().is_empty() {
        body.to_owned()
    } else if body.is_empty() {
        format!("\n\n{signature}")
    } else {
        format!("\n\n{signature}\n\n{body}")
    }
}
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
/// Drafts saved before signatures became body text kept theirs apart. Show
/// that signature at the end of the body; the next save stores it there.
fn merge_kept_signature(row: &mut Value) {
    let signature = row["signature_text"].as_str().unwrap_or("").to_owned();
    if signature.trim().is_empty() || row["discarded"] == true {
        return;
    }
    let body = row["body"].as_str().unwrap_or("");
    row["body"] = json!(if body.trim().is_empty() {
        format!("\n\n{signature}")
    } else {
        format!("{body}\n\n{signature}")
    });
    if let Some(html) = row["body_html"].as_str().filter(|h| !h.is_empty()) {
        row["body_html"] = json!(format!(
            "{html}<br><br>{}",
            escape_html(&signature).replace('\n', "<br>")
        ));
    }
    row["signature_text"] = json!("");
}
impl Store {
    #[cfg(test)]
    pub(crate) fn draft(&self, account: &str, cid: &str) -> Result<Value> {
        self.composition(account, &json!({"conversation_id":cid}))
    }

    /// Reads the stored draft row for `p`'s selector as raw JSON, before
    /// projection, or `None` if there is none.
    pub(crate) fn stored_composition(&self, account: &str, p: &Value) -> Result<Option<Value>> {
        let (selected, legacy) = selector(p)?;
        let row:Option<Value>=self.db.query_row(
            "SELECT draft_id,conversation_id,mode,source_message_id,source_hash,routing_json,body,revision,body_html,attachments_json,discarded,signature_text,from_name,identity_revision,created_at,updated_at,legacy_reply,origin,recipient_inputs_json FROM drafts WHERE account=?1 AND ((?3 AND legacy_reply=1 AND conversation_id=?2) OR (NOT ?3 AND draft_id=?2))",
            params![account,selected,legacy],|r|Ok(json!({"draft_id":r.get::<_,String>(0)?,"conversation_id":r.get::<_,Option<String>>(1)?,
                "mode":r.get::<_,String>(2)?,"source_message_id":r.get::<_,Option<String>>(3)?,"source_hash":r.get::<_,Option<String>>(4)?,
                "routing_json":r.get::<_,String>(5)?,"body":r.get::<_,String>(6)?,"revision":r.get::<_,i64>(7)?,"body_html":r.get::<_,Option<String>>(8)?,
                "attachments_json":r.get::<_,String>(9)?,"discarded":r.get::<_,bool>(10)?,"signature_text":r.get::<_,String>(11)?,
                "from_name":r.get::<_,String>(12)?,"identity_revision":r.get::<_,i64>(13)?,"created_at":r.get::<_,i64>(14)?,
                "updated_at":r.get::<_,i64>(15)?,"legacy_reply":r.get::<_,bool>(16)?,"origin":r.get::<_,String>(17)?,"recipient_inputs_json":r.get::<_,String>(18)?}))).optional()?;
        Ok(row)
    }

    /// Returns the draft for `p`'s selector in the shape clients edit.
    ///
    /// A discarded draft reopens empty apart from the current signature. A
    /// legacy reply never saved is built on the fly at revision 0 and not
    /// written. Fails with `not_found` for a `draft_id` outside the account.
    pub(crate) fn composition(&self, account: &str, p: &Value) -> Result<Value> {
        let (selected, legacy) = selector(p)?;
        let row = if let Some(mut row) = self.stored_composition(account, p)? {
            if row["discarded"] == true {
                // A discarded draft reopens like a new one, with the current signature.
                let (_, signature, _) = self.draft_identity(account)?;
                row["body"] = json!(signed_body("", &signature));
                row["body_html"] = Value::Null;
            } else {
                merge_kept_signature(&mut row);
            }
            row
        } else if legacy {
            self.require_active_account(account)?;
            let (source, hash, routing, _) = self.legacy_routing(account, selected)?;
            let (name, signature, revision) = self.draft_identity(account)?;
            json!({"draft_id":selected,"conversation_id":selected,"mode":"reply","source_message_id":source,"source_hash":hash,
                "routing_json":serde_json::to_string(&routing)?,"body":signed_body("",&signature),"body_html":null,"attachments_json":"[]",
                "revision":0,"discarded":false,"signature_text":"","from_name":name,"identity_revision":revision,
                "created_at":0,"updated_at":0,"legacy_reply":true,"origin":"local"})
        } else {
            return Err(err("not_found", "Draft is outside this account"));
        };
        self.project_composition(account, row)
    }

    /// Expands a stored draft row for clients: routing fields inlined,
    /// attachment metadata (a vanished attachment shows as `missing`),
    /// read-only state, account transfer, and Google upload, send and delete
    /// status.
    ///
    /// A draft is read-only when it is a retained Gmail snapshot, belongs to
    /// an archived account, or moved to another account.
    pub(crate) fn project_composition(&self, account: &str, mut row: Value) -> Result<Value> {
        row["recipient_inputs"] = crate::draft_discard::recipient_inputs(&serde_json::from_str(
            row["recipient_inputs_json"].as_str().unwrap_or("{}"),
        )?)?;
        row.as_object_mut().unwrap().remove("recipient_inputs_json");
        let routing: Value = serde_json::from_str(row["routing_json"].as_str().unwrap())?;
        for (key, value) in routing
            .as_object()
            .ok_or_else(|| err("storage_error", "Draft routing is invalid"))?
        {
            row[key] = value.clone();
        }
        let ids: Vec<String> = serde_json::from_str(row["attachments_json"].as_str().unwrap())?;
        if ids.len() > 50 {
            return Err(err(
                "storage_error",
                "Draft attachment references exceed their bound",
            ));
        }
        let attachments=ids.iter().map(|aid|self.db.query_row("SELECT name,mime_type,size,state FROM attachments WHERE account=?1 AND id=?2",
            params![account,aid],|r|Ok(json!({"id":aid,"name":r.get::<_,String>(0)?,"mime_type":r.get::<_,String>(1)?,"size":r.get::<_,i64>(2)?,"state":r.get::<_,String>(3)?})))
            .optional().map(|row|row.unwrap_or_else(||json!({"id":aid,"state":"missing"})))).collect::<std::result::Result<Vec<_>,_>>()?;
        row["attachment_ids"] = json!(ids);
        row["attachments"] = json!(attachments);
        row["account"] = json!(account);
        row["account_archived"] = json!(self.account_lifecycle(account)?["state"] == "removed");
        row["read_only"] =
            json!(row["origin"] == "google_snapshot" || row["account_archived"] == true);
        row["transferred"] =
            json!(self.draft_transferred(account, row["draft_id"].as_str().unwrap())?);
        row["account_transfer"] =
            self.draft_account_transfer(account, row["draft_id"].as_str().unwrap())?;
        if row["transferred"] == true {
            row["read_only"] = json!(true);
        }
        row["google_delete"] =
            self.google_delete_for_draft(account, row["draft_id"].as_str().unwrap())?;
        row["remote_source"] = if row["revision"].as_i64().unwrap_or(0) > 0 {
            self.remote_draft_source(account, row["draft_id"].as_str().unwrap())?
        } else {
            Value::Null
        };
        row["google"] =
            if row["origin"] != "google_snapshot" && row["revision"].as_i64().unwrap_or(0) > 0 {
                self.google_draft_status(account, row["draft_id"].as_str().unwrap())?
            } else {
                Value::Null
            };
        row["google_send"] =
            if row["origin"] != "google_snapshot" && row["revision"].as_i64().unwrap_or(0) > 0 {
                self.google_send_for_draft(account, row["draft_id"].as_str().unwrap())?
            } else {
                Value::Null
            };
        row["to"] = row["to_recipients"]
            .as_array()
            .and_then(|v| v.first())
            .map(|v| v["email"].clone())
            .unwrap_or(json!(""));
        for key in ["routing_json", "attachments_json"] {
            row.as_object_mut().unwrap().remove(key);
        }
        Ok(row)
    }

    /// Returns the account's display name, signature and metadata revision.
    /// A draft records the name and revision when it is first saved, so later
    /// identity edits do not silently change a draft in progress.
    pub(crate) fn draft_identity(&self, account: &str) -> Result<(String, String, i64)> {
        Ok(self.db.query_row("SELECT a.name,m.signature_text,m.revision FROM accounts a JOIN account_metadata m ON m.account=a.id WHERE a.id=?1",
            [account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?)
    }

    fn legacy_routing(
        &self,
        account: &str,
        cid: &str,
    ) -> Result<(Option<String>, Option<String>, Value, String)> {
        self.legacy_routing_at(account, cid, None)
    }

    /// Builds a legacy reply's routing: from `source_message` when given,
    /// otherwise from the newest message someone else sent in the
    /// conversation (the account's own newest message if there is none).
    /// Returns the source message ID, its source hash, the routing and the
    /// quoted body.
    ///
    /// When the source cannot be read, the reply still addresses the stored
    /// sender and is flagged `source_unavailable`.
    fn legacy_routing_at(
        &self,
        account: &str,
        cid: &str,
        source_message: Option<&str>,
    ) -> Result<(Option<String>, Option<String>, Value, String)> {
        let title: Option<String> = self
            .db
            .query_row(
                "SELECT subject FROM conversations WHERE account=?1 AND id=?2",
                params![account, id(cid)?],
                |r| r.get(0),
            )
            .optional()?;
        let title =
            title.ok_or_else(|| err("not_found", "Conversation is outside this account"))?;
        let from: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let message:Option<(String,String,String,String)>=self.db.query_row(
            "SELECT id,source_hash,sender_email,recipient FROM messages WHERE account=?1 AND conversation_id=?2 AND (?4 IS NULL OR id=?4) ORDER BY (sender_email<>?3) DESC,julianday(sent_at) DESC,id DESC LIMIT 1",
            params![account,cid,from,source_message],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if message.is_none() && source_message.is_some() {
            return Err(err(
                "not_found",
                "Reply source is outside this account and conversation",
            ));
        }
        if let Some((mid, hash, sender, to)) = message {
            if let Ok((mut routing, body)) = self.source_routing(account, cid, &mid, "reply") {
                // A reply goes where the source says (Reply-To, else From);
                // the stored sender stands in only when it names nobody.
                if routing["to_recipients"]
                    .as_array()
                    .is_none_or(|rows| rows.is_empty())
                {
                    routing["to_recipients"] =
                        json!([{"email":if sender==from {to} else {sender}}]);
                }
                routing["subject"] = json!(format!("Re: {title}"));
                return Ok((Some(mid), Some(hash), routing, body));
            }
            let routing = json!({"from":from,"subject":format!("Re: {title}"),"to_recipients":[{"email":if sender==from {to} else {sender}}],
                "cc_recipients":[],"bcc_recipients":[],"reply_to_message_id":null,"references":[],"reply_all_available":false,"source_unavailable":true});
            return Ok((Some(mid), Some(hash), routing, String::new()));
        }
        Ok((
            None,
            None,
            json!({"from":from,"subject":format!("Re: {title}"),"to_recipients":[],"cc_recipients":[],"bcc_recipients":[],
            "reply_to_message_id":null,"references":[],"reply_all_available":false,"source_unavailable":true}),
            String::new(),
        ))
    }

    /// Reads message `mid` of conversation `cid` from its verified raw source
    /// and derives routing and quoted text for `mode`; see
    /// [`routing_from_source`].
    fn source_routing(
        &self,
        account: &str,
        cid: &str,
        mid: &str,
        mode: &str,
    ) -> Result<(Value, String)> {
        let source:Option<(String,String)>=self.db.query_row("SELECT source_hash,body FROM messages WHERE account=?1 AND conversation_id=?2 AND id=?3",
            params![account,id(cid)?,id(mid)?],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (hash, body) = source.ok_or_else(|| {
            err(
                "not_found",
                "Source member is outside this account and conversation",
            )
        })?;
        let bytes = self.composition_source_bytes(&hash)?;
        let from: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        routing_from_source(&bytes, &from, mode, &body)
    }
}

/// Derives routing for `mode` (`reply`, `reply_all` or `forward`) from a raw
/// message, plus the quoted text a forward starts with.
///
/// A reply goes to Reply-To, else From; when that names only the account
/// itself (`from`), it goes to the original To instead. Reply all adds the
/// other To recipients and keeps Cc. The account's own address and duplicates
/// are dropped, case-insensitively. A forward has no recipients or threading
/// headers and quotes `body`, the stored text, line by line with `> `. The
/// subject gains `Re:` or `Fwd:` unless it already starts with it.
/// `references` holds the source's own References; composing appends its
/// Message-ID from `reply_to_message_id`.
pub(crate) fn routing_from_source(
    bytes: &[u8],
    from: &str,
    mode: &str,
    body: &str,
) -> Result<(Value, String)> {
    let parsed = mail_parser::MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| err("content_unavailable", "Reply source cannot be parsed"))?;
    let own = from.to_lowercase();
    let addresses = |value: Option<&mail_parser::Address<'_>>| -> Vec<Value> {
        value
            .map(|v| {
                v.iter()
                    .filter_map(|a| {
                        a.address()
                            .map(|email| json!({"name":a.name(),"email":email}))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut seen = HashSet::from([own]);
    let mut unique = |rows: Vec<Value>| -> Vec<Value> {
        rows.into_iter()
            .filter(|r| seen.insert(r["email"].as_str().unwrap().to_lowercase()))
            .collect()
    };
    let mut reply = unique(addresses(parsed.reply_to().or_else(|| parsed.from())));
    if reply.is_empty() {
        reply = unique(addresses(parsed.to()));
    }
    let additional_to = unique(addresses(parsed.to()));
    let additional_cc = unique(addresses(parsed.cc()));
    let reply_all_available = !additional_to.is_empty() || !additional_cc.is_empty();
    let mut to = reply;
    let cc = if mode == "reply_all" {
        to.extend(additional_to);
        additional_cc
    } else {
        vec![]
    };
    if mode == "forward" {
        to.clear();
    }
    let original_subject = parsed.subject().unwrap_or("(no subject)");
    let prefix = if mode == "forward" { "Fwd:" } else { "Re:" };
    let title = if original_subject
        .to_lowercase()
        .starts_with(&prefix.to_lowercase())
    {
        original_subject.to_owned()
    } else {
        format!("{prefix} {original_subject}")
    };
    let references: Vec<String> = if mode == "forward" {
        vec![]
    } else {
        parsed
            .references()
            .as_text_list()
            .map(|ids| ids.iter().map(|v| format!("<{v}>")).collect())
            .unwrap_or_default()
    };
    if references.len() > 100 {
        return Err(err("limit_exceeded", "Reply source exceeds 100 references"));
    }
    let routing = json!({"from":from,"subject":title,"to_recipients":to,"cc_recipients":cc,"bcc_recipients":[],
            "reply_to_message_id":if mode=="forward" {None} else {parsed.message_id().map(|v|format!("<{v}>"))},
            "references":references,"reply_all_available":reply_all_available,"source_unavailable":false});
    validate_routing(&routing)?;
    let quoted = if mode == "forward" {
        let author = addresses(parsed.from())
            .into_iter()
            .map(|r| r["email"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        let date = parsed.date().map(|d| d.to_rfc3339()).unwrap_or_default();
        format!("---------- Forwarded message ----------\nFrom: {author}\nDate: {date}\nSubject: {original_subject}\n\n{}",body.lines().map(|line|format!("> {line}")).collect::<Vec<_>>().join("\n"))
    } else {
        String::new()
    };
    if quoted.len() > MAX_BODY {
        return Err(err(
            "limit_exceeded",
            "Forwarded text exceeds the draft body limit",
        ));
    }
    Ok((routing, quoted))
}

impl Store {
    /// Reads a stored raw source by hash and re-verifies its size and SHA-256
    /// after reading. Fails with `content_unavailable` if the blob is missing
    /// or changed on disk.
    pub(crate) fn composition_source_bytes(&self, hash: &str) -> Result<Vec<u8>> {
        if !self.blob_present(hash) {
            return Err(err(
                "content_unavailable",
                "Original composition source is unavailable",
            ));
        }
        let size = std::fs::symlink_metadata(self.profile.join("blobs").join(hash))?.len();
        let path = self.verified_blob(hash, size)?;
        // Hash what was actually read: the file could change between the
        // check above and this read. Reading one byte past `size` catches growth.
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(size + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(err(
                "content_unavailable",
                "Original composition source changed",
            ));
        }
        Ok(bytes)
    }

    /// Creates a draft for `p.mode` (`new`, `reply`, `reply_all` or `forward`)
    /// at revision 1 and returns it projected.
    ///
    /// Reply and forward modes take `conversation_id` and `source_message_id`
    /// and read routing from the source's raw bytes. The body starts with the
    /// account signature (see [`signed_body`]). The draft ID is derived from
    /// the account and `request_id`, so one request cannot create two drafts.
    pub(crate) fn create_composition(&self, account: &str, p: &Value) -> Result<Value> {
        let mode = string(p, "mode")?;
        if !["new", "reply", "reply_all", "forward"].contains(&mode) {
            return Err(err("invalid_request", "Unknown composition mode"));
        }
        let (cid, mid, hash, routing, body) = if mode == "new" {
            if p.get("conversation_id").is_some() || p.get("source_message_id").is_some() {
                return Err(err(
                    "invalid_request",
                    "New composition has no source conversation",
                ));
            }
            let from: String =
                self.db
                    .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                        r.get(0)
                    })?;
            (
                None,
                None,
                None,
                json!({"from":from,"subject":"","to_recipients":[],"cc_recipients":[],"bcc_recipients":[],"reply_to_message_id":null,"references":[],"reply_all_available":false,"source_unavailable":false}),
                String::new(),
            )
        } else {
            let cid = string(p, "conversation_id")?;
            let mid = string(p, "source_message_id")?;
            let (routing, body) = self.source_routing(account, cid, mid, mode)?;
            let hash: String = self.db.query_row(
                "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
                params![account, mid],
                |r| r.get(0),
            )?;
            (Some(cid), Some(mid), Some(hash), routing, body)
        };
        let draft_id = format!(
            "draft-{:x}",
            Sha256::digest(format!("{account}\0{}", string(p, "request_id")?).as_bytes())
        );
        let (name, signature, identity) = self.draft_identity(account)?;
        let timestamp = now()?;
        let body = signed_body(&body, &signature);
        self.db.execute("INSERT INTO drafts(account,draft_id,conversation_id,mode,source_message_id,source_hash,routing_json,body,revision,signature_text,from_name,identity_revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,'',?9,?10,?11,?11)",
            params![account,draft_id,cid,mode,mid,hash,serde_json::to_string(&routing)?,body,name,identity,timestamp])?;
        self.composition(account, &json!({"draft_id":draft_id}))
    }

    /// Saves an edit to the draft at revision `expected` and returns it
    /// projected at the next revision; with `discard`, empties it instead.
    ///
    /// `p` carries the whole `body`. Subject, recipients, `attachment_ids`,
    /// `body_html` and `signature_text` keep their stored values when absent,
    /// except that a changed body without `body_html` drops the stored HTML,
    /// which would no longer match. The first save of a legacy reply may pick
    /// its source message; after that the source cannot change. `prepared` is
    /// the draft as the caller already loaded and verified it, so attachments
    /// are not checked again. A save that changes no authored field does not
    /// turn an adopted Gmail draft into an upload. Fails with `stale_revision`
    /// on a revision mismatch and `draft_read_only` for a transferred or
    /// retained remote draft.
    pub(crate) fn save_composition_prepared(
        &self,
        account: &str,
        p: &Value,
        expected: i64,
        discard: bool,
        prepared: Option<&Value>,
    ) -> Result<Value> {
        self.check_draft_writable(account, p)?;
        let mut draft = match prepared {
            Some(draft) => draft.clone(),
            None => self.composition(account, p)?,
        };
        crate::mutations::check_revision(expected, draft["revision"].as_i64().unwrap())?;
        if let Some(source) = p.get("source_message_id") {
            let source = id(source.as_str().ok_or_else(|| {
                err("invalid_request", "source_message_id must be an identifier")
            })?)?;
            if expected == 0 && draft["legacy_reply"] == true && prepared.is_none() {
                let cid = draft["conversation_id"].as_str().unwrap();
                let (mid, hash, routing, _) = self.legacy_routing_at(account, cid, Some(source))?;
                draft["source_message_id"] = json!(mid);
                draft["source_hash"] = json!(hash);
                for (key, value) in routing.as_object().unwrap() {
                    draft[key] = value.clone();
                }
            } else if draft["source_message_id"].as_str() != Some(source) {
                return Err(err(
                    "invalid_request",
                    "Saved draft source cannot be retargeted",
                ));
            }
        }
        let was_discarded = draft["discarded"] == true;
        let original_signature = draft["signature_text"].clone();
        // Recipient text not yet parsed into addresses is kept per field. A
        // field whose parsed addresses arrive in this save no longer has any.
        let pending = if discard {
            json!({})
        } else if let Some(inputs) = p.get("recipient_inputs") {
            crate::draft_discard::recipient_inputs(inputs)?
        } else {
            let mut inputs = draft
                .get("recipient_inputs")
                .cloned()
                .unwrap_or_else(|| json!({}));
            for field in ["to_recipients", "cc_recipients", "bcc_recipients"] {
                if p.get(field).is_some() {
                    inputs.as_object_mut().unwrap().remove(field);
                }
            }
            inputs
        };
        let body = if discard {
            ""
        } else {
            p["body"]
                .as_str()
                .ok_or_else(|| err("invalid_request", "Draft body must be a string"))?
        };
        if body.len() > MAX_BODY {
            return Err(err("invalid_request", "Draft body exceeds 256 KiB"));
        }
        let html = if discard {
            Value::Null
        } else {
            match p.get("body_html") {
                Some(Value::Null) => Value::Null,
                Some(Value::String(html)) if html.len() <= MAX_BODY => json!(html),
                Some(_) => {
                    return Err(err(
                        "invalid_request",
                        "Draft HTML must be null or text of at most 256 KiB",
                    ))
                }
                None if draft["body"] == body => draft["body_html"].clone(),
                None => Value::Null,
            }
        };
        let attachments = if discard {
            json!([])
        } else {
            p.get("attachment_ids")
                .cloned()
                .unwrap_or_else(|| draft["attachment_ids"].clone())
        };
        if prepared.is_none() {
            self.validate_draft_attachments(account, &attachments)?;
        }
        // The From identity is captured when a draft starts, or starts over.
        if draft["revision"] == 0 || was_discarded {
            let (name, _, revision) = self.draft_identity(account)?;
            draft["from_name"] = json!(name);
            draft["identity_revision"] = json!(revision);
        }
        // The signature lives in the body the author saves; a kept separate one
        // was shown merged into that body and is not added again.
        draft["signature_text"] = json!("");
        if let Some(signature) = p.get("signature_text") {
            draft["signature_text"] = json!(signature
                .as_str()
                .filter(|s| s.len() <= 16 * 1024 && !s.contains('\0'))
                .ok_or_else(|| err(
                    "invalid_request",
                    "Draft signature must be text of at most 16 KiB"
                ))?);
        }
        if discard {
            draft["signature_text"] = json!("");
        }
        let mut routing = json!({});
        for key in [
            "from",
            "subject",
            "to_recipients",
            "cc_recipients",
            "bcc_recipients",
            "reply_to_message_id",
            "references",
            "reply_all_available",
            "source_unavailable",
        ] {
            routing[key] = draft[key].clone();
        }
        for key in [
            "subject",
            "to_recipients",
            "cc_recipients",
            "bcc_recipients",
        ] {
            if let Some(value) = p.get(key) {
                routing[key] = value.clone();
            }
        }
        validate_routing(&routing)?;
        // Whether this save changes anything the author wrote. An unchanged
        // save of an adopted Gmail draft must not queue an upload.
        let unchanged = !discard
            && draft
                .get("recipient_inputs")
                .cloned()
                .unwrap_or_else(|| json!({}))
                == pending
            && draft["body"] == body
            && draft["body_html"] == html
            && draft["attachment_ids"] == attachments
            && [
                "from",
                "subject",
                "to_recipients",
                "cc_recipients",
                "bcc_recipients",
                "reply_to_message_id",
                "references",
            ]
            .iter()
            .all(|key| draft[*key] == routing[*key])
            && p.get("signature_text")
                .is_none_or(|v| original_signature == *v);
        let timestamp = now()?;
        let next = expected
            .checked_add(1)
            .ok_or_else(|| err("limit_exceeded", "Draft revision exhausted"))?;
        self.db.execute("INSERT INTO drafts(account,draft_id,conversation_id,legacy_reply,mode,source_message_id,source_hash,routing_json,body,revision,body_html,attachments_json,discarded,signature_text,from_name,identity_revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18) ON CONFLICT(account,draft_id) DO UPDATE SET body=excluded.body,revision=excluded.revision,body_html=excluded.body_html,attachments_json=excluded.attachments_json,discarded=excluded.discarded,signature_text=excluded.signature_text,from_name=excluded.from_name,identity_revision=excluded.identity_revision,routing_json=excluded.routing_json,updated_at=excluded.updated_at",
            params![account,draft["draft_id"].as_str(),draft["conversation_id"].as_str(),draft["legacy_reply"].as_bool().unwrap(),draft["mode"].as_str(),draft["source_message_id"].as_str(),draft["source_hash"].as_str(),serde_json::to_string(&routing)?,body,next,html.as_str(),serde_json::to_string(&attachments)?,discard,draft["signature_text"].as_str(),draft["from_name"].as_str(),draft["identity_revision"].as_i64(),if expected==0 {timestamp} else {draft["created_at"].as_i64().unwrap()},timestamp])?;
        self.db.execute(
            "UPDATE drafts SET recipient_inputs_json=?3 WHERE account=?1 AND draft_id=?2",
            params![
                account,
                draft["draft_id"].as_str(),
                serde_json::to_string(&pending)?
            ],
        )?;
        if unchanged {
            self.advance_unchanged_adoption(
                account,
                draft["draft_id"].as_str().unwrap(),
                expected,
                next,
            )?;
        }
        self.composition(account, &json!({"draft_id":draft["draft_id"]}))
    }

    /// Requires at most 50 distinct attachment IDs of this account, each fully
    /// stored and verified on disk, totalling at most the MIME builder's
    /// attachment limit.
    fn validate_draft_attachments(&self, account: &str, value: &Value) -> Result<()> {
        let ids = value
            .as_array()
            .filter(|v| v.len() <= 50)
            .ok_or_else(|| err("invalid_request", "Draft supports at most 50 attachments"))?;
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for value in ids {
            let aid = id(value
                .as_str()
                .ok_or_else(|| err("invalid_request", "Attachment ID must be a string"))?)?;
            if !seen.insert(aid) {
                return Err(err("invalid_request", "Draft repeats an attachment"));
            }
            let row: Option<(String, i64, String, String)> = self
                .db
                .query_row(
                    "SELECT state,size,blob_hash,name FROM attachments WHERE account=?1 AND id=?2",
                    params![account, aid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let (state, size, hash, name) =
                row.ok_or_else(|| err("not_found", "Draft attachment is outside this account"))?;
            let size = u64::try_from(size)
                .map_err(|_| err("content_unavailable", "Draft attachment size is invalid"))?;
            total = total
                .checked_add(size)
                .filter(|v| *v <= fm_mail::MAX_ATTACHMENT_BYTES as u64)
                .ok_or_else(|| err("invalid_request", "Draft attachments exceed 25 MiB"))?;
            if state != "available" {
                return Err(err(
                    "content_unavailable",
                    "Draft attachment is not fully retained",
                ));
            }
            crate::attachment_content::verify(
                &self.google_blob_store(),
                &hash,
                size,
                &name,
                self.attachment_source(account, aid)?.as_ref(),
            )?;
        }
        Ok(())
    }

    /// Lists the account's open drafts, newest first, optionally filtered by
    /// `query` in the mail search grammar.
    ///
    /// Takes `limit` (1 to 100, default 30) and `offset`; a page also stops
    /// at 512 KiB of summaries. A search matches in Rust over every open
    /// draft rather than through the full-text index. The reply carries
    /// `total` matches, `next_offset` and the parsed `search_terms` for chips.
    pub(crate) fn drafts(&self, account: &str, p: &Value) -> Result<Value> {
        let archived = self.account_lifecycle(account)?["state"] == "removed";
        fields(p, &["query", "limit", "offset"])?;
        let query = match p.get("query") {
            None => "",
            Some(Value::String(s)) if s.len() <= 1024 => s,
            _ => {
                return Err(err(
                    "invalid_request",
                    "Draft query must be text of at most 1024 bytes",
                ))
            }
        };
        // The mail search grammar. Drafts are all from the user, so from:, to:
        // and bare addresses all name a draft's recipients. Drafts have no
        // labels, attachment filter or review state.
        let text = crate::search_query::parse(query)?;
        if text.label.is_some() || text.has_attachment || text.is.is_some() {
            return Err(err(
                "invalid_request",
                "Draft search supports words, subject:, body:, from: and to:",
            ));
        }
        let lowered = |values: &[String]| {
            values
                .iter()
                .flat_map(|v| crate::search_query::tokens(v))
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        };
        let (words, subject_words, body_words) = (
            lowered(&text.words),
            lowered(&text.subject),
            lowered(&text.body),
        );
        // Each phrase must appear as written, as in mail search.
        let phrases = |values: &[String]| {
            values
                .iter()
                .filter(|v| crate::search_query::tokens(v).next().is_some())
                .map(|v| crate::search_query::fold(v))
                .collect::<Vec<_>>()
        };
        let (word_phrases, subject_phrases, body_phrases) = (
            phrases(&text.phrases),
            phrases(&text.subject),
            phrases(&text.body),
        );
        let people_values = text
            .from
            .iter()
            .chain(&text.to)
            .chain(&text.people)
            .map(|v| v.to_lowercase())
            .collect::<Vec<_>>();
        let untokenized = text
            .subject
            .iter()
            .chain(&text.body)
            .any(|v| crate::search_query::tokens(v).next().is_none());
        if words.len() + subject_words.len() + body_words.len() > 32
            || untokenized
            || (!text.words.is_empty()
                && words.is_empty()
                && subject_words.is_empty()
                && body_words.is_empty()
                && people_values.is_empty())
        {
            return Err(err("invalid_request", "Draft search needs 1–32 words"));
        }
        let search_terms = if query.trim().is_empty() {
            Value::Null
        } else {
            text.terms(None)
        };
        let number = |key: &str, default: i64, min, max| -> Result<i64> {
            let n = p
                .get(key)
                .map(|v| v.as_i64().unwrap_or(-1))
                .unwrap_or(default);
            if n < min || n > max {
                Err(err("invalid_request", "Draft pagination exceeds bounds"))
            } else {
                Ok(n)
            }
        };
        let limit = number("limit", 30, 1, 100)?;
        let offset = number("offset", 0, 0, 10_000_000)?;
        if text.is_empty() {
            let mut result = self.drafts_unfiltered(account, limit, offset)?;
            result["search_terms"] = search_terms;
            return Ok(result);
        }
        // The same open-draft predicate as `drafts_unfiltered`. Every open
        // draft is read so `total` counts all matches, not only this page.
        let mut stmt=self.db.prepare("SELECT draft_id,conversation_id,mode,body,routing_json,updated_at,revision,origin FROM drafts d WHERE account=?1 AND discarded=0 AND COALESCE(json_extract(d.google_json,'$.submission.last_accepted_draft_revision'),0)<d.revision AND (origin='local' OR NOT EXISTS(SELECT 1 FROM google_draft_sources s WHERE s.account=d.account AND s.draft_id=d.draft_id AND s.is_current=1 AND s.linked_local_draft_id IS NOT NULL)) ORDER BY updated_at DESC,draft_id")?;
        let mut rows = stmt.query([account])?;
        let mut count = 0i64;
        let mut total = 0i64;
        let mut out = Vec::new();
        let mut bytes = 0usize;
        let mut page_full = false;
        while let Some(row) = rows.next()? {
            count += 1;
            let body: String = row.get(3)?;
            let routing: Value = serde_json::from_str(&row.get::<_, String>(4)?)?;
            let in_subject = crate::search_query::fold(routing["subject"].as_str().unwrap_or(""));
            let in_body = crate::search_query::fold(&body);
            let has =
                |text: &str, phrase: &String| crate::search_query::contains_phrase(text, phrase);
            let mut people = String::new();
            for key in ["to_recipients", "cc_recipients", "bcc_recipients"] {
                for address in routing[key].as_array().into_iter().flatten() {
                    people.push(' ');
                    people.push_str(address["email"].as_str().unwrap_or(""));
                    people.push(' ');
                    people.push_str(address["name"].as_str().unwrap_or(""));
                }
            }
            let people = people.to_lowercase();
            if !(word_phrases
                .iter()
                .all(|t| has(&in_subject, t) || has(&in_body, t))
                && subject_phrases.iter().all(|t| has(&in_subject, t))
                && body_phrases.iter().all(|t| has(&in_body, t))
                && people_values.iter().all(|v| people.contains(v.as_str())))
            {
                continue;
            }
            if total >= offset && !page_full {
                let summary = json!({"draft_id":row.get::<_,String>(0)?,"account":account,"conversation_id":row.get::<_,Option<String>>(1)?,"mode":row.get::<_,String>(2)?,
                    "subject":routing["subject"],"body":body.chars().take(512).collect::<String>(),"snippet":body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(180).collect::<String>(),
                    "to_recipients":routing["to_recipients"],"updated_at":row.get::<_,i64>(5)?,"revision":row.get::<_,i64>(6)?,"origin":row.get::<_,String>(7)?,"read_only":archived || row.get::<_,String>(7)?=="google_snapshot","account_archived":archived});
                let size = serde_json::to_vec(&summary)?.len();
                if out.len() as i64 >= limit || bytes + size > 512 * 1024 {
                    page_full = true;
                } else {
                    bytes += size;
                    out.push(summary);
                }
            }
            total += 1;
        }
        self.enrich_draft_summaries(account, &mut out)?;
        let next = offset + out.len() as i64;
        Ok(
            json!({"drafts":out,"total":total,"draft_count":count,"next_offset":if next<total {Some(next)} else {None},"search_terms":search_terms}),
        )
    }
}

impl Store {
    /// Attaches each summary's remote Gmail source, without raw headers or
    /// attachment IDs, and trims the page again to its 512 KiB budget.
    fn enrich_draft_summaries(&self, account: &str, rows: &mut Vec<Value>) -> Result<()> {
        let ids: Vec<String> = rows
            .iter()
            .filter_map(|r| r["draft_id"].as_str().map(Into::into))
            .collect();
        let mut sources = self.remote_draft_sources(account, &ids)?;
        let mut bytes = 0;
        let mut keep = 0;
        for row in rows.iter_mut() {
            let mut source = sources
                .remove(row["draft_id"].as_str().unwrap())
                .unwrap_or(Value::Null);
            if let Some(object) = source.as_object_mut() {
                object.remove("raw_headers");
                object.remove("attachment_ids");
            }
            row["remote_source"] = source;
            let size = serde_json::to_vec(row)?.len();
            if bytes + size > 512 * 1024 {
                break;
            }
            bytes += size;
            keep += 1;
        }
        rows.truncate(keep);
        Ok(())
    }
    /// One page of open drafts with no search, counted and paged in SQL.
    fn drafts_unfiltered(&self, account: &str, limit: i64, offset: i64) -> Result<Value> {
        let archived = self.account_lifecycle(account)?["state"] == "removed";
        // Open drafts: not discarded, current revision not yet accepted for
        // sending, and not a Gmail snapshot already linked to a local draft,
        // which would otherwise be listed twice.
        let predicate="account=?1 AND discarded=0 AND COALESCE(json_extract(d.google_json,'$.submission.last_accepted_draft_revision'),0)<d.revision AND (origin='local' OR NOT EXISTS(SELECT 1 FROM google_draft_sources s WHERE s.account=d.account AND s.draft_id=d.draft_id AND s.is_current=1 AND s.linked_local_draft_id IS NOT NULL))";
        let count: i64 = self.db.query_row(
            &format!("SELECT COUNT(*) FROM drafts d WHERE {predicate}"),
            [account],
            |r| r.get(0),
        )?;
        let mut stmt=self.db.prepare(&format!("SELECT draft_id,conversation_id,mode,body,routing_json,updated_at,revision,origin FROM drafts d WHERE {predicate} ORDER BY updated_at DESC,draft_id LIMIT ?2 OFFSET ?3"))?;
        let mut rows = stmt.query(params![account, limit, offset])?;
        let mut out = Vec::new();
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let body: String = row.get(3)?;
            let routing: Value = serde_json::from_str(&row.get::<_, String>(4)?)?;
            let origin: String = row.get(7)?;
            let value = json!({"draft_id":row.get::<_,String>(0)?,"account":account,"conversation_id":row.get::<_,Option<String>>(1)?,"mode":row.get::<_,String>(2)?,
                "subject":routing["subject"],"body":body.chars().take(512).collect::<String>(),"snippet":body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(180).collect::<String>(),
                "to_recipients":routing["to_recipients"],"updated_at":row.get::<_,i64>(5)?,"revision":row.get::<_,i64>(6)?,"read_only":archived || origin=="google_snapshot","account_archived":archived,"origin":origin});
            let size = serde_json::to_vec(&value)?.len();
            if bytes + size > 512 * 1024 {
                break;
            }
            bytes += size;
            out.push(value);
        }
        self.enrich_draft_summaries(account, &mut out)?;
        let next = offset + out.len() as i64;
        Ok(
            json!({"drafts":out,"total":count,"draft_count":count,"next_offset":if next<count {Some(next)} else {None}}),
        )
    }
}
