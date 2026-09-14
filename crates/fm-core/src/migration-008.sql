INSERT OR IGNORE INTO meta(key,value) VALUES('profile_kind',0);
CREATE TABLE profile_identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),vault_namespace TEXT NOT NULL UNIQUE);
INSERT INTO profile_identity VALUES(1,lower(hex(randomblob(32))));
CREATE TABLE google_accounts(
    account TEXT PRIMARY KEY,
    google_subject TEXT NOT NULL UNIQUE,
    scopes_json TEXT NOT NULL,
    connection_state TEXT NOT NULL CHECK(connection_state IN ('connected','disconnected','reauthorization_required')),
    revision INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY(account) REFERENCES accounts(id)
);
