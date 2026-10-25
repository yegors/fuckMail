-- The name an account's mail is sent under: the one the person chose (empty
-- follows Gmail) and Gmail's own send-as name, refreshed by sync.
ALTER TABLE account_metadata ADD COLUMN from_name TEXT NOT NULL DEFAULT '';
ALTER TABLE account_metadata ADD COLUMN gmail_from_name TEXT NOT NULL DEFAULT '';
