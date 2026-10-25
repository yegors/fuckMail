//! The keyring's requests, sending signed and encrypted mail, and reading
//! protected mail, on a live profile with a synthetic Google account.
use crate::google_send::{
    load_google_send_upload, prepare_google_send, GoogleSendScope, GoogleSendStep,
};
use crate::{pgp::MemoryKek, Store};
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Arc;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../fm-pgp/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

/// A live store whose account is ann@example.test, with an in-memory key.
fn setup(path: &std::path::Path) -> (Store, GoogleSendScope) {
    let mut store = Store::open_live(path).unwrap();
    store.set_pgp_kek(Arc::new(MemoryKek::new()));
    let account = store
        .bind_google_account(
            "pgp-owner",
            "ann@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    let scope = GoogleSendScope {
        account,
        connection_generation: store.google_account_bindings().unwrap()[0].connection_generation,
        primary_email: "ann@example.test".into(),
    };
    (store, scope)
}

fn call(store: &mut Store, method: &str, account: Option<&str>, params: Value) -> Value {
    store.handle(method, account, params).unwrap()
}

fn failure(
    store: &mut Store,
    method: &str,
    account: Option<&str>,
    params: Value,
) -> crate::CoreError {
    store.handle(method, account, params).unwrap_err()
}

fn generate(store: &mut Store, account: &str) -> String {
    call(
        store,
        "pgp_key_generate",
        None,
        json!({"name":"Ann Example","email":"ann@example.test","algorithm":"curve25519","expires_days":1095,"account":account,"request_id":"generate"}),
    )["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn import(store: &mut Store, armored: &str, request: &str) -> Value {
    call(
        store,
        "pgp_key_import",
        None,
        json!({"text":armored,"request_id":request}),
    )
}

#[test]
fn generated_keys_belong_to_their_account_and_replay() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let fingerprint = generate(&mut store, &account);
    // The same request ID returns the same key; different parameters conflict.
    assert_eq!(generate(&mut store, &account), fingerprint);
    let conflict = failure(
        &mut store,
        "pgp_key_generate",
        None,
        json!({"email":"other@example.test","request_id":"generate"}),
    );
    assert_eq!(conflict.code, "request_id_conflict");
    let keys = call(&mut store, "pgp_keys", None, json!({}));
    assert_eq!(keys["keys"].as_array().unwrap().len(), 1);
    let key = &keys["keys"][0];
    assert_eq!(key["fingerprint"], fingerprint);
    assert_eq!(key["secret"], true);
    assert_eq!(key["origin"], "generated");
    assert_eq!(key["status"], "valid");
    assert_eq!(key["accounts"], json!([account]));
    assert_eq!(keys["accounts"][&account]["sign_by_default"], true);
    assert_eq!(keys["accounts"][&account]["encrypt_by_default"], false);
    // Account rows show which key the account uses.
    let accounts = call(&mut store, "accounts", None, json!({}));
    assert_eq!(accounts["accounts"][0]["pgp"]["fingerprint"], fingerprint);
    // The secret is stored sealed: no OpenPGP secret key packet is readable.
    let sealed: Vec<u8> = store
        .db
        .query_row("SELECT secret_key FROM pgp_keys", [], |r| r.get(0))
        .unwrap();
    assert!(sealed.starts_with(b"FMPGP1"));
    // Without the keychain, the key can still be listed but not used.
    store.set_pgp_kek(Arc::new(crate::pgp::NoKeychain));
    assert_eq!(
        call(&mut store, "pgp_keys", None, json!({}))["keys"][0]["fingerprint"],
        fingerprint
    );
    let locked = failure(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"revocation"}),
    );
    assert_eq!(locked.code, "store_unavailable");
}

