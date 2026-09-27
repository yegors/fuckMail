-- Messages once refused as too large, or for listing more attachments than
-- were accepted, now fit. Marking them unseen this generation makes the next
-- reconciliation pass fetch or re-prepare each one.
UPDATE google_mail_coverage SET observed_generation=0
 WHERE deleted=0 AND issue IN ('item_too_large','attachment_limit');
