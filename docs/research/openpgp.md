# OpenPGP: library, key choices and interoperability

Recorded 2026-10-22 for [D217](../decisions.md). Owners: [Core](../contracts/core-storage.md) for the keyring, sealing and sending, [Mail UI](../contracts/mail-ui.md) for Settings → Encryption, the composer and the reader, [Platform](../contracts/platform.md) for the keychain entry. Everything below was run on this Mac (arm64, macOS 15) with synthetic keys and addresses; no real account, key or mail was used.

## The question

The founder asked for PGP support: use existing keys, create new ones that can be exported to other applications, follow best practice, manage keys in a new Settings section, and show which accounts have a key. "End to end" means signing, encrypting, decrypting and verifying mail as well as managing keys.

## Library

| Option | License | Build | Fit |
| --- | --- | --- | --- |
| rpgp (`pgp` crate) 0.21.0 | MIT OR Apache-2.0 | Pure Rust; its MSRV, 1.88, is the workspace toolchain | RFC 9580 and RFC 4880 packets, PGP/MIME building blocks, GnuPG's OCB packet behind an option. Low level: it verifies a signature but leaves which self-signatures count, expiry and revocation to the application. Used by Delta Chat |
| Sequoia (`sequoia-openpgp`) | LGPL-2.0-or-later | Needs a crypto backend; the default is the Nettle C library, and the pure-Rust backend is marked experimental | A complete policy engine, but LGPL in a statically linked MIT binary and a C dependency on Windows |
| GnuPG through GPGME | GPL/LGPL, a separate installation and `gpg-agent` process | External | Can't be assumed installed, adds a background agent, and Windows packaging would need Gpg4win |

Chosen: rpgp, in a new pure crate `fm-pgp` that adds the missing policy. `cargo info pgp` and the crate's own `Cargo.toml` were read on 2026-10-22. The workspace now links 330 crates (`scripts/third_party_notices.py`); every new one is MIT, Apache-2.0, BSD-3-Clause or the bzip2 license (the pure-Rust `libbz2-rs-sys`).

## Policy fm-pgp adds

- **Self-signatures.** A user ID, direct-key signature or subkey binding counts when it verifies with the primary key, was made no more than a day ahead of the clock, hasn't expired and uses an acceptable hash. The newest one wins; a newer certification revocation hides a user ID.
- **Hashes.** SHA-2 and SHA-3 count. SHA-1 counts only for key bindings made before 2023-02-01, so older keys still work while new SHA-1 signatures don't (Sequoia's standard policy draws the same line). MD5 and RIPEMD-160 never count. A message signature using SHA-1 is reported as weak.
- **Key size.** RSA, DSA and ElGamal under 2048 bits are weak and are never used to sign or encrypt.
- **Subkeys.** A signing subkey needs a valid back-signature (primary key binding), or anyone could claim another person's subkey. Key flags decide usage; a binding without flags falls back to what the algorithm can do.
- **Signature checks.** A signature is good when it verifies with a key in the keyring that was valid and allowed to sign when it was made. Revoked keys, expired keys and weak hashes are reported separately. Whether the signer's user IDs include the From address is reported too, so a valid signature by someone else's key isn't shown as the sender's. Only binary and text document signatures count. A signature that names its intended recipients, none of them a key that decrypted the message, is reported as possibly forwarded.

## Key generation

GnuPG 2.4.8's defaults, observed with `gpg --quick-gen-key "Dee Default <dee@example.test>" default default`: an Ed25519 primary key that certifies and signs, a Curve25519 encryption subkey, and an expiry three years out.