#[test]
fn imports_merge_unlock_secret_keys_and_apply_revocations() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, _) = setup(temp.path());
    let added = import(&mut store, &text("gus.pub.asc"), "gus");
    assert_eq!(added["added"].as_array().unwrap().len(), 1);
    let gus = added["added"][0].as_str().unwrap().to_owned();
    // The same key again only updates.
    let again = import(&mut store, &text("gus.pub.asc"), "gus-again");
    assert_eq!(again["updated"], json!([gus]));
    // A protected secret key asks for its passphrase and says which key it is.
    let asked = failure(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("rita.sec.asc"),"request_id":"rita"}),
    );
    assert_eq!(asked.code, "passphrase_required");
    assert_eq!(
        asked.details["user_ids"],
        json!(["Rita RSA <rita@example.test>"])
    );
    let wrong = failure(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("rita.sec.asc"),"passphrase":"not it at all","request_id":"rita-wrong"}),
    );
    assert_eq!(wrong.code, "wrong_passphrase");
    let rita = call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("rita.sec.asc"),"passphrase":"fixture passphrase rita","request_id":"rita-right"}),
    );
    let rita = rita["added"][0].as_str().unwrap().to_owned();
    let keys = call(&mut store, "pgp_keys", None, json!({}));
    let rows = keys["keys"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|k| k["fingerprint"] == gus && k["secret"] == false));
    assert!(rows
        .iter()
        .any(|k| k["fingerprint"] == rita && k["secret"] == true));
    // Importing only the public key later keeps the secret.
    import(&mut store, &text("rita.pub.asc"), "rita-public");
    let keys = call(&mut store, "pgp_keys", None, json!({}));
    assert!(keys["keys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["fingerprint"] == rita && k["secret"] == true));
    // A revocation certificate from Rita's own export revokes her key here.
    let revocation = call(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":rita,"kind":"revocation"}),
    )["armored"]
        .as_str()
        .unwrap()
        .to_owned();
    let revoked = import(&mut store, &revocation, "revoke");
    assert_eq!(revoked["revoked"], json!([rita]));
    let keys = call(&mut store, "pgp_keys", None, json!({}));
    let rita_row = keys["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["fingerprint"] == rita)
        .unwrap()
        .clone();
    assert_eq!(rita_row["status"], "revoked");
    assert_eq!(
        failure(
            &mut store,
            "pgp_key_import",
            None,
            json!({"text":"nothing","request_id":"junk"})
        )
        .code,
        "invalid_key"
    );
}

#[test]
fn exports_round_trip_and_secret_exports_need_a_passphrase() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let fingerprint = generate(&mut store, &scope.account);
    let public = call(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"public"}),
    );
    assert!(public["armored"]
        .as_str()
        .unwrap()
        .starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    assert!(public["filename"]
        .as_str()
        .unwrap()
        .ends_with("-public.asc"));
    let weak = failure(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"secret","passphrase":"short"}),
    );
    assert_eq!(weak.code, "weak_passphrase");
    let secret = call(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"secret","passphrase":"a long backup passphrase"}),
    );
    let armored = secret["armored"].as_str().unwrap();
    assert!(armored.starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
    // The export is protected and opens with its passphrase.
    let parsed = match fm_pgp::parse_keys(armored.as_bytes()).unwrap().remove(0) {
        fm_pgp::ParsedKey::Secret(key) => key,
        _ => panic!("expected a secret key"),
    };
    assert!(fm_pgp::is_protected(&parsed));
    assert!(fm_pgp::unlock(&parsed, Some("a long backup passphrase")).is_ok());
    // Someone else's key has no secret to export.
    let gus = import(&mut store, &text("gus.pub.asc"), "gus")["added"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let none = failure(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":gus,"kind":"secret","passphrase":"a long backup passphrase"}),
    );
    assert_eq!(none.code, "no_secret_key");
}

#[test]
fn expiry_renews_and_delete_clears_the_account() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let fingerprint = generate(&mut store, &account);
    let renewed = call(
        &mut store,
        "pgp_key_expiry",
        None,
        json!({"fingerprint":fingerprint,"expires_days":30,"request_id":"renew"}),
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        renewed["expires_at"]
            .as_u64()
            .unwrap()
            .abs_diff(now + 30 * 86_400)
            < 60
    );
    let forever = call(
        &mut store,
        "pgp_key_expiry",
        None,
        json!({"fingerprint":fingerprint,"expires_days":null,"request_id":"forever"}),
    );
    assert_eq!(forever["expires_at"], Value::Null);
    let deleted = call(
        &mut store,
        "pgp_key_delete",
        None,
        json!({"fingerprint":fingerprint,"request_id":"delete"}),
    );
    assert_eq!(deleted["used_by"], json!([account]));
    assert_eq!(
        call(&mut store, "pgp_keys", None, json!({}))["keys"],
        json!([])
    );
    assert_eq!(
        call(&mut store, "accounts", None, json!({}))["accounts"][0]["pgp"],
        Value::Null
    );
}

#[test]
fn an_account_uses_only_its_owners_key_for_its_address() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let gus = import(&mut store, &text("gus.pub.asc"), "gus")["added"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let public_only = failure(
        &mut store,
        "pgp_account",
        Some(&account),
        json!({"fingerprint":gus,"sign_by_default":true,"encrypt_by_default":false,"request_id":"use-gus"}),
    );
    assert_eq!(public_only.code, "pgp_key_unusable");
    let other = call(
        &mut store,
        "pgp_key_generate",
        None,
        json!({"email":"someone@example.test","request_id":"other"}),
    )["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let wrong_address = failure(
        &mut store,
        "pgp_account",
        Some(&account),
        json!({"fingerprint":other,"sign_by_default":true,"encrypt_by_default":false,"request_id":"use-other"}),
    );
    assert_eq!(wrong_address.code, "pgp_key_unusable");
    let own = call(
        &mut store,
        "pgp_key_generate",
        None,
        json!({"email":"ann@example.test","request_id":"own"}),
    )["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let chosen = call(
        &mut store,
        "pgp_account",
        Some(&account),
        json!({"fingerprint":own,"sign_by_default":false,"encrypt_by_default":true,"request_id":"use-own"}),
    );
    assert_eq!(chosen["pgp"]["encrypt_by_default"], true);
    let cleared = call(
        &mut store,
        "pgp_account",
        Some(&account),
        json!({"fingerprint":null,"sign_by_default":true,"encrypt_by_default":false,"request_id":"off"}),
    );
    assert_eq!(cleared["pgp"], Value::Null);
    assert_eq!(
        failure(
            &mut store,
            "pgp_account",
            None,
            json!({"fingerprint":null,"request_id":"x"})
        )
        .code,
        "account_required"
    );
    // Recipients with keys are ready to encrypt to; others are missing.
    let recipients = call(
        &mut store,
        "pgp_recipients",
        Some(&account),
        json!({"addresses":["Gus@Example.test","nobody@example.test"]}),
    );
    assert_eq!(recipients["recipients"][0]["status"], "ready");
    assert_eq!(recipients["recipients"][0]["fingerprint"], gus);
    assert_eq!(recipients["recipients"][1]["status"], "missing");
}

