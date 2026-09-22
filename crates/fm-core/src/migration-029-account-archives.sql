CREATE TABLE deleted_account_archives(account TEXT PRIMARY KEY,base_account TEXT NOT NULL,replacement_account TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,result_json TEXT NOT NULL);
CREATE INDEX deleted_account_base ON deleted_account_archives(base_account);
CREATE TABLE archive_blob_cleanup(hash TEXT PRIMARY KEY CHECK(length(hash)=64));
ALTER TABLE account_metadata ADD COLUMN mail_retention TEXT NOT NULL DEFAULT 'all' CHECK(mail_retention IN ('1_month','3_months','1_year','all'));
ALTER TABLE account_metadata ADD COLUMN mail_since INTEGER;