- **Default.** The same: a version 4 Ed25519 (EdDSA, algorithm 22) primary key with a Curve25519 ECDH subkey, expiring after three years unless the person picks 1, 2 or 5 years or never. fm-pgp sets the expiry on the user ID self-signature and on the subkey binding.
- **Alternative.** RSA 4096 for both keys, for software without elliptic-curve support. Unoptimized RSA key generation took over 30 seconds in tests, so dev builds now optimize `rsa` and `num-bigint-dig`.
- **Not version 6.** rpgp can make RFC 9580 version 6 keys, but GnuPG 2.4.8 refused one ("read_block: read error: Invalid packet", import failed). A key made here must work in other applications, so new keys are version 4. Imported version 6 keys are read and used.
- **Preferences.** AES-256, AES-192, AES-128; SHA-512, SHA-384, SHA-256, SHA-224; ZLIB, BZip2, ZIP, none; the SEIPD version 1 (MDC) feature.

## Secret keys at rest and on export

- **At rest.** A secret key is stored without passphrase protection inside an AES-256-GCM envelope sealed with a 256-bit key-encryption key; the fingerprint is the associated data, so one key's row can't be swapped for another's. The key-encryption key lives in the OS keychain (`fuckmail.pgp.v1.<sha256 of the profile namespace>`, user `keyring`) and is kept in the engine's memory after first use. A missing or refused keychain fails with `store_unavailable`; there is no plaintext fallback. Thunderbird works the same way: an imported key's passphrase is removed and the key is re-protected by a secret in its password manager. Demo profiles, which hold synthetic data only, keep the key-encryption key in a 0600 file in the profile so tests never reach the keychain.
- **Import.** A protected secret key asks for its passphrase once. A GnuPG export that holds only subkeys (a primary-key stub) is refused with `secret_key_incomplete`.
- **Export.** The secret key is protected with a passphrase of at least 12 characters, using the iterated and salted SHA-256 S2K with AES-256 (S2K usage 254), which every OpenPGP program reads, at the highest iteration count (255: 65,011,712 bytes hashed; rpgp's default is 224, 16,777,216). Version 6 keys keep rpgp's Argon2 default. The engine writes exports to a file readable only by the user on every platform (see [the audit](#audit-2026-10-24)). A revocation certificate is armored as a public key block, as GnuPG expects: GnuPG refused the same signature armored as a signature block ("no valid OpenPGP data found") and imported it as a key block, marking the key revoked.

## Messages

- **Format.** PGP/MIME (RFC 3156). Signed mail is `multipart/signed` with SHA-512 (`micalg=pgp-sha512`). Encrypted mail is `multipart/encrypted`, signed inside the encryption (RFC 3156 section 6.2) when signing is on. Symmetric encryption is AES-256 in SEIPD version 1, falling back to AES-128 only when a recipient's preferences leave AES-256 out; SEIPD version 2 isn't used, since GnuPG doesn't read it. Every message is also encrypted to the sender, so their Sent copy stays readable. Bcc recipients get anonymous (wildcard) key IDs, so other recipients can't see who else could read it. Nothing is ever sent unencrypted in place of a failed encryption.
- **Canonical form.** The composer's MIME builder wrote small text attachments as 7bit with bare LF line endings. Signatures over that didn't verify, because RFC 3156 signs the CRLF form and transport may rewrite line endings. Attachments are now always base64, and fm-pgp converts any remaining bare LF to CRLF before signing or encrypting.
- **Subject.** Not hidden. The Gmail API threads a message only when the subject matches the thread's ([threads guide](https://developers.google.com/gmail/api/guides/threads)), so a "..." subject in the RFC 9788 style would break replies apart. Recipients, the sender and the date stay visible too, as in all OpenPGP mail. Since the audit, a copy of these fields is protected inside the signature or encryption ([RFC 9788](https://www.rfc-editor.org/rfc/rfc9788) header protection), so changes made on the way are detected.
- **Reading.** Mail is decrypted only to be read, in memory, and the plaintext is never stored. The source, its search text and the list preview keep only what was outside the encryption. The reader and other applications need files for pictures and opened attachments; a first version put those in the image cache (256 MiB, oldest first) and `exports/`, where they could outlast the session by weeks. They now go to `<profile>/decrypted/`, which the engine deletes as it starts and stops. GnuPG 2.4 encrypts to keys that announce its AEAD feature (including RSA keys it made) with its own OCB packet (type 20): rpgp refused it by default ("GnuPG's AEAD packet (type 20) is non-standard"), so decryption enables that option. It is authenticated like SEIPD. Unauthenticated legacy encryption (no MDC) stays refused. Decrypted plaintext is capped at 64 MiB, which also bounds compression bombs.
- **Drafts.** A draft with Encrypt on is never uploaded to Gmail Drafts, by the background worker or on request (`pgp_draft_local`). A draft uploaded before Encrypt was turned on keeps its Gmail copy until it is sent, which replaces it.

## Interoperability with GnuPG

GnuPG 2.4.8 (libgcrypt 1.11.2, Homebrew), each run in a throwaway `GNUPGHOME` under `target/`.

| Check | Result |
| --- | --- |
| GnuPG imports a key made by fm-pgp and exported with a passphrase; lists `sec ed25519 [SC]` and `ssb cv25519 [E]` with the expiry | Pass |
| GnuPG imports our revocation certificate and marks the key revoked | Pass |
| GnuPG verifies our PGP/MIME signature over a composer-built message with an attachment ("Good signature") | Pass, after the canonical-form fix |
| GnuPG decrypts our signed and encrypted message and reports a good signature | Pass |
| fm-pgp reads GnuPG's Ed25519/Curve25519 and RSA 3072 keys, and unlocks their protected exports | Pass |
| fm-pgp decrypts and verifies GnuPG's PGP/MIME `--sign --encrypt` message with an attachment inside | Pass |
| fm-pgp verifies GnuPG's PGP/MIME detached signature (SHA-256) | Pass |
| fm-pgp decrypts GnuPG's inline armored message and verifies its clear-signed text, dash escaping included | Pass |
| fm-pgp decrypts GnuPG's OCB-encrypted message to an RSA key | Pass, with the OCB option |

The GnuPG-made keys and messages are committed as synthetic fixtures in `crates/fm-pgp/tests/fixtures/`. `cargo test -p fm-pgp -- --ignored` repeats the two GnuPG round trips where GnuPG is installed.

## Through real Gmail

Recorded 2026-10-23 for [D218](../decisions.md), on Windows 11 between the founder's two accounts (a gmail.com address and a Google Workspace one), each sending to the other through the Gmail API. Only the MIME structure and the engine's results were read; no message content is recorded here.

| Message | Sender's Sent copy | Recipient's copy |
| --- | --- | --- |
| Encrypted (`multipart/encrypted`, signed inside) | As sent | Rewritten as `multipart/mixed`: a `text/plain` part holding the armored `PGP MESSAGE`, and the `application/pgp-encrypted` version part as an attachment with no name (shown as `attachment.bin`) |
| Signed (`multipart/signed`) | As sent | Rewritten as `multipart/mixed`: the signed `multipart/alternative` with Gmail's own boundaries and encodings, and `signature.asc` as an attachment |

- **Encrypted mail still opens.** The inline path found the armor and decrypted it, but showed the plaintext, the PGP/MIME entity, as text: the reader displayed its `Content-Type` lines and base64. Decrypted inline content that starts with a `Content-*` or `MIME-Version` field and has a header block is now read as MIME under the outer headers, as PGP/MIME is, with a signed entity inside checked the same way. The version part is hidden in the reader.
- **Signed mail can't be checked.** The signed bytes no longer exist: the boundaries and transfer encodings changed, so the signature can't verify, and the message isn't `multipart/signed` any more. It reads as ordinary mail with a `signature.asc` attachment, which the reader shows as an OpenPGP signature card. Encrypting avoids this, since the signature is inside the encryption. Whether another upload path (`uploadType=media` instead of the JSON `raw` field) keeps the structure wasn't tried.

## Audit 2026-10-24

A security review of the feature against current practice, for [D219](../decisions.md). Each finding is fixed and tested; the details of the fixes are in the decision and the Core106 contract.

| Finding | Severity | Attack | Fix |
| --- | --- | --- | --- |
| One-click import of keys attached to mail | High | A stranger mails a key claiming a colleague's address. It became the newest key for that address, so replies were encrypted to the stranger, and the stranger's signatures showed as the colleague's. Secret keys in a file became "your" keys. | Preview first (fingerprint, user IDs, sender check), public keys only from mail, conflicts need **Import anyway**; the address uses the key stored last, not the one claiming to be newest. |
| Text outside inline armor shown as protected | High | The "Johnny, You Are Fired" partial-signing and wrapping attacks: someone forwards a validly signed block with their own text around it. | Only the signed or encrypted block's text is shown, with a warning when there is more. |
| Encrypt silently turned off | Medium | A reply to encrypted mail lost Encrypt when a recipient had no key, so its text, often quoting the encrypted message, went to Gmail Drafts and could be sent readable. | Encrypt stays on and sending waits; turning it off shows a warning. |
| Files in signed mail hidden and deleted | Medium | Any `application/octet-stream` file inside `multipart/signed` was taken for the signature, so a signed message's files vanished, and the repair pass deleted their rows. | Only the real signature and encrypted-payload parts are plumbing; a repair pass restores the rows. |
| Signature type unchecked | Medium | rpgp hashes a standalone or timestamp signature (types 0x02 and 0x40) over the first byte of the data only (`SignatureConfig::hash_data_to_sign` in pgp 0.21.0), so one could be moved onto any message starting with the same byte. Certification types are already refused over data. | Only binary and text document signatures count. |
| Export files not owner-only on Windows | Medium | `QSaveFile` inherits the folder's access list, and the CLI's 0600 is Unix-only, so a secret-key backup in a shared folder was readable by other users. The GUI also replaced existing files. | The engine writes every export through one owner-only helper; on Windows the access list is checked in a test. |
| Sealed-until-asked and exports enforced by the GUI | Medium | Any program running as the user can ask the engine to decrypt or export; on macOS that skips the keychain's per-app prompt. | Accepted: OS accounts are the boundary, as elsewhere in the app. Recorded in SECURITY.md. |
| Unprotected subject, sender and date; no intended recipients | Low | Headers can be changed in transit without breaking the signature, and a signed message can be re-encrypted to someone else ("surreptitious forwarding"). | RFC 9788 header protection, RFC 9580 intended recipient fingerprints (left out when anyone is Bcc), and reader warnings for both. |
| S2K count 224 | Low | Offline guessing of a backup's passphrase. | Highest count, 255. |
| Keys could be flooded | Low | Certificate flooding as on the SKS keyservers in 2019: junk third-party signatures and user IDs merged into a stored key without limit. | Only signatures the primary key made and that verify are kept, and stored keys are capped at 1 MiB. |

Sources: [RFC 9580](https://www.rfc-editor.org/rfc/rfc9580) (the intended recipient fingerprint subpacket), [RFC 9788](https://www.rfc-editor.org/rfc/rfc9788) (header protection), Müller et al., "Johnny, You Are Fired!" (USENIX Security 2019), and the SKS keyserver certificate flooding of June 2019.

## Not verified

- Thunderbird, Proton, Mailvelope and other clients were not run; only GnuPG was.
- PGP/MIME through Gmail to a non-Gmail mailbox, and from other clients into Gmail.
- The keychain entry on Windows and Linux, and the macOS keychain prompt for it; tests use an in-memory key and demo profiles a file.
- Autocrypt headers and key discovery (WKD, keyservers) are not implemented; keys come from files, pasted text and keys attached to mail, after a preview.
- How other clients show our protected headers and intended recipients, and whether Gmail keeps the `hp` parameter on delivery.
