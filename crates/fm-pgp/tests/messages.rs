//! PGP/MIME signing, encryption and opening, against our own output and
//! messages GnuPG made.
use fm_pgp::{
    encrypt_message, generate, inspect_message, open_message, parse_keys, sign_message,
    signing_key, unlock, Algorithm, Decryption, Keyring, NewKey, ParsedKey, Recipient,
    SignatureStatus,
};
use mail_parser::MimeHeaders;
use pgp::composed::{SignedPublicKey, SignedSecretKey};
use sha2::{Digest, Sha256};

const FIXTURE_NOW: u64 = 1_800_000_000;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn public(name: &str) -> SignedPublicKey {
    match parse_keys(&fixture(name)).unwrap().remove(0) {
        ParsedKey::Public(key) => key,
        _ => panic!("expected a public key"),
    }
}

fn secret(name: &str, passphrase: &str) -> SignedSecretKey {
    match parse_keys(&fixture(name)).unwrap().remove(0) {
        ParsedKey::Secret(key) => unlock(&key, Some(passphrase)).unwrap(),
        _ => panic!("expected a secret key"),
    }
}

fn person(name: &str, email: &str) -> SignedSecretKey {
    generate(&NewKey {
        name: name.into(),
        email: email.into(),
        algorithm: Algorithm::Curve25519,
        expires_in: None,
    })
    .unwrap()
}

/// A message as the composer's MIME builder writes it, with an attachment.
fn composed(from: &str, to: &str, bcc: Option<&str>) -> Vec<u8> {
    let address = |email: &str| fm_mail::Address {
        name: None,
        email: email.into(),
    };
    let bytes = b"Step one.\nStep two.\n".to_vec();
    let attachment = fm_mail::StagedAttachment {
        account_id: "acct".into(),
        filename: "plan.txt".into(),
        content_type: "text/plain".into(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        bytes,
        content_id: None,
    };
    fm_mail::build_mail(
        &fm_mail::ComposeRequest {
            account_id: "acct".into(),
            to: vec![address(to)],
            cc: vec![],
            bcc: bcc.map(|b| vec![address(b)]).unwrap_or_default(),
            subject: "Plan".into(),
            body_text: "Hello, the plan is attached.".into(),
            body_html: Some("<p>Hello, the <b>plan</b> is attached.</p>".into()),
            forwarded_html: None,
            message_id: "<test@example.test>".into(),
            reply_to_message_id: None,
            references: vec![],
            timestamp_secs: 1_790_000_000,
        },
        &fm_mail::AccountIdentity {
            account_id: "acct".into(),
            from: address(from),
            signature_text: String::new(),
            signature_html: None,
        },
        &[attachment],
    )
    .unwrap()
    .raw
}

fn no_secrets() -> fm_pgp::Result<Vec<SignedSecretKey>> {
    panic!("a signed message must not ask for secret keys")
}

fn text_of(message: &[u8]) -> String {
    let parsed = mail_parser::MessageParser::default()
        .parse(message)
        .unwrap();
    parsed.body_text(0).unwrap().into_owned()
}

#[test]
fn signed_messages_verify_and_unwrap() {
    let ann = person("Ann", "ann@example.test");
    let ann_public = ann.to_public_key();
    let raw = composed("ann@example.test", "bob@example.test", None);
    let signed = sign_message(&raw, signing_key(&ann_public, &ann, now()).unwrap()).unwrap();
    let text = String::from_utf8_lossy(&signed);
    assert!(text.contains("multipart/signed; micalg=pgp-sha512"));
    assert!(text.contains("Subject: Plan"));
    let keyring = Keyring {
        certs: vec![ann_public.clone()],
    };
    let opened = open_message(&signed, &keyring, &mut no_secrets, now()).unwrap();
    assert!(!opened.encrypted);
    assert_eq!(opened.signatures.len(), 1);
    let check = &opened.signatures[0];
    assert_eq!(check.status, SignatureStatus::Good);
    assert!(check.sender_matches);
    assert_eq!(check.signer.as_deref(), Some("Ann <ann@example.test>"));
    // The unwrapped message reads like the original.
    let message = opened.message.unwrap();
    assert_eq!(text_of(&message), text_of(&raw));
    let parsed = mail_parser::MessageParser::default()
        .parse(&message[..])
        .unwrap();
    assert_eq!(parsed.attachments().count(), 1);
    assert_eq!(parsed.subject(), Some("Plan"));

    // Unknown to a keyring without the key.
    let empty = Keyring { certs: vec![] };
    let opened = open_message(&signed, &empty, &mut no_secrets, now()).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::UnknownKey);
    assert_eq!(
        opened.signatures[0].key_id.as_deref(),
        Some(
            fm_pgp::inspect(&ann_public, true, now())
                .fingerprint
                .as_str()
        )
    );

    // Any change to the signed content breaks the signature.
    let tampered =
        String::from_utf8(signed.clone())
            .unwrap()
            .replacen("U3RlcCBvbmUu", "U3RlcCB0d28u", 1);
    assert_ne!(tampered.as_bytes(), signed.as_slice());
    let opened = open_message(tampered.as_bytes(), &keyring, &mut no_secrets, now()).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Bad);
}

