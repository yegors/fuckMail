CREATE TABLE message_sender_kinds(
    account TEXT NOT NULL,
    message_id TEXT NOT NULL,
    source_hash TEXT NOT NULL CHECK(length(source_hash)=64),
    kind TEXT NOT NULL CHECK(kind IN ('people','automated')),
    provenance TEXT NOT NULL CHECK(provenance='fixture'),
    PRIMARY KEY(account,message_id),
    FOREIGN KEY(account,message_id) REFERENCES messages(account,id) ON DELETE CASCADE
);
