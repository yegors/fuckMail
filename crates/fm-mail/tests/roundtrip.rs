use fm_mail::*;
use mail_parser::{MessageParser, MimeHeaders};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn address(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.into()),
        email: email.into(),
    }
}
fn identity() -> AccountIdentity {
    AccountIdentity {
        account_id: "work".into(),
        from: address("Yegor — Работа", "me@example.test"),
        signature_text: "Yegor\nWork signature — Montréal".into(),
        signature_html: None,
    }
}
fn draft() -> ComposeRequest {
    ComposeRequest {
        account_id: "work".into(),
        to: vec![address("Éléna", "elena@example.test")],
        cc: vec![address("Reviewer", "review@example.test")],
        bcc: vec![address("Private copy", "private@example.test")],
        subject: "Re: Café budget — 日本語".into(),
        body_text: "Hi Éléna,\n\nThe café budget is €42. 日本語 — спасибо.\n> Original question\n"
            .into(),
        body_html: None,
        message_id: "<local-draft-1@example.test>".into(),
        reply_to_message_id: Some("<parent@example.test>".into()),
        references: vec!["<root@example.test>".into()],
        timestamp_secs: 1_789_128_000,
    }
}
fn staged(filename: &str, cid: Option<&str>, bytes: &[u8]) -> StagedAttachment {
    StagedAttachment {
        account_id: "work".into(),
        filename: filename.into(),
        content_type: if cid.is_some() {
            "image/png".into()
        } else {
            "application/octet-stream".into()
        },
        bytes: bytes.to_vec(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        content_id: cid.map(str::to_owned),
    }
}

fn recipient_free_draft() -> ComposeRequest {
    let mut request = draft();
    request.to.clear();
    request.cc.clear();
    request.bcc.clear();
    request
}

#[test]
fn recipient_free_plain_draft_keeps_exact_headers_and_text_without_an_envelope() {
    let mut request = recipient_free_draft();
    request.body_text = "Unfinished café draft\r\n日本語\n".into();
    let account = identity();
    let built = build_draft_mail(&request, &account, &[]).unwrap();
    let parsed = MessageParser::default().parse(&built.raw).unwrap();
    assert!(parsed.to().is_none() && parsed.cc().is_none() && parsed.bcc().is_none());
    assert_eq!(
        parsed.from().unwrap().first().unwrap().name(),
        account.from.name.as_deref()
    );
    assert_eq!(
        parsed.from().unwrap().first().unwrap().address(),
        Some(account.from.email.as_str())
    );
    assert_eq!(parsed.subject(), Some(request.subject.as_str()));
    assert_eq!(parsed.message_id(), Some("local-draft-1@example.test"));
    assert_eq!(parsed.in_reply_to().as_text(), Some("parent@example.test"));
    assert_eq!(
        parsed.references().as_text_list().unwrap(),
        ["root@example.test", "parent@example.test"]
    );
    assert_eq!(
        parsed.date().unwrap().to_timestamp(),
        request.timestamp_secs as i64
    );
    let plain: Vec<_> = parsed
        .parts
        .iter()
        .filter(|part| part.is_content_type("text", "plain"))
        .collect();
    assert_eq!(plain.len(), 1);
    assert_eq!(
        plain[0].contents(),
        format!("{}\n\n{}", request.body_text, account.signature_text).as_bytes()
    );
    assert!(!parsed
        .parts
        .iter()
        .any(|part| part.is_content_type("text", "html")));
    assert!(parsed.parts[0].is_content_type("multipart", "mixed"));
    let raw = std::str::from_utf8(&built.raw).unwrap();
    let top_headers = raw.split_once("\r\n\r\n").unwrap().0;
    assert!(top_headers.contains("\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed;"));
    assert_eq!(built.sha256, format!("{:x}", Sha256::digest(&built.raw)));
    assert_eq!(
        build_draft_mail(&request, &account, &[]).unwrap().raw,
        built.raw
    );
    assert!(matches!(
        build_mail(&request, &account, &[]),
        Err(ComposeError::NoRecipients)
    ));

    request.body_text.clear();
    request.subject.clear();
    request.reply_to_message_id = None;
    request.references.clear();
    let mut empty_identity = identity();
    empty_identity.signature_text.clear();
    let empty = build_draft_mail(&request, &empty_identity, &[]).unwrap();
    let parsed = MessageParser::default().parse(&empty.raw).unwrap();
    assert!(parsed
        .parts
        .iter()
        .any(|part| part.is_content_type("text", "plain") && part.contents().is_empty()));
    assert!(parsed.to().is_none() && parsed.cc().is_none() && parsed.bcc().is_none());
}

#[test]
fn recipient_free_rich_draft_retains_exact_alternatives_and_attachment_metadata() {
    let mut request = recipient_free_draft();
    request.body_text = "Authored café 日本語\r\nSecond line\n".into();
    request.body_html =
        Some("<p>Authored café 日本語</p><img src=\"cid:logo-1\" alt=\"Logo\">".into());
    let mut account = identity();
    account.signature_html = Some("<b>Signature</b>".into());
    let attachments = [
        staged("résumé 📄.bin", None, b"\0\x01\xffexact attachment\r\n"),
        staged("logo.png", Some("logo-1"), b"\x89PNG\r\n\0\xffexact inline"),
    ];
    let built = build_draft_mail(&request, &account, &attachments).unwrap();
    let parsed = MessageParser::default().parse(&built.raw).unwrap();
    assert!(parsed.to().is_none() && parsed.cc().is_none() && parsed.bcc().is_none());
    let texts: Vec<_> = parsed
        .parts
        .iter()
        .filter(|part| part.is_content_type("text", "plain"))
        .collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(
        texts[0].contents(),
        format!("{}\n\n{}", request.body_text, account.signature_text).as_bytes()
    );
    let html: Vec<_> = parsed
        .parts
        .iter()
        .filter(|part| part.is_content_type("text", "html"))
        .collect();
    assert_eq!(html.len(), 1);
    assert_eq!(html[0].contents(), "<p>Authored café 日本語</p><img src=\"cid:logo-1\" alt=\"Logo\"><br><br><b>Signature</b>".as_bytes());
    let parts: BTreeMap<_, _> = parsed
        .attachments()
        .map(|part| (part.attachment_name().unwrap(), part))
        .collect();
    assert_eq!(parts.len(), 2);
    for attachment in &attachments {
        let part = parts[attachment.filename.as_str()];
        assert_eq!(part.contents(), attachment.bytes);
        assert_eq!(part.content_id(), attachment.content_id.as_deref());
        let (main, sub) = attachment.content_type.split_once('/').unwrap();
        assert!(part.is_content_type(main, sub));
        assert_eq!(
            part.content_disposition().unwrap().ctype(),
            if attachment.content_id.is_some() {
                "inline"
            } else {
                "attachment"
            }
        );
    }
    assert!(!built.html_was_sanitized);
}

#[test]
fn recipient_bearing_drafts_are_byte_identical_to_existing_submission_formatting() {
    for rich in [false, true] {
        for recipients in 0..4 {
            let mut request = draft();
            if recipients != 0 && recipients != 1 {
                request.to.clear();
            }
            if recipients != 0 && recipients != 2 {
                request.cc.clear();
            }
            if recipients != 0 && recipients != 3 {
                request.bcc.clear();
            }
            let mut attachments = vec![staged("binary.bin", None, b"\0\xfe\xffattachment\r\n")];
            if rich {
                request.body_html =
                    Some("<p>Exact Unicode 日本語</p><img src=\"cid:logo-1\" alt=\"Logo\">".into());
                attachments.push(staged("logo.png", Some("logo-1"), b"\x89PNG\0\xff"));
            }
            let submission = build_mail(&request, &identity(), &attachments).unwrap();
            let stored = build_draft_mail(&request, &identity(), &attachments).unwrap();
            assert_eq!(stored.raw, submission.raw);
            assert_eq!(stored.sha256, submission.sha256);
            assert_eq!(stored.html_was_sanitized, submission.html_was_sanitized);
            if !request.bcc.is_empty() {
                let parsed = MessageParser::default().parse(&stored.raw).unwrap();
                assert_eq!(
                    parsed.bcc().unwrap().first().unwrap().address(),
                    Some("private@example.test")
                );
            }
        }
    }
}

#[test]
fn unfinished_drafts_keep_account_header_address_and_attachment_validation() {
    let request = recipient_free_draft();
    let mut cases = Vec::new();
    let mut invalid = request.clone();
    invalid.subject = "subject\r\nInjected: header".into();
    cases.push((invalid, ComposeError::InvalidHeader("subject")));
    let mut invalid = request.clone();
    invalid.message_id = "missing-brackets@example.test".into();
    cases.push((invalid, ComposeError::InvalidHeader("message identity")));
    let mut invalid = request.clone();
    invalid.references = vec!["bad reference".into()];
    cases.push((invalid, ComposeError::InvalidHeader("message identity")));
    let mut invalid = request.clone();
    invalid.reply_to_message_id = Some("bad reply".into());
    cases.push((invalid, ComposeError::InvalidHeader("message identity")));
    let mut invalid = request.clone();
    invalid.account_id = "personal".into();
    cases.push((invalid, ComposeError::WrongAccount));
    let mut invalid = request.clone();
    invalid.timestamp_secs = 253_402_300_800;
    cases.push((invalid, ComposeError::InvalidDate));
    let mut invalid = request.clone();
    invalid.body_text = "x".repeat(MAX_BODY_BYTES + 1);
    cases.push((invalid, ComposeError::LimitExceeded("authored body")));
    let mut invalid = request.clone();
    invalid.to = vec![address("Invalid", "not-an-address")];
    cases.push((invalid, ComposeError::InvalidAddress));
    for (invalid, expected) in cases {
        assert_eq!(
            build_draft_mail(&invalid, &identity(), &[]).err(),
            Some(expected)
        );
    }
    let mut invalid_from = identity();
    invalid_from.from.email = "not-an-address".into();
    assert_eq!(
        build_draft_mail(&request, &invalid_from, &[]).err(),
        Some(ComposeError::InvalidAddress)
    );
    invalid_from.from = address("Name\r\nInjected: header", "valid@example.test");
    assert_eq!(
        build_draft_mail(&request, &invalid_from, &[]).err(),
        Some(ComposeError::InvalidHeader("display name"))
    );

    let mut parts = Vec::new();
    let mut invalid = staged("file.bin", None, b"x");
    invalid.account_id = "personal".into();
    parts.push((invalid, ComposeError::WrongAccount));
    let mut invalid = staged("file.bin", None, b"x");
    invalid.sha256 = "wrong".into();
    parts.push((invalid, ComposeError::AttachmentNotVerified));
    let mut invalid = staged("file.bin", None, b"x");
    invalid.content_type = "not a MIME type".into();
    parts.push((invalid, ComposeError::InvalidContentType));
    parts.push((
        staged("file\r\nInjected: header", None, b"x"),
        ComposeError::InvalidHeader("filename"),
    ));
    parts.push((
        staged("", None, b"x"),
        ComposeError::InvalidHeader("filename"),
    ));
    parts.push((
        staged("logo.png", Some("bad cid"), b"x"),
        ComposeError::InvalidContentId,
    ));
    for (invalid, expected) in parts {
        assert_eq!(
            build_draft_mail(&request, &identity(), &[invalid]).err(),
            Some(expected)
        );
    }
    let parts: Vec<_> = (0..51).map(|_| staged("file.bin", None, b"x")).collect();
    assert_eq!(
        build_draft_mail(&request, &identity(), &parts).err(),
        Some(ComposeError::LimitExceeded("attachments"))
    );
    let oversized = StagedAttachment {
        bytes: vec![0; MAX_ATTACHMENT_BYTES + 1],
        ..staged("large.bin", None, b"")
    };
    assert_eq!(
        build_draft_mail(&request, &identity(), &[oversized]).err(),
        Some(ComposeError::LimitExceeded("attachment bytes"))
    );
}

#[test]
fn plain_unicode_reply_roundtrips_exact_author_text_signature_and_headers() {
    let request = draft();
    let account = identity();
    let built = build_mail(&request, &account, &[]).unwrap();
    let parsed = MessageParser::default().parse(&built.raw).unwrap();
    assert_eq!(parsed.subject(), Some(request.subject.as_str()));
    assert_eq!(
        parsed.from().unwrap().first().unwrap().address(),
        Some("me@example.test")
    );
    assert_eq!(parsed.to().unwrap().first().unwrap().name(), Some("Éléna"));
    assert_eq!(
        parsed.cc().unwrap().first().unwrap().address(),
        Some("review@example.test")
    );
    // Gmail needs this header for Bcc routing; there is no separate SMTP envelope.
    assert_eq!(
        parsed.bcc().unwrap().first().unwrap().address(),
        Some("private@example.test")
    );
    assert_eq!(parsed.message_id(), Some("local-draft-1@example.test"));
    assert_eq!(parsed.in_reply_to().as_text(), Some("parent@example.test"));
    let refs = parsed.references().as_text_list().unwrap();
    assert_eq!(
        refs.iter().map(|value| value.as_ref()).collect::<Vec<_>>(),
        vec!["root@example.test", "parent@example.test"]
    );
    assert_eq!(
        parsed.body_text(0).unwrap(),
        format!("{}\n\n{}", request.body_text, account.signature_text)
    );
    assert_eq!(
        build_mail(&request, &account, &[]).unwrap().raw,
        built.raw,
        "immutable send intent must rebuild identically"
    );
    assert!(!built.html_was_sanitized);
    // mail-parser supplies a synthesized HTML accessor for plain mail; inspect MIME parts.
    assert!(!parsed
        .parts
        .iter()
        .any(|part| part.is_content_type("text", "html")));
}

#[test]
fn html_alternative_keeps_qt_formatting_and_inline_attachment_bytes() {
    let mut request = draft();
    request.body_html = Some("<p>Hello <span style=\"font-weight:700;font-style:italic\">Éléna</span>.</p><img src=\"cid:logo-1\" alt=\"Our logo\">".into());
    let attachments = vec![
        staged("résumé 📄.pdf", None, b"\0\x01\xfffixture-pdf\r\n"),
        staged("logo.png", Some("logo-1"), b"\x89PNG\r\nfixture-bytes"),
    ];
    let built = build_mail(&request, &identity(), &attachments).unwrap();
    let parsed = MessageParser::default().parse(&built.raw).unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap(),
        with_signature(&request.body_text, &identity().signature_text)
    );
    let html = parsed.body_html(0).unwrap();
    assert!(html.contains("font-weight:700"));
    assert!(html.contains("font-style:italic"));
    assert!(html.contains("cid:logo-1"));
    assert!(html.contains("Montréal"));
    let parts: BTreeMap<_, _> = parsed
        .attachments()
        .map(|part| (part.attachment_name().unwrap(), part))
        .collect();
    assert_eq!(parts.len(), 2);
    for attachment in &attachments {
        let part = parts[attachment.filename.as_str()];
        assert_eq!(part.contents(), attachment.bytes);
        assert_eq!(part.content_id(), attachment.content_id.as_deref());
    }
    for part in &parsed.parts {
        if let Some(boundary) = part
            .content_type()
            .and_then(|content_type| content_type.attribute("boundary"))
        {
            assert!(boundary.len() <= 70);
        }
    }
}

