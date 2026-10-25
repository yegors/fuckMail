//! Key generation, validity, sealing, export and import, including keys that
//! GnuPG made.
use fm_pgp::{
    export_public, export_revocation, export_secret, generate, inspect, is_protected, merge,
    parse_keys, revocations, seal, set_expiry, unlock, unseal, Algorithm, KeyStatus, NewKey,
    ParsedKey,
};
use pgp::composed::{SignedPublicKey, SignedSecretKey};

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

fn new_key(expires_in: Option<u64>) -> SignedSecretKey {
    generate(&NewKey {
        name: "Ann Example".into(),
        email: "ann@example.test".into(),
        algorithm: Algorithm::Curve25519,
        expires_in,
    })
    .unwrap()
}

fn one_public(bytes: &[u8]) -> SignedPublicKey {
    match parse_keys(bytes).unwrap().remove(0) {
        ParsedKey::Public(key) => key,
        _ => panic!("expected a public key"),
    }
}

fn one_secret(bytes: &[u8]) -> SignedSecretKey {
    match parse_keys(bytes).unwrap().remove(0) {
        ParsedKey::Secret(key) => key,
        _ => panic!("expected a secret key"),
    }
}

#[test]
fn a_new_key_signs_encrypts_and_expires_when_asked() {
    let three_years = 3 * 365 * 86_400;
    let key = new_key(Some(three_years));
    let info = inspect(&key.to_public_key(), true, now());
    assert_eq!(info.status, KeyStatus::Valid);
    assert_eq!(info.version, 4);
    assert_eq!(info.algorithm, "Ed25519");
    assert_eq!(info.user_ids, ["Ann Example <ann@example.test>"]);
    assert_eq!(info.addresses, ["ann@example.test"]);
    assert!(info.can_sign && info.can_encrypt && info.secret);
    assert_eq!(info.fingerprint.len(), 40);
    let expires = info.expires_at.unwrap();
    assert!(expires.abs_diff(info.created_at + three_years) < 5);
    assert_eq!(info.subkeys.len(), 1);
    assert_eq!(info.subkeys[0].algorithm, "Curve25519");
    assert_eq!(info.subkeys[0].usage, ["encrypt"]);
    assert_eq!(info.subkeys[0].expires_at, Some(expires));
    // Past the expiry the key is expired and can do nothing.
    let later = inspect(&key.to_public_key(), true, expires + 1);
    assert_eq!(later.status, KeyStatus::Expired);
    assert!(!later.can_sign && !later.can_encrypt);
}

#[test]
fn expiry_can_be_moved_or_removed() {
    let key = new_key(Some(86_400 * 30));
    let start = now();
    let renewed = set_expiry(&key, Some(86_400 * 365), start).unwrap();
    let info = inspect(&renewed.to_public_key(), true, start);
    assert!(info.expires_at.unwrap().abs_diff(start + 86_400 * 365) < 5);
    assert_eq!(info.subkeys[0].expires_at, info.expires_at);
    // A stored certificate that merges the renewal follows the newest signature.
    let merged = merge(&key.to_public_key(), &renewed.to_public_key());
    assert_eq!(inspect(&merged, false, start).expires_at, info.expires_at);
    let forever = set_expiry(&renewed, None, start + 10).unwrap();
    let info = inspect(&forever.to_public_key(), true, start + 10);
    assert_eq!(info.expires_at, None);
    assert_eq!(info.status, KeyStatus::Valid);
    assert!(set_expiry(&key, Some(60), start).is_err());
}

#[test]
fn rsa_keys_are_offered_for_older_software() {
    let key = generate(&NewKey {
        name: "".into(),
        email: "rsa@example.test".into(),
        algorithm: Algorithm::Rsa4096,
        expires_in: None,
    })
    .unwrap();
    let info = inspect(&key.to_public_key(), true, now());
    assert_eq!(info.algorithm, "RSA 4096");
    assert_eq!(info.user_ids, ["<rsa@example.test>"]);
    assert!(info.can_sign && info.can_encrypt);
}

#[test]
fn bad_user_ids_are_refused() {
    for (name, email) in [
        ("A <b>", "a@example.test"),
        ("Ann", "not an address"),
        ("Ann", ""),
    ] {
        let result = generate(&NewKey {
            name: name.into(),
            email: email.into(),
            algorithm: Algorithm::Curve25519,
            expires_in: None,
        });
        assert_eq!(
            result.unwrap_err().code,
            "invalid_request",
            "{name} {email}"
        );
    }
}

