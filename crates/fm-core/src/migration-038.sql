-- Resolve received attachments from original MIME while preserving authored blobs.
CREATE TABLE attachment_sources(
    account TEXT NOT NULL,
    id TEXT NOT NULL,
    source_hash TEXT NOT NULL CHECK(length(source_hash)=64),
    part_index INTEGER NOT NULL CHECK(part_index>=0),
    PRIMARY KEY(account,id),
    FOREIGN KEY(account,id) REFERENCES attachments(account,id) ON DELETE CASCADE
) STRICT;
