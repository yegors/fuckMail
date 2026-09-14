-- Local draft twins share the immutable capture journal and content store.
ALTER TABLE drafts ADD COLUMN google_json TEXT CHECK(google_json IS NULL OR (json_valid(google_json) AND length(CAST(google_json AS BLOB)) <= 32768));
ALTER TABLE capture_operations ADD COLUMN execution_kind TEXT NOT NULL DEFAULT 'synthetic' CHECK(execution_kind IN ('synthetic','google'));
CREATE INDEX capture_execution_kind ON capture_operations(execution_kind,account);
CREATE INDEX google_drafts_due ON drafts(account,updated_at,draft_id)
WHERE discarded=0
AND (google_json IS NULL OR (json_extract(google_json,'$.state') IN ('local','synced','prepared','acknowledged')
AND (json_extract(google_json,'$.state') IN ('prepared','acknowledged') OR revision>COALESCE(json_extract(google_json,'$.baseline.draft_revision'),0)))
OR (json_extract(google_json,'$.state')='failed' AND revision>COALESCE(json_extract(google_json,'$.preparation_failure.draft_revision'),json_extract(google_json,'$.failed_draft_revision'))));
