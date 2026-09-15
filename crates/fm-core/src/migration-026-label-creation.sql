-- Retain accepted/uncertain provider identities if the local category is removed.
CREATE TABLE google_label_creations (
    account TEXT NOT NULL REFERENCES accounts(id),
    category_id TEXT NOT NULL,
    name TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('unknown','created','rejected')),
    provider_label_id TEXT,
    error_code TEXT,
    PRIMARY KEY(account,category_id)
);
