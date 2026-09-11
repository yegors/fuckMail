ALTER TABLE drafts ADD COLUMN body_html TEXT;
ALTER TABLE drafts ADD COLUMN attachments_json TEXT NOT NULL DEFAULT '[]';
