CREATE TABLE draft_transfers(
    account TEXT NOT NULL,
    request_id TEXT NOT NULL,
    source_draft_id TEXT NOT NULL,
    target_account TEXT NOT NULL,
    target_draft_id TEXT NOT NULL,
    source_lifecycle INTEGER NOT NULL,
    target_lifecycle INTEGER NOT NULL,
    keep_original INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('waiting_target','cleanup_pending','complete','blocked')),
    source_google_json TEXT NOT NULL,
    cleanup_request_id TEXT,
    cleanup_generation INTEGER NOT NULL DEFAULT 0,
    error_json TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(account,request_id),
    FOREIGN KEY(account,source_draft_id) REFERENCES drafts(account,draft_id),
    FOREIGN KEY(target_account,target_draft_id) REFERENCES drafts(account,draft_id)
);
CREATE INDEX draft_transfers_source ON draft_transfers(account,source_draft_id);
CREATE INDEX draft_transfers_target ON draft_transfers(target_account,target_draft_id);
CREATE INDEX draft_transfers_pending ON draft_transfers(account,state,created_at);
