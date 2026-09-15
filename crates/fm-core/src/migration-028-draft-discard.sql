ALTER TABLE drafts ADD COLUMN recipient_inputs_json TEXT NOT NULL DEFAULT '{}';
CREATE TABLE draft_discards(
    account TEXT NOT NULL,
    request_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    discarded_revision INTEGER NOT NULL CHECK(discarded_revision > 0),
    lifecycle_revision INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    PRIMARY KEY(account,request_id),
    FOREIGN KEY(account,draft_id) REFERENCES drafts(account,draft_id)
);
