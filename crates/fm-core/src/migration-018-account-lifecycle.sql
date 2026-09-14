-- Retain identity and every existing journal byte. Cleanup is current metadata;
-- removal commands use the existing immutable operations journal.
ALTER TABLE accounts ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active'
    CHECK(lifecycle_state IN ('active','removed'));
ALTER TABLE accounts ADD COLUMN lifecycle_revision INTEGER NOT NULL DEFAULT 1
    CHECK(lifecycle_revision > 0);
ALTER TABLE accounts ADD COLUMN cleanup_json TEXT
    CHECK(cleanup_json IS NULL OR (json_valid(cleanup_json) AND length(cleanup_json)<=4096));
CREATE INDEX accounts_active ON accounts(lifecycle_state,id);
