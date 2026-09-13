//! Pure bounded Calendar preparation. Objects are opaque and cannot be deserialized.
use super::*;
use fm_google::transport::CancellationToken;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::sync::Arc;

pub const CALENDAR_PAGE_BYTES: usize = 16 * 1024 * 1024;
pub const CALENDAR_SOURCE_BYTES: usize = 32 * 1024 * 1024;

pub struct PreparedCalendarPage {
    pub(super) lease: CalendarSyncLease,
    pub(super) page_id: String,
    pub(super) manifest: String,
    pub(super) input_cursor: Option<String>,
    pub(super) items: Vec<Source>,
    pub(super) next_page_token: Option<String>,
    pub(super) next_sync_token: Option<String>,
    bytes: usize,
}
impl PreparedCalendarPage {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn serialized_bytes(&self) -> usize {
        self.bytes
    }
}
pub(super) struct Source {
    pub raw: Arc<str>,
    pub hash: String,
    pub id: Option<String>,
    pub cancelled: bool,
    pub catalog: Option<CatalogFields>,
    pub catalog_role: &'static str,
}
#[derive(Clone)]
pub(super) struct CatalogFields {
    pub id: String,
    pub role: &'static str,
    pub zone: Option<String>,
    pub deleted: bool,
}
impl CatalogFields {
    pub fn from_source(source: &Value) -> Option<Self> {
        valid_catalog(source).then(|| Self {
            id: source["id"].as_str().unwrap().into(),
            role: role(source),
            zone: source["timeZone"].as_str().map(str::to_owned),
            deleted: source["deleted"] == true,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Context {
    pub profile: String,
    pub kind: String,
    pub query: String,
    pub base: i64,
    pub permission: i64,
    pub connection: i64,
    pub projection: i64,
    pub zone: String,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum SourceStamp {
    Missing,
    Existing {
        generation: i64,
        pending: Option<i64>,
    },
}
pub(super) struct Previous {
    pub stamp: SourceStamp,
    pub raw: Option<String>,
}
pub struct CalendarChunkInput {
    pub(super) context: Context,
    pub(super) page_id: String,
    pub(super) manifest: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) previous: Vec<Option<Previous>>,
    pub(super) bytes: usize,
}
impl CalendarChunkInput {
    pub fn input_bytes(&self) -> usize {
        self.bytes
    }
    pub fn item_count(&self) -> usize {
        self.end - self.start
    }
}
pub struct PreparedCalendarChunk {
    pub(super) context: Context,
    pub(super) page_id: String,
    pub(super) manifest: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) items: Vec<Item>,
}
pub struct CalendarCatalogChunkInput {
    context: Context,
    lease: CalendarSyncLease,
    rows: Vec<CatalogRow>,
}
struct CatalogRow {
    page_id: String,
    ordinal: i64,
    hash: String,
    raw: String,
}
pub struct PreparedCalendarCatalogChunk {
    context: Context,
    lease: CalendarSyncLease,
    rows: Vec<PreparedCatalogRow>,
}
pub struct PreparedCalendarCatalogEntry {
    fields: CatalogFields,
    raw: String,
}
pub fn prepare_calendar_catalog_entry(
    source: Value,
    cancel: &CancellationToken,
) -> Result<PreparedCalendarCatalogEntry> {
    check_cancel(cancel)?;
    let fields = CatalogFields::from_source(&source)
        .ok_or_else(|| err("invalid_response", "Calendar metadata is invalid"))?;
    let raw = serialize_bounded(&source, 2 * 1024 * 1024).map_err(|error| {
        if error.code == "calendar_preparation_limit" {
            err("invalid_response", "Calendar metadata exceeds byte bounds")
        } else {
            error
        }
    })?;
    check_cancel(cancel)?;
    Ok(PreparedCalendarCatalogEntry { fields, raw })
}
struct PreparedCatalogRow {
    source: CatalogRow,
    fields: CatalogFields,
}
pub fn prepare_calendar_catalog_chunk(
    input: CalendarCatalogChunkInput,
    cancel: &CancellationToken,
) -> Result<PreparedCalendarCatalogChunk> {
    let mut rows = Vec::with_capacity(input.rows.len());
    for source in input.rows {
        check_cancel(cancel)?;
        if format!("{:x}", Sha256::digest(source.raw.as_bytes())) != source.hash {
            return Err(err(
                "stale_page",
                "Retained Calendar catalog source differs from its receipt",
            ));
        }
        let value: Value = serde_json::from_str(&source.raw)?;
        let fields = CatalogFields::from_source(&value).ok_or_else(|| {
            err(
                "invalid_response",
                "Retained Calendar catalog source is invalid",
            )
        })?;
        rows.push(PreparedCatalogRow { source, fields });
    }
    Ok(PreparedCalendarCatalogChunk {
        context: input.context,
        lease: input.lease,
        rows,
    })
}
pub(super) struct Item {
    pub ordinal: usize,
    pub previous: Option<SourceStamp>,
    pub event: Option<PreparedEvent>,
    pub error: Option<&'static str>,
}
pub(super) struct PreparedEvent {
    pub source: Arc<str>,
    pub index: EventIndex,
}
pub(super) struct EventIndex {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub series: Option<String>,
    pub original_ms: Option<i64>,
    pub original_date: Option<String>,
    pub recurring: bool,
    pub cancelled: bool,
}
impl EventIndex {
    pub fn from_event(event: &Event) -> Self {
        let bounds = |time: &Option<EventTime>| match time {
            Some(EventTime::Date { date }) => (None, Some(date.to_string())),
            Some(EventTime::Zoned { instant, .. }) => (Some(instant.timestamp_millis()), None),
            None => (None, None),
        };
        let (start_ms, start_date) = bounds(&event.start);
        let (end_ms, end_date) = bounds(&event.end);
        let (original_ms, original_date) = bounds(&event.original_start);
        Self {
            start_ms,
            end_ms,
            start_date,
            end_date,
            series: event.series_id.clone(),
            original_ms,
            original_date,
            recurring: !event.recurrence.is_empty(),
            cancelled: event.cancelled,
        }
    }
    pub fn write(
        &self,
        db: &rusqlite::Connection,
        key: (&str, &str, &str),
        slot: i64,
        generation: i64,
    ) -> Result<()> {
        db.execute("INSERT INTO calendar_event_index VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(account,calendar_id,event_id,slot) DO UPDATE SET generation=excluded.generation,start_ms=excluded.start_ms,end_ms=excluded.end_ms,start_date=excluded.start_date,end_date=excluded.end_date,series_id=excluded.series_id,original_ms=excluded.original_ms,original_date=excluded.original_date,recurring=excluded.recurring,cancelled=excluded.cancelled",params![key.0,key.1,key.2,slot,generation,self.start_ms,self.end_ms,self.start_date,self.end_date,self.series,self.original_ms,self.original_date,self.recurring,self.cancelled])?;
        Ok(())
    }
}

pub fn prepare_calendar_page(
    lease: CalendarSyncLease,
    page: CalendarPage,
    cancel: &CancellationToken,
) -> Result<PreparedCalendarPage> {
    check_cancel(cancel)?;
    if page.items.len() > 250 || page.next_page_token.is_some() == page.next_sync_token.is_some() {
        return Err(err(
            "invalid_request",
            "Calendar page exceeds bounds or has ambiguous continuation",
        ));
    }
    for value in [
        &page.input_cursor,
        &page.next_page_token,
        &page.next_sync_token,
    ]
    .into_iter()
    .flatten()
    {
        token(value)?;
    }
    let page_id = fingerprint(&json!([lease.run, page.input_cursor]))?;
    let mut manifest = Sha256::new();
    manifest.update(b"[[");
    let mut items = Vec::with_capacity(page.items.len());
    let mut bytes = 2;
    for source in page.items {
        check_cancel(cancel)?;
        let allowance = CALENDAR_PAGE_BYTES
            .checked_sub(bytes + usize::from(!items.is_empty()))
            .ok_or_else(|| err("invalid_request", "Calendar page exceeds byte bounds"))?;
        let raw = serialize_bounded(&source, allowance).map_err(|error| {
            if error.code == "calendar_preparation_limit" {
                err("invalid_request", "Calendar page exceeds byte bounds")
            } else {
                error
            }
        })?;
        bytes += raw.len() + usize::from(!items.is_empty());
        if bytes > CALENDAR_PAGE_BYTES {
            return Err(err("invalid_request", "Calendar page exceeds byte bounds"));
        }
        if !items.is_empty() {
            manifest.update(b",");
        }
        manifest.update(raw.as_bytes());
        let hash = format!("{:x}", Sha256::digest(raw.as_bytes()));
        let id = source["id"]
            .as_str()
            .filter(|id| provider_id(id).is_ok())
            .map(str::to_owned);
        let cancelled = source["status"] == "cancelled";
        let catalog = if lease.calendar.is_empty() {
            CatalogFields::from_source(&source)
        } else {
            None
        };
        items.push(Source {
            raw: raw.into(),
            hash,
            id,
            cancelled,
            catalog,
            catalog_role: role(&source),
        });
    }
    manifest.update(b"],");
    manifest.update(serde_json::to_vec(&page.next_page_token)?);
    manifest.update(b",");
    manifest.update(serde_json::to_vec(&page.next_sync_token)?);
    manifest.update(b"]");
    check_cancel(cancel)?;
    Ok(PreparedCalendarPage {
        lease,
        page_id,
        manifest: format!("{:x}", manifest.finalize()),
        input_cursor: page.input_cursor,
        items,
        next_page_token: page.next_page_token,
        next_sync_token: page.next_sync_token,
        bytes,
    })
}

pub fn prepare_calendar_chunk(
    page: &PreparedCalendarPage,
    input: CalendarChunkInput,
    cancel: &CancellationToken,
) -> Result<PreparedCalendarChunk> {
    check_cancel(cancel)?;
    if input.page_id != page.page_id
        || input.manifest != page.manifest
        || input.end > page.items.len()
        || input.previous.len() != input.end - input.start
    {
        return Err(err(
            "scope_mismatch",
            "Calendar preparation belongs to another page",
        ));
    }
    let mut items = Vec::with_capacity(input.previous.len());
    for (ordinal, previous) in (input.start..input.end).zip(input.previous) {
        check_cancel(cancel)?;
        let source = &page.items[ordinal];
        let event = if input.context.kind == "events" && source.id.is_some() {
            prepare_event(
                &page.lease,
                &input.context.zone,
                source,
                previous.as_ref().and_then(|v| v.raw.as_deref()),
            )?
        } else {
            None
        };
        let error = if source.id.is_none() {
            Some("invalid_identity")
        } else if input.context.kind == "events" && event.is_none() {
            Some("invalid_event")
        } else if input.context.kind == "catalog" && source.catalog.is_none() {
            Some("invalid_calendar")
        } else {
            None
        };
        items.push(Item {
            ordinal,
            previous: previous.map(|v| v.stamp),
            event,
            error,
        });
    }
    check_cancel(cancel)?;
    Ok(PreparedCalendarChunk {
        context: input.context,
        page_id: input.page_id,
        manifest: input.manifest,
        start: input.start,
        end: input.end,
        items,
    })
}
fn prepare_event(
    lease: &CalendarSyncLease,
    zone: &str,
    source: &Source,
    previous: Option<&str>,
) -> Result<Option<PreparedEvent>> {
    let observed: Value = serde_json::from_str(&source.raw)?;
    let mut merged = observed;
    let merged_previous = source.cancelled && previous.is_some();
    if let Some(previous) = previous.filter(|_| source.cancelled) {
        if previous.len() > CALENDAR_SOURCE_BYTES {
            return Err(preparation_limit());
        }
        let mut base: Value = serde_json::from_str(previous)?;
        if let Some(fields) = merged.as_object() {
            for (key, value) in fields {
                base[key] = value.clone();
            }
        } else {
            return Ok(None);
        }
        merged = base;
    }
    let raw = if merged_previous {
        let raw = serialize_bounded(&merged, CALENDAR_SOURCE_BYTES)?;
        Arc::from(raw)
    } else {
        source.raw.clone()
    };
    let event = match fm_calendar::normalize(&lease.account, &lease.calendar, zone, &merged) {
        Ok(event) => event,
        Err(_) => return Ok(None),
    };
    Ok(Some(PreparedEvent {
        source: raw,
        index: EventIndex::from_event(&event),
    }))
}
pub(super) fn preparation_limit() -> crate::CoreError {
    err(
        "calendar_preparation_limit",
        "A retained Calendar source exceeds the preparation byte limit",
    )
}
fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(err("cancelled", "Calendar preparation stopped"))
    } else {
        Ok(())
    }
}

fn serialize_bounded(value: &Value, limit: usize) -> Result<String> {
    struct Buffer {
        bytes: Vec<u8>,
        limit: usize,
        exceeded: bool,
    }
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .filter(|next| *next <= self.limit);
            let Some(next) = next else {
                self.exceeded = true;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Calendar serialization limit",
                ));
            };
            if next > self.bytes.capacity() {
                let capacity = next
                    .max(self.bytes.capacity().saturating_mul(2).max(512))
                    .min(self.limit);
                self.bytes.reserve_exact(capacity - self.bytes.len());
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut buffer, value);
    if buffer.exceeded {
        return Err(preparation_limit());
    }
    result?;
    String::from_utf8(buffer.bytes)
        .map_err(|_| err("invalid_response", "Calendar source encoding is invalid"))
}

