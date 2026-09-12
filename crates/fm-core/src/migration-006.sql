CREATE TABLE drafts_next(
    account TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    conversation_id TEXT,
    legacy_reply INTEGER NOT NULL DEFAULT 0,
    mode TEXT NOT NULL CHECK(mode IN ('new','reply','reply_all','forward')),
    source_message_id TEXT,
    source_hash TEXT,
    routing_json TEXT,
    body TEXT NOT NULL,
    revision INTEGER NOT NULL,
    body_html TEXT,
    attachments_json TEXT NOT NULL DEFAULT '[]',
    discarded INTEGER NOT NULL DEFAULT 0,
    signature_text TEXT NOT NULL DEFAULT '',
    from_name TEXT NOT NULL DEFAULT '',
    identity_revision INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(account,draft_id),
    FOREIGN KEY(account) REFERENCES accounts(id),
    FOREIGN KEY(account,conversation_id) REFERENCES conversations(account,id)
);
INSERT INTO drafts_next(account,draft_id,conversation_id,legacy_reply,mode,body,revision,body_html,attachments_json,discarded,signature_text,from_name,identity_revision)
SELECT account,conversation_id,conversation_id,1,'reply',body,revision,body_html,attachments_json,discarded,signature_text,from_name,identity_revision FROM drafts;
DROP TABLE drafts;
ALTER TABLE drafts_next RENAME TO drafts;
CREATE UNIQUE INDEX drafts_legacy_reply ON drafts(account,conversation_id) WHERE legacy_reply=1;
CREATE INDEX drafts_listing ON drafts(account,discarded,updated_at DESC,draft_id);
