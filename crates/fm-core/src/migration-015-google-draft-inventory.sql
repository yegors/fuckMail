ALTER TABLE drafts ADD COLUMN origin TEXT NOT NULL DEFAULT 'local' CHECK(origin IN ('local','google_snapshot'));
CREATE TABLE google_draft_inventory(
 account TEXT PRIMARY KEY,
 state_json TEXT NOT NULL CHECK(json_valid(state_json) AND length(CAST(state_json AS BLOB))<=65536),
 FOREIGN KEY(account) REFERENCES google_accounts(account)
);
CREATE TABLE google_draft_sources(
 account TEXT NOT NULL,source_revision INTEGER NOT NULL CHECK(source_revision>0),
 provider_draft_id TEXT NOT NULL,draft_id TEXT NOT NULL,linked_local_draft_id TEXT,
 is_current INTEGER NOT NULL CHECK(is_current IN (0,1)),seen_generation INTEGER NOT NULL,
 observed_at INTEGER NOT NULL,last_observed_at INTEGER NOT NULL,
 metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json) AND length(CAST(metadata_json AS BLOB))<=65536),
 PRIMARY KEY(account,source_revision),
 FOREIGN KEY(account,draft_id) REFERENCES drafts(account,draft_id),
 FOREIGN KEY(account,linked_local_draft_id) REFERENCES drafts(account,draft_id)
);
CREATE UNIQUE INDEX google_draft_source_current ON google_draft_sources(account,provider_draft_id) WHERE is_current=1;
CREATE INDEX google_draft_source_history ON google_draft_sources(account,draft_id,source_revision DESC);
CREATE INDEX google_draft_source_linked ON google_draft_sources(account,linked_local_draft_id,source_revision DESC);
CREATE INDEX google_draft_current_owner ON google_draft_sources(account,draft_id,source_revision DESC) WHERE is_current=1;
CREATE INDEX google_draft_current_link ON google_draft_sources(account,linked_local_draft_id,source_revision DESC) WHERE is_current=1;
CREATE INDEX google_draft_known_target ON capture_operations(account,json_extract(json,'$.target')) WHERE execution_kind='google';
CREATE INDEX google_draft_known_ack ON capture_operations(account,json_extract(json,'$.ack.draft_id')) WHERE execution_kind='google';
CREATE INDEX google_send_known_target ON capture_operations(account,json_extract(json,'$.provider_draft_id')) WHERE execution_kind='google';
CREATE INDEX google_draft_baseline_target ON drafts(account,json_extract(google_json,'$.baseline.draft_id')) WHERE origin='local';
CREATE INDEX google_draft_unknown_create ON capture_operations(account,id,blob_hash)
WHERE execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft'
AND json_extract(json,'$.state')='acceptance_unknown' AND json_extract(json,'$.target') IS NULL AND json_extract(json,'$.ack') IS NULL;
DROP INDEX google_drafts_due;
CREATE INDEX google_drafts_due ON drafts(account,updated_at,draft_id)
WHERE discarded=0 AND origin='local'
AND COALESCE(json_extract(google_json,'$.submission.blocks_drafts'),0)=0
AND (google_json IS NULL OR (json_extract(google_json,'$.state') IN ('local','synced','prepared','acknowledged')
AND (json_extract(google_json,'$.state') IN ('prepared','acknowledged') OR revision>COALESCE(json_extract(google_json,'$.baseline.draft_revision'),0)))
OR (json_extract(google_json,'$.state')='failed' AND revision>COALESCE(json_extract(google_json,'$.preparation_failure.draft_revision'),json_extract(google_json,'$.failed_draft_revision'))));

CREATE INDEX google_draft_source_duplicate ON google_draft_sources(account,draft_id) WHERE is_current=1 AND linked_local_draft_id IS NOT NULL;
CREATE INDEX drafts_count ON drafts(account,discarded,origin);
