-- Historical snapshots/receipts retain their exact bytes and local-only meaning.
ALTER TABLE draft_discards ADD COLUMN cleanup_state TEXT NOT NULL DEFAULT 'not_requested'
    CHECK(cleanup_state IN ('not_requested','waiting','pending','complete','blocked','cancelled'));
ALTER TABLE draft_discards ADD COLUMN cleanup_not_before_ms INTEGER;
ALTER TABLE draft_discards ADD COLUMN cleanup_request_id TEXT;
ALTER TABLE draft_discards ADD COLUMN cleanup_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE draft_discards ADD COLUMN cleanup_error_json TEXT;
CREATE INDEX draft_discard_cleanup_due ON draft_discards(account,cleanup_state,cleanup_not_before_ms);
CREATE UNIQUE INDEX draft_discard_cleanup_request ON draft_discards(account,cleanup_request_id)
    WHERE cleanup_request_id IS NOT NULL;
