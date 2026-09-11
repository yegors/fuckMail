BEGIN IMMEDIATE;
CREATE TABLE capture_operations(account TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,blob_hash TEXT NOT NULL,PRIMARY KEY(account,id),FOREIGN KEY(account) REFERENCES accounts(id));
PRAGMA user_version=2;
COMMIT;