#[test]
fn pasted_html_drops_active_content_remote_images_and_unbounded_styles() {
    let mut request = draft();
    let original = "<p onclick=\"unused()\" style=\"font-weight:700;position:fixed;background:url(https://example.test/pixel)\">Authored text</p><script>DROP_ME</script><iframe src=\"https://example.test\"></iframe><img src=\"https://example.test/pixel\"><a href=\"javascript:unused()\">label</a>";
    request.body_html = Some(original.into());
    let built = build_mail(&request, &identity(), &[]).unwrap();
    let parsed = MessageParser::default().parse(&built.raw).unwrap();
    let html = parsed.body_html(0).unwrap();
    for excluded in [
        "onclick",
        "position:",
        "background:",
        "DROP_ME",
        "<script",
        "<iframe",
        "https://example.test/pixel",
        "javascript:",
    ] {
        assert!(
            !html.contains(excluded),
            "unexpected active/pasted content: {excluded}"
        );
    }
    assert!(html.contains("font-weight:700"));
    assert!(html.contains("Authored text"));
    assert!(built.html_was_sanitized);
    assert_eq!(request.body_html.as_deref(), Some(original));
}

#[test]
fn injected_header_newlines_are_rejected_before_serialization() {
    let mut cases = Vec::new();
    let mut request = draft();
    request.subject = "subject\r\nX-Unwanted: yes".into();
    cases.push(request);
    let mut request = draft();
    request.to[0].name = Some("name\nX-Unwanted: yes".into());
    cases.push(request);
    let mut request = draft();
    request.to[0].email = "a@example.test\r\nX-Unwanted: yes".into();
    cases.push(request);
    let mut request = draft();
    request.message_id = "<id@example.test>\nX-Unwanted: yes".into();
    cases.push(request);
    let mut request = draft();
    request.references = vec!["<id@example.test>\nX-Unwanted: yes".into()];
    cases.push(request);
    let mut request = draft();
    request.reply_to_message_id = Some("<id@example.test>\nX-Unwanted: yes".into());
    cases.push(request);
    for request in cases {
        assert!(matches!(
            build_mail(&request, &identity(), &[]),
            Err(ComposeError::InvalidHeader(_))
        ));
    }
    let part = staged("file\r\nContent-Type: text/plain", None, b"x");
    assert!(matches!(
        build_mail(&draft(), &identity(), &[part]),
        Err(ComposeError::InvalidHeader("filename"))
    ));
}

