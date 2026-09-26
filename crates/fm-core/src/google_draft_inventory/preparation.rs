//! Off-writer work for observed Gmail drafts: parsing RAW into a bounded
//! preview and staging its parts as blobs before the store records the import,
//! and exporting a stored draft attachment.
use super::*;
use mail_parser::MimeHeaders;

fn issue(source: &mut Source, code: &str) {
    if source.issues.len() < 32 && !source.issues.iter().any(|s| s == code) {
        source.issues.push(code.into());
    }
}
fn bounded(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}
fn empty_preview() -> Preview {
    Preview {
        routing: json!({"from":"","subject":"","to_recipients":[],"cc_recipients":[],"bcc_recipients":[],
        "references":[],"reply_to_message_id":null,"reply_all_available":false,"source_unavailable":true}),
        body: String::new(),
        html: None,
        from_name: String::new(),
    }
}

/// RAW stays in this admitted blocking worker; Store receives only bounded
/// normalized fields and unforgeable references to already published blobs.
pub fn prepare_google_draft_import(
    plan: GoogleDraftImportPlan,
    outcome: GoogleDraftImportOutcome,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftImport> {
    cancelled(cancel)?;
    let mut source = Source {
        provider_draft_id: plan.item.draft_id.clone(),
        message_id: Some(plan.item.message_id.clone()),
        thread_id: None,
        sha256: None,
        size: None,
        source_state: "unavailable".into(),
        preview_state: "unavailable".into(),
        issues: Vec::new(),
        part_manifest_sha256: None,
        part_manifest_size: None,
        attachment_ids: Vec::new(),
        possible_unknown_create: false,
        candidate_request_ids: Vec::new(),
        raw_headers: json!({}),
    };
    let raw = match outcome {
        GoogleDraftImportOutcome::Missing => {
            source.source_state = "missing".into();
            issue(&mut source, "not_found");
            None
        }
        GoogleDraftImportOutcome::Unavailable { code } => {
            if code.is_empty()
                || code.len() > 96
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit())
            {
                return Err(invalid());
            }
            issue(&mut source, &code);
            None
        }
        GoogleDraftImportOutcome::Available {
            draft_id,
            message_id,
            thread_id,
            raw,
        } => {
            identifier(&draft_id)?;
            identifier(&message_id)?;
            identifier(&thread_id)?;
            if draft_id != plan.item.draft_id {
                return Err(err(
                    "invalid_google_response",
                    "Fetched draft container differs from requested identity",
                ));
            }
            if raw.is_empty() || raw.len() > MAX_IMPORT_RAW_BYTES {
                issue(&mut source, "source_too_large");
                None
            } else {
                source.message_id = Some(message_id);
                source.thread_id = Some(thread_id);
                source.sha256 = Some(plan.blobs.stage_blob(&raw)?);
                source.size = Some(raw.len() as u64);
                source.source_state = "available".into();
                Some(raw)
            }
        }
    };
    let mut preview = None;
    let mut parts = Vec::new();
    if let Some(raw) = raw {
        cancelled(cancel)?;
        let mut displayed = empty_preview();
        if let Some(parsed) = mail_parser::MessageParser::default().parse(&raw) {
            for (key, name) in [
                ("from", "From"),
                ("to", "To"),
                ("cc", "Cc"),
                ("bcc", "Bcc"),
                ("subject", "Subject"),
                ("reply_to", "Reply-To"),
                ("in_reply_to", "In-Reply-To"),
                ("references", "References"),
                ("message_id", "Message-ID"),
            ] {
                let text = parsed.header_raw(name).unwrap_or("");
                if text.len() > 2048 {
                    issue(&mut source, "header_preview_truncated");
                }
                source.raw_headers[key] = json!(bounded(text, 2048));
            }
            let mut recipients = 0;
            for (key, header, addresses) in [
                ("to_recipients", "to", parsed.to()),
                ("cc_recipients", "cc", parsed.cc()),
                ("bcc_recipients", "bcc", parsed.bcc()),
            ] {
                let mut rows = Vec::new();
                for address in addresses.into_iter().flat_map(|a| a.iter()) {
                    if recipients >= 100 {
                        issue(&mut source, "recipient_preview_limit");
                        break;
                    }
                    let Some(email) = address.address() else {
                        issue(&mut source, "unparsed_recipient");
                        continue;
                    };
                    if email.len() > 2048 || address.name().is_some_and(|v| v.len() > 1024) {
                        issue(&mut source, "recipient_preview_limit");
                        continue;
                    }
                    rows.push(json!({"email":email,"name":address.name()}));
                    recipients += 1;
                }
                if rows.is_empty()
                    && !source.raw_headers[header]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .is_empty()
                {
                    issue(&mut source, "unparsed_recipient");
                }
                displayed.routing[key] = json!(rows);
            }
            let sender = parsed.from().and_then(|a| a.first());
            let from = sender.and_then(|a| a.address()).unwrap_or("");
            let from_name = sender.and_then(|a| a.name()).unwrap_or("");
            if from.len() > 2048 || from_name.len() > 1024 {
                issue(&mut source, "from_preview_limit");
            }
            displayed.routing["from"] = json!(bounded(from, 2048));
            displayed.from_name = bounded(from_name, 1024);
            if from.is_empty()
                && !source.raw_headers["from"]
                    .as_str()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            {
                issue(&mut source, "unparsed_from");
            }
            if parsed.from().is_some_and(|a| a.iter().count() > 1) {
                issue(&mut source, "multiple_from_preview");
            }
            let title = parsed.subject().unwrap_or("");
            if title.len() > 2048 {
                issue(&mut source, "subject_preview_limit");
            }
            displayed.routing["subject"] = json!(bounded(title, 2048));
            displayed.routing["source_unavailable"] = json!(false);
            let reply_to = parsed.in_reply_to().as_text();
            if reply_to.is_some_and(|v| v.len() > 2048) {
                issue(&mut source, "references_preview_limit");
            }
            displayed.routing["reply_to_message_id"] = json!(reply_to.map(|v| bounded(v, 2048)));
            let references = parsed.references().as_text_list().unwrap_or_default();
            if references.len() > 100 || references.iter().any(|v| v.len() > 2048) {
                issue(&mut source, "references_preview_limit");
            }
            let refs: Vec<_> = references
                .iter()
                .take(100)
                .map(|v| bounded(v, 2048))
                .collect();
            displayed.routing["references"] = json!(refs);
            match fm_mail::extract_parsed_incoming_text(&parsed) {
                Ok(body) => {
                    if body.len() > 256 * 1024 {
                        issue(&mut source, "body_preview_limit");
                    }
                    displayed.body = bounded(&body, 256 * 1024);
                }
                Err(_) => issue(&mut source, "body_preview_unavailable"),
            }
            if let Some(html) = parsed.body_html(0) {
                if html.len() <= 128 * 1024 {
                    displayed.html = Some(html.into_owned());
                } else {
                    issue(&mut source, "html_preview_limit");
                }
            }
            let attachment_indices: BTreeSet<_> = parsed.attachments.iter().copied().collect();
            let mut total = 0usize;
            for (index, part) in parsed.parts.iter().enumerate() {
                if !attachment_indices.contains(&(index as u32)) && part.content_id().is_none() {
                    continue;
                }
                cancelled(cancel)?;
                let bytes = part.contents();
                if parts.len() >= 50 || total.saturating_add(bytes.len()) > 25 * 1024 * 1024 {
                    issue(&mut source, "attachment_preview_limit");
                    continue;
                }
                if part.is_encoding_problem {
                    issue(&mut source, "attachment_decode_problem");
                    continue;
                }
                if part.content_id().is_some_and(|id| id.len() > 2048) {
                    issue(&mut source, "cid_preview_limit");
                    continue;
                }
                total += bytes.len();
                let sha256 = plan.blobs.stage_blob(bytes)?;
                let aid = format!(
                    "google-draft-part-{}",
                    hash(
                        format!(
                            "{}\0{}\0{}\0{index}\0{sha256}",
                            plan.scope.account,
                            plan.item.draft_id,
                            source.sha256.as_ref().unwrap()
                        )
                        .as_bytes()
                    )
                );
                let name = part.attachment_name().unwrap_or("attachment.bin");
                if name.len() > 1024 {
                    issue(&mut source, "attachment_name_preview_limit");
                }
                let mime_type = part
                    .content_type()
                    .map(|t| {
                        format!(
                            "{}/{}",
                            t.c_type,
                            t.c_subtype.as_deref().unwrap_or("octet-stream")
                        )
                    })
                    .unwrap_or_else(|| "application/octet-stream".into());
                let disposition = part.content_disposition().map(|v| v.c_type.as_ref());
                if mime_type.len() > 256 || disposition.is_some_and(|v| v.len() > 128) {
                    issue(&mut source, "attachment_metadata_preview_limit");
                }
                parts.push(Part {
                    id: aid,
                    name: bounded(name, 1024),
                    mime_type: bounded(&mime_type, 256),
                    size: bytes.len() as u64,
                    sha256,
                    content_id: part.content_id().map(Into::into),
                    disposition: part.content_disposition().map(|v| bounded(&v.c_type, 128)),
                    ordinal: index,
                });
            }
            if parsed.parts.iter().any(|p| p.is_encoding_problem) {
                issue(&mut source, "mime_decode_problem");
            }
            source.preview_state = if source.issues.is_empty() {
                "complete"
            } else {
                "partial"
            }
            .into();
        } else {
            issue(&mut source, "malformed_mime");
        }
        // Even a malformed preview retains the new exact RAW and replaces stale
        // display text with an explicit unavailable preview, never authored data.
        source.attachment_ids = parts.iter().map(|p| p.id.clone()).collect();
        let manifest = serde_json::to_vec(&parts)?;
        source.part_manifest_sha256 = Some(plan.blobs.stage_blob(&manifest)?);
        source.part_manifest_size = Some(manifest.len() as u64);
        if serde_json::to_vec(&displayed.routing)?.len() > 65536 {
            issue(&mut source, "routing_preview_limit");
            displayed.routing = empty_preview().routing;
            source.preview_state = "partial".into();
        }
        preview = Some(displayed);
    }
    cancelled(cancel)?;
    Ok(PreparedGoogleDraftImport {
        plan,
        source,
        preview,
        parts,
        cancel: cancel.clone(),
    })
}

/// Cached exact-revision export: no provider request and no Store/file I/O mix.
pub fn export_google_draft_source(
    plan: GoogleDraftSourcePlan,
    cancel: &CancellationToken,
) -> Result<Value> {
    cancelled(cancel)?;
    let path = plan
        .blobs
        .export_attachment(&plan.hash, plan.size, &plan.name)?;
    cancelled(cancel)?;
    Ok(
        json!({"account":plan.account,"draft_id":plan.draft_id,"source_revision":plan.source_revision,
        "attachment_id":plan.attachment_id,"path":path.to_str().ok_or_else(invalid)?,"sha256":plan.hash,
        "size":plan.size,"name":plan.name,"mime_type":plan.mime_type}),
    )
}