fn draft(store: &mut Store, scope: &GoogleSendScope, to: &str, flags: Value) -> String {
    let created = call(
        store,
        "draft_create",
        Some(&scope.account),
        json!({"mode":"new","request_id":format!("create-{to}")}),
    );
    let id = created["draft_id"].as_str().unwrap().to_owned();
    let mut params = json!({"draft_id":id,"expected_revision":1,"request_id":format!("save-{to}"),
        "body":"The secret plan.","subject":"Plan","to_recipients":[{"email":to}],
        "bcc_recipients":[{"email":"hidden@example.test"}]});
    for (key, value) in flags.as_object().unwrap() {
        params[key] = value.clone();
    }
    call(store, "draft_save", Some(&scope.account), params);
    id
}

fn send_request(store: &Store, scope: &GoogleSendScope, id: &str) -> Value {
    let twin = store.google_draft_status(&scope.account, id).unwrap();
    json!({"draft_id":id,"expected_revision":twin["local_revision"],"expected_google_revision":twin["revision"],"request_id":format!("send-{id}")})
}

/// The frozen bytes a send would submit.
fn frozen(store: &mut Store, scope: &GoogleSendScope, id: &str) -> Vec<u8> {
    let p = send_request(store, scope, id);
    let plan = match store.google_send_begin(scope, &p).unwrap() {
        GoogleSendStep::Prepare(plan) => plan,
        _ => panic!("expected a plan"),
    };
    let proof = prepare_google_send(plan, &CancellationToken::new()).unwrap();
    let status = store.google_send_finish(proof).unwrap();
    let lease = store
        .google_send_claim(
            scope,
            status["request_id"].as_str().unwrap(),
            status["revision"].as_i64().unwrap(),
        )
        .unwrap();
    load_google_send_upload(lease.take_upload(), &CancellationToken::new()).unwrap()
}

#[test]
fn drafts_default_to_signing_and_send_signed() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    generate(&mut store, &scope.account);
    let id = draft(&mut store, &scope, "gus@example.test", json!({}));
    let saved = call(
        &mut store,
        "draft_get",
        Some(&scope.account),
        json!({"draft_id":id}),
    );
    assert_eq!(saved["pgp_sign"], true);
    assert!(saved["pgp_encrypt"].is_null());
    let raw = frozen(&mut store, &scope, &id);
    let text = String::from_utf8_lossy(&raw);
    assert!(text.contains("multipart/signed"));
    assert!(text.contains("Subject: Plan"));
    // It verifies against the sender's key, as a recipient would check it.
    let keyring = store.pgp_keyring().unwrap();
    let opened = keyring.open(&raw, &crate::pgp::NoKeychain, true).unwrap();
    assert_eq!(opened.signatures[0].status, fm_pgp::SignatureStatus::Good);
    assert!(opened.signatures[0].sender_matches);
    // The journal still previews a signed message.
    let preview: String = store
        .db
        .query_row(
            "SELECT json_extract(json,'$.headers.preview') FROM capture_operations WHERE kind='send'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    assert!(preview.is_empty() || preview.contains("secret plan"));
}