#[test]
fn limits_missing_recipients_and_cross_account_attachments_are_explicit_errors() {
    let mut request = draft();
    request.body_text = "x".repeat(MAX_BODY_BYTES + 1);
    assert!(matches!(
        build_mail(&request, &identity(), &[]),
        Err(ComposeError::LimitExceeded("authored body"))
    ));
    let mut request = draft();
    request.to.clear();
    request.cc.clear();
    request.bcc.clear();
    assert!(matches!(
        build_mail(&request, &identity(), &[]),
        Err(ComposeError::NoRecipients)
    ));
    let mut wrong = identity();
    wrong.account_id = "personal".into();
    assert!(matches!(
        build_mail(&draft(), &wrong, &[]),
        Err(ComposeError::WrongAccount)
    ));
    let mut part = staged("x.bin", None, b"x");
    part.account_id = "personal".into();
    assert!(matches!(
        build_mail(&draft(), &identity(), &[part]),
        Err(ComposeError::WrongAccount)
    ));
    let mut part = staged("x.bin", None, b"x");
    part.sha256 = "incorrect".into();
    assert!(matches!(
        build_mail(&draft(), &identity(), &[part]),
        Err(ComposeError::AttachmentNotVerified)
    ));
    let parts = (0..51)
        .map(|_| staged("x.bin", None, b"x"))
        .collect::<Vec<_>>();
    assert!(matches!(
        build_mail(&draft(), &identity(), &parts),
        Err(ComposeError::LimitExceeded("attachments"))
    ));
}

