-- Direct fresh-profile schema. Keep this in sync with the reviewed storage baseline.
-- Schema version and profile-kind markers are committed by Store::open_profile.

-- tables
CREATE TABLE account_metadata(account TEXT PRIMARY KEY,revision INTEGER NOT NULL DEFAULT 0,signature_text TEXT NOT NULL, mail_retention TEXT NOT NULL DEFAULT 'all' CHECK(mail_retention IN ('1_month','3_months','90_days','1_year','2_years','5_years','all')), mail_since INTEGER,FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE accounts(id TEXT PRIMARY KEY,name TEXT NOT NULL,email TEXT NOT NULL,provider TEXT NOT NULL, lifecycle_state TEXT NOT NULL DEFAULT 'active'
    CHECK(lifecycle_state IN ('active','removed')), lifecycle_revision INTEGER NOT NULL DEFAULT 1
    CHECK(lifecycle_revision > 0), cleanup_json TEXT
    CHECK(cleanup_json IS NULL OR (json_valid(cleanup_json) AND length(cleanup_json)<=4096))) STRICT;
CREATE TABLE archive_blob_cleanup(hash TEXT PRIMARY KEY CHECK(length(hash)=64)) STRICT;
CREATE TABLE assist_category_intents(account TEXT NOT NULL,message_id TEXT NOT NULL,category_id TEXT NOT NULL,value INTEGER NOT NULL,revision INTEGER NOT NULL,request_id TEXT NOT NULL,PRIMARY KEY(account,message_id,category_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id),FOREIGN KEY(account,category_id) REFERENCES categories(account,id)) STRICT;
CREATE TABLE assist_policies(account TEXT PRIMARY KEY,revision INTEGER NOT NULL,mode TEXT NOT NULL,summaries INTEGER NOT NULL,categories_json TEXT NOT NULL,FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE assist_results(account TEXT NOT NULL,request_id TEXT NOT NULL,conversation_id TEXT NOT NULL,request_fingerprint TEXT NOT NULL,snapshot_json TEXT NOT NULL,output_sha256 TEXT NOT NULL,result_json TEXT NOT NULL,PRIMARY KEY(account,request_id),FOREIGN KEY(account,conversation_id) REFERENCES conversations(account,id)) STRICT;
CREATE TABLE "attachments"(
    account TEXT NOT NULL,
    id TEXT NOT NULL,
    message_id TEXT,
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    state TEXT NOT NULL,
    blob_hash TEXT NOT NULL,
    PRIMARY KEY(account,id),
    FOREIGN KEY(account) REFERENCES accounts(id),
    FOREIGN KEY(account,message_id) REFERENCES messages(account,id)
) STRICT;
CREATE TABLE calendar_batch_commits(account TEXT NOT NULL,calendar_id TEXT NOT NULL,expected_revision INTEGER NOT NULL,fingerprint TEXT NOT NULL,result_json TEXT NOT NULL,PRIMARY KEY(account,calendar_id,expected_revision),FOREIGN KEY(account,calendar_id) REFERENCES calendar_sync_state(account,calendar_id)) STRICT;
CREATE TABLE calendar_busy_snapshots(account TEXT NOT NULL,calendar_id TEXT NOT NULL,from_ms INTEGER NOT NULL,to_ms INTEGER NOT NULL,observed_at INTEGER NOT NULL,permission_epoch INTEGER NOT NULL,json TEXT NOT NULL,available INTEGER NOT NULL,PRIMARY KEY(account,calendar_id,from_ms,to_ms),FOREIGN KEY(account,calendar_id) REFERENCES calendar_sync_state(account,calendar_id)) STRICT;
CREATE TABLE calendar_catalog(account TEXT NOT NULL,calendar_id TEXT NOT NULL,json TEXT NOT NULL,deleted INTEGER NOT NULL,observed_epoch INTEGER NOT NULL,PRIMARY KEY(account,calendar_id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE calendar_catalog_state(account TEXT PRIMARY KEY,sync_token TEXT,revision INTEGER NOT NULL DEFAULT 0,complete INTEGER NOT NULL DEFAULT 0,full_generation INTEGER NOT NULL DEFAULT 0,FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE calendar_event_index(account TEXT NOT NULL,calendar_id TEXT NOT NULL,event_id TEXT NOT NULL,slot INTEGER NOT NULL CHECK(slot IN (0,1)),generation INTEGER NOT NULL,start_ms INTEGER,end_ms INTEGER,start_date TEXT,end_date TEXT,series_id TEXT,original_ms INTEGER,original_date TEXT,recurring INTEGER NOT NULL,cancelled INTEGER NOT NULL,PRIMARY KEY(account,calendar_id,event_id,slot),FOREIGN KEY(account,calendar_id,event_id) REFERENCES calendar_sources(account,calendar_id,event_id)) STRICT;
CREATE TABLE calendar_event_state(account TEXT NOT NULL,calendar_id TEXT NOT NULL,event_id TEXT NOT NULL,visible INTEGER NOT NULL DEFAULT 1,observed_json TEXT NOT NULL, seen_full_generation INTEGER NOT NULL DEFAULT 0, pending_seen_full_generation INTEGER, pending_visible INTEGER, pending_observed_json TEXT,PRIMARY KEY(account,calendar_id,event_id),FOREIGN KEY(account,calendar_id,event_id) REFERENCES calendar_sources(account,calendar_id,event_id)) STRICT;
CREATE TABLE calendar_publication(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL) STRICT;
CREATE TABLE calendar_reminder_receipts (
 account TEXT NOT NULL,
 calendar_id TEXT NOT NULL,
 occurrence_id TEXT NOT NULL,
 start TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('shown','dismissed')),
 recorded_at INTEGER NOT NULL,
 PRIMARY KEY(account,calendar_id,occurrence_id,start)
) STRICT;
CREATE TABLE calendar_sources(account TEXT NOT NULL,calendar_id TEXT NOT NULL,event_id TEXT NOT NULL,json TEXT NOT NULL, source_generation INTEGER NOT NULL DEFAULT 0, pending_generation INTEGER, pending_json TEXT,PRIMARY KEY(account,calendar_id,event_id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE calendar_sync_items(run INTEGER NOT NULL,page_id TEXT NOT NULL,ordinal INTEGER NOT NULL,event_id TEXT,json TEXT,fingerprint TEXT NOT NULL,error_code TEXT, catalog_applied INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(run,page_id,ordinal),FOREIGN KEY(run,page_id) REFERENCES calendar_sync_pages(run,page_id)) STRICT;
CREATE TABLE calendar_sync_pages(run INTEGER NOT NULL,page_id TEXT NOT NULL,input_cursor TEXT NOT NULL,item_count INTEGER NOT NULL,manifest TEXT NOT NULL,next_page_token TEXT,next_sync_token TEXT,sealed INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(run,page_id),UNIQUE(run,input_cursor),FOREIGN KEY(run) REFERENCES calendar_sync_runs(id)) STRICT;
CREATE TABLE calendar_sync_runs(id INTEGER PRIMARY KEY AUTOINCREMENT,account TEXT NOT NULL,calendar_id TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('events','catalog')),mode TEXT NOT NULL CHECK(mode IN ('full','delta')),state TEXT NOT NULL CHECK(state IN ('staging','published','expired')),query_fingerprint TEXT NOT NULL,base_revision INTEGER NOT NULL,permission_epoch INTEGER NOT NULL,connection_generation INTEGER NOT NULL,projection_epoch INTEGER NOT NULL,input_sync_token TEXT,next_page_token TEXT,final_sync_token TEXT,identity_complete INTEGER NOT NULL DEFAULT 1,errors INTEGER NOT NULL DEFAULT 0,FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE calendar_sync_state(account TEXT NOT NULL,calendar_id TEXT NOT NULL,query_fingerprint TEXT NOT NULL,time_zone TEXT NOT NULL,access TEXT NOT NULL,sync_token TEXT,revision INTEGER NOT NULL, published_generation INTEGER NOT NULL DEFAULT 0, full_generation INTEGER NOT NULL DEFAULT 0, permission_epoch INTEGER NOT NULL DEFAULT 0, permission_observation INTEGER NOT NULL DEFAULT 0, retained_errors INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(account,calendar_id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE calendar_write_operations(account TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,intent_json TEXT NOT NULL,result_json TEXT, execution_mode TEXT NOT NULL DEFAULT 'local' CHECK(execution_mode IN ('local','live')), state TEXT NOT NULL DEFAULT 'prepared', operation_revision INTEGER NOT NULL DEFAULT 0, attempt INTEGER NOT NULL DEFAULT 0, connection_generation INTEGER, permission_epoch INTEGER, dispatch_json TEXT, outcome_json TEXT, recurring_steps_json TEXT,PRIMARY KEY(account,request_id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE capture_operations(account TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,blob_hash TEXT NOT NULL, execution_kind TEXT NOT NULL DEFAULT 'synthetic' CHECK(execution_kind IN ('synthetic','google')),PRIMARY KEY(account,id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE categories(account TEXT NOT NULL,id TEXT NOT NULL,name TEXT NOT NULL,PRIMARY KEY(account,id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE conversations(account TEXT NOT NULL,id TEXT NOT NULL,subject TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1,PRIMARY KEY(account,id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE deleted_account_archives(account TEXT PRIMARY KEY,base_account TEXT NOT NULL,replacement_account TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,result_json TEXT NOT NULL) STRICT;
CREATE TABLE draft_discards(
    account TEXT NOT NULL,
    request_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    discarded_revision INTEGER NOT NULL CHECK(discarded_revision > 0),
    lifecycle_revision INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    PRIMARY KEY(account,request_id),
    FOREIGN KEY(account,draft_id) REFERENCES drafts(account,draft_id)
) STRICT;
CREATE TABLE draft_transfers(
    account TEXT NOT NULL,
    request_id TEXT NOT NULL,
    source_draft_id TEXT NOT NULL,
    target_account TEXT NOT NULL,
    target_draft_id TEXT NOT NULL,
    source_lifecycle INTEGER NOT NULL,
    target_lifecycle INTEGER NOT NULL,
    keep_original INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('waiting_target','cleanup_pending','complete','blocked')),
    source_google_json TEXT NOT NULL,
    cleanup_request_id TEXT,
    cleanup_generation INTEGER NOT NULL DEFAULT 0,
    error_json TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(account,request_id),
    FOREIGN KEY(account,source_draft_id) REFERENCES drafts(account,draft_id),
    FOREIGN KEY(target_account,target_draft_id) REFERENCES drafts(account,draft_id)
) STRICT;
CREATE TABLE "drafts"(
    account TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    conversation_id TEXT,
    legacy_reply INTEGER NOT NULL DEFAULT 0,
    mode TEXT NOT NULL CHECK(mode IN ('new','reply','reply_all','forward')),
    source_message_id TEXT,
    source_hash TEXT,
    routing_json TEXT,
    body TEXT NOT NULL,
    revision INTEGER NOT NULL,
    body_html TEXT,
    attachments_json TEXT NOT NULL DEFAULT '[]',
    discarded INTEGER NOT NULL DEFAULT 0,
    signature_text TEXT NOT NULL DEFAULT '',
    from_name TEXT NOT NULL DEFAULT '',
    identity_revision INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0, google_json TEXT CHECK(google_json IS NULL OR (json_valid(google_json) AND length(CAST(google_json AS BLOB)) <= 32768)), origin TEXT NOT NULL DEFAULT 'local' CHECK(origin IN ('local','google_snapshot')), recipient_inputs_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY(account,draft_id),
    FOREIGN KEY(account) REFERENCES accounts(id),
    FOREIGN KEY(account,conversation_id) REFERENCES conversations(account,id)
) STRICT;
CREATE TABLE google_account_clients (
    account TEXT PRIMARY KEY REFERENCES google_accounts(account),
    client_id TEXT NOT NULL,
    config_path TEXT NOT NULL
) STRICT;
CREATE TABLE google_accounts(
    account TEXT PRIMARY KEY,
    google_subject TEXT NOT NULL UNIQUE,
    scopes_json TEXT NOT NULL,
    connection_state TEXT NOT NULL CHECK(connection_state IN ('connected','disconnected','reauthorization_required')),
    revision INTEGER NOT NULL DEFAULT 1, connection_generation INTEGER NOT NULL DEFAULT 1, mail_generation INTEGER NOT NULL DEFAULT 1 CHECK(mail_generation>0),
    FOREIGN KEY(account) REFERENCES accounts(id)
) STRICT;
CREATE TABLE google_category_labels(account TEXT NOT NULL,category_id TEXT NOT NULL,provider_label_id TEXT NOT NULL,PRIMARY KEY(account,category_id),UNIQUE(account,provider_label_id),FOREIGN KEY(account,category_id) REFERENCES categories(account,id)) STRICT;
CREATE TABLE google_checkpoints(account TEXT PRIMARY KEY,json TEXT NOT NULL,FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE google_draft_inventory(
 account TEXT PRIMARY KEY,
 state_json TEXT NOT NULL CHECK(json_valid(state_json) AND length(CAST(state_json AS BLOB))<=65536),
 FOREIGN KEY(account) REFERENCES google_accounts(account)
) STRICT;
CREATE TABLE google_draft_sources(
 account TEXT NOT NULL,source_revision INTEGER NOT NULL CHECK(source_revision>0),
 provider_draft_id TEXT NOT NULL,draft_id TEXT NOT NULL,linked_local_draft_id TEXT,
 is_current INTEGER NOT NULL CHECK(is_current IN (0,1)),seen_generation INTEGER NOT NULL,
 observed_at INTEGER NOT NULL,last_observed_at INTEGER NOT NULL,
 metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json) AND length(CAST(metadata_json AS BLOB))<=65536),
 PRIMARY KEY(account,source_revision),
 FOREIGN KEY(account,draft_id) REFERENCES drafts(account,draft_id),
 FOREIGN KEY(account,linked_local_draft_id) REFERENCES drafts(account,draft_id)
) STRICT;
CREATE TABLE google_label_catalog_items (
    account TEXT NOT NULL REFERENCES google_accounts(account),
    generation INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    label_id TEXT NOT NULL,
    name TEXT NOT NULL,
    label_type TEXT NOT NULL CHECK(label_type IN ('user','system')),
    PRIMARY KEY(account,generation,label_id),
    UNIQUE(account,generation,ordinal)
) STRICT;
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
) STRICT;
CREATE TABLE google_label_creations (
    account TEXT NOT NULL REFERENCES accounts(id),
    category_id TEXT NOT NULL,
    name TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('unknown','created','rejected')),
    provider_label_id TEXT,
    error_code TEXT,
    PRIMARY KEY(account,category_id)
) STRICT;
CREATE TABLE google_live_lanes(
 account TEXT NOT NULL, lane TEXT NOT NULL CHECK(lane IN ('recent','inventory','delta')),
 generation INTEGER NOT NULL, checkpoint_json TEXT NOT NULL, query_json TEXT NOT NULL,
 complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0,1)),
 PRIMARY KEY(account,lane), FOREIGN KEY(account) REFERENCES google_accounts(account)
) STRICT;
CREATE TABLE google_live_page_items(
 account TEXT NOT NULL, lane TEXT NOT NULL, generation INTEGER NOT NULL, expected_revision INTEGER NOT NULL,
 remote_id TEXT NOT NULL, ordinal INTEGER NOT NULL, required_action TEXT NOT NULL CHECK(required_action IN ('fetch','delete','observe')),
 manifest_json TEXT, manifest_hash TEXT, part_key TEXT, applied_revision INTEGER,
 PRIMARY KEY(account,lane,generation,expected_revision,remote_id),
 FOREIGN KEY(account,lane,generation,expected_revision) REFERENCES google_live_pages(account,lane,generation,expected_revision)
) STRICT;
CREATE TABLE google_live_pages(
 account TEXT NOT NULL, lane TEXT NOT NULL, generation INTEGER NOT NULL, expected_revision INTEGER NOT NULL,
 connection_generation INTEGER NOT NULL, fingerprint TEXT NOT NULL, plan_json TEXT NOT NULL,
 complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0,1)),
 PRIMARY KEY(account,lane,generation,expected_revision), FOREIGN KEY(account) REFERENCES google_accounts(account)
) STRICT;
CREATE TABLE google_mail_coverage(
 account TEXT NOT NULL, remote_id TEXT NOT NULL, last_inventory_generation INTEGER, observed_generation INTEGER NOT NULL,
 remote_thread_id TEXT, history_id TEXT, source_state TEXT NOT NULL, index_state TEXT NOT NULL,
 attachment_state TEXT NOT NULL, source_hash TEXT, source_size INTEGER, issue TEXT,
 internal_date_ms INTEGER, deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1)),
 PRIMARY KEY(account,remote_id), FOREIGN KEY(account) REFERENCES google_accounts(account)
) STRICT;
CREATE TABLE google_mail_metrics(
 account TEXT PRIMARY KEY, observed INTEGER NOT NULL DEFAULT 0,
 retained_sources INTEGER NOT NULL DEFAULT 0, indexed INTEGER NOT NULL DEFAULT 0,
 complete_attachments INTEGER NOT NULL DEFAULT 0, unavailable INTEGER NOT NULL DEFAULT 0,
 FOREIGN KEY(account) REFERENCES google_accounts(account)
) STRICT;
CREATE TABLE google_mail_mutations(
 account TEXT NOT NULL,request_id TEXT NOT NULL,message_id TEXT NOT NULL,remote_id TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('modify','trash','restore')),desired_json TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','in_flight','reconcile','confirmed','superseded','failed')),
 attempt INTEGER NOT NULL DEFAULT 0,connection_generation INTEGER,
 retry_at_ms INTEGER NOT NULL DEFAULT 0,error_code TEXT,
 PRIMARY KEY(account,request_id,message_id),
 FOREIGN KEY(account,request_id) REFERENCES operations(account,request_id)
) STRICT;
CREATE TABLE google_manual_intents(account TEXT NOT NULL,message_id TEXT NOT NULL,field TEXT NOT NULL,category_id TEXT NOT NULL DEFAULT '',value INTEGER NOT NULL,revision INTEGER NOT NULL,PRIMARY KEY(account,message_id,field,category_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id)) STRICT;
CREATE TABLE google_message_state(account TEXT NOT NULL,message_id TEXT NOT NULL,remote_thread_id TEXT NOT NULL,history_id TEXT NOT NULL,provider_labels_json TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(account,message_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id)) STRICT;
CREATE TABLE google_page_commits(account TEXT NOT NULL,generation INTEGER NOT NULL,expected_revision INTEGER NOT NULL,fingerprint TEXT NOT NULL,next_json TEXT NOT NULL,PRIMARY KEY(account,generation,expected_revision),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE google_staged_messages(account TEXT NOT NULL,generation INTEGER NOT NULL,remote_id TEXT NOT NULL,expected_revision INTEGER NOT NULL,json TEXT NOT NULL,source_hash TEXT NOT NULL,source_size INTEGER NOT NULL,PRIMARY KEY(account,generation,remote_id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE local_model_label_overrides(account TEXT NOT NULL,message_id TEXT NOT NULL,PRIMARY KEY(account,message_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id) ON DELETE CASCADE) STRICT;
CREATE TABLE local_model_results(account TEXT NOT NULL,conversation_id TEXT NOT NULL,task TEXT NOT NULL,target TEXT NOT NULL,snapshot_json TEXT NOT NULL,result_json TEXT NOT NULL,PRIMARY KEY(account,conversation_id,task,target),FOREIGN KEY(account,conversation_id) REFERENCES conversations(account,id) ON DELETE CASCADE) STRICT;
CREATE TABLE local_model_settings(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL,settings_json TEXT NOT NULL) STRICT;
CREATE TABLE local_model_settings_history(revision INTEGER PRIMARY KEY,settings_json TEXT NOT NULL) STRICT;
CREATE TABLE message_categories(account TEXT NOT NULL,message_id TEXT NOT NULL,category_id TEXT NOT NULL,PRIMARY KEY(account,message_id,category_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id),FOREIGN KEY(account,category_id) REFERENCES categories(account,id)) STRICT;
CREATE VIRTUAL TABLE message_fts USING fts5(sender,subject,body,tokenize='unicode61 remove_diacritics 2',prefix='2 3 4',content='',contentless_delete=1);
CREATE TABLE message_sender_kinds(account TEXT NOT NULL,message_id TEXT NOT NULL,source_hash TEXT NOT NULL CHECK(length(source_hash)=64),kind TEXT NOT NULL CHECK(kind IN ('people','automated','unknown')),provenance TEXT NOT NULL CHECK(provenance IN ('fixture','ollama','openai_compatible','manual')),PRIMARY KEY(account,message_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id) ON DELETE CASCADE) STRICT;
CREATE TABLE messages(rowid INTEGER PRIMARY KEY,account TEXT NOT NULL,id TEXT NOT NULL,conversation_id TEXT NOT NULL,remote_id TEXT NOT NULL,sender_name TEXT NOT NULL,sender_email TEXT NOT NULL,recipient TEXT NOT NULL,subject TEXT NOT NULL,body TEXT NOT NULL,sent_at TEXT NOT NULL,unread INTEGER NOT NULL,starred INTEGER NOT NULL,inbox INTEGER NOT NULL,sent INTEGER NOT NULL,source_hash TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1, trashed INTEGER NOT NULL DEFAULT 0 CHECK(trashed IN (0,1)),UNIQUE(account,id),UNIQUE(account,remote_id),FOREIGN KEY(account,conversation_id) REFERENCES conversations(account,id)) STRICT;
CREATE TABLE meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL) STRICT;
CREATE TABLE operations(account TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,result_json TEXT NOT NULL,PRIMARY KEY(account,request_id),FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;
CREATE TABLE preference_operations(request_id TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,result_json TEXT NOT NULL) STRICT;
CREATE TABLE preferences(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL,json TEXT NOT NULL) STRICT;
CREATE TABLE profile_identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),vault_namespace TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE views(account TEXT PRIMARY KEY,revision INTEGER NOT NULL,json TEXT NOT NULL,FOREIGN KEY(account) REFERENCES accounts(id)) STRICT;

-- indexs
CREATE INDEX accounts_active ON accounts(lifecycle_state,id);
CREATE INDEX attachments_message ON attachments(account,message_id);
CREATE INDEX calendar_index_dates ON calendar_event_index(account,calendar_id,start_date,end_date);
CREATE INDEX calendar_index_dates_end ON calendar_event_index(account,calendar_id,end_date,start_date);
CREATE INDEX calendar_index_masters ON calendar_event_index(account,calendar_id,recurring,event_id);
CREATE INDEX calendar_index_series ON calendar_event_index(account,calendar_id,series_id,event_id);
CREATE INDEX calendar_index_timed ON calendar_event_index(account,calendar_id,start_ms,end_ms);
CREATE INDEX calendar_index_timed_end ON calendar_event_index(account,calendar_id,end_ms,start_ms);
CREATE UNIQUE INDEX calendar_live_event_attempt ON calendar_write_operations(account,json_extract(intent_json,'$.target.calendar'),json_extract(intent_json,'$.target.event')) WHERE execution_mode='live' AND state IN ('in_flight','unknown');
CREATE INDEX calendar_live_write_pending ON calendar_write_operations(account,state,request_id) WHERE execution_mode='live';
CREATE UNIQUE INDEX calendar_one_staging_run ON calendar_sync_runs(account,calendar_id,kind) WHERE state='staging';
CREATE INDEX calendar_sources_generation ON calendar_sources(account,calendar_id,source_generation,event_id);
CREATE INDEX calendar_sources_pending ON calendar_sources(account,calendar_id,pending_generation,event_id) WHERE pending_generation IS NOT NULL;
CREATE INDEX calendar_sync_catalog_pending ON calendar_sync_items(run,catalog_applied,error_code,page_id,ordinal);
CREATE INDEX calendar_sync_item_identity ON calendar_sync_items(run,event_id);
CREATE INDEX capture_execution_kind ON capture_operations(execution_kind,account);
CREATE INDEX deleted_account_base ON deleted_account_archives(base_account);
CREATE INDEX draft_transfers_pending ON draft_transfers(account,state,created_at);
CREATE INDEX draft_transfers_source ON draft_transfers(account,source_draft_id);
CREATE INDEX draft_transfers_target ON draft_transfers(target_account,target_draft_id);
CREATE INDEX drafts_count ON drafts(account,discarded,origin);
CREATE UNIQUE INDEX drafts_legacy_reply ON drafts(account,conversation_id) WHERE legacy_reply=1;
CREATE INDEX drafts_listing ON drafts(account,discarded,updated_at DESC,draft_id);
CREATE INDEX google_delete_history ON capture_operations(account,json_extract(json,'$.draft_id'),id) WHERE execution_kind='google' AND json_extract(json,'$.kind')='draft_delete';
CREATE INDEX google_delete_recovery ON capture_operations(account,id) WHERE execution_kind='google' AND json_extract(json,'$.kind')='draft_delete' AND json_extract(json,'$.state')='in_flight';
CREATE INDEX google_delete_target ON capture_operations(account,json_extract(json,'$.source.provider_draft_id'),json_extract(json,'$.state')) WHERE execution_kind='google' AND json_extract(json,'$.kind')='draft_delete';
CREATE INDEX google_draft_adoption_target ON drafts(account,json_extract(google_json,'$.adoption.target.provider_draft_id')) WHERE origin='local';
CREATE INDEX google_draft_baseline_target ON drafts(account,json_extract(google_json,'$.baseline.draft_id')) WHERE origin='local';
CREATE INDEX google_draft_current_link ON google_draft_sources(account,linked_local_draft_id,source_revision DESC) WHERE is_current=1;
CREATE INDEX google_draft_current_owner ON google_draft_sources(account,draft_id,source_revision DESC) WHERE is_current=1;
CREATE INDEX google_draft_known_ack ON capture_operations(account,json_extract(json,'$.ack.draft_id')) WHERE execution_kind='google';
CREATE INDEX google_draft_known_target ON capture_operations(account,json_extract(json,'$.target')) WHERE execution_kind='google';
CREATE UNIQUE INDEX google_draft_source_current ON google_draft_sources(account,provider_draft_id) WHERE is_current=1;
CREATE INDEX google_draft_source_duplicate ON google_draft_sources(account,draft_id) WHERE is_current=1 AND linked_local_draft_id IS NOT NULL;
CREATE INDEX google_draft_source_history ON google_draft_sources(account,draft_id,source_revision DESC);
CREATE INDEX google_draft_source_linked ON google_draft_sources(account,linked_local_draft_id,source_revision DESC);
CREATE INDEX google_draft_unknown_create ON capture_operations(account,id,blob_hash)
WHERE execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft'
AND json_extract(json,'$.state')='acceptance_unknown' AND json_extract(json,'$.target') IS NULL AND json_extract(json,'$.ack') IS NULL;
CREATE INDEX google_drafts_due ON drafts(account,updated_at,draft_id) WHERE discarded=0 AND origin='local' AND COALESCE(json_extract(google_json,'$.submission.blocks_drafts'),0)=0 AND COALESCE(json_extract(google_json,'$.deletion.blocks_target'),0)=0 AND (google_json IS NULL OR (json_extract(google_json,'$.state') IN ('local','synced','adopted','prepared','acknowledged') AND (json_extract(google_json,'$.state') IN ('prepared','acknowledged') OR revision>COALESCE(json_extract(google_json,'$.baseline.draft_revision'),json_extract(google_json,'$.adoption.draft_revision'),0))) OR (json_extract(google_json,'$.state')='failed' AND revision>COALESCE(json_extract(google_json,'$.preparation_failure.draft_revision'),json_extract(google_json,'$.failed_draft_revision'))));
CREATE INDEX google_live_pending ON google_live_page_items(account,lane,generation,expected_revision,applied_revision,ordinal);
CREATE INDEX google_live_unfinished ON google_live_pages(account,generation,complete);
CREATE INDEX google_mail_coverage_scan ON google_mail_coverage(account,last_inventory_generation,deleted);
CREATE INDEX google_mail_gaps ON google_mail_coverage(account,remote_id) WHERE issue IS NOT NULL AND deleted=0;
CREATE INDEX google_mail_mutations_ready ON google_mail_mutations(account,state,retry_at_ms);
CREATE INDEX google_mail_reconcile ON google_mail_coverage(account,deleted,observed_generation,remote_id);
CREATE INDEX google_mail_coverage_by_date ON google_mail_coverage(account,internal_date_ms) WHERE deleted=0;
CREATE TABLE link_hosts(id INTEGER PRIMARY KEY,rhost TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE message_link_hosts(host INTEGER NOT NULL REFERENCES link_hosts(id),message_rowid INTEGER NOT NULL REFERENCES messages(rowid) ON DELETE CASCADE,PRIMARY KEY(host,message_rowid)) STRICT, WITHOUT ROWID;
CREATE INDEX message_link_hosts_message ON message_link_hosts(message_rowid);
CREATE TABLE link_index_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1),indexed_rowid INTEGER NOT NULL) STRICT;
CREATE INDEX google_send_known_target ON capture_operations(account,json_extract(json,'$.provider_draft_id')) WHERE execution_kind='google';
CREATE INDEX google_send_recovery ON capture_operations(account,id)
WHERE execution_kind='google' AND json_extract(json,'$.kind')='send' AND json_extract(json,'$.state')='in_flight';
CREATE INDEX messages_conversation ON messages(account,conversation_id,sent_at,id);
CREATE INDEX messages_order ON messages(account,sent_at DESC,id);
CREATE INDEX messages_projection ON messages(account,conversation_id,sent_at,id,unread,starred,inbox,sent,source_hash);
CREATE INDEX message_categories_by_category ON message_categories(account,category_id,message_id);

-- views
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
CREATE VIEW calendar_visible_sources AS
 SELECT account,calendar_id,event_id,json,slot,generation
 FROM calendar_published_sources
 WHERE visible=1 AND seen_full_generation>=full_generation;

-- triggers
CREATE TRIGGER calendar_scope_insert_published AFTER INSERT ON calendar_sync_state BEGIN UPDATE calendar_publication SET revision=revision+1; END;
CREATE TRIGGER calendar_scope_update_published AFTER UPDATE ON calendar_sync_state BEGIN UPDATE calendar_publication SET revision=revision+1; END;
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
CREATE TRIGGER calendar_source_insert_published AFTER INSERT ON calendar_sources WHEN NEW.source_generation<=(SELECT published_generation FROM calendar_sync_state WHERE account=NEW.account AND calendar_id=NEW.calendar_id) BEGIN UPDATE calendar_publication SET revision=revision+1; END;
CREATE TRIGGER calendar_source_update_published AFTER UPDATE OF json ON calendar_sources WHEN OLD.pending_generation IS NULL AND NEW.source_generation<=(SELECT published_generation FROM calendar_sync_state WHERE account=NEW.account AND calendar_id=NEW.calendar_id) BEGIN UPDATE calendar_publication SET revision=revision+1; END;