#[test]
fn encrypted_sends_need_every_recipients_key_and_stay_out_of_gmail_drafts() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    generate(&mut store, &scope.account);
    let id = draft(
        &mut store,
        &scope,
        "gus@example.test",
        json!({"pgp_encrypt":true}),
    );
    let plain = draft(&mut store, &scope, "plain@example.test", json!({}));
    // An encrypted draft is never due for upload, and an explicit sync refuses.
    let due = store
        .google_draft_due_after(&scope.account, scope.connection_generation, 10, 0)
        .unwrap();
    assert!(due.iter().any(|d| d.draft_id == plain));
    assert!(!due.iter().any(|d| d.draft_id == id));
    let twin = store.google_draft_status(&scope.account, &id).unwrap();
    let refused = store
        .google_draft_begin(
            &scope,
            &json!({"draft_id":id,"expected_revision":twin["local_revision"],"request_id":"upload"}),
        )
        .err()
        .unwrap();
    assert_eq!(refused.code, "pgp_draft_local");
    // Without the recipients' keys the send stops before anything is frozen.
    let p = send_request(&store, &scope, &id);
    let missing = store.google_send_begin(&scope, &p).err().unwrap();
    assert_eq!(missing.code, "pgp_recipient_key_missing");
    assert_eq!(
        missing.details["addresses"],
        json!(["gus@example.test", "hidden@example.test"])
    );
    import(&mut store, &text("gus.pub.asc"), "gus");
    call(
        &mut store,
        "pgp_key_generate",
        None,
        json!({"email":"hidden@example.test","request_id":"hidden"}),
    );
    let raw = frozen(&mut store, &scope, &id);
    let text = String::from_utf8_lossy(&raw);
    assert!(text.contains("multipart/encrypted"));
    assert!(!text.contains("secret plan"));
    assert!(text.contains("Subject: Plan"));
    // Gus reads it with his own key; the sender reads it with theirs.
    let gus = match fm_pgp::parse_keys(&fixture("gus.sec.asc"))
        .unwrap()
        .remove(0)
    {
        fm_pgp::ParsedKey::Secret(key) => {
            fm_pgp::unlock(&key, Some("fixture passphrase gus")).unwrap()
        }
        _ => panic!(),
    };
    let keyring = fm_pgp::Keyring {
        certs: store.pgp_keyring().unwrap().certs(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let opened = fm_pgp::open_message(&raw, &keyring, &mut || Ok(vec![gus.clone()]), now).unwrap();
    assert!(matches!(
        opened.decryption,
        Some(fm_pgp::Decryption::Decrypted)
    ));
    assert_eq!(opened.signatures[0].status, fm_pgp::SignatureStatus::Good);
    let own = store.pgp_read_keys().unwrap().readable(raw.clone());
    assert!(own.decrypted());
    assert!(String::from_utf8_lossy(&own.bytes).contains("Content-Type: multipart/mixed"));
    // The journal keeps no readable preview of an encrypted message.
    let preview: String = store
        .db
        .query_row(
            "SELECT COALESCE(json_extract(json,'$.headers.preview'),'') FROM capture_operations WHERE kind='send'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    assert_eq!(preview, "");
}

#[test]
fn sending_without_a_key_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let id = draft(
        &mut store,
        &scope,
        "gus@example.test",
        json!({"pgp_sign":true}),
    );
    let p = send_request(&store, &scope, &id);
    assert_eq!(
        store.google_send_begin(&scope, &p).err().unwrap().code,
        "pgp_not_set_up"
    );
    // Turning the flag off sends as before.
    let saved = call(
        &mut store,
        "draft_get",
        Some(&scope.account),
        json!({"draft_id":id}),
    );
    call(
        &mut store,
        "draft_save",
        Some(&scope.account),
        json!({"draft_id":id,"expected_revision":saved["revision"],"request_id":"off","body":"The secret plan.","pgp_sign":false}),
    );
    let raw = frozen(&mut store, &scope, &id);
    assert!(!String::from_utf8_lossy(&raw).contains("multipart/signed"));
}

/// Publishes `source` as a received message and returns its id.
fn receive(store: &mut Store, account: &str, source: &[u8], remote: &str) -> String {
    let hash = store.stage_blob(source).unwrap();
    let staged = crate::google_store::prepare_message(&fm_google::sync::MessageContent {
        id: remote.into(),
        thread_id: format!("thread-{remote}"),
        label_ids: vec!["INBOX".into(), "UNREAD".into()],
        history_id: "1".into(),
        source_mime: source.to_vec(),
    })
    .unwrap();
    crate::google_store::publish_message(&store.db, account, &staged, &hash).unwrap();
    store
        .db
        .query_row(
            "SELECT id FROM messages WHERE account=?1 AND remote_id=?2",
            [account, remote],
            |r| r.get(0),
        )
        .unwrap()
}

fn source(store: &Store, account: &str, message: &str) -> Value {
    read_source(store, account, message, true)
}

/// A read as the reader makes it before the person chooses Decrypt.
fn sealed_source(store: &Store, account: &str, message: &str) -> Value {
    read_source(store, account, message, false)
}

fn read_source(store: &Store, account: &str, message: &str, decrypt: bool) -> Value {
    let version: String = store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
            [account, message],
            |r| r.get(0),
        )
        .unwrap();
    let plan = store
        .message_source_begin(
            account,
            &json!({"id":message,"source_version":version,"decrypt":decrypt}),
        )
        .unwrap();
    crate::message_source::read(plan, &CancellationToken::new()).unwrap()
}

#[test]
fn encrypted_mail_is_marked_at_sync_and_decrypted_only_to_read() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let message = receive(
        &mut store,
        &account,
        &fixture("gus-to-ann-encrypted.eml"),
        "enc",
    );
    // Stored: marked encrypted, no readable text, no PGP/MIME plumbing listed.
    let (pgp, body): (i64, String) = store
        .db
        .query_row(
            "SELECT pgp,body FROM messages WHERE id=?1",
            [&message],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pgp, 2);
    assert!(!body.contains("plan is attached"));
    let attachments: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM attachments WHERE message_id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(attachments, 0);
    let conversation: String = store
        .db
        .query_row(
            "SELECT conversation_id FROM messages WHERE id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    let thread = store.conversation(&account, &conversation).unwrap();
    assert_eq!(thread["messages"][0]["pgp"], "encrypted");
    // Without Ann's key it reads as undecryptable, and sealed it says so.
    let locked = source(&store, &account, &message);
    assert_eq!(locked["pgp"]["decryption"]["state"], "no_key");
    let sealed = sealed_source(&store, &account, &message);
    assert_eq!(sealed["pgp"]["decryption"]["state"], "sealed");
    assert_eq!(sealed["pgp"]["decryption"]["can_decrypt"], false);
    // With it, the reader sees the text, Gus's signature and the attachment.
    call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("ann.sec.asc"),"passphrase":"fixture passphrase ann","request_id":"ann"}),
    );
    import(&mut store, &text("gus.pub.asc"), "gus");
    // Until the reader asks, it stays sealed: no text, no files, no signature.
    let sealed = sealed_source(&store, &account, &message);
    assert_eq!(sealed["pgp"]["decryption"]["state"], "sealed");
    assert_eq!(sealed["pgp"]["decryption"]["can_decrypt"], true);
    assert!(!sealed["body_text"]
        .as_str()
        .unwrap_or_default()
        .contains("plan is attached"));
    assert!(sealed["pgp"]["attachments"].is_null());
    assert_eq!(sealed["pgp"]["signatures"], json!([]));
    let read = source(&store, &account, &message);
    assert_eq!(read["pgp"]["decryption"]["state"], "decrypted");
    assert_eq!(read["pgp"]["signatures"][0]["status"], "good");
    assert_eq!(read["pgp"]["signatures"][0]["sender_matches"], true);
    assert!(read["body_text"]
        .as_str()
        .unwrap()
        .contains("Hello Ann, the plan is attached."));
    // The raw source stays as received.
    assert!(read["raw"].as_str().unwrap().contains("BEGIN PGP MESSAGE"));
    let files = read["pgp"]["attachments"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "plan.txt");
    let id = files[0]["id"].as_str().unwrap();
    let exported = store
        .attachment_begin(&account, &json!({"id":id}))
        .unwrap()
        .export()
        .unwrap();
    assert_eq!(exported["name"], "plan.txt");
    let path = exported["path"].as_str().unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"Step one.\nStep two.\n");
    // The decrypted copy is kept apart from ordinary exports, and goes when
    // the engine starts or stops.
    let decrypted = std::fs::canonicalize(temp.path().join("decrypted")).unwrap();
    assert!(std::path::Path::new(path).starts_with(&decrypted));
    assert!(!temp.path().join("exports").exists());
    crate::pgp::forget_decrypted(temp.path()).unwrap();
    assert!(!std::path::Path::new(path).exists() && !decrypted.exists());
    crate::pgp::forget_decrypted(temp.path()).unwrap();
    // A tampered id is refused.
    let forged = id.replace(&id[id.len() - 4..], "0000");
    assert!(store
        .attachment_begin(&account, &json!({"id":forged}))
        .unwrap()
        .export()
        .is_err());
}