#[test]
fn malformed_html_and_cids_do_not_discard_the_plain_authored_alternative() {
    let mut request = draft();
    request.body_html = Some("<div><strong>unfinished <img src=\"cid:missing\"><p>text".into());
    let built = build_mail(&request, &identity(), &[]).unwrap();
    let parsed = MessageParser::default().parse(&built.raw).unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap(),
        with_signature(&request.body_text, &identity().signature_text)
    );
    assert!(!parsed.body_html(0).unwrap().contains("cid:missing"));
    let parts = [
        staged("one.png", Some("same"), b"x"),
        staged("two.png", Some("same"), b"y"),
    ];
    assert!(matches!(
        build_mail(&request, &identity(), &parts),
        Err(ComposeError::DuplicateContentId)
    ));
}

#[test]
fn oversized_attachment_and_invalid_mime_metadata_fail_before_encoding() {
    let oversized = StagedAttachment {
        account_id: "work".into(),
        filename: "large.bin".into(),
        content_type: "application/octet-stream".into(),
        bytes: vec![0; MAX_ATTACHMENT_BYTES + 1],
        sha256: String::new(),
        content_id: None,
    };
    assert!(matches!(
        build_mail(&draft(), &identity(), &[oversized]),
        Err(ComposeError::LimitExceeded("attachment bytes"))
    ));
    let mut invalid = staged("small.bin", None, b"x");
    invalid.content_type = "not a mime type".into();
    assert!(matches!(
        build_mail(&draft(), &identity(), &[invalid]),
        Err(ComposeError::InvalidContentType)
    ));
    let mut request = draft();
    request.body_html = Some("<p>Inline</p>".into());
    let invalid = staged("logo.png", Some("bad cid"), b"x");
    assert!(matches!(
        build_mail(&request, &identity(), &[invalid]),
        Err(ComposeError::InvalidContentId)
    ));
}

