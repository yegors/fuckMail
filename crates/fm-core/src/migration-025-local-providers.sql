ALTER TABLE message_sender_kinds RENAME TO message_sender_kinds_previous;
CREATE TABLE message_sender_kinds(account TEXT NOT NULL,message_id TEXT NOT NULL,source_hash TEXT NOT NULL CHECK(length(source_hash)=64),kind TEXT NOT NULL CHECK(kind IN ('people','automated','unknown')),provenance TEXT NOT NULL CHECK(provenance IN ('fixture','ollama','openai_compatible','manual')),PRIMARY KEY(account,message_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id) ON DELETE CASCADE);
INSERT INTO message_sender_kinds SELECT * FROM message_sender_kinds_previous;
DROP TABLE message_sender_kinds_previous;
