CREATE TABLE local_model_settings(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL,settings_json TEXT NOT NULL);
CREATE TABLE local_model_results(account TEXT NOT NULL,conversation_id TEXT NOT NULL,task TEXT NOT NULL,target TEXT NOT NULL,snapshot_json TEXT NOT NULL,result_json TEXT NOT NULL,PRIMARY KEY(account,conversation_id,task,target),FOREIGN KEY(account,conversation_id) REFERENCES conversations(account,id) ON DELETE CASCADE);
ALTER TABLE message_sender_kinds RENAME TO message_sender_kinds_old;
CREATE TABLE message_sender_kinds(account TEXT NOT NULL,message_id TEXT NOT NULL,source_hash TEXT NOT NULL CHECK(length(source_hash)=64),kind TEXT NOT NULL CHECK(kind IN ('people','automated','unknown')),provenance TEXT NOT NULL CHECK(provenance IN ('fixture','ollama','manual')),PRIMARY KEY(account,message_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id) ON DELETE CASCADE);
INSERT INTO message_sender_kinds SELECT * FROM message_sender_kinds_old;
DROP TABLE message_sender_kinds_old;