#[test]
fn shared_incoming_corpus_retains_sources_and_attachment_hashes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("mime-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["fixture_revision"], 1);
    for entry in manifest["messages"].as_array().unwrap() {
        let source = std::fs::read(root.join(entry["path"].as_str().unwrap())).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            entry["source_sha256"].as_str().unwrap()
        );
        assert_eq!(source.len() as u64, entry["source_size"].as_u64().unwrap());
        let parsed = MessageParser::default().parse(&source).unwrap();
        let mut inline = 0;
        let mut regular = 0;
        for part in parsed.attachments() {
            if part.content_id().is_some() {
                inline += 1;
            } else {
                regular += 1;
            }
            let expected = entry["attachments"]
                .as_array()
                .unwrap()
                .iter()
                .find(|candidate| candidate["name"].as_str() == part.attachment_name())
                .unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(part.contents())),
                expected["sha256"].as_str().unwrap()
            );
        }
        assert_eq!(inline, entry["inline_count"].as_u64().unwrap());
        assert_eq!(regular, entry["attachment_count"].as_u64().unwrap());
        // This is the exact shared path Core uses for seed and provider imports.
        let text = extract_incoming_text(&source).unwrap();
        for expected in entry["expected_text_contains"].as_array().unwrap() {
            assert!(
                text.contains(expected.as_str().unwrap()),
                "{}: {text:?}",
                entry["path"]
            );
        }
        if let Some(excluded) = entry["expected_text_not_contains"].as_array() {
            for unexpected in excluded {
                assert!(
                    !text.contains(unexpected.as_str().unwrap()),
                    "{}: {text:?}",
                    entry["path"]
                );
            }
        }
    }
}

