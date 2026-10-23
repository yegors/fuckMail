-- OpenPGP: the profile's keyring, each account's key choice, and how each
-- received message is protected (0 none, 1 signed, 2 encrypted).
CREATE TABLE pgp_keys(
    fingerprint TEXT PRIMARY KEY CHECK(length(fingerprint) IN (40,64)),
    public_key BLOB NOT NULL,
    secret_key BLOB,
    origin TEXT NOT NULL CHECK(origin IN ('generated','imported')),
    added_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE pgp_key_addresses(
    address TEXT NOT NULL,
    fingerprint TEXT NOT NULL REFERENCES pgp_keys(fingerprint) ON DELETE CASCADE,
    PRIMARY KEY(address,fingerprint)
) STRICT;
CREATE INDEX pgp_key_addresses_by_key ON pgp_key_addresses(fingerprint);
CREATE TABLE pgp_accounts(
    account TEXT PRIMARY KEY REFERENCES accounts(id),
    fingerprint TEXT NOT NULL REFERENCES pgp_keys(fingerprint) ON DELETE CASCADE,
    sign_by_default INTEGER NOT NULL CHECK(sign_by_default IN (0,1)),
    encrypt_by_default INTEGER NOT NULL CHECK(encrypt_by_default IN (0,1))
) STRICT;
CREATE INDEX pgp_accounts_by_key ON pgp_accounts(fingerprint);
ALTER TABLE messages ADD COLUMN pgp INTEGER NOT NULL DEFAULT 0 CHECK(pgp IN (0,1,2));