#[test]
fn signed_mail_keeps_its_text_and_hides_its_signature() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let message = receive(&mut store, &account, &fixture("gus-signed.eml"), "signed");
    let (pgp, body): (i64, String) = store
        .db
        .query_row(
            "SELECT pgp,body FROM messages WHERE id=?1",
            [&message],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pgp, 1);
    assert!(body.contains("Signed hello from Gus."));
    let attachments: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM attachments WHERE message_id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(attachments, 0);
    let unknown = source(&store, &account, &message);
    assert_eq!(unknown["pgp"]["signatures"][0]["status"], "unknown_key");
    import(&mut store, &text("gus.pub.asc"), "gus");
    let known = source(&store, &account, &message);
    assert_eq!(known["pgp"]["signatures"][0]["status"], "good");
    // A read that leaves encrypted mail sealed still checks signatures.
    let sealed = sealed_source(&store, &account, &message);
    assert_eq!(sealed["pgp"]["signatures"][0]["status"], "good");
    assert!(sealed["pgp"]["decryption"].is_null());
    assert!(known["body_html"].as_str().unwrap().contains("<b>Gus</b>"));
    // Inline armor never reaches search text.
    let inline = receive(
        &mut store,
        &account,
        &fixture("gus-to-ann-inline.eml"),
        "inline",
    );
    let body: String = store
        .db
        .query_row("SELECT body FROM messages WHERE id=?1", [&inline], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(!body.contains("BEGIN PGP"));
    assert!(body.contains("Before the block."));
}

#[test]
fn mail_synced_before_openpgp_support_is_repaired() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let message = receive(
        &mut store,
        &account,
        &fixture("gus-to-ann-encrypted.eml"),
        "old",
    );
    // As an older build stored it: unmarked, with its plumbing as attachments.
    store
        .db
        .execute("UPDATE messages SET pgp=0 WHERE id=?1", [&message])
        .unwrap();
    store
        .db
        .execute(
            "INSERT INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,'plumbing',?2,'encrypted.asc','application/pgp-encrypted',10,'available',?3)",
            rusqlite::params![account, message, format!("{:x}", sha2::Digest::finalize(<sha2::Sha256 as sha2::Digest>::new()))],
        )
        .unwrap();
    while store.repair_pgp_messages(10).unwrap() {}
    let pgp: i64 = store
        .db
        .query_row("SELECT pgp FROM messages WHERE id=?1", [&message], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(pgp, 2);
    assert!(!store.repair_pgp_messages(10).unwrap());
}

#[test]
fn encrypt_by_default_waits_for_every_recipient_key() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let own = generate(&mut store, &account);
    call(
        &mut store,
        "pgp_account",
        Some(&account),
        json!({"fingerprint":own,"sign_by_default":false,"encrypt_by_default":true,"request_id":"encrypt"}),
    );
    // A new message has no recipients yet, so nothing to encrypt to.
    let created = call(
        &mut store,
        "draft_create",
        Some(&account),
        json!({"mode":"new","request_id":"new"}),
    );
    assert!(created["pgp_sign"].is_null());
    assert!(created["pgp_encrypt"].is_null());
    // Replying to someone without a key starts unencrypted; with their key,
    // encrypted.
    let message = receive(&mut store, &account, &fixture("gus-signed.eml"), "signed");
    let conversation: String = store
        .db
        .query_row(
            "SELECT conversation_id FROM messages WHERE id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    let reply = |store: &mut Store| {
        call(
            store,
            "draft_get",
            Some(&account),
            json!({"conversation_id":conversation}),
        )
    };
    assert!(reply(&mut store)["pgp_encrypt"].is_null());
    import(&mut store, &text("gus.pub.asc"), "gus");
    assert_eq!(reply(&mut store)["pgp_encrypt"], true);
}

