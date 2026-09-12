CREATE TABLE account_metadata(account TEXT PRIMARY KEY,revision INTEGER NOT NULL DEFAULT 0,signature_text TEXT NOT NULL,FOREIGN KEY(account) REFERENCES accounts(id));
INSERT INTO account_metadata(account,signature_text) SELECT id,'—'||char(10)||name FROM accounts;
ALTER TABLE drafts ADD COLUMN discarded INTEGER NOT NULL DEFAULT 0;
ALTER TABLE drafts ADD COLUMN signature_text TEXT NOT NULL DEFAULT '';
ALTER TABLE drafts ADD COLUMN from_name TEXT NOT NULL DEFAULT '';
ALTER TABLE drafts ADD COLUMN identity_revision INTEGER NOT NULL DEFAULT 0;
UPDATE drafts SET signature_text=(SELECT signature_text FROM account_metadata WHERE account=drafts.account),from_name=(SELECT name FROM accounts WHERE id=drafts.account);
CREATE TABLE preferences(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL,json TEXT NOT NULL);
CREATE TABLE preference_operations(request_id TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,result_json TEXT NOT NULL);

CREATE INDEX messages_projection ON messages(account,conversation_id,sent_at,id,unread,starred,inbox,sent,source_hash);
CREATE INDEX attachments_message ON attachments(account,message_id);