#[test]
fn sealed_keys_open_only_with_their_key_and_fingerprint() {
    let key = new_key(None);
    let fingerprint = inspect(&key.to_public_key(), true, now()).fingerprint;
    let kek = fm_pgp::new_kek();
    let blob = seal(&key, &kek).unwrap();
    // Nothing of the secret is readable in the blob.
    assert!(!blob.windows(8).any(|w| pgp::ser::Serialize::to_bytes(&key)
        .unwrap()
        .windows(8)
        .any(|k| k == w)));
    let opened = unseal(&blob, &fingerprint, &kek).unwrap();
    assert_eq!(opened.primary_key, key.primary_key);
    assert_eq!(
        unseal(&blob, &fingerprint, &fm_pgp::new_kek())
            .unwrap_err()
            .code,
        "key_locked"
    );
    let other = inspect(&new_key(None).to_public_key(), true, now()).fingerprint;
    assert_eq!(unseal(&blob, &other, &kek).unwrap_err().code, "key_locked");
    let mut tampered = blob.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert_eq!(
        unseal(&tampered, &fingerprint, &kek).unwrap_err().code,
        "key_locked"
    );
}

#[test]
fn exported_secret_keys_need_a_strong_passphrase_and_round_trip() {
    let key = new_key(None);
    assert_eq!(
        export_secret(&key, "short").unwrap_err().code,
        "weak_passphrase"
    );
    let armored = export_secret(&key, "a long enough passphrase").unwrap();
    assert!(armored.starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
    let imported = one_secret(armored.as_bytes());
    assert!(is_protected(&imported));
    // AES-256 under the iterated and salted SHA-256 S2K at its highest count,
    // for the primary key and the subkey alike.
    use pgp::{
        crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
        types::{S2kParams, SecretParams, StringToKey},
    };
    let parts = std::iter::once(imported.primary_key.secret_params()).chain(
        imported
            .secret_subkeys
            .iter()
            .map(|s| s.key.secret_params()),
    );
    for params in parts {
        let SecretParams::Encrypted(encrypted) = params else {
            panic!("expected a protected key");
        };
        let S2kParams::Cfb { sym_alg, s2k, .. } = encrypted.string_to_key_params() else {
            panic!("expected CFB protection");
        };
        assert_eq!(*sym_alg, SymmetricKeyAlgorithm::AES256);
        assert!(matches!(
            s2k,
            StringToKey::IteratedAndSalted {
                hash_alg: HashAlgorithm::Sha256,
                count: fm_pgp::EXPORT_S2K_COUNT,
                ..
            }
        ));
    }
    assert_eq!(
        unlock(&imported, None).unwrap_err().code,
        "passphrase_required"
    );
    assert_eq!(
        unlock(&imported, Some("the wrong passphrase"))
            .unwrap_err()
            .code,
        "wrong_passphrase"
    );
    let unlocked = unlock(&imported, Some("a long enough passphrase")).unwrap();
    assert!(!is_protected(&unlocked));
    assert_eq!(unlocked.to_public_key(), key.to_public_key());
    // The unlocked secret is the same: it seals, and signs for the same key.
    let kek = fm_pgp::new_kek();
    let fingerprint = fm_pgp::inspect(&key.to_public_key(), true, now()).fingerprint;
    let reopened = unseal(&seal(&unlocked, &kek).unwrap(), &fingerprint, &kek).unwrap();
    let signer = fm_pgp::signing_key(&key.to_public_key(), &reopened, now()).unwrap();
    let raw = b"From: ann@example.test\r\nTo: bob@example.test\r\nSubject: Hi\r\nMIME-Version: 1.0\r\nContent-Type: text/plain\r\n\r\nHello\r\n";
    let signed = fm_pgp::sign_message(raw, signer).unwrap();
    let opened = fm_pgp::open_message(
        &signed,
        &fm_pgp::Keyring {
            certs: vec![key.to_public_key()],
        },
        &mut || unreachable!(),
        now(),
    )
    .unwrap();
    assert_eq!(opened.signatures[0].status, fm_pgp::SignatureStatus::Good);
}

#[test]
fn public_exports_and_revocation_certificates_import_back() {
    let key = new_key(None);
    let public = key.to_public_key();
    let armored = export_public(&public).unwrap();
    assert!(armored.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    assert_eq!(
        one_public(armored.as_bytes()).primary_key,
        public.primary_key
    );
    let revocation = export_revocation(&key, now()).unwrap();
    // GnuPG reads revocation certificates as public key blocks.
    assert!(revocation.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    let sig = match parse_keys(revocation.as_bytes()).unwrap().remove(0) {
        ParsedKey::Revocation(sig) => sig,
        _ => panic!("expected a revocation"),
    };
    let mut revoked = public.clone();
    // A revocation made by another key changes nothing.
    let stranger = new_key(None).to_public_key();
    let mut untouched = stranger.clone();
    assert!(!revocations(&mut untouched, std::slice::from_ref(&sig)));
    assert!(revocations(&mut revoked, std::slice::from_ref(&sig)));
    assert!(!revocations(&mut revoked, &[sig]));
    let info = inspect(&revoked, false, now());
    assert_eq!(info.status, KeyStatus::Revoked);
    assert!(!info.can_encrypt);
}

#[test]
fn imports_keep_only_what_the_primary_key_vouches_for() {
    let ann = new_key(None).to_public_key();
    let mallory = generate(&NewKey {
        name: "Mallory".into(),
        email: "mallory@example.test".into(),
        algorithm: Algorithm::Curve25519,
        expires_in: None,
    })
    .unwrap()
    .to_public_key();
    // A copy of Ann's key padded with Mallory's user ID and subkey (signed by
    // Mallory, not Ann) and Mallory's self-signature on Ann's user ID.
    let mut padded = ann.clone();
    padded.details.users.push(mallory.details.users[0].clone());
    padded
        .public_subkeys
        .push(mallory.public_subkeys[0].clone());
    padded.details.users[0]
        .signatures
        .push(mallory.details.users[0].signatures[0].clone());
    let merged = merge(&ann, &padded);
    assert_eq!(merged, ann);
    let info = inspect(&merged, false, now());
    assert_eq!(info.user_ids, ["Ann Example <ann@example.test>"]);
    assert_eq!(info.subkeys.len(), 1);
    // A new user ID Ann did sign still merges in.
    let renewed = set_expiry(&new_key(None), Some(400 * 86_400), now()).unwrap();
    let both = merge(
        &renewed.to_public_key(),
        &set_expiry(&renewed, None, now() + 10)
            .unwrap()
            .to_public_key(),
    );
    assert_eq!(
        both.details.users[0].signatures.len(),
        2,
        "both self-signatures are kept"
    );
}

#[test]
fn several_keys_in_one_file_and_junk_around_them() {
    let mut text = String::from("Here are our keys:\n\n");
    text.push_str(std::str::from_utf8(&fixture("gus.pub.asc")).unwrap());
    text.push_str("\nand\n");
    text.push_str(std::str::from_utf8(&fixture("rita.pub.asc")).unwrap());
    let keys = parse_keys(text.as_bytes()).unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(parse_keys(b"not a key").unwrap_err().code, "invalid_key");
    assert_eq!(
        parse_keys(&vec![b'a'; fm_pgp::MAX_KEY_BYTES + 1])
            .unwrap_err()
            .code,
        "limit_exceeded"
    );
}

#[test]
fn gnupg_keys_read_like_ours() {
    let gus = one_public(&fixture("gus.pub.asc"));
    let info = inspect(&gus, false, FIXTURE_NOW);
    assert_eq!(info.status, KeyStatus::Valid);
    assert_eq!(info.user_ids, ["Gus GnuPG <gus@example.test>"]);
    assert_eq!(info.algorithm, "Ed25519");
    assert!(info.can_sign && info.can_encrypt);
    let rita = one_public(&fixture("rita.pub.asc"));
    let info = inspect(&rita, false, FIXTURE_NOW);
    assert_eq!(info.algorithm, "RSA 3072");
    assert_eq!(info.subkeys[0].usage, ["encrypt"]);
    // GnuPG's passphrase-protected exports unlock with their passphrase.
    let secret = one_secret(&fixture("rita.sec.asc"));
    assert!(is_protected(&secret));
    assert!(!is_protected(
        &unlock(&secret, Some("fixture passphrase rita")).unwrap()
    ));
    let ann = one_secret(&fixture("ann.sec.asc"));
    assert!(unlock(&ann, Some("fixture passphrase ann")).is_ok());
}

/// Round trip through GnuPG, when it is installed. Run with
/// `cargo test -p fm-pgp -- --ignored`.
#[test]
#[ignore]
fn gnupg_reads_our_exports() {
    let gpg = |home: &std::path::Path, args: &[&str], input: Option<&[u8]>| {
        use std::io::Write;
        let mut child = std::process::Command::new("gpg")
            .env("GNUPGHOME", home)
            .args(["--batch", "--pinentry-mode", "loopback"])
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("gpg is installed");
        if let Some(input) = input {
            child.stdin.take().unwrap().write_all(input).unwrap();
        }
        child.wait_with_output().unwrap()
    };
    // GnuPG's agent socket must fit a short path, so use one under target/.
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fm-pgp-gnupg");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let key = new_key(Some(2 * 365 * 86_400));
    let secret = export_secret(&key, "interop passphrase here").unwrap();
    let out = gpg(
        &home,
        &["--passphrase", "interop passphrase here", "--import"],
        Some(secret.as_bytes()),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let listed = gpg(&home, &["--with-colons", "--list-secret-keys"], None);
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert!(listing.contains("ann@example.test"), "{listing}");
    assert!(listing.lines().any(|l| l.starts_with("ssb:")), "{listing}");
    let revocation = export_revocation(&key, now()).unwrap();
    let out = gpg(&home, &["--import"], Some(revocation.as_bytes()));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let listed = gpg(
        &home,
        &["--with-colons", "--list-keys", "ann@example.test"],
        None,
    );
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert!(listing.contains("pub:r:"), "{listing}");
    let _ = std::process::Command::new("gpgconf")
        .env("GNUPGHOME", &home)
        .args(["--kill", "gpg-agent"])
        .status();
}
