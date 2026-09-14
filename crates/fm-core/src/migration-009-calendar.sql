ALTER TABLE calendar_sync_state ADD COLUMN published_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_sync_state ADD COLUMN full_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_sync_state ADD COLUMN permission_epoch INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_sync_state ADD COLUMN permission_observation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_sync_state ADD COLUMN retained_errors INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_sources ADD COLUMN source_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_sources ADD COLUMN pending_generation INTEGER;
ALTER TABLE calendar_sources ADD COLUMN pending_json TEXT;
ALTER TABLE calendar_event_state ADD COLUMN seen_full_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_event_state ADD COLUMN pending_seen_full_generation INTEGER;
ALTER TABLE calendar_event_state ADD COLUMN pending_visible INTEGER;
ALTER TABLE calendar_event_state ADD COLUMN pending_observed_json TEXT;

CREATE TABLE calendar_publication(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL);
INSERT INTO calendar_publication VALUES(1,0);
CREATE TABLE calendar_catalog(account TEXT NOT NULL,calendar_id TEXT NOT NULL,json TEXT NOT NULL,deleted INTEGER NOT NULL,observed_epoch INTEGER NOT NULL,PRIMARY KEY(account,calendar_id),FOREIGN KEY(account) REFERENCES accounts(id));
CREATE TABLE calendar_catalog_state(account TEXT PRIMARY KEY,sync_token TEXT,revision INTEGER NOT NULL DEFAULT 0,complete INTEGER NOT NULL DEFAULT 0,full_generation INTEGER NOT NULL DEFAULT 0,FOREIGN KEY(account) REFERENCES accounts(id));
CREATE TABLE calendar_sync_runs(id INTEGER PRIMARY KEY AUTOINCREMENT,account TEXT NOT NULL,calendar_id TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('events','catalog')),mode TEXT NOT NULL CHECK(mode IN ('full','delta')),state TEXT NOT NULL CHECK(state IN ('staging','published','expired')),query_fingerprint TEXT NOT NULL,base_revision INTEGER NOT NULL,permission_epoch INTEGER NOT NULL,connection_generation INTEGER NOT NULL,projection_epoch INTEGER NOT NULL,input_sync_token TEXT,next_page_token TEXT,final_sync_token TEXT,identity_complete INTEGER NOT NULL DEFAULT 1,errors INTEGER NOT NULL DEFAULT 0,FOREIGN KEY(account) REFERENCES accounts(id));
CREATE UNIQUE INDEX calendar_one_staging_run ON calendar_sync_runs(account,calendar_id,kind) WHERE state='staging';
CREATE TABLE calendar_sync_pages(run INTEGER NOT NULL,page_id TEXT NOT NULL,input_cursor TEXT NOT NULL,item_count INTEGER NOT NULL,manifest TEXT NOT NULL,next_page_token TEXT,next_sync_token TEXT,sealed INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(run,page_id),UNIQUE(run,input_cursor),FOREIGN KEY(run) REFERENCES calendar_sync_runs(id));
CREATE TABLE calendar_sync_items(run INTEGER NOT NULL,page_id TEXT NOT NULL,ordinal INTEGER NOT NULL,event_id TEXT,json TEXT NOT NULL,fingerprint TEXT NOT NULL,error_code TEXT,PRIMARY KEY(run,page_id,ordinal),FOREIGN KEY(run,page_id) REFERENCES calendar_sync_pages(run,page_id));
ALTER TABLE calendar_sync_items ADD COLUMN catalog_applied INTEGER NOT NULL DEFAULT 0;
CREATE INDEX calendar_sync_item_identity ON calendar_sync_items(run,event_id);
CREATE INDEX calendar_sync_catalog_pending ON calendar_sync_items(run,catalog_applied,error_code,page_id,ordinal);
CREATE INDEX calendar_sources_pending ON calendar_sources(account,calendar_id,pending_generation,event_id) WHERE pending_generation IS NOT NULL;
CREATE INDEX calendar_sources_generation ON calendar_sources(account,calendar_id,source_generation,event_id);
CREATE TABLE calendar_event_index(account TEXT NOT NULL,calendar_id TEXT NOT NULL,event_id TEXT NOT NULL,slot INTEGER NOT NULL CHECK(slot IN (0,1)),generation INTEGER NOT NULL,start_ms INTEGER,end_ms INTEGER,start_date TEXT,end_date TEXT,series_id TEXT,original_ms INTEGER,original_date TEXT,recurring INTEGER NOT NULL,cancelled INTEGER NOT NULL,PRIMARY KEY(account,calendar_id,event_id,slot),FOREIGN KEY(account,calendar_id,event_id) REFERENCES calendar_sources(account,calendar_id,event_id));
CREATE INDEX calendar_index_timed ON calendar_event_index(account,calendar_id,start_ms,end_ms);
CREATE INDEX calendar_index_timed_end ON calendar_event_index(account,calendar_id,end_ms,start_ms);
CREATE INDEX calendar_index_dates ON calendar_event_index(account,calendar_id,start_date,end_date);
CREATE INDEX calendar_index_dates_end ON calendar_event_index(account,calendar_id,end_date,start_date);
CREATE INDEX calendar_index_masters ON calendar_event_index(account,calendar_id,recurring,event_id);
CREATE INDEX calendar_index_series ON calendar_event_index(account,calendar_id,series_id,event_id);
INSERT INTO calendar_event_index SELECT account,calendar_id,event_id,0,0,
 CAST(strftime('%s',json_extract(json,'$.start.dateTime')) AS INTEGER)*1000,
 CAST(strftime('%s',json_extract(json,'$.end.dateTime')) AS INTEGER)*1000+999,
 json_extract(json,'$.start.date'),json_extract(json,'$.end.date'),json_extract(json,'$.recurringEventId'),
 CAST(strftime('%s',json_extract(json,'$.originalStartTime.dateTime')) AS INTEGER)*1000,
 json_extract(json,'$.originalStartTime.date'),COALESCE(json_array_length(json_extract(json,'$.recurrence')),0)>0,COALESCE(json_extract(json,'$.status')='cancelled',0)
 FROM calendar_sources WHERE json_valid(json);
