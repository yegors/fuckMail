use super::*;
use mail_parser::{Address, MimeHeaders, PartType};

fn reason(reasons: &mut BTreeSet<String>, code: &str) {
    reasons.insert(code.into());
}

/// Full-source assessment is deliberately separate from the bounded preview.
/// A displayable preview never grants permission to serialize the source again.
pub fn prepare_google_draft_authoring(
    plan: GoogleDraftAuthoringPlan,
    fresh_remote: Option<GoogleDraftActionRemote>,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftAuthoring> {
    cancelled(cancel)?;
    let c = &plan.context;
    let raw = local_content::read_blob(&c.blobs, &c.source.sha256, c.source.size)?;
    let mut changed_source = None;
    if plan.method == "google_draft_adopt" {
        let fresh = fresh_remote.ok_or_else(|| {
            err(
                "invalid_request",
                "Adoption requires a fresh exact-target observation",
            )
        })?;
        if let Err(error) = verify_remote(&c.source, &fresh) {
            if error.code != "stale_source_revision" {
                return Err(error);
            }
            changed_source = Some(prepare_action_source(
                c,
                plan.scope
                    .as_ref()
                    .ok_or_else(invalid)?
                    .connection_generation,
                Some(fresh),
                cancel,
            )?);
        }
    } else if fresh_remote.is_some() {
        return Err(invalid());
    }
    let mut copy = BTreeSet::new();
    let mut adopt = BTreeSet::new();
    let mut losses = BTreeSet::from(["primary_from", "reply_thread", "separate_autosync"]);
    let mut authored = None;
    if let Some(parsed) = mail_parser::MessageParser::default().parse(&raw) {
        let mut routing = json!({"from":c.identity["email"],"subject":parsed.subject().unwrap_or(""),"to_recipients":[],"cc_recipients":[],"bcc_recipients":[],"references":[],"reply_to_message_id":null,"reply_all_available":false,"source_unavailable":false});
        let addresses =
            |value: Option<&Address<'_>>, header: &str, reasons: &mut BTreeSet<String>| -> Value {
                if matches!(value, Some(Address::Group(_))) {
                    reason(reasons, "address_groups");
                }
                let mut rows = Vec::new();
                for address in value.into_iter().flat_map(|v| v.iter()) {
                    if let Some(email) = address.address() {
                        rows.push(json!({"email":email,"name":address.name()}));
                    } else {
                        reason(reasons, "unparsed_routing");
                    }
                }
                if rows.is_empty()
                    && parsed
                        .header_raw(header)
                        .is_some_and(|v| !v.trim().is_empty())
                {
                    reason(reasons, "unparsed_routing");
                }
                json!(rows)
            };
        for (key, header, value) in [
            ("to_recipients", "To", parsed.to()),
            ("cc_recipients", "Cc", parsed.cc()),
            ("bcc_recipients", "Bcc", parsed.bcc()),
        ] {
            routing[key] = addresses(value, header, &mut copy);
        }
        let from = addresses(parsed.from(), "From", &mut copy);
        if from.as_array().is_none_or(|a| a.len() != 1) || composition::recipients(&from).is_err() {
            reason(&mut copy, "unparsed_routing");
        }
        if from[0]["email"]
            .as_str()
            .zip(c.identity["email"].as_str())
            .is_none_or(|(a, b)| a.to_lowercase() != b.to_lowercase())
        {
            reason(&mut adopt, "primary_from_mismatch");
        }
        // Duplicate routing fields, reply redirection and other special headers
        // cannot be represented by the native composition model.
        let mut headers = BTreeSet::new();
        for h in &parsed.parts[0].headers {
            let name = h.name.to_string().to_ascii_lowercase();
            if [
                "from",
                "to",
                "cc",
                "bcc",
                "subject",
                "in-reply-to",
                "references",
            ]
            .contains(&name.as_str())
                && !headers.insert(name.clone())
            {
                reason(&mut copy, "duplicate_routing");
            }
            if name == "reply-to" || name == "sender" || name.starts_with("resent-") {
                reason(&mut copy, "unsupported_routing");
            }
            if ![
                "from",
                "to",
                "cc",
                "bcc",
                "subject",
                "in-reply-to",
                "references",
                "date",
                "message-id",
                "mime-version",
                "content-type",
                "content-transfer-encoding",
            ]
            .contains(&name.as_str())
            {
                reason(&mut adopt, "unsupported_headers");
            }
        }
        let references = parsed.references().as_text_list().unwrap_or_default();
        if references.len() > 100 {
            reason(&mut adopt, "references_limit");
        }
        let refs: Vec<_> = references.iter().map(|r| format!("<{r}>")).collect();
        let reply = parsed.in_reply_to().as_text().map(|r| format!("<{r}>"));
        if parsed
            .in_reply_to()
            .as_text_list()
            .is_some_and(|v| v.len() > 1)
        {
            reason(&mut adopt, "unsupported_routing");
        }
        if refs
            .iter()
            .chain(reply.iter())
            .any(|v| v.len() > 2048 || v.chars().any(char::is_control))
        {
            reason(&mut adopt, "references_limit");
        }
        if plan.method == "google_draft_adopt" {
            routing["references"] = json!(&refs);
            routing["reply_to_message_id"] = json!(&reply);
        }
        if composition::validate_routing(&routing).is_err() {
            reason(&mut copy, "routing_limit");
        }
        let body = match fm_mail::extract_parsed_incoming_text(&parsed) {
            Ok(body) => body,
            Err(_) => {
                reason(&mut copy, "text_unavailable");
                String::new()
            }
        };
        if body.len() > 256 * 1024 || body.contains('\0') {
            reason(&mut copy, "body_limit");
        }
        if parsed
            .parts
            .iter()
            .any(|p| matches!(p.body, PartType::Html(_)))
        {
            reason(&mut adopt, "html");
            losses.insert("html");
        }
        if parsed
            .text_body
            .iter()
            .filter(|i| matches!(parsed.parts[**i as usize].body, PartType::Text(_)))
            .count()
            != 1
        {
            reason(&mut adopt, "plain_text_structure");
        }
        let attachment_indices: BTreeSet<_> = parsed.attachments.iter().copied().collect();
        let body_indices: BTreeSet<_> = parsed
            .text_body
            .iter()
            .chain(&parsed.html_body)
            .copied()
            .collect();
        let mut parts = Vec::new();
        let mut total = 0usize;
        for (index, part) in parsed.parts.iter().enumerate() {
            cancelled(cancel)?;
            if part.is_encoding_problem {
                reason(&mut copy, "mime_decode_problem");
            }
            let ct = part.content_type();
            let mime = ct
                .map(|t| {
                    format!(
                        "{}/{}",
                        t.c_type,
                        t.c_subtype.as_deref().unwrap_or("octet-stream")
                    )
                })
                .unwrap_or_else(|| "text/plain".into());
            let mime = mime.to_ascii_lowercase();
            if matches!(
                mime.as_str(),
                "multipart/signed"
                    | "multipart/encrypted"
                    | "text/calendar"
                    | "application/pgp-encrypted"
                    | "application/pkcs7-mime"
                    | "application/x-pkcs7-mime"
            ) {
                reason(&mut copy, "protected_or_calendar_mime");
            }
            if matches!(part.body, PartType::Multipart(_)) {
                if mime != "multipart/mixed" {
                    reason(&mut adopt, "multipart_structure");
                }
                continue;
            }
            if matches!(part.body, PartType::Message(_)) {
                reason(&mut copy, "nested_message");
            }
            if ct.and_then(|c| c.attribute("format")).is_some()
                || ct.and_then(|c| c.attribute("delsp")).is_some()
            {
                reason(&mut adopt, "flowed_text");
            }
            let is_file = attachment_indices.contains(&(index as u32))
                || (part.content_id().is_some() && !body_indices.contains(&(index as u32)));
            if part.content_id().is_some()
                || part
                    .content_disposition()
                    .is_some_and(|d| d.c_type.eq_ignore_ascii_case("inline"))
            {
                reason(&mut adopt, "inline_placement");
                losses.insert("inline_placement");
            }
            if !is_file {
                if !body_indices.contains(&(index as u32)) {
                    reason(&mut copy, "unrepresented_part");
                }
                continue;
            }
            let name = part.attachment_name().unwrap_or("attachment.bin");
            let bytes = part.contents();
            total = total.saturating_add(bytes.len());
            if parts.len() >= 50 || total > 25 * 1024 * 1024 {
                reason(&mut copy, "attachment_limit");
                continue;
            }
            if name.len() > 1024 || name.chars().any(char::is_control) || mime.len() > 256 {
                reason(&mut copy, "attachment_metadata");
                continue;
            }
            let sha256 = c.blobs.stage_blob(bytes)?;
            let id = format!(
                "source-copy-part-{}",
                hash(format!("{}\0{}\0{index}\0{sha256}", c.account, c.source.sha256).as_bytes())
            );
            parts.push(Part {
                id,
                name: name.into(),
                mime_type: mime,
                size: bytes.len() as u64,
                sha256,
                content_id: None,
                disposition: Some("attachment".into()),
                ordinal: index,
            });
        }
        let from_name = if plan.method == "google_draft_adopt" {
            from[0]["name"].as_str().unwrap_or("").to_owned()
        } else {
            c.identity["name"].as_str().unwrap_or("").to_owned()
        };
        if copy.is_empty() {
            let mut copy_routing = routing.clone();
            copy_routing["references"] = json!([]);
            copy_routing["reply_to_message_id"] = Value::Null;
            if !can_build(c, &copy_routing, &body, &from_name, &parts)? {
                reason(&mut copy, "mime_not_representable");
            }
            let mut adopt_routing = routing.clone();
            adopt_routing["references"] = json!(refs);
            adopt_routing["reply_to_message_id"] = json!(reply);
            if !can_build(
                c,
                &adopt_routing,
                &body,
                from[0]["name"].as_str().unwrap_or(""),
                &parts,
            )? {
                reason(&mut adopt, "mime_not_representable");
            }
        }
        adopt.extend(copy.iter().cloned());
        authored = Some(Authored {
            routing,
            body,
            from_name,
            parts,
        });
    } else {
        reason(&mut copy, "mime_unparseable");
        reason(&mut adopt, "mime_unparseable");
    }
    let assessment = json!({"account":c.account,"draft_id":c.selected,"source_revision":c.source.revision,"source_sha256":c.source.sha256,"provider_draft_id":c.source.provider_draft_id,"primary_from":c.identity,
        "copy":{"allowed":copy.is_empty(),"reasons":copy,"losses":losses},"adopt":{"allowed":adopt.is_empty()&&c.source.is_current,"reasons":adopt,"requires_remote_check":true},"delete":{"allowed":c.source.is_current,"reasons":if c.source.is_current {vec![]} else {vec!["stale_source_revision"]},"requires_remote_check":true}});
    cancelled(cancel)?;
    Ok(PreparedGoogleDraftAuthoring {
        plan,
        assessment,
        authored,
        cancel: cancel.clone(),
        changed_source,
    })
}