#[test]
fn replies_to_encrypted_mail_start_encrypted() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    generate(&mut store, &account);
    let message = receive(
        &mut store,
        &account,
        &fixture("gus-to-ann-encrypted.eml"),
        "enc",
    );
    let conversation: String = store
        .db
        .query_row(
            "SELECT conversation_id FROM messages WHERE id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    // Even without Gus's key it starts encrypted: sending waits for his key
    // (or for the person to turn Encrypt off), and the draft never goes to
    // Gmail Drafts meanwhile.
    let unready = call(
        &mut store,
        "draft_create",
        Some(&account),
        json!({"mode":"reply","conversation_id":conversation,"source_message_id":message,"request_id":"unready"}),
    );
    assert_eq!(unready["pgp_sign"], true);
    assert_eq!(unready["pgp_encrypt"], true);
    assert_eq!(unready["replies_to_encrypted"], true);
    let unready_id = unready["draft_id"].as_str().unwrap().to_owned();
    let saved = call(
        &mut store,
        "draft_save",
        Some(&account),
        json!({"draft_id":unready_id,"expected_revision":unready["revision"],"request_id":"unready-save","body":"About the plan."}),
    );
    assert_eq!(saved["pgp_encrypt"], true);
    let due = store
        .google_draft_due_after(&account, scope.connection_generation, 10, 0)
        .unwrap();
    assert!(!due.iter().any(|d| d.draft_id == unready_id));
    let p = send_request(&store, &scope, &unready_id);
    assert_eq!(
        store.google_send_begin(&scope, &p).err().unwrap().code,
        "pgp_recipient_key_missing"
    );
    import(&mut store, &text("gus.pub.asc"), "gus");
    let reply = call(
        &mut store,
        "draft_create",
        Some(&account),
        json!({"mode":"reply","conversation_id":conversation,"source_message_id":message,"request_id":"reply"}),
    );
    assert_eq!(reply["pgp_sign"], true);
    assert_eq!(reply["pgp_encrypt"], true);
    // The reply box shown before anything is saved starts the same way.
    let unsaved = call(
        &mut store,
        "draft_get",
        Some(&account),
        json!({"conversation_id":conversation}),
    );
    assert_eq!(unsaved["revision"], 0);
    assert_eq!(unsaved["pgp_sign"], true);
    assert_eq!(unsaved["pgp_encrypt"], true);
    let forward = call(
        &mut store,
        "draft_create",
        Some(&account),
        json!({"mode":"forward","conversation_id":conversation,"source_message_id":message,"request_id":"forward"}),
    );
    assert!(forward["pgp_encrypt"].is_null());
    assert_eq!(forward["replies_to_encrypted"], false);
}

#[test]
fn encrypted_html_shows_once_decrypted_and_never_from_a_failed_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let address = |email: &str| fm_mail::Address {
        name: None,
        email: email.into(),
    };
    let built = fm_mail::build_mail(
        &fm_mail::ComposeRequest {
            account_id: "gus".into(),
            to: vec![address("ann@example.test")],
            cc: vec![],
            bcc: vec![],
            subject: "Rich".into(),
            body_text: "Plain words".into(),
            body_html: Some("<p>Rich <b>words</b></p>".into()),
            forwarded_html: None,
            message_id: "<rich@example.test>".into(),
            reply_to_message_id: None,
            references: vec![],
            timestamp_secs: 1_790_000_000,
        },
        &fm_mail::AccountIdentity {
            account_id: "gus".into(),
            from: address("gus@example.test"),
            signature_text: String::new(),
            signature_html: None,
        },
        &[],
    )
    .unwrap();
    let ann = match fm_pgp::parse_keys(&fixture("ann.pub.asc"))
        .unwrap()
        .remove(0)
    {
        fm_pgp::ParsedKey::Public(key) => key,
        _ => panic!(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let encrypted = fm_pgp::encrypt_message(
        &built.raw,
        &[fm_pgp::Recipient {
            cert: &ann,
            hidden: false,
        }],
        None,
        now,
    )
    .unwrap();
    let message = receive(&mut store, &account, &encrypted, "rich");
    let version: String = store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    let cache = crate::view_cache::ViewCache::new(temp.path());
    let html = |store: &Store, decrypt: bool| {
        let plan = store
            .message_html_begin(
                &account,
                &json!({"id":message,"source_version":version,"decrypt":decrypt}),
                &[],
            )
            .unwrap();
        crate::html_content::read_html(plan, &cache, &CancellationToken::new()).unwrap()
    };
    assert_eq!(html(&store, true)["has_html"], false);
    call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("ann.sec.asc"),"passphrase":"fixture passphrase ann","request_id":"ann"}),
    );
    assert_eq!(html(&store, false)["has_html"], false);
    let view = html(&store, true);
    assert_eq!(view["has_html"], true);
    assert!(view["html"].as_str().unwrap().contains("words"));
    // The decrypted view held in memory isn't handed to a sealed read.
    assert_eq!(html(&store, false)["has_html"], false);
}