CREATE TABLE calendar_busy_snapshots(account TEXT NOT NULL,calendar_id TEXT NOT NULL,from_ms INTEGER NOT NULL,to_ms INTEGER NOT NULL,observed_at INTEGER NOT NULL,permission_epoch INTEGER NOT NULL,json TEXT NOT NULL,available INTEGER NOT NULL,PRIMARY KEY(account,calendar_id,from_ms,to_ms),FOREIGN KEY(account,calendar_id) REFERENCES calendar_sync_state(account,calendar_id));

CREATE VIEW calendar_visible_sources AS SELECT s.account,s.calendar_id,s.event_id,
 CASE WHEN s.pending_generation<=c.published_generation THEN s.pending_json ELSE s.json END AS json,
 CASE WHEN s.pending_generation<=c.published_generation THEN 1 ELSE 0 END AS slot,
 CASE WHEN s.pending_generation<=c.published_generation THEN s.pending_generation ELSE s.source_generation END AS generation
 FROM calendar_sources s JOIN calendar_sync_state c USING(account,calendar_id)
 LEFT JOIN calendar_event_state e USING(account,calendar_id,event_id)
 WHERE CASE WHEN s.pending_generation<=c.published_generation THEN s.pending_generation ELSE s.source_generation END<=c.published_generation
 AND CASE WHEN s.pending_generation<=c.published_generation THEN e.pending_visible ELSE COALESCE(e.visible,1) END=1
 AND CASE WHEN s.pending_generation<=c.published_generation THEN e.pending_seen_full_generation ELSE COALESCE(e.seen_full_generation,0) END>=c.full_generation;

-- Existing synthetic wrappers and acknowledged writes retain the same source table.
-- Live staging uses its separate slot and overwrites these conservative SQL bounds
-- with normalized millisecond/time-zone bounds before the chunk commits.
CREATE TRIGGER calendar_source_index_insert AFTER INSERT ON calendar_sources BEGIN
 INSERT INTO calendar_event_index SELECT NEW.account,NEW.calendar_id,NEW.event_id,0,NEW.source_generation,
 CAST(strftime('%s',json_extract(NEW.json,'$.start.dateTime')) AS INTEGER)*1000,
 CAST(strftime('%s',json_extract(NEW.json,'$.end.dateTime')) AS INTEGER)*1000+999,
 json_extract(NEW.json,'$.start.date'),json_extract(NEW.json,'$.end.date'),json_extract(NEW.json,'$.recurringEventId'),
 CAST(strftime('%s',json_extract(NEW.json,'$.originalStartTime.dateTime')) AS INTEGER)*1000,
 json_extract(NEW.json,'$.originalStartTime.date'),COALESCE(json_array_length(json_extract(NEW.json,'$.recurrence')),0)>0,COALESCE(json_extract(NEW.json,'$.status')='cancelled',0);
END;
CREATE TRIGGER calendar_source_index_update AFTER UPDATE OF json ON calendar_sources WHEN OLD.pending_generation IS NULL BEGIN
 UPDATE calendar_event_index SET generation=NEW.source_generation,
 start_ms=CAST(strftime('%s',json_extract(NEW.json,'$.start.dateTime')) AS INTEGER)*1000,
 end_ms=CAST(strftime('%s',json_extract(NEW.json,'$.end.dateTime')) AS INTEGER)*1000+999,
 start_date=json_extract(NEW.json,'$.start.date'),end_date=json_extract(NEW.json,'$.end.date'),series_id=json_extract(NEW.json,'$.recurringEventId'),
 original_ms=CAST(strftime('%s',json_extract(NEW.json,'$.originalStartTime.dateTime')) AS INTEGER)*1000,
 original_date=json_extract(NEW.json,'$.originalStartTime.date'),recurring=COALESCE(json_array_length(json_extract(NEW.json,'$.recurrence')),0)>0,cancelled=COALESCE(json_extract(NEW.json,'$.status')='cancelled',0)
 WHERE account=NEW.account AND calendar_id=NEW.calendar_id AND event_id=NEW.event_id AND slot=0;
END;

CREATE TRIGGER calendar_scope_insert_published AFTER INSERT ON calendar_sync_state BEGIN UPDATE calendar_publication SET revision=revision+1; END;
CREATE TRIGGER calendar_scope_update_published AFTER UPDATE ON calendar_sync_state BEGIN UPDATE calendar_publication SET revision=revision+1; END;
CREATE TRIGGER calendar_source_insert_published AFTER INSERT ON calendar_sources WHEN NEW.source_generation<=(SELECT published_generation FROM calendar_sync_state WHERE account=NEW.account AND calendar_id=NEW.calendar_id) BEGIN UPDATE calendar_publication SET revision=revision+1; END;
CREATE TRIGGER calendar_source_update_published AFTER UPDATE OF json ON calendar_sources WHEN OLD.pending_generation IS NULL AND NEW.source_generation<=(SELECT published_generation FROM calendar_sync_state WHERE account=NEW.account AND calendar_id=NEW.calendar_id) BEGIN UPDATE calendar_publication SET revision=revision+1; END;
