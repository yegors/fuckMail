-- Provider catalog metadata only. Categories, assignments and human receipts
-- continue using their existing tables. Unpublished generations stay invisible.
CREATE TABLE google_label_catalogs (
    account TEXT PRIMARY KEY REFERENCES google_accounts(account),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
    generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0),
    next_generation INTEGER NOT NULL DEFAULT 0 CHECK(next_generation>=0),
    connection_generation INTEGER NOT NULL DEFAULT 0,
    observed_at_ms INTEGER NOT NULL DEFAULT 0,
    pending_generation INTEGER,
    pending_connection_generation INTEGER,
    pending_fingerprint TEXT,
    pending_count INTEGER NOT NULL DEFAULT 0 CHECK(pending_count BETWEEN 0 AND 10000),
    pending_offset INTEGER NOT NULL DEFAULT 0 CHECK(pending_offset BETWEEN 0 AND 10000)
);
CREATE TABLE google_label_catalog_items (
    account TEXT NOT NULL REFERENCES google_accounts(account),
    generation INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    label_id TEXT NOT NULL,
    name TEXT NOT NULL,
    label_type TEXT NOT NULL CHECK(label_type IN ('user','system')),
    PRIMARY KEY(account,generation,label_id),
    UNIQUE(account,generation,ordinal)
);
