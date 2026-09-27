-- A Gmail draft's source row is written before the local draft that mirrors
-- it, so only its link to an existing local draft stays a foreign key.
CREATE TABLE google_draft_sources_next(
 account TEXT NOT NULL,source_revision INTEGER NOT NULL CHECK(source_revision>0),
 provider_draft_id TEXT NOT NULL,draft_id TEXT NOT NULL,linked_local_draft_id TEXT,
 is_current INTEGER NOT NULL CHECK(is_current IN (0,1)),seen_generation INTEGER NOT NULL,
 observed_at INTEGER NOT NULL,last_observed_at INTEGER NOT NULL,
 metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json) AND length(CAST(metadata_json AS BLOB))<=65536),
 PRIMARY KEY(account,source_revision),
 FOREIGN KEY(account,linked_local_draft_id) REFERENCES drafts(account,draft_id)
) STRICT;
INSERT INTO google_draft_sources_next(account,source_revision,provider_draft_id,draft_id,linked_local_draft_id,is_current,seen_generation,observed_at,last_observed_at,metadata_json)
 SELECT account,source_revision,provider_draft_id,draft_id,linked_local_draft_id,is_current,seen_generation,observed_at,last_observed_at,metadata_json FROM google_draft_sources;
DROP TABLE google_draft_sources;
ALTER TABLE google_draft_sources_next RENAME TO google_draft_sources;
CREATE INDEX google_draft_current_link ON google_draft_sources(account,linked_local_draft_id,source_revision DESC) WHERE is_current=1;
CREATE INDEX google_draft_current_owner ON google_draft_sources(account,draft_id,source_revision DESC) WHERE is_current=1;
CREATE UNIQUE INDEX google_draft_source_current ON google_draft_sources(account,provider_draft_id) WHERE is_current=1;
CREATE INDEX google_draft_source_duplicate ON google_draft_sources(account,draft_id) WHERE is_current=1 AND linked_local_draft_id IS NOT NULL;
CREATE INDEX google_draft_source_history ON google_draft_sources(account,draft_id,source_revision DESC);
CREATE INDEX google_draft_source_linked ON google_draft_sources(account,linked_local_draft_id,source_revision DESC);