fn can_build(
    c: &ActionContext,
    routing: &Value,
    body: &str,
    from_name: &str,
    parts: &[Part],
) -> Result<bool> {
    let mut draft = routing.clone();
    draft["body"] = json!(body);
    draft["body_html"] = Value::Null;
    draft["signature_text"] = json!("");
    draft["from_name"] = json!(from_name);
    let Ok((request, identity)) = composition::compose_request(
        &c.account,
        &draft,
        "<assessment@fuckmail.invalid>".into(),
        0,
    ) else {
        return Ok(false);
    };
    let staged = parts
        .iter()
        .map(|part| {
            Ok(fm_mail::StagedAttachment {
                account_id: c.account.clone(),
                filename: part.name.clone(),
                content_type: part.mime_type.clone(),
                bytes: local_content::read_blob(&c.blobs, &part.sha256, part.size)?,
                sha256: part.sha256.clone(),
                content_id: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(fm_mail::build_draft_mail(&request, &identity, &staged).is_ok())
}

pub(super) fn verify_remote(source: &SourceStamp, remote: &GoogleDraftActionRemote) -> Result<()> {
    identifier(&remote.draft_id)?;
    identifier(&remote.message_id)?;
    identifier(&remote.thread_id)?;
    if remote.raw.is_empty() || remote.raw.len() > MAX_IMPORT_RAW_BYTES {
        return Err(err("limit_exceeded", "Remote draft source exceeds 40 MiB"));
    }
    if source.provider_draft_id != remote.draft_id
        || source.message_id != remote.message_id
        || source.thread_id != remote.thread_id
        || source.size != remote.raw.len() as u64
        || source.sha256 != hash(&remote.raw)
    {
        return Err(err(
            "stale_source_revision",
            "Remote draft changed; refresh and inspect its retained source before acting",
        ));
    }
    Ok(())
}
