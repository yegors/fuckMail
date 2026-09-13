-- Live work metadata shares existing messages, FTS, attachments and intent tables.
ALTER TABLE google_accounts ADD COLUMN mail_generation INTEGER NOT NULL DEFAULT 1 CHECK(mail_generation>0);
CREATE TABLE google_live_lanes(
 account TEXT NOT NULL, lane TEXT NOT NULL CHECK(lane IN ('recent','inventory','delta')),
 generation INTEGER NOT NULL, checkpoint_json TEXT NOT NULL, query_json TEXT NOT NULL,
 complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0,1)),
 PRIMARY KEY(account,lane), FOREIGN KEY(account) REFERENCES google_accounts(account)
);
CREATE TABLE google_live_pages(
 account TEXT NOT NULL, lane TEXT NOT NULL, generation INTEGER NOT NULL, expected_revision INTEGER NOT NULL,
 connection_generation INTEGER NOT NULL, fingerprint TEXT NOT NULL, plan_json TEXT NOT NULL,
 complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0,1)),
 PRIMARY KEY(account,lane,generation,expected_revision), FOREIGN KEY(account) REFERENCES google_accounts(account)
);
CREATE TABLE google_live_page_items(
 account TEXT NOT NULL, lane TEXT NOT NULL, generation INTEGER NOT NULL, expected_revision INTEGER NOT NULL,
 remote_id TEXT NOT NULL, ordinal INTEGER NOT NULL, required_action TEXT NOT NULL CHECK(required_action IN ('fetch','delete','observe')),
 manifest_json TEXT, manifest_hash TEXT, part_key TEXT, applied_revision INTEGER,
 PRIMARY KEY(account,lane,generation,expected_revision,remote_id),
 FOREIGN KEY(account,lane,generation,expected_revision) REFERENCES google_live_pages(account,lane,generation,expected_revision)
);
CREATE INDEX google_live_unfinished ON google_live_pages(account,generation,complete);
CREATE INDEX google_live_pending ON google_live_page_items(account,lane,generation,expected_revision,applied_revision,ordinal);
CREATE TABLE google_mail_coverage(
 account TEXT NOT NULL, remote_id TEXT NOT NULL, last_inventory_generation INTEGER, observed_generation INTEGER NOT NULL,
 remote_thread_id TEXT, history_id TEXT, source_state TEXT NOT NULL, index_state TEXT NOT NULL,
 attachment_state TEXT NOT NULL, source_hash TEXT, source_size INTEGER, issue TEXT,
 internal_date_ms INTEGER, deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1)),
 PRIMARY KEY(account,remote_id), FOREIGN KEY(account) REFERENCES google_accounts(account)
);
CREATE INDEX google_mail_coverage_scan ON google_mail_coverage(account,last_inventory_generation,deleted);
CREATE INDEX google_mail_reconcile ON google_mail_coverage(account,deleted,observed_generation,remote_id);
CREATE INDEX google_mail_gaps ON google_mail_coverage(account,remote_id) WHERE issue IS NOT NULL AND deleted=0;
CREATE TABLE google_mail_metrics(
 account TEXT PRIMARY KEY, observed INTEGER NOT NULL DEFAULT 0,
 retained_sources INTEGER NOT NULL DEFAULT 0, indexed INTEGER NOT NULL DEFAULT 0,
 complete_attachments INTEGER NOT NULL DEFAULT 0, unavailable INTEGER NOT NULL DEFAULT 0,
 FOREIGN KEY(account) REFERENCES google_accounts(account)
);