#[test]
fn pictures_in_encrypted_html_stay_out_of_the_image_cache() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    // A 2x2 PNG, shown inline through its Content-ID at a size that isn't
    // taken for a tracking pixel.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEUlEQVR4nGP4z8DA8B+MgBgAHfAD/dPQfSYAAAAASUVORK5CYII=";
    let raw = format!(
        "From: Gus <gus@example.test>\r\nTo: Ann <ann@example.test>\r\nSubject: Logo\r\nMessage-ID: <logo@example.test>\r\nDate: Thu, 22 Oct 2026 12:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Our logo <img src=\"cid:logo@example.test\" width=\"40\" height=\"40\"></p>\r\n--b\r\nContent-Type: image/png\r\nContent-ID: <logo@example.test>\r\nContent-Transfer-Encoding: base64\r\n\r\n{png}\r\n--b--\r\n"
    );
    let ann = match fm_pgp::parse_keys(&fixture("ann.pub.asc"))
        .unwrap()
        .remove(0)
    {
        fm_pgp::ParsedKey::Public(key) => key,
        _ => panic!(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let encrypted = fm_pgp::encrypt_message(
        raw.as_bytes(),
        &[fm_pgp::Recipient {
            cert: &ann,
            hidden: false,
        }],
        None,
        now,
    )
    .unwrap();
    let message = receive(&mut store, &account, &encrypted, "logo");
    call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("ann.sec.asc"),"passphrase":"fixture passphrase ann","request_id":"ann"}),
    );
    let version: String = store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    let cache = crate::view_cache::ViewCache::new(temp.path());
    let plan = || {
        store
            .message_html_begin(
                &account,
                &json!({"id":message,"source_version":version,"decrypt":true}),
                &[],
            )
            .unwrap()
    };
    let view = crate::html_content::read_html(plan(), &cache, &CancellationToken::new()).unwrap();
    let resource = view["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "cid")
        .unwrap()["id"]
        .as_u64()
        .unwrap() as usize;
    let shown = match crate::html_content::resolve_image(
        &plan(),
        resource,
        &cache,
        &CancellationToken::new(),
    )
    .unwrap()
    {
        crate::html_content::ImageSource::Ready { path, .. } => path,
        crate::html_content::ImageSource::Remote { .. } => panic!("an inline image is local"),
    };
    let decrypted = std::fs::canonicalize(temp.path().join("decrypted")).unwrap();
    assert!(std::fs::canonicalize(&shown)
        .unwrap()
        .starts_with(&decrypted));
    assert!(!temp.path().join("cache").join("images").exists());
    // The next view finds it there without decoding it again.
    let again = crate::html_content::read_html(plan(), &cache, &CancellationToken::new()).unwrap();
    let item = again["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "cid")
        .unwrap()
        .clone();
    assert_eq!(item["image"]["path"], shown.as_str());
    crate::pgp::forget_decrypted(temp.path()).unwrap();
    assert!(!std::path::Path::new(&shown).exists());
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A new public key, armored, whose user ID claims `email`, and its
/// fingerprint.
fn claiming(email: &str) -> (String, String) {
    let key = fm_pgp::generate(&fm_pgp::NewKey {
        name: "Claimant".into(),
        email: email.into(),
        algorithm: fm_pgp::Algorithm::Curve25519,
        expires_in: None,
    })
    .unwrap()
    .to_public_key();
    (
        fm_pgp::export_public(&key).unwrap(),
        fm_pgp::inspect(&key, false, now()).fingerprint,
    )
}

fn key_count(store: &mut Store) -> usize {
    call(store, "pgp_keys", None, json!({}))["keys"]
        .as_array()
        .unwrap()
        .len()
}

fn recipient_key(store: &mut Store, account: &str, address: &str) -> Value {
    call(
        store,
        "pgp_recipients",
        Some(account),
        json!({"addresses":[address]}),
    )["recipients"][0]["fingerprint"]
        .clone()
}

#[test]
fn a_key_for_an_address_another_key_has_needs_the_persons_say_so() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let gus = import(&mut store, &text("gus.pub.asc"), "gus")["added"][0]
        .as_str()
        .unwrap()
        .to_owned();
    // Someone else's key naming Gus's address, as a mail attachment might.
    let (armored, other) = claiming("gus@example.test");
    // A preview lists the key and the conflict, and changes nothing.
    let preview = call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":armored,"preview":true}),
    );
    assert_eq!(preview["keys"][0]["fingerprint"], other);
    assert_eq!(preview["keys"][0]["action"], "add");
    assert_eq!(preview["keys"][0]["secret"], false);
    assert_eq!(
        preview["conflicts"],
        json!([{"address":"gus@example.test","fingerprint":other,"holders":[gus],"own":false}])
    );
    assert_eq!(key_count(&mut store), 1);
    // Importing it is refused, and mail to Gus still uses his key.
    let refused = failure(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":armored,"request_id":"other"}),
    );
    assert_eq!(refused.code, "pgp_key_conflict");
    assert_eq!(refused.details["conflicts"][0]["holders"], json!([gus]));
    assert_eq!(key_count(&mut store), 1);
    assert_eq!(recipient_key(&mut store, &account, "gus@example.test"), gus);
    // Once the person says to use it, it is the key for that address.
    call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":armored,"replace":true,"request_id":"other-replace"}),
    );
    assert_eq!(
        recipient_key(&mut store, &account, "gus@example.test"),
        other
    );
    // Updating Gus's key again doesn't take the address back.
    import(&mut store, &text("gus.pub.asc"), "gus-again");
    assert_eq!(
        recipient_key(&mut store, &account, "gus@example.test"),
        other
    );
}

#[test]
fn the_key_stored_last_wins_whatever_it_says_its_age_is() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    // A key made now claims Gus's address first.
    let (armored, newer) = claiming("gus@example.test");
    import(&mut store, &armored, "newer");
    assert_eq!(
        recipient_key(&mut store, &account, "gus@example.test"),
        newer
    );
    // Gus's real key, made earlier, is chosen once the person accepts it.
    let gus = call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("gus.pub.asc"),"replace":true,"request_id":"gus"}),
    )["added"][0]
        .clone();
    assert_eq!(recipient_key(&mut store, &account, "gus@example.test"), gus);
}

