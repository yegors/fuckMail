-- Delivery metadata extends the existing exact local operation receipts.
ALTER TABLE messages ADD COLUMN trashed INTEGER NOT NULL DEFAULT 0 CHECK(trashed IN (0,1));
UPDATE messages SET trashed=1 WHERE EXISTS (
 SELECT 1 FROM google_message_state s,json_each(s.provider_labels_json) l
 WHERE s.account=messages.account AND s.message_id=messages.id AND l.value='TRASH'
);
CREATE TABLE google_mail_mutations(
 account TEXT NOT NULL,request_id TEXT NOT NULL,message_id TEXT NOT NULL,remote_id TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('modify','trash','restore')),desired_json TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','in_flight','reconcile','confirmed','superseded','failed')),
 attempt INTEGER NOT NULL DEFAULT 0,connection_generation INTEGER,
 retry_at_ms INTEGER NOT NULL DEFAULT 0,error_code TEXT,
 PRIMARY KEY(account,request_id,message_id),
 FOREIGN KEY(account,request_id) REFERENCES operations(account,request_id)
);
CREATE INDEX google_mail_mutations_ready ON google_mail_mutations(account,state,retry_at_ms);
