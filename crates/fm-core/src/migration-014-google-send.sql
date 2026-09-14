-- A schema boundary prevents an old engine from interpreting submissions as
-- draft uploads. Existing absent Google operation kind continues to mean draft;
-- no old journal JSON, receipt or blob is rewritten by this migration.
DROP INDEX google_drafts_due;
CREATE INDEX google_drafts_due ON drafts(account,updated_at,draft_id)
WHERE discarded=0
AND COALESCE(json_extract(google_json,'$.submission.blocks_drafts'),0)=0
AND (google_json IS NULL OR (json_extract(google_json,'$.state') IN ('local','synced','prepared','acknowledged')
AND (json_extract(google_json,'$.state') IN ('prepared','acknowledged') OR revision>COALESCE(json_extract(google_json,'$.baseline.draft_revision'),0)))
OR (json_extract(google_json,'$.state')='failed' AND revision>COALESCE(json_extract(google_json,'$.preparation_failure.draft_revision'),json_extract(google_json,'$.failed_draft_revision'))));
CREATE INDEX google_send_recovery ON capture_operations(account,id)
WHERE execution_kind='google' AND json_extract(json,'$.kind')='send' AND json_extract(json,'$.state')='in_flight';