#[test]
fn keys_from_mail_are_public_only_and_your_address_is_yours() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, _) = setup(temp.path());
    // A secret key from a message is refused, even before its passphrase.
    let refused = failure(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("rita.sec.asc"),"public_only":true,"request_id":"rita"}),
    );
    assert_eq!(refused.code, "secret_key_not_allowed");
    // A preview names a secret key without asking for its passphrase.
    let preview = call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("rita.sec.asc"),"preview":true}),
    );
    assert_eq!(preview["keys"][0]["secret"], true);
    assert_eq!(
        preview["keys"][0]["user_ids"],
        json!(["Rita RSA <rita@example.test>"])
    );
    // Someone's public key for the account's own address needs a say-so,
    // though no key holds that address yet.
    let (armored, fingerprint) = claiming("ann@example.test");
    let own = failure(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":armored,"public_only":true,"request_id":"own"}),
    );
    assert_eq!(own.code, "pgp_key_conflict");
    assert_eq!(
        own.details["conflicts"],
        json!([{"address":"ann@example.test","fingerprint":fingerprint,"holders":[],"own":true}])
    );
    // The person's own secret key for that address imports as before.
    call(
        &mut store,
        "pgp_key_import",
        None,
        json!({"text":text("ann.sec.asc"),"passphrase":"fixture passphrase ann","request_id":"ann"}),
    );
    assert_eq!(key_count(&mut store), 1);
}

#[test]
fn exported_keys_are_written_to_files_only_by_the_engine() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let fingerprint = generate(&mut store, &scope.account);
    let target = temp.path().join("key.asc");
    let direct = failure(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"public","path":target.to_str().unwrap()}),
    );
    assert_eq!(direct.code, "invalid_request");
    let relative = failure(
        &mut store,
        "pgp_key_export",
        None,
        json!({"fingerprint":fingerprint,"kind":"public","path":"key.asc"}),
    );
    assert_eq!(relative.code, "invalid_request");
    assert!(!target.exists());
}

#[test]
fn reads_report_text_outside_the_signed_part() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    import(&mut store, &text("gus.pub.asc"), "gus");
    let clear = receive(&mut store, &account, &fixture("gus-cleartext.eml"), "clear");
    let read = source(&store, &account, &clear);
    assert_eq!(read["pgp"]["unprotected"], false);
    assert!(read["pgp"]["headers"].is_null());
    let wrapped = text("gus-cleartext.eml").replacen(
        "-----BEGIN PGP SIGNED MESSAGE-----",
        "Pay the new account today.\r\n\r\n-----BEGIN PGP SIGNED MESSAGE-----",
        1,
    );
    let wrapped = receive(&mut store, &account, wrapped.as_bytes(), "wrapped");
    let read = source(&store, &account, &wrapped);
    assert_eq!(read["pgp"]["signatures"][0]["status"], "good");
    assert_eq!(read["pgp"]["unprotected"], true);
    assert!(!read["body_text"]
        .as_str()
        .unwrap()
        .contains("Pay the new account"));
    // Search still has what arrived outside the armor.
    let body: String = store
        .db
        .query_row("SELECT body FROM messages WHERE id=?1", [&wrapped], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(body.contains("Pay the new account"));
}

/// A `multipart/signed` message whose signed content has a file of type
/// `application/octet-stream`, as many mailers send a zip or a PDF.
fn signed_with_a_file() -> Vec<u8> {
    [
        "From: Gus GnuPG <gus@example.test>",
        "To: ann@example.test",
        "Subject: Contract",
        "Date: Thu, 22 Oct 2026 12:00:00 +0000",
        "Message-ID: <signed-file@example.test>",
        "MIME-Version: 1.0",
        "Content-Type: multipart/signed; micalg=pgp-sha256; protocol=\"application/pgp-signature\"; boundary=\"outer\"",
        "",
        "--outer",
        "Content-Type: multipart/mixed; boundary=\"inner\"",
        "",
        "--inner",
        "Content-Type: text/plain",
        "",
        "The contract is attached.",
        "--inner",
        "Content-Type: application/octet-stream; name=\"contract.zip\"",
        "Content-Disposition: attachment; filename=\"contract.zip\"",
        "Content-Transfer-Encoding: base64",
        "",
        "UEsDBA==",
        "--inner--",
        "--outer",
        "Content-Type: application/pgp-signature; name=\"signature.asc\"",
        "",
        "-----BEGIN PGP SIGNATURE-----",
        "",
        "AA==",
        "-----END PGP SIGNATURE-----",
        "--outer--",
        "",
    ]
    .join("\r\n")
    .into_bytes()
}

#[test]
fn files_in_signed_mail_are_listed_and_restored() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(temp.path());
    let account = scope.account.clone();
    let message = receive(&mut store, &account, &signed_with_a_file(), "signed-file");
    let names = |store: &Store| -> Vec<String> {
        store
            .db
            .prepare("SELECT name FROM attachments WHERE message_id=?1 ORDER BY name")
            .unwrap()
            .query_map([&message], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    // The file is listed; the signature isn't.
    assert_eq!(names(&store), ["contract.zip"]);
    // As an earlier build left it: the file's row deleted, the protection
    // repair done.
    store
        .db
        .execute("DELETE FROM attachments WHERE message_id=?1", [&message])
        .unwrap();
    store
        .db
        .execute(
            "INSERT INTO meta(key,value) VALUES('pgp_message_repair',-1)",
            [],
        )
        .unwrap();
    while store.repair_pgp_messages(10).unwrap() {}
    assert_eq!(names(&store), ["contract.zip"]);
    // It reads back from the retained source.
    let id: String = store
        .db
        .query_row(
            "SELECT id FROM attachments WHERE message_id=?1",
            [&message],
            |r| r.get(0),
        )
        .unwrap();
    let exported = store
        .attachment_begin(&account, &json!({"id":id}))
        .unwrap()
        .export()
        .unwrap();
    assert_eq!(
        std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
        b"PK\x03\x04"
    );
    // The repair runs once.
    store
        .db
        .execute("DELETE FROM attachments WHERE message_id=?1", [&message])
        .unwrap();
    assert!(!store.repair_pgp_messages(10).unwrap());
    assert!(names(&store).is_empty());
}
