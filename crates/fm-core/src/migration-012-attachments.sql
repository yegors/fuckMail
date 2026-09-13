CREATE TABLE attachments_next(
    account TEXT NOT NULL,
    id TEXT NOT NULL,
    message_id TEXT,
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    state TEXT NOT NULL,
    blob_hash TEXT NOT NULL,
    PRIMARY KEY(account,id),
    FOREIGN KEY(account) REFERENCES accounts(id),
    FOREIGN KEY(account,message_id) REFERENCES messages(account,id)
);
INSERT INTO attachments_next SELECT account,id,message_id,name,mime_type,size,state,blob_hash FROM attachments;
DROP TABLE attachments;
ALTER TABLE attachments_next RENAME TO attachments;
CREATE INDEX attachments_message ON attachments(account,message_id);