#[test]
fn shared_mime_bytes_reach_google_capture_once_with_unchanged_authored_text() {
    use fm_google::{send::*, AccountId, GoogleError};
    struct Journal(SendOperation);
    impl SendJournal for Journal {
        fn transition(
            &mut self,
            expected: &SendOperation,
            next: &SendOperation,
        ) -> Result<(), GoogleError> {
            if &self.0 != expected {
                return Err(GoogleError::StaleCheckpoint);
            }
            self.0 = next.clone();
            Ok(())
        }
    }
    #[derive(Default)]
    struct Capture {
        submitted: Vec<Vec<u8>>,
    }
    impl SubmissionTransport for Capture {
        fn send(&mut self, account: &AccountId, mime: &[u8]) -> SubmissionResult {
            assert_eq!(account.as_str(), "work");
            self.submitted.push(mime.to_vec());
            SubmissionResult::Accepted {
                message_id: "fixture-sent".into(),
                thread_id: "fixture-thread".into(),
            }
        }
    }
    let request = draft();
    let built = build_mail(&request, &identity(), &[]).unwrap();
    let mut operation = SendOperation::prepared(
        AccountId::new("work").unwrap(),
        "capture-intent".into(),
        &built.raw,
    )
    .unwrap();
    let mut journal = Journal(operation.clone());
    let mut capture = Capture::default();
    submit_once(&mut operation, &built.raw, &mut capture, &mut journal).unwrap();
    assert_eq!(capture.submitted, vec![built.raw]);
    let parsed = MessageParser::default()
        .parse(&capture.submitted[0])
        .unwrap();
    assert_eq!(
        parsed.body_text(0).unwrap(),
        with_signature(&request.body_text, &identity().signature_text)
    );
    assert!(matches!(operation.state, SendState::Accepted { .. }));
    assert!(submit_once(
        &mut operation,
        &capture.submitted[0].clone(),
        &mut capture,
        &mut journal
    )
    .is_err());
    assert_eq!(capture.submitted.len(), 1);
}