#[test]
fn a_key_for_another_address_is_flagged() {
    let mallory = person("Mallory", "mallory@example.test");
    let mallory_public = mallory.to_public_key();
    let raw = composed("ann@example.test", "bob@example.test", None);
    let signed =
        sign_message(&raw, signing_key(&mallory_public, &mallory, now()).unwrap()).unwrap();
    let keyring = Keyring {
        certs: vec![mallory_public],
    };
    let opened = open_message(&signed, &keyring, &mut no_secrets, now()).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(!opened.signatures[0].sender_matches);
}

#[test]
fn encrypted_messages_decrypt_for_every_recipient_and_the_sender() {
    let ann = person("Ann", "ann@example.test");
    let bob = person("Bob", "bob@example.test");
    let carol = person("Carol", "carol@example.test");
    let (ann_public, bob_public, carol_public) = (
        ann.to_public_key(),
        bob.to_public_key(),
        carol.to_public_key(),
    );
    let raw = composed(
        "ann@example.test",
        "bob@example.test",
        Some("carol@example.test"),
    );
    let encrypted = encrypt_message(
        &raw,
        &[
            Recipient {
                cert: &bob_public,
                hidden: false,
            },
            Recipient {
                cert: &carol_public,
                hidden: true,
            },
            Recipient {
                cert: &ann_public,
                hidden: false,
            },
        ],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    let text = String::from_utf8_lossy(&encrypted);
    assert!(text.contains("multipart/encrypted"));
    assert!(!text.contains("the plan is attached"));
    assert!(!text.contains("plan.txt"));
    assert!(text.contains("Subject: Plan"));
    let keyring = Keyring {
        certs: vec![ann_public.clone(), bob_public.clone(), carol_public.clone()],
    };
    for reader in [&bob, &carol, &ann] {
        let mut secrets = || Ok(vec![reader.clone()]);
        let opened = open_message(&encrypted, &keyring, &mut secrets, now()).unwrap();
        assert!(opened.encrypted);
        assert!(matches!(opened.decryption, Some(Decryption::Decrypted)));
        assert_eq!(opened.signatures.len(), 1);
        assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
        assert!(opened.signatures[0].sender_matches);
        let message = opened.message.unwrap();
        assert_eq!(text_of(&message), text_of(&raw));
        let parsed = mail_parser::MessageParser::default()
            .parse(&message[..])
            .unwrap();
        let attachment = parsed.attachments().next().unwrap();
        assert_eq!(attachment.attachment_name(), Some("plan.txt"));
        assert_eq!(attachment.contents(), b"Step one.\nStep two.\n");
    }
    // The hidden recipient's key ID is not in the message.
    let stranger = person("Dave", "dave@example.test");
    let mut secrets = || Ok(vec![stranger.clone()]);
    let opened = open_message(&encrypted, &keyring, &mut secrets, now()).unwrap();
    let Some(Decryption::NoKey { recipients }) = opened.decryption else {
        panic!("expected no key");
    };
    let id = |key: &SignedSecretKey| {
        fm_pgp::inspect(&key.to_public_key(), true, now()).subkeys[0].fingerprint[24..].to_string()
    };
    assert!(recipients.contains(&id(&bob)));
    assert!(recipients.contains(&id(&ann)));
    assert!(!recipients.contains(&id(&carol)));
    assert!(opened.message.is_none());
    // A refused keychain is reported, not treated as a missing key.
    let mut refused = || {
        Err(fm_pgp::PgpError {
            code: "store_unavailable",
            message: "The keychain said no".into(),
        })
    };
    let opened = open_message(&encrypted, &keyring, &mut refused, now()).unwrap();
    assert!(matches!(
        opened.decryption,
        Some(Decryption::Unavailable { ref code, .. }) if code == "store_unavailable"
    ));
}

#[test]
fn expired_or_missing_keys_never_fall_back_to_plaintext() {
    let ann = person("Ann", "ann@example.test");
    let bob = generate(&NewKey {
        name: "Bob".into(),
        email: "bob@example.test".into(),
        algorithm: Algorithm::Curve25519,
        expires_in: Some(86_400),
    })
    .unwrap();
    let raw = composed("ann@example.test", "bob@example.test", None);
    let bob_public = bob.to_public_key();
    let error = encrypt_message(
        &raw,
        &[Recipient {
            cert: &bob_public,
            hidden: false,
        }],
        None,
        now() + 2 * 86_400,
    )
    .unwrap_err();
    assert_eq!(error.code, "recipient_key_unusable");
    assert!(encrypt_message(
        &raw,
        &[],
        signing_key(&ann.to_public_key(), &ann, now()),
        now()
    )
    .is_err());
}

#[test]
fn ordinary_mail_is_not_opened() {
    let raw = composed("ann@example.test", "bob@example.test", None);
    assert!(open_message(&raw, &Keyring { certs: vec![] }, &mut no_secrets, now()).is_none());
}

#[test]
fn gnupg_pgp_mime_encrypted_and_signed() {
    let keyring = Keyring {
        certs: vec![public("gus.pub.asc"), public("ann.pub.asc")],
    };
    let ann = secret("ann.sec.asc", "fixture passphrase ann");
    let mut secrets = || Ok(vec![ann.clone()]);
    let opened = open_message(
        &fixture("gus-to-ann-encrypted.eml"),
        &keyring,
        &mut secrets,
        FIXTURE_NOW,
    )
    .unwrap();
    assert!(matches!(opened.decryption, Some(Decryption::Decrypted)));
    assert_eq!(opened.signatures.len(), 1);
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(opened.signatures[0].sender_matches);
    let message = opened.message.unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(&message[..])
        .unwrap();
    assert_eq!(parsed.subject(), Some("Encrypted plan"));
    assert!(parsed
        .body_text(0)
        .unwrap()
        .contains("Hello Ann, the plan is attached."));
    let attachment = parsed.attachments().next().unwrap();
    assert_eq!(attachment.attachment_name(), Some("plan.txt"));
    assert_eq!(attachment.contents(), b"Step one.\nStep two.\n");
}

/// The GnuPG PGP/MIME fixture as Gmail delivers it to another mailbox: a
/// `multipart/mixed` whose text part holds the armored message, with the
/// version part as an attachment.
fn gmail_rewritten() -> Vec<u8> {
    let original = String::from_utf8(fixture("gus-to-ann-encrypted.eml")).unwrap();
    let start = original.find("-----BEGIN PGP MESSAGE-----").unwrap();
    let end =
        original.find("-----END PGP MESSAGE-----").unwrap() + "-----END PGP MESSAGE-----".len();
    let armor = original[start..end]
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    [
        "From: Gus GnuPG <gus@example.test>",
        "To: Ann Example <ann@example.test>",
        "Date: Thu, 22 Oct 2026 12:00:00 +0000",
        "Message-ID: <fixture-encrypted@example.test>",
        "Subject: Encrypted plan",
        "MIME-Version: 1.0",
        "Content-Type: multipart/mixed; boundary=\"gm\"",
        "",
        "--gm",
        "Content-Type: text/plain; charset=\"UTF-8\"",
        "",
        &armor,
        "--gm",
        "Content-Type: application/pgp-encrypted",
        "Content-Disposition: attachment",
        "Content-Transfer-Encoding: base64",
        "",
        "VmVyc2lvbjogMQ0K",
        "--gm--",
        "",
    ]
    .join("\r\n")
    .into_bytes()
}

#[test]
fn pgp_mime_rewritten_by_gmail_reads_as_pgp_mime() {
    let keyring = Keyring {
        certs: vec![public("gus.pub.asc"), public("ann.pub.asc")],
    };
    let ann = secret("ann.sec.asc", "fixture passphrase ann");
    let mut secrets = || Ok(vec![ann.clone()]);
    let opened = open_message(&gmail_rewritten(), &keyring, &mut secrets, FIXTURE_NOW).unwrap();
    assert!(opened.encrypted && !opened.inline);
    assert!(matches!(opened.decryption, Some(Decryption::Decrypted)));
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    // The decrypted entity is read as MIME, not shown as text.
    let message = opened.message.unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(&message[..])
        .unwrap();
    assert_eq!(parsed.subject(), Some("Encrypted plan"));
    let text = parsed.body_text(0).unwrap();
    assert!(text.contains("Hello Ann, the plan is attached."));
    assert!(!text.contains("Content-Type"));
    assert_eq!(
        parsed.attachments().next().unwrap().attachment_name(),
        Some("plan.txt")
    );
}

#[test]
fn encrypted_mail_stays_sealed_until_asked() {
    let keyring = Keyring {
        certs: vec![public("gus.pub.asc"), public("ann.pub.asc")],
    };
    for raw in [fixture("gus-to-ann-encrypted.eml"), gmail_rewritten()] {
        let opened = inspect_message(&raw, &keyring, FIXTURE_NOW).unwrap();
        assert!(opened.encrypted);
        let Some(Decryption::Sealed { recipients }) = opened.decryption else {
            panic!("expected sealed");
        };
        assert!(!recipients.is_empty());
        assert!(opened.message.is_none() && opened.signatures.is_empty());
    }
    // Signed mail is still checked.
    let signed = inspect_message(&fixture("gus-signed.eml"), &keyring, FIXTURE_NOW).unwrap();
    assert_eq!(signed.signatures[0].status, SignatureStatus::Good);
}

#[test]
fn gnupg_pgp_mime_signed() {
    let keyring = Keyring {
        certs: vec![public("gus.pub.asc")],
    };
    let opened = open_message(
        &fixture("gus-signed.eml"),
        &keyring,
        &mut no_secrets,
        FIXTURE_NOW,
    )
    .unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert_eq!(
        opened.signatures[0].signer.as_deref(),
        Some("Gus GnuPG <gus@example.test>")
    );
    let message = opened.message.unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(&message[..])
        .unwrap();
    assert!(parsed
        .body_text(0)
        .unwrap()
        .contains("Signed hello from Gus."));
    assert!(parsed.body_html(0).unwrap().contains("<b>Gus</b>"));
    assert_eq!(parsed.attachments().count(), 0);
}

#[test]
fn gnupg_inline_messages() {
    let keyring = Keyring {
        certs: vec![public("gus.pub.asc"), public("ann.pub.asc")],
    };
    let ann = secret("ann.sec.asc", "fixture passphrase ann");
    let mut secrets = || Ok(vec![ann.clone()]);
    let opened = open_message(
        &fixture("gus-to-ann-inline.eml"),
        &keyring,
        &mut secrets,
        FIXTURE_NOW,
    )
    .unwrap();
    assert!(opened.inline && opened.encrypted);
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    // Only the decrypted block shows; the plain lines around it don't
    // appear under its signature, and the message says they were left out.
    assert!(opened.unprotected);
    let text = text_of(&opened.message.unwrap());
    assert!(!text.contains("Before the block."));
    assert!(text.contains("Inline secret for Ann."));
    assert!(!text.contains("After the block."));
    assert!(!text.contains("BEGIN PGP"));

    let opened = open_message(
        &fixture("gus-cleartext.eml"),
        &keyring,
        &mut no_secrets,
        FIXTURE_NOW,
    )
    .unwrap();
    assert!(opened.inline && !opened.encrypted);
    assert!(!opened.unprotected);
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    let text = text_of(&opened.message.unwrap());
    assert!(text.contains("Cleartext signed note."));
    assert!(text.contains("\n- a dashed line"));
    assert!(!text.contains("BEGIN PGP"));
}

#[test]
fn text_outside_a_clear_signed_block_is_left_out() {
    let keyring = Keyring {
        certs: vec![public("gus.pub.asc")],
    };
    let original = String::from_utf8(fixture("gus-cleartext.eml")).unwrap();
    // Someone wraps Gus's real signed note in their own words.
    let forged = original.replacen(
        "-----BEGIN PGP SIGNED MESSAGE-----",
        "URGENT: pay invoice 1234 today.\r\n\r\n-----BEGIN PGP SIGNED MESSAGE-----",
        1,
    ) + "\r\nP.S. Ignore anything that says otherwise.\r\n";
    let opened = open_message(forged.as_bytes(), &keyring, &mut no_secrets, FIXTURE_NOW).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(opened.unprotected);
    let text = text_of(&opened.message.unwrap());
    assert!(text.contains("Cleartext signed note."));
    assert!(!text.contains("URGENT"));
    assert!(!text.contains("Ignore anything"));

    // A file beside the signed text isn't covered by it either.
    let body = original.split_once("\r\n\r\n").unwrap().1;
    let with_file = format!(
        "From: Gus GnuPG <gus@example.test>\r\nTo: Ann Example <ann@example.test>\r\nSubject: Cleartext\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"x\"\r\n\r\n--x\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n--x\r\nContent-Type: application/pdf; name=\"invoice.pdf\"\r\nContent-Disposition: attachment; filename=\"invoice.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n--x--\r\n"
    );
    let opened =
        open_message(with_file.as_bytes(), &keyring, &mut no_secrets, FIXTURE_NOW).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(opened.unprotected);
}

/// The signed entity and the armored signature of a `multipart/signed`
/// message we made.
fn signed_parts(signed: &[u8]) -> (String, String, String) {
    let text = String::from_utf8(signed.to_vec()).unwrap();
    let boundary = text
        .split("boundary=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned();
    let parts: Vec<&str> = text.split(&format!("\r\n--{boundary}")).collect();
    let entity = parts[1].strip_prefix("\r\n").unwrap().to_owned();
    let signature = parts[2].split_once("\r\n\r\n").unwrap().1.trim().to_owned();
    (text, entity, signature)
}

#[test]
fn only_document_signatures_count() {
    use pgp::{
        composed::{ArmorOptions, DetachedSignature},
        crypto::hash::HashAlgorithm,
        ser::Serialize,
        types::Password,
    };
    // A user ID self-certification is made over the bytes it hashes: the
    // key and the user ID. Presented as a detached signature over exactly
    // those bytes, it must not count as the signer's signature on them.
    // (rpgp also hashes only the first byte of the data for standalone and
    // timestamp signatures, which no API here can make; the same type check
    // refuses those.)
    let (ann, hashed) = (0..32)
        .find_map(|_| {
            let ann = person("Ann", "ann@example.test");
            let key = ann.primary_key.public_key().to_bytes().unwrap();
            let uid = ann.details.users[0].id.id().to_vec();
            let mut hashed = vec![0x99];
            hashed.extend_from_slice(&(key.len() as u16).to_be_bytes());
            hashed.extend_from_slice(&key);
            hashed.push(0xB4);
            hashed.extend_from_slice(&(uid.len() as u32).to_be_bytes());
            hashed.extend_from_slice(&uid);
            // MIME carries the bytes as lines, so line endings can't appear.
            (!hashed.iter().any(|b| *b == b'\n' || *b == b'\r')).then_some((ann, hashed))
        })
        .unwrap();
    let ann_public = ann.to_public_key();
    let certification = ann_public.details.users[0].signatures[0].clone();
    assert!(certification
        .verify_certification(
            &ann_public.primary_key,
            pgp::types::Tag::UserId,
            &ann_public.details.users[0].id
        )
        .is_ok());
    let keyring = Keyring {
        certs: vec![ann_public],
    };
    let open = |sig: DetachedSignature| {
        let armored = sig
            .to_armored_string(ArmorOptions::default())
            .unwrap()
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let mut raw = b"From: ann@example.test\r\nTo: bob@example.test\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/signed; micalg=pgp-sha512; protocol=\"application/pgp-signature\"; boundary=\"b\"\r\n\r\n--b\r\n".to_vec();
        raw.extend_from_slice(&hashed);
        raw.extend_from_slice(
            format!(
                "\r\n--b\r\nContent-Type: application/pgp-signature\r\n\r\n{armored}\r\n--b--\r\n"
            )
            .as_bytes(),
        );
        open_message(&raw, &keyring, &mut no_secrets, now())
            .unwrap()
            .signatures[0]
            .status
    };
    // The same bytes signed as a document verify; the certification doesn't count.
    let document = DetachedSignature::sign_binary_data(
        rand::rngs::OsRng,
        &ann.primary_key,
        &Password::empty(),
        HashAlgorithm::Sha512,
        &hashed[..],
    )
    .unwrap();
    assert_eq!(open(document), SignatureStatus::Good);
    assert_eq!(
        open(DetachedSignature::new(certification)),
        SignatureStatus::Bad
    );
}

#[test]
fn files_inside_signed_content_are_not_plumbing() {
    let raw = b"From: a@example.test\r\nTo: b@example.test\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/signed; micalg=pgp-sha512; protocol=\"application/pgp-signature\"; boundary=\"outer\"\r\n\r\n\
--outer\r\nContent-Type: multipart/mixed; boundary=\"inner\"\r\n\r\n\
--inner\r\nContent-Type: text/plain\r\n\r\nSee the contract.\r\n\
--inner\r\nContent-Type: application/octet-stream; name=\"contract.zip\"\r\nContent-Disposition: attachment; filename=\"contract.zip\"\r\nContent-Transfer-Encoding: base64\r\n\r\nUEsDBA==\r\n\
--inner\r\nContent-Type: application/pgp-signature; name=\"forwarded.asc\"\r\nContent-Disposition: attachment; filename=\"forwarded.asc\"\r\n\r\nnot ours\r\n\
--inner--\r\n\
--outer\r\nContent-Type: application/pgp-signature; name=\"signature.asc\"\r\n\r\n-----BEGIN PGP SIGNATURE-----\r\n\r\nAA==\r\n-----END PGP SIGNATURE-----\r\n\
--outer--\r\n";
    let parsed = mail_parser::MessageParser::default()
        .parse(&raw[..])
        .unwrap();
    let plumbing: Vec<_> = parsed
        .attachments()
        .filter(|part| fm_pgp::is_protection_part(&parsed, part))
        .map(|part| part.attachment_name().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(plumbing, ["signature.asc"]);
}

#[test]
fn protected_headers_show_changes_made_after_sending() {
    let ann = person("Ann", "ann@example.test");
    let bob = person("Bob", "bob@example.test");
    let (ann_public, bob_public) = (ann.to_public_key(), bob.to_public_key());
    let raw = composed(
        "ann@example.test",
        "bob@example.test",
        Some("carol@example.test"),
    );
    let keyring = Keyring {
        certs: vec![ann_public.clone(), bob_public.clone()],
    };

    // Signed: the signed entity carries the fields, but not the Bcc.
    let signed = sign_message(&raw, signing_key(&ann_public, &ann, now()).unwrap()).unwrap();
    let (_, entity, _) = signed_parts(&signed);
    assert!(entity.contains("hp=\"clear\""));
    assert!(entity.contains("Subject: Plan\r\n"));
    assert!(!entity.contains("carol@example.test"));
    assert!(!entity.contains("HP-Outer"));
    let opened = open_message(&signed, &keyring, &mut no_secrets, now()).unwrap();
    let headers = opened.headers.unwrap();
    assert_eq!(headers.subject.as_deref(), Some("Plan"));
    assert!(headers.altered.is_empty());
    // The protected copies don't reappear in the message as read.
    let message = opened.message.unwrap();
    let read = String::from_utf8_lossy(&message);
    assert_eq!(read.matches("Subject: Plan").count(), 1);
    type Secrets<'a> = &'a mut dyn FnMut() -> fm_pgp::Result<Vec<SignedSecretKey>>;
    let altered = |message: &[u8], secrets: Secrets<'_>| {
        let changed = String::from_utf8(message.to_vec())
            .unwrap()
            .replacen("Subject: Plan\r\n", "Subject: New bank details\r\n", 1)
            .replacen(
                "From: ann@example.test\r\n",
                "From: mallory@example.test\r\n",
                1,
            );
        let opened = open_message(changed.as_bytes(), &keyring, secrets, now()).unwrap();
        (opened.signatures[0].status, opened.headers.unwrap())
    };
    let (status, headers) = altered(&signed, &mut no_secrets);
    // The signature still holds: only the outer copy changed.
    assert_eq!(status, SignatureStatus::Good);
    assert_eq!(headers.altered, ["subject", "from"]);
    assert_eq!(headers.subject.as_deref(), Some("Plan"));

    // Encrypted: HP-Outer records what was sent outside.
    let encrypted = encrypt_message(
        &raw,
        &[
            Recipient {
                cert: &bob_public,
                hidden: false,
            },
            Recipient {
                cert: &ann_public,
                hidden: false,
            },
        ],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    let mut secrets = || Ok(vec![bob.clone()]);
    let opened = open_message(&encrypted, &keyring, &mut secrets, now()).unwrap();
    let headers = opened.headers.unwrap();
    assert!(headers.altered.is_empty());
    assert_eq!(headers.subject.as_deref(), Some("Plan"));
    let (status, headers) = altered(&encrypted, &mut secrets);
    assert_eq!(status, SignatureStatus::Good);
    assert_eq!(headers.altered, ["subject", "from"]);

    // GnuPG's messages have no header protection.
    let gus = open_message(
        &fixture("gus-signed.eml"),
        &Keyring {
            certs: vec![public("gus.pub.asc")],
        },
        &mut no_secrets,
        FIXTURE_NOW,
    )
    .unwrap();
    assert!(gus.headers.is_none());
}

/// Our encrypted message as Gmail delivers it to another mailbox: the armor
/// in a text part.
fn rewritten_by_gmail(encrypted: &[u8]) -> Vec<u8> {
    let text = String::from_utf8(encrypted.to_vec()).unwrap();
    let start = text.find("-----BEGIN PGP MESSAGE-----").unwrap();
    let end = text.find("-----END PGP MESSAGE-----").unwrap() + "-----END PGP MESSAGE-----".len();
    let head = text
        .split_once("Content-Type: multipart/encrypted")
        .unwrap()
        .0;
    format!(
        "{head}Content-Type: multipart/mixed; boundary=\"gm\"\r\n\r\n--gm\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\n{}\r\n--gm\r\nContent-Type: application/pgp-encrypted\r\nContent-Disposition: attachment\r\n\r\nVersion: 1\r\n--gm--\r\n",
        &text[start..end]
    )
    .into_bytes()
}

#[test]
fn our_encrypted_mail_rewritten_by_gmail_reads_as_mime() {
    let ann = person("Ann", "ann@example.test");
    let bob = person("Bob", "bob@example.test");
    let (ann_public, bob_public) = (ann.to_public_key(), bob.to_public_key());
    let raw = composed("ann@example.test", "bob@example.test", None);
    let encrypted = encrypt_message(
        &raw,
        &[
            Recipient {
                cert: &bob_public,
                hidden: false,
            },
            Recipient {
                cert: &ann_public,
                hidden: false,
            },
        ],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    let keyring = Keyring {
        certs: vec![ann_public.clone(), bob_public],
    };
    let mut secrets = || Ok(vec![bob.clone()]);
    // The decrypted entity starts with protected header fields, not
    // Content-Type, and is still read as MIME.
    let opened = open_message(
        &rewritten_by_gmail(&encrypted),
        &keyring,
        &mut secrets,
        now(),
    )
    .unwrap();
    assert!(opened.encrypted && !opened.inline && !opened.unprotected);
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(opened.headers.unwrap().altered.is_empty());
    let message = opened.message.unwrap();
    assert_eq!(text_of(&message), text_of(&raw));
    let parsed = mail_parser::MessageParser::default()
        .parse(&message[..])
        .unwrap();
    assert_eq!(
        parsed.attachments().next().unwrap().attachment_name(),
        Some("plan.txt")
    );
}

#[test]
fn signatures_name_their_intended_recipients() {
    use pgp::{
        composed::{ArmorOptions, MessageBuilder, SubpacketConfig},
        crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
        packet::{Subpacket, SubpacketData},
        types::{KeyDetails, Password, Timestamp},
    };
    let ann = person("Ann", "ann@example.test");
    let bob = person("Bob", "bob@example.test");
    let carol = person("Carol", "carol@example.test");
    let (ann_public, bob_public, carol_public) = (
        ann.to_public_key(),
        bob.to_public_key(),
        carol.to_public_key(),
    );
    let keyring = Keyring {
        certs: vec![ann_public.clone(), bob_public.clone(), carol_public.clone()],
    };
    let raw = composed("ann@example.test", "bob@example.test", None);
    let to = |cert| Recipient {
        cert,
        hidden: false,
    };
    let encrypted = encrypt_message(
        &raw,
        &[to(&bob_public), to(&ann_public)],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    for reader in [&bob, &ann] {
        let mut secrets = || Ok(vec![reader.clone()]);
        let opened = open_message(&encrypted, &keyring, &mut secrets, now()).unwrap();
        assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
        assert!(!opened.signatures[0].forwarded);
    }
    // With a Bcc recipient the list is left out (naming them would reveal
    // them), so their copy isn't taken for someone else's either.
    let bcc = encrypt_message(
        &raw,
        &[
            to(&bob_public),
            Recipient {
                cert: &carol_public,
                hidden: true,
            },
            to(&ann_public),
        ],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    for reader in [&bob, &carol] {
        let mut secrets = || Ok(vec![reader.clone()]);
        let opened = open_message(&bcc, &keyring, &mut secrets, now()).unwrap();
        assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
        assert!(!opened.signatures[0].forwarded);
    }

    // Ann's message to Bob, re-encrypted by Bob to Carol: the signature
    // still verifies, but names Bob and Ann, not Carol.
    let entity = text_of(&raw);
    let mut builder = MessageBuilder::from_bytes("", entity.into_bytes())
        .seipd_v1(rand::rngs::OsRng, SymmetricKeyAlgorithm::AES256);
    builder
        .encrypt_to_key(rand::rngs::OsRng, &carol_public.public_subkeys[0].key)
        .unwrap();
    let hashed = vec![
        Subpacket::regular(SubpacketData::IssuerFingerprint(
            ann.primary_key.fingerprint(),
        ))
        .unwrap(),
        Subpacket::regular(SubpacketData::SignatureCreationTime(Timestamp::from_secs(
            now() as u32,
        )))
        .unwrap(),
        Subpacket::regular(SubpacketData::IntendedRecipientFingerprint(
            bob_public.primary_key.fingerprint(),
        ))
        .unwrap(),
        Subpacket::regular(SubpacketData::IntendedRecipientFingerprint(
            ann_public.primary_key.fingerprint(),
        ))
        .unwrap(),
    ];
    builder.sign_with_subpackets(
        &ann.primary_key,
        Password::empty(),
        HashAlgorithm::Sha512,
        SubpacketConfig::UserDefined {
            hashed,
            unhashed: vec![],
        },
    );
    let armored = builder
        .to_armored_string(rand::rngs::OsRng, ArmorOptions::default())
        .unwrap();
    let forwarded = format!(
        "From: Ann <ann@example.test>\r\nTo: carol@example.test\r\nSubject: Plan\r\nMIME-Version: 1.0\r\nContent-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n\r\n--b\r\nContent-Type: application/octet-stream\r\n\r\n{}\r\n--b--\r\n",
        armored.replace("\r\n", "\n").replace('\n', "\r\n")
    );
    let mut secrets = || Ok(vec![carol.clone()]);
    let opened = open_message(forwarded.as_bytes(), &keyring, &mut secrets, now()).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(opened.signatures[0].forwarded);

    // With a Bcc recipient the list is left out, so the Bcc reader isn't
    // warned about their own copy.
    let encrypted = encrypt_message(
        &raw,
        &[
            to(&bob_public),
            Recipient {
                cert: &carol_public,
                hidden: true,
            },
            to(&ann_public),
        ],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    let mut secrets = || Ok(vec![carol.clone()]);
    let opened = open_message(&encrypted, &keyring, &mut secrets, now()).unwrap();
    assert_eq!(opened.signatures[0].status, SignatureStatus::Good);
    assert!(!opened.signatures[0].forwarded);
}

#[test]
fn gnupg_rsa_recipient() {
    let rita = secret("rita.sec.asc", "fixture passphrase rita");
    let mut secrets = || Ok(vec![rita.clone()]);
    let opened = open_message(
        &fixture("gus-to-rita-encrypted.eml"),
        &Keyring { certs: vec![] },
        &mut secrets,
        FIXTURE_NOW,
    )
    .unwrap();
    assert!(matches!(opened.decryption, Some(Decryption::Decrypted)));
    assert!(opened.signatures.is_empty());
    assert!(text_of(&opened.message.unwrap()).contains("For Rita only."));
}

/// GnuPG verifies our signatures and decrypts our messages, when it is
/// installed. Run with `cargo test -p fm-pgp -- --ignored`.
#[test]
#[ignore]
fn gnupg_reads_our_messages() {
    use std::io::Write;
    let home =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fm-pgp-gnupg-mail");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let gpg = |args: &[&str], input: &[u8]| {
        let mut child = std::process::Command::new("gpg")
            .env("GNUPGHOME", &home)
            .args([
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--trust-model",
                "always",
            ])
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("gpg is installed");
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    };
    let ann = person("Ann", "ann@example.test");
    let ann_public = ann.to_public_key();
    // GnuPG holds Bob's secret key, exported by us.
    let bob = person("Bob", "bob@example.test");
    let bob_public = bob.to_public_key();
    let out = gpg(
        &["--passphrase", "bob's interop passphrase", "--import"],
        fm_pgp::export_secret(&bob, "bob's interop passphrase")
            .unwrap()
            .as_bytes(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = gpg(
        &["--import"],
        fm_pgp::export_public(&ann_public).unwrap().as_bytes(),
    );
    assert!(out.status.success());

    let raw = composed("ann@example.test", "bob@example.test", None);
    let signed = sign_message(&raw, signing_key(&ann_public, &ann, now()).unwrap()).unwrap();
    let text = String::from_utf8(signed).unwrap();
    let boundary = text
        .split("boundary=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let parts: Vec<&str> = text.split(&format!("\r\n--{boundary}")).collect();
    let entity = parts[1].strip_prefix("\r\n").unwrap();
    let signature = parts[2].split_once("\r\n\r\n").unwrap().1;
    std::fs::write(home.join("entity"), entity).unwrap();
    std::fs::write(home.join("entity.asc"), signature).unwrap();
    let verify = gpg(
        &[
            "--verify",
            home.join("entity.asc").to_str().unwrap(),
            home.join("entity").to_str().unwrap(),
        ],
        b"",
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(
        stderr.contains("Good signature from \"Ann <ann@example.test>\""),
        "{stderr}"
    );

    let encrypted = encrypt_message(
        &raw,
        &[Recipient {
            cert: &bob_public,
            hidden: false,
        }],
        signing_key(&ann_public, &ann, now()),
        now(),
    )
    .unwrap();
    let text = String::from_utf8(encrypted).unwrap();
    let start = text.find("-----BEGIN PGP MESSAGE-----").unwrap();
    let end = text.find("-----END PGP MESSAGE-----").unwrap() + 25;
    let out = gpg(
        &["--passphrase", "bob's interop passphrase", "--decrypt"],
        &text.as_bytes()[start..end],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("Good signature from \"Ann <ann@example.test>\""),
        "{stderr}"
    );
    let plain = String::from_utf8_lossy(&out.stdout);
    assert!(
        plain.starts_with("Content-Type: multipart/mixed"),
        "{plain}"
    );
    assert!(plain.contains("plan.txt"));
    let _ = std::process::Command::new("gpgconf")
        .env("GNUPGHOME", &home)
        .args(["--kill", "gpg-agent"])
        .status();
}

#[test]
fn structure_alone_tells_signed_from_encrypted() {
    let parse = |name: &str| fixture(name);
    let kinds: Vec<_> = [
        "gus-to-ann-encrypted.eml",
        "gus-signed.eml",
        "gus-to-ann-inline.eml",
        "gus-cleartext.eml",
    ]
    .iter()
    .map(|name| {
        let raw = parse(name);
        let parsed = mail_parser::MessageParser::default()
            .parse(&raw[..])
            .unwrap();
        let plumbing = parsed
            .attachments()
            .filter(|part| fm_pgp::is_protection_part(&parsed, part))
            .count();
        (fm_pgp::protection(&parsed), plumbing)
    })
    .collect();
    use fm_pgp::Protection;
    assert_eq!(
        kinds,
        [
            (Protection::Encrypted, 2),
            (Protection::Signed, 1),
            (Protection::Encrypted, 0),
            (Protection::Signed, 0)
        ]
    );
    let raw = composed("ann@example.test", "bob@example.test", None);
    let parsed = mail_parser::MessageParser::default()
        .parse(&raw[..])
        .unwrap();
    assert_eq!(fm_pgp::protection(&parsed), Protection::None);
    assert!(!parsed
        .attachments()
        .any(|part| fm_pgp::is_protection_part(&parsed, part)));
}

#[test]
fn readable_text_drops_armor() {
    let raw = fixture("gus-to-ann-inline.eml");
    let parsed = mail_parser::MessageParser::default()
        .parse(&raw[..])
        .unwrap();
    let text = fm_pgp::readable_text(&parsed.body_text(0).unwrap());
    assert_eq!(text, "Before the block.\n\nAfter the block.");
    let raw = fixture("gus-cleartext.eml");
    let parsed = mail_parser::MessageParser::default()
        .parse(&raw[..])
        .unwrap();
    let text = fm_pgp::readable_text(&parsed.body_text(0).unwrap());
    assert_eq!(text, "Cleartext signed note.\n- a dashed line");
}
