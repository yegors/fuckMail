-- Separate publication from current-inventory visibility. Only recurrence
-- expansion may additionally use a retained published cancelled child.
CREATE VIEW calendar_published_sources AS SELECT s.account,s.calendar_id,s.event_id,
 CASE WHEN s.pending_generation<=c.published_generation THEN s.pending_json ELSE s.json END AS json,
 CASE WHEN s.pending_generation<=c.published_generation THEN 1 ELSE 0 END AS slot,
 CASE WHEN s.pending_generation<=c.published_generation THEN s.pending_generation ELSE s.source_generation END AS generation,
 CASE WHEN s.pending_generation<=c.published_generation THEN e.pending_visible ELSE COALESCE(e.visible,1) END AS visible,
 CASE WHEN s.pending_generation<=c.published_generation THEN e.pending_seen_full_generation ELSE COALESCE(e.seen_full_generation,0) END AS seen_full_generation,
 c.full_generation
 FROM calendar_sources s JOIN calendar_sync_state c USING(account,calendar_id)
 LEFT JOIN calendar_event_state e USING(account,calendar_id,event_id)
 WHERE CASE WHEN s.pending_generation<=c.published_generation THEN s.pending_generation ELSE s.source_generation END<=c.published_generation;

DROP VIEW calendar_visible_sources;
CREATE VIEW calendar_visible_sources AS
 SELECT account,calendar_id,event_id,json,slot,generation
 FROM calendar_published_sources
 WHERE visible=1 AND seen_full_generation>=full_generation;
