-- Version four Calendar intents use the existing operation journal. Old engines
-- must refuse these profiles before interpreting a partially completed split.
CREATE TABLE calendar_reminder_receipts (
 account TEXT NOT NULL,
 calendar_id TEXT NOT NULL,
 occurrence_id TEXT NOT NULL,
 start TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('shown','dismissed')),
 recorded_at INTEGER NOT NULL,
 PRIMARY KEY(account,calendar_id,occurrence_id,start)
);

-- Bounded history of the first step of a following operation, in the same journal.
ALTER TABLE calendar_write_operations ADD COLUMN recurring_steps_json TEXT;