impl Store {
    pub fn calendar_observe_prepared_catalog_entry(
        &mut self,
        account: &str,
        source: &PreparedCalendarCatalogEntry,
        connection_generation: i64,
        expected_permission_epoch: i64,
    ) -> Result<i64> {
        let calendar = &source.fields.id;
        let checkpoint = self.calendar_checkpoint(account, calendar)?;
        let zone = source
            .fields
            .zone
            .as_deref()
            .or(checkpoint["time_zone"].as_str())
            .unwrap();
        let epoch = self.calendar_observe_permission(
            account,
            calendar,
            connection_generation,
            expected_permission_epoch,
            parse_access(source.fields.role)?,
            zone,
        )?;
        self.db.execute(
            "UPDATE calendar_catalog SET json=?3,deleted=?4 WHERE account=?1 AND calendar_id=?2",
            params![account, calendar, source.raw, source.fields.deleted],
        )?;
        Ok(epoch)
    }
    pub fn calendar_catalog_prepare_finalization(
        &self,
        lease: &CalendarSyncLease,
    ) -> Result<CalendarCatalogChunkInput> {
        let run = self.calendar_run(lease)?;
        self.check_calendar_run(lease, &run)?;
        if run.kind != "catalog" || run.final_token.is_none() {
            return Err(err("page_incomplete", "Catalog final page is not sealed"));
        }
        let mut statement=self.db.prepare("SELECT page_id,ordinal,fingerprint,length(CAST(json AS BLOB)) FROM calendar_sync_items WHERE run=?1 AND error_code IS NULL AND catalog_applied=0 ORDER BY page_id,ordinal LIMIT 32")?;
        let mut query = statement.query([lease.run])?;
        let mut rows = Vec::new();
        let mut bytes = 0;
        while let Some(row) = query.next()? {
            let size: usize = row.get(3)?;
            if size > CALENDAR_PAGE_BYTES {
                return Err(preparation_limit());
            }
            if !rows.is_empty() && bytes + size > CHUNK_BYTES {
                break;
            }
            let page_id: String = row.get(0)?;
            let ordinal: i64 = row.get(1)?;
            let hash: String = row.get(2)?;
            let raw = self.db.query_row(
                "SELECT json FROM calendar_sync_items WHERE run=?1 AND page_id=?2 AND ordinal=?3",
                params![lease.run, page_id, ordinal],
                |r| r.get(0),
            )?;
            rows.push(CatalogRow {
                page_id,
                ordinal,
                hash,
                raw,
            });
            bytes += size;
        }
        Ok(CalendarCatalogChunkInput {
            context: self.calendar_preparation_context(lease, &run)?,
            lease: lease.clone(),
            rows,
        })
    }
    pub fn calendar_catalog_finalize_prepared(
        &mut self,
        prepared: &PreparedCalendarCatalogChunk,
    ) -> Result<usize> {
        let lease = &prepared.lease;
        let run = self.calendar_run(lease)?;
        self.check_calendar_run(lease, &run)?;
        if prepared.context != self.calendar_preparation_context(lease, &run)? {
            return Err(err(
                "stale_preparation",
                "Calendar catalog context changed during preparation",
            ));
        }
        let tx = self.db.unchecked_transaction()?;
        for item in &prepared.rows {
            let row = &item.source;
            let (hash,applied):(String,bool)=tx.query_row("SELECT fingerprint,catalog_applied FROM calendar_sync_items WHERE run=?1 AND page_id=?2 AND ordinal=?3",params![lease.run,row.page_id,row.ordinal],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if hash != row.hash {
                return Err(err(
                    "stale_page",
                    "Calendar catalog receipt changed during preparation",
                ));
            }
            if !applied {
                publish_catalog_prepared(&tx, lease, run.projection, &item.fields, &row.raw)?;
                tx.execute("UPDATE calendar_sync_items SET catalog_applied=1 WHERE run=?1 AND page_id=?2 AND ordinal=?3",params![lease.run,row.page_id,row.ordinal])?;
            }
        }
        tx.commit()?;
        Ok(prepared.rows.len())
    }
    fn calendar_preparation_context(
        &self,
        lease: &CalendarSyncLease,
        run: &Run,
    ) -> Result<Context> {
        let zone = if run.kind == "events" {
            self.db.query_row(
                "SELECT time_zone FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
                params![lease.account, lease.calendar],
                |r| r.get(0),
            )?
        } else {
            String::new()
        };
        Ok(Context {
            profile: self.vault_namespace()?,
            kind: run.kind.clone(),
            query: run.query.clone(),
            base: run.base,
            permission: run.permission,
            connection: run.connection,
            projection: run.projection,
            zone,
        })
    }
    fn check_prepared_page(
        &self,
        lease: &CalendarSyncLease,
        page: &PreparedCalendarPage,
        run: &Run,
        start: usize,
    ) -> Result<bool> {
        self.check_calendar_run(lease, run)?;
        if page.lease.run != lease.run
            || page.lease.account != lease.account
            || page.lease.calendar != lease.calendar
            || page.lease.connection_generation != lease.connection_generation
        {
            return Err(err(
                "scope_mismatch",
                "Calendar preparation belongs to another run",
            ));
        }
        let prior: Option<(String, bool)> = self
            .db
            .query_row(
                "SELECT manifest,sealed FROM calendar_sync_pages WHERE run=?1 AND page_id=?2",
                params![lease.run, page.page_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((manifest, sealed)) = prior {
            if manifest != page.manifest {
                return Err(err("stale_page", "Calendar page changed during replay"));
            }
            if sealed {
                return Ok(true);
            }
        }
        if run.final_token.is_some() || page.input_cursor != run.next || start > page.items.len() {
            return Err(err(
                "stale_page",
                "Unexpected Calendar page or item position",
            ));
        }
        Ok(false)
    }
    /// Copy only bounded prior cancellation sources. No JSON parsing or hashes.
    pub fn calendar_prepare_chunk(
        &self,
        lease: &CalendarSyncLease,
        page: &PreparedCalendarPage,
        start: usize,
    ) -> Result<CalendarChunkInput> {
        let run = self.calendar_run(lease)?;
        let sealed = self.check_prepared_page(lease, page, &run, start)?;
        let context = self.calendar_preparation_context(lease, &run)?;
        if sealed {
            return Ok(CalendarChunkInput {
                context,
                page_id: page.page_id.clone(),
                manifest: page.manifest.clone(),
                start: page.len(),
                end: page.len(),
                previous: vec![],
                bytes: 0,
            });
        }
        let mut previous = Vec::new();
        let mut bytes = 0;
        for source in page.items.iter().skip(start).take(CALENDAR_STAGE_CHUNK) {
            let candidate = if run.kind == "events" && source.cancelled {
                if let Some(id) = &source.id {
                    self.db.query_row("SELECT source_generation,pending_generation,length(CAST(COALESCE(CASE WHEN pending_generation=?4 THEN pending_json END,json) AS BLOB)) FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id,lease.run],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,Option<i64>>(1)?,r.get::<_,usize>(2)?))).optional()?
                } else {
                    None
                }
            } else {
                None
            };
            let prior_bytes = candidate.as_ref().map_or(0, |v| v.2);
            if prior_bytes > CALENDAR_SOURCE_BYTES {
                return Err(preparation_limit());
            }
            let next_bytes = source.raw.len() + prior_bytes;
            if !previous.is_empty() && bytes + next_bytes > CHUNK_BYTES {
                break;
            }
            let prior = if run.kind == "events" && source.cancelled && source.id.is_some() {
                let (stamp, raw) = if let Some((generation, pending, _)) = candidate {
                    let raw=self.db.query_row("SELECT COALESCE(CASE WHEN pending_generation=?4 THEN pending_json END,json) FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,source.id,lease.run],|r|r.get(0))?;
                    (
                        SourceStamp::Existing {
                            generation,
                            pending,
                        },
                        Some(raw),
                    )
                } else {
                    (SourceStamp::Missing, None)
                };
                Some(Previous { stamp, raw })
            } else {
                None
            };
            bytes += next_bytes;
            previous.push(prior);
        }
        Ok(CalendarChunkInput {
            context,
            page_id: page.page_id.clone(),
            manifest: page.manifest.clone(),
            start,
            end: start + previous.len(),
            previous,
            bytes,
        })
    }
    /// The prepared object proves its source/normalization provenance; current
    /// profile, run, policy and cancellation-source stamps are rechecked here.
    pub fn calendar_stage_prepared_chunk(
        &mut self,
        lease: &CalendarSyncLease,
        page: &PreparedCalendarPage,
        chunk: &PreparedCalendarChunk,
    ) -> Result<usize> {
        let run = self.calendar_run(lease)?;
        let sealed = self.check_prepared_page(lease, page, &run, chunk.start)?;
        if chunk.page_id != page.page_id
            || chunk.manifest != page.manifest
            || chunk.context != self.calendar_preparation_context(lease, &run)?
        {
            return Err(err(
                "stale_preparation",
                "Calendar source context changed during preparation",
            ));
        }
        if sealed {
            return Ok(page.len());
        }
        for item in &chunk.items {
            if let Some(expected) = &item.previous {
                let source = &page.items[item.ordinal];
                let current=self.db.query_row("SELECT source_generation,pending_generation FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,source.id],|r|Ok(SourceStamp::Existing{generation:r.get(0)?,pending:r.get(1)?})).optional()?.unwrap_or(SourceStamp::Missing);
                if &current != expected {
                    return Err(err(
                        "stale_preparation",
                        "Calendar cancellation source changed during preparation",
                    ));
                }
            }
        }
        let tx = self.db.unchecked_transaction()?;
        tx.execute("INSERT OR IGNORE INTO calendar_sync_pages(run,page_id,input_cursor,item_count,manifest,next_page_token,next_sync_token) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![lease.run,page.page_id,page.input_cursor.as_deref().unwrap_or(""),page.len() as i64,page.manifest,page.next_page_token,page.next_sync_token])?;
        for item in &chunk.items {
            let source = &page.items[item.ordinal];
            let old:Option<String>=tx.query_row("SELECT fingerprint FROM calendar_sync_items WHERE run=?1 AND page_id=?2 AND ordinal=?3",params![lease.run,page.page_id,item.ordinal as i64],|r|r.get(0)).optional()?;
            if let Some(old) = old {
                if old != source.hash {
                    return Err(err("stale_page", "Calendar item changed during replay"));
                }
                continue;
            }
            let duplicate = if let Some(id) = &source.id {
                tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM calendar_sync_items WHERE run=?1 AND event_id=?2)",
                    params![lease.run, id],
                    |r| r.get::<_, bool>(0),
                )?
            } else {
                false
            };
            let error = item.error.or(if duplicate {
                Some("duplicate_identity")
            } else {
                None
            });
            tx.execute("INSERT INTO calendar_sync_items(run,page_id,ordinal,event_id,json,fingerprint,error_code) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![lease.run,page.page_id,item.ordinal as i64,source.id,source.raw.as_ref(),source.hash,error])?;
            if let (Some(id), Some(event), None) = (&source.id, &item.event, error) {
                stage_event(&tx, lease, &run, id, &source.raw, event)?;
            }
            if run.kind == "catalog" {
                if let Some(id) = &source.id {
                    restrict_catalog_role(&tx, lease, id, source.catalog_role)?;
                }
            }
            if error.is_some() {
                tx.execute("UPDATE calendar_sync_runs SET errors=errors+1,identity_complete=MIN(identity_complete,?2) WHERE id=?1",params![lease.run,i64::from(source.id.is_some())])?;
            }
        }
        tx.commit()?;
        Ok(chunk.end)
    }
    pub fn calendar_seal_prepared_page(
        &mut self,
        lease: &CalendarSyncLease,
        page: &PreparedCalendarPage,
    ) -> Result<()> {
        let run = self.calendar_run(lease)?;
        if self.check_prepared_page(lease, page, &run, 0)? {
            return Ok(());
        }
        let tx = self.db.unchecked_transaction()?;
        let count: Option<i64> = tx
            .query_row(
                "SELECT item_count FROM calendar_sync_pages WHERE run=?1 AND page_id=?2",
                params![lease.run, page.page_id],
                |r| r.get(0),
            )
            .optional()?;
        let count = count.ok_or_else(|| err("page_incomplete", "Calendar page is not staged"))?;
        let actual: i64 = tx.query_row(
            "SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1 AND page_id=?2",
            params![lease.run, page.page_id],
            |r| r.get(0),
        )?;
        if actual != count {
            return Err(err(
                "page_incomplete",
                "All Calendar page items must be durable before its cursor",
            ));
        }
        if let Some(next) = &page.next_page_token {
            if tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM calendar_sync_pages WHERE run=?1 AND input_cursor=?2)",
                params![lease.run, next],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(err("stale_page", "Calendar cursor cycle"));
            }
        }
        tx.execute(
            "UPDATE calendar_sync_pages SET sealed=1 WHERE run=?1 AND page_id=?2",
            params![lease.run, page.page_id],
        )?;
        tx.execute(
            "UPDATE calendar_sync_runs SET next_page_token=?2,final_sync_token=?3 WHERE id=?1",
            params![lease.run, page.next_page_token, page.next_sync_token],
        )?;
        tx.commit()?;
        Ok(())
    }
}
