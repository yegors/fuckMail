//! Durable Calendar pages in the existing Core store. No provider or browser calls.
use crate::calendar_store::{calendar_error, fingerprint, parse_access};
use crate::{err, Result, Store};
use fm_calendar::google_api::{BusySnapshot, CalendarBusy};
use fm_calendar::{Access, Event, EventTime};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub const CALENDAR_STAGE_CHUNK: usize = 32;
const CHUNK_BYTES: usize = 1024 * 1024;
pub(crate) fn provider_id(value: &str) -> Result<&str> {
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(err("invalid_request", "Invalid Calendar provider identity"));
    }
    Ok(value)
}
fn token(value: &str) -> Result<&str> {
    if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
        return Err(err("invalid_request", "Invalid Calendar cursor"));
    }
    Ok(value)
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalendarSyncMode {
    Full,
    Delta,
}
impl CalendarSyncMode {
    fn text(self) -> &'static str {
        if self == Self::Full {
            "full"
        } else {
            "delta"
        }
    }
}
#[derive(Clone)]
pub struct CalendarSyncLease {
    pub run: i64,
    pub account: String,
    pub calendar: String,
    pub connection_generation: i64,
}
pub struct CalendarPage {
    pub input_cursor: Option<String>,
    pub items: Vec<Value>,
    pub next_page_token: Option<String>,
    pub next_sync_token: Option<String>,
}
struct Run {
    kind: String,
    mode: String,
    state: String,
    query: String,
    base: i64,
    permission: i64,
    connection: i64,
    projection: i64,
    next: Option<String>,
    final_token: Option<String>,
    identity_complete: bool,
    errors: i64,
}

impl Store {
    /// Native-only catalog cursor; it is never part of a GUI/CLI projection.
    pub fn calendar_catalog_checkpoint(&self, account: &str) -> Result<Value> {
        crate::id(account)?;
        Ok(self.db.query_row(
            "SELECT sync_token,revision,complete FROM calendar_catalog_state WHERE account=?1",
            [account],
            |r| Ok(json!({"sync_token":r.get::<_,Option<String>>(0)?,"revision":r.get::<_,i64>(1)?,"complete":r.get::<_,bool>(2)?})),
        ).optional()?.unwrap_or_else(||json!({"sync_token":null,"revision":0,"complete":false})))
    }

    /// Find only outstanding source consolidation, including after a process restart.
    /// The source indexes bound this lookup independently of completed run history.
    pub fn calendar_sync_cleanup(
        &self,
        account: &str,
        calendar: &str,
    ) -> Result<Option<CalendarSyncLease>> {
        crate::id(account)?;
        provider_id(calendar)?;
        self.db.query_row(
            "WITH pending AS (SELECT pending_generation AS generation FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND pending_generation IS NOT NULL ORDER BY pending_generation LIMIT 1), unpublished AS (SELECT source_generation AS generation FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND source_generation>(SELECT published_generation FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2) ORDER BY source_generation LIMIT 1) SELECT r.id,r.connection_generation FROM calendar_sync_runs r WHERE r.id IN (SELECT generation FROM pending UNION SELECT generation FROM unpublished) AND r.account=?1 AND r.calendar_id=?2 AND r.state IN ('published','expired') ORDER BY r.id LIMIT 1",
            params![account,calendar],
            |r|Ok(CalendarSyncLease{run:r.get(0)?,account:account.into(),calendar:calendar.into(),connection_generation:r.get(1)?}),
        ).optional().map_err(Into::into)
    }

    /// Rebind retained, unmodified staging to the current verified connection.
    /// Old HTTP responses retain the old lease and cannot commit after rebinding.
    pub fn calendar_sync_rebind(&mut self, lease: &CalendarSyncLease) -> Result<CalendarSyncLease> {
        let run = self.calendar_run(lease)?;
        let connection = self.calendar_connection(&lease.account, &run.kind)?;
        let current = CalendarSyncLease {
            connection_generation: connection,
            ..lease.clone()
        };
        let tx = self.db.unchecked_transaction()?;
        tx.execute("UPDATE calendar_sync_runs SET connection_generation=?2 WHERE id=?1 AND state='staging'",params![lease.run,connection])?;
        let updated = self.calendar_run(&current)?;
        self.check_calendar_run(&current, &updated)?;
        tx.commit()?;
        Ok(current)
    }

    /// Native worker paging, including scopes beyond the bounded UI catalog.
    /// Tokens are internal checkpoint data and must never be logged or sent as UI rows.
    pub fn calendar_sync_scope(&self, account: &str, calendar: &str) -> Result<Value> {
        let mut scope = self.calendar_checkpoint(account, calendar)?;
        let denied = self.calendar_membership_denied(account, calendar)?;
        if denied {
            scope["access"] = "none".into();
        }
        scope["membership_denied"] = denied.into();
        Ok(scope)
    }

    pub fn calendar_sync_scopes(
        &self,
        account: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Value> {
        crate::id(account)?;
        if let Some(after) = after {
            provider_id(after)?;
        }
        if limit == 0 || limit > 100 {
            return Err(err("invalid_request", "Calendar scope page limit is 1–100"));
        }
        let mut statement=self.db.prepare("SELECT calendar_id FROM calendar_sync_state WHERE account=?1 AND calendar_id>?2 ORDER BY calendar_id LIMIT ?3")?;
        let ids = statement
            .query_map(
                params![account, after.unwrap_or(""), (limit + 1) as i64],
                |r| r.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let has_more = ids.len() > limit;
        let mut scopes = Vec::new();
        let mut continuation = None;
        for id in ids.into_iter().take(limit) {
            let scope = self.calendar_sync_scope(account, &id)?;
            continuation = Some(id);
            scopes.push(scope);
        }
        Ok(json!({"scopes":scopes,"continuation":if has_more{continuation}else{None}}))
    }
    pub fn calendar_sync_begin(
        &mut self,
        account: &str,
        calendar: &str,
        mode: CalendarSyncMode,
    ) -> Result<CalendarSyncLease> {
        provider_id(calendar)?;
        if self.calendar_membership_denied(account, calendar)? {
            return Err(err(
                "permission_denied",
                "Calendar is absent from the committed catalog",
            ));
        }
        self.begin_calendar_run(
            account,
            calendar,
            "events",
            mode,
            fm_calendar::google_api::EVENT_SYNC_FINGERPRINT,
        )
    }
    pub fn calendar_catalog_begin(
        &mut self,
        account: &str,
        mode: CalendarSyncMode,
    ) -> Result<CalendarSyncLease> {
        self.begin_calendar_run(
            account,
            "",
            "catalog",
            mode,
            fm_calendar::google_api::CATALOG_SYNC_FINGERPRINT,
        )
    }
    fn begin_calendar_run(
        &mut self,
        account: &str,
        calendar: &str,
        kind: &str,
        mode: CalendarSyncMode,
        query: &str,
    ) -> Result<CalendarSyncLease> {
        let connection = self.calendar_connection(account, kind)?;
        if self.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_sources s JOIN calendar_sync_state c USING(account,calendar_id) WHERE s.account=?1 AND s.calendar_id=?2 AND (s.pending_generation IS NOT NULL OR s.source_generation>c.published_generation))",params![account,calendar],|r|r.get::<_,bool>(0))? { return Err(err("calendar_finalize_pending","Consolidate or discard the prior Calendar run first")); }
        let (base, permission, input): (i64, i64, Option<String>) = if kind == "events" {
            let row:(i64,i64,Option<String>,String,String)=self.db.query_row("SELECT revision,permission_epoch,sync_token,access,query_fingerprint FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",params![account,calendar],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?.ok_or_else(||err("not_found","Calendar is not discovered"))?;
            if row.4 != query {
                return Err(err(
                    "scope_mismatch",
                    "Calendar cursor belongs to a different query",
                ));
            }
            if matches!(parse_access(&row.3)?, Access::None | Access::FreeBusy) {
                return Err(err("permission_denied", "Calendar details are unavailable"));
            }
            (row.0, row.1, row.2)
        } else {
            self.db.execute(
                "INSERT OR IGNORE INTO calendar_catalog_state(account) VALUES(?1)",
                [account],
            )?;
            self.db.query_row(
                "SELECT revision,0,sync_token FROM calendar_catalog_state WHERE account=?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?
        };
        if mode == CalendarSyncMode::Delta && input.is_none() {
            return Err(err(
                "invalid_request",
                "A delta requires a committed Calendar token",
            ));
        }
        let projection: i64 =
            self.db
                .query_row("SELECT revision FROM calendar_publication", [], |r| {
                    r.get(0)
                })?;
        self.db.execute("INSERT INTO calendar_sync_runs(account,calendar_id,kind,mode,state,query_fingerprint,base_revision,permission_epoch,connection_generation,projection_epoch,input_sync_token) VALUES(?1,?2,?3,?4,'staging',?5,?6,?7,?8,?9,?10)",params![account,calendar,kind,mode.text(),query,base,permission,connection,projection,if mode==CalendarSyncMode::Delta {input} else {None}])?;
        Ok(CalendarSyncLease {
            run: self.db.last_insert_rowid(),
            account: account.into(),
            calendar: calendar.into(),
            connection_generation: connection,
        })
    }
    pub fn calendar_sync_resume(
        &self,
        account: &str,
        calendar: &str,
    ) -> Result<Option<CalendarSyncLease>> {
        crate::id(account)?;
        if !calendar.is_empty() {
            provider_id(calendar)?;
        }
        self.db.query_row("SELECT id,connection_generation FROM calendar_sync_runs WHERE account=?1 AND calendar_id=?2 AND state='staging'",params![account,calendar],|r|Ok(CalendarSyncLease{run:r.get(0)?,account:account.into(),calendar:calendar.into(),connection_generation:r.get(1)?})).optional().map_err(Into::into)
    }
    pub fn calendar_sync_progress(&self, lease: &CalendarSyncLease) -> Result<Value> {
        let run = self.calendar_run(lease)?;
        let input: Option<String> = self.db.query_row(
            "SELECT input_sync_token FROM calendar_sync_runs WHERE id=?1",
            [lease.run],
            |r| r.get(0),
        )?;
        Ok(
            json!({"run":lease.run,"kind":run.kind,"mode":run.mode,"state":run.state,"query_fingerprint":run.query,"sync_token":input,"next_page_token":run.next,"final_page_staged":run.final_token.is_some(),"errors":run.errors,"identity_complete":run.identity_complete}),
        )
    }
    /// Replaying an already durable chunk is idempotent. No cursor is advanced here.
    /// Caller yields the writer between calls; each chunk is at most 32 items / 1 MiB,
    /// except one individually bounded response item that must be retained intact.
    pub fn calendar_stage_page_chunk(
        &mut self,
        lease: &CalendarSyncLease,
        page: &CalendarPage,
        start: usize,
    ) -> Result<usize> {
        let run = self.calendar_run(lease)?;
        self.check_calendar_run(lease, &run)?;
        validate_page(page)?;
        let manifest = fingerprint(&json!([
            page.items,
            page.next_page_token,
            page.next_sync_token
        ]))?;
        let page_id = fingerprint(&json!([lease.run, page.input_cursor]))?;
        let previous: Option<(String, i64)> = self
            .db
            .query_row(
                "SELECT manifest,sealed FROM calendar_sync_pages WHERE run=?1 AND page_id=?2",
                params![lease.run, page_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old, sealed)) = previous {
            if old != manifest {
                return Err(err("stale_page", "Calendar page changed during replay"));
            }
            if sealed != 0 {
                return Ok(page.items.len());
            }
        }
        if run.final_token.is_some() || page.input_cursor != run.next || start > page.items.len() {
            return Err(err(
                "stale_page",
                "Unexpected Calendar page or item position",
            ));
        }
        let zone = if run.kind == "events" {
            self.db.query_row(
                "SELECT time_zone FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
                params![lease.account, lease.calendar],
                |r| r.get::<_, String>(0),
            )?
        } else {
            String::new()
        };
        let mut prepared = Vec::new();
        let mut bytes = 0;
        for (ordinal, source) in page
            .items
            .iter()
            .enumerate()
            .skip(start)
            .take(CALENDAR_STAGE_CHUNK)
        {
            let raw = serde_json::to_string(source)?;
            if !prepared.is_empty() && bytes + raw.len() > CHUNK_BYTES {
                break;
            }
            bytes += raw.len();
            let hash = fingerprint(source)?;
            let id = source["id"]
                .as_str()
                .filter(|v| provider_id(v).is_ok())
                .map(str::to_owned);
            let event = if run.kind == "events" {
                id.as_ref()
                    .and_then(|id| self.prepare_calendar_event(lease, id, source, &zone).ok())
            } else {
                None
            };
            let error = if id.is_none() {
                Some("invalid_identity")
            } else if run.kind == "events" && event.is_none() {
                Some("invalid_event")
            } else if run.kind == "catalog" && !valid_catalog(source) {
                Some("invalid_calendar")
            } else {
                None
            };
            prepared.push((ordinal, id, raw, hash, error, event));
        }
        let tx = self.db.unchecked_transaction()?;
        tx.execute("INSERT OR IGNORE INTO calendar_sync_pages(run,page_id,input_cursor,item_count,manifest,next_page_token,next_sync_token) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![lease.run,page_id,page.input_cursor.as_deref().unwrap_or(""),page.items.len() as i64,manifest,page.next_page_token,page.next_sync_token])?;
        for (ordinal, id, raw, hash, error, event) in &prepared {
            let old:Option<String>=tx.query_row("SELECT fingerprint FROM calendar_sync_items WHERE run=?1 AND page_id=?2 AND ordinal=?3",params![lease.run,page_id,*ordinal as i64],|r|r.get(0)).optional()?;
            if let Some(old) = old {
                if old != *hash {
                    return Err(err("stale_page", "Calendar item changed during replay"));
                }
                continue;
            }
            let duplicate = if let Some(id) = id {
                tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM calendar_sync_items WHERE run=?1 AND event_id=?2)",
                    params![lease.run, id],
                    |r| r.get::<_, bool>(0),
                )?
            } else {
                false
            };
            let error = error.or(if duplicate {
                Some("duplicate_identity")
            } else {
                None
            });
            tx.execute("INSERT INTO calendar_sync_items(run,page_id,ordinal,event_id,json,fingerprint,error_code) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![lease.run,page_id,*ordinal as i64,id,raw,hash,error])?;
            if let (Some(id), Some(event), None) = (id, event, error) {
                stage_event(&tx, lease, &run, id, raw, event)?;
            }
            if run.kind == "catalog" && id.is_some() {
                restrict_catalog_entry(&tx, lease, run.projection, &serde_json::from_str(raw)?)?;
            }
            if error.is_some() {
                tx.execute("UPDATE calendar_sync_runs SET errors=errors+1,identity_complete=MIN(identity_complete,?2) WHERE id=?1",params![lease.run,i64::from(id.is_some())])?;
            }
        }
        tx.commit()?;
        Ok(start + prepared.len())
    }
    pub fn calendar_seal_page(
        &mut self,
        lease: &CalendarSyncLease,
        page: &CalendarPage,
    ) -> Result<()> {
        let run = self.calendar_run(lease)?;
        self.check_calendar_run(lease, &run)?;
        validate_page(page)?;
        let page_id = fingerprint(&json!([lease.run, page.input_cursor]))?;
        let manifest = fingerprint(&json!([
            page.items,
            page.next_page_token,
            page.next_sync_token
        ]))?;
        let tx = self.db.unchecked_transaction()?;
        let (stored,sealed,count):(String,i64,i64)=tx.query_row("SELECT manifest,sealed,item_count FROM calendar_sync_pages WHERE run=?1 AND page_id=?2",params![lease.run,page_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||err("page_incomplete","Calendar page is not staged"))?;
        if stored != manifest {
            return Err(err("stale_page", "Calendar page changed before sealing"));
        }
        if sealed != 0 {
            return Ok(());
        }
        if page.input_cursor != run.next || run.final_token.is_some() {
            return Err(err("stale_page", "Calendar continuation changed"));
        }
        let actual: i64 = tx.query_row(
            "SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1 AND page_id=?2",
            params![lease.run, page_id],
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
            params![lease.run, page_id],
        )?;
        tx.execute(
            "UPDATE calendar_sync_runs SET next_page_token=?2,final_sync_token=?3 WHERE id=?1",
            params![lease.run, page.next_page_token, page.next_sync_token],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn calendar_publish_sync(&mut self, lease: &CalendarSyncLease) -> Result<Value> {
        let run = self.calendar_run(lease)?;
        if run.state == "published" {
            return Ok(
                json!({"run":lease.run,"state":"published","errors":run.errors,"complete":run.identity_complete&&run.errors==0}),
            );
        }
        self.check_calendar_run(lease, &run)?;
        let final_token = run
            .final_token
            .as_deref()
            .ok_or_else(|| err("page_incomplete", "Calendar final page is not sealed"))?;
        let tx = self.db.unchecked_transaction()?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM calendar_sync_pages WHERE run=?1 AND sealed=0)",
            [lease.run],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(err("page_incomplete", "Calendar pages remain unsealed"));
        }
        if run.kind == "events" {
            tx.execute("UPDATE calendar_sync_state SET published_generation=?3,full_generation=CASE WHEN ?4 AND ?5 THEN ?3 ELSE full_generation END,sync_token=?6,retained_errors=CASE WHEN ?4 AND ?5 THEN 0 ELSE retained_errors+?7 END,revision=revision+1 WHERE account=?1 AND calendar_id=?2",params![lease.account,lease.calendar,lease.run,run.mode=="full",run.identity_complete&&run.errors==0,final_token,run.errors])?;
        } else {
            // Catalog is bounded separately: publish item chunks via catalog_finalize_chunk
            // before this pointer. The catalog checkpoint never precedes those commits.
            let remaining:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM calendar_sync_items WHERE run=?1 AND error_code IS NULL AND catalog_applied=0)",params![lease.run],|r|r.get(0))?;
            if remaining {
                return Err(err(
                    "calendar_finalize_pending",
                    "Finalize staged catalog entries in bounded chunks",
                ));
            }
            tx.execute("UPDATE calendar_catalog_state SET sync_token=?2,revision=revision+1,complete=CASE WHEN ?4 THEN ?3 ELSE complete AND ?3 END,full_generation=CASE WHEN ?4 AND ?3 THEN ?5 ELSE full_generation END WHERE account=?1",params![lease.account,final_token,run.identity_complete&&run.errors==0,run.mode=="full",lease.run])?;
            // Absent entries are filtered by catalog membership queries, without a whole catalog rewrite.
            tx.execute("UPDATE calendar_publication SET revision=revision+1", [])?;
        }
        tx.execute(
            "UPDATE calendar_sync_runs SET state='published' WHERE id=?1",
            [lease.run],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(
            json!({"run":lease.run,"state":"published","errors":run.errors,"complete":run.identity_complete&&run.errors==0}),
        )
    }
    pub fn calendar_expire_sync(&mut self, lease: &CalendarSyncLease) -> Result<()> {
        let _ = self.calendar_run(lease)?;
        self.db.execute(
            "UPDATE calendar_sync_runs SET state='expired' WHERE id=?1 AND state='staging'",
            [lease.run],
        )?;
        Ok(())
    }
    /// Consolidation and expiry cleanup are bounded and restartable. No source/journal bulk deletion.
    pub fn calendar_finalize_chunk(&mut self, lease: &CalendarSyncLease) -> Result<usize> {
        let run = self.calendar_run(lease)?;
        if run.state == "staging" {
            return Err(err(
                "calendar_sync_pending",
                "Seal and publish or expire the run first",
            ));
        }
        let mut q=self.db.prepare("SELECT s.event_id FROM calendar_sources s WHERE s.account=?1 AND s.calendar_id=?2 AND (s.pending_generation=?3 OR (s.source_generation=?3 AND ?4='expired')) ORDER BY s.event_id LIMIT 32")?;
        let ids = q
            .query_map(
                params![lease.account, lease.calendar, lease.run, run.state],
                |r| r.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(q);
        let tx = self.db.unchecked_transaction()?;
        for id in &ids {
            if run.state == "published" {
                tx.execute("UPDATE calendar_event_state SET observed_json=pending_observed_json,visible=pending_visible,seen_full_generation=pending_seen_full_generation,pending_observed_json=NULL,pending_visible=NULL,pending_seen_full_generation=NULL WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id])?;
                tx.execute("UPDATE calendar_sources SET json=pending_json,source_generation=pending_generation,pending_json=NULL,pending_generation=NULL WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id])?;
                tx.execute("DELETE FROM calendar_event_index WHERE account=?1 AND calendar_id=?2 AND event_id=?3 AND slot=0",params![lease.account,lease.calendar,id])?;
                tx.execute("UPDATE calendar_event_index SET slot=0 WHERE account=?1 AND calendar_id=?2 AND event_id=?3 AND slot=1",params![lease.account,lease.calendar,id])?;
            } else {
                tx.execute("DELETE FROM calendar_event_index WHERE account=?1 AND calendar_id=?2 AND event_id=?3 AND generation=?4",params![lease.account,lease.calendar,id,lease.run])?;
                tx.execute("UPDATE calendar_event_state SET pending_observed_json=NULL,pending_visible=NULL,pending_seen_full_generation=NULL WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id])?;
                tx.execute("UPDATE calendar_sources SET pending_json=NULL,pending_generation=NULL WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id])?;
                let new:bool=tx.query_row("SELECT source_generation=?4 FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id,lease.run],|r|r.get(0))?;
                if new {
                    tx.execute("DELETE FROM calendar_event_state WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id])?;
                    tx.execute("DELETE FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id])?;
                }
            }
        }
        tx.commit()?;
        Ok(ids.len())
    }
    pub fn calendar_catalog_finalize_chunk(&mut self, lease: &CalendarSyncLease) -> Result<usize> {
        let run = self.calendar_run(lease)?;
        self.check_calendar_run(lease, &run)?;
        if run.kind != "catalog" || run.final_token.is_none() {
            return Err(err("page_incomplete", "Catalog final page is not sealed"));
        }
        let mut q=self.db.prepare("SELECT page_id,ordinal,json FROM calendar_sync_items WHERE run=?1 AND error_code IS NULL AND catalog_applied=0 ORDER BY page_id,ordinal LIMIT 32")?;
        let rows = q
            .query_map(params![lease.run], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(q);
        let tx = self.db.unchecked_transaction()?;
        for (page_id, ordinal, raw) in &rows {
            publish_catalog_entry(&tx, lease, run.projection, &serde_json::from_str(raw)?)?;
            tx.execute("UPDATE calendar_sync_items SET catalog_applied=1 WHERE run=?1 AND page_id=?2 AND ordinal=?3",params![lease.run,page_id,ordinal])?;
        }
        tx.commit()?;
        Ok(rows.len())
    }
    pub fn calendar_busy_commit(
        &mut self,
        snapshot: &BusySnapshot,
        observed_at: i64,
        connection_generation: i64,
        permission_epochs: &std::collections::BTreeMap<String, i64>,
    ) -> Result<()> {
        if self.calendar_connection(snapshot.account.as_str(), "busy")? != connection_generation
            || snapshot.calendars.len() > 50
            || snapshot.to <= snapshot.from
            || snapshot.to - snapshot.from > chrono::Duration::days(43)
        {
            return Err(err(
                "stale_revision",
                "Free/busy scope is stale or unbounded",
            ));
        }
        let tx = self.db.unchecked_transaction()?;
        for (calendar, busy) in &snapshot.calendars {
            provider_id(calendar)?;
            let epoch:i64=tx.query_row("SELECT permission_epoch FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",params![snapshot.account.as_str(),calendar],|r|r.get(0))?;
            if permission_epochs.get(calendar) != Some(&epoch) {
                return Err(err("stale_revision", "Free/busy permission changed"));
            }
            let (available, value) = match busy {
                CalendarBusy::Available(intervals) => {
                    if intervals.len() > 1000
                        || intervals.iter().any(|v| {
                            v.end <= v.start || v.start < snapshot.from || v.end > snapshot.to
                        })
                    {
                        return Err(err(
                            "invalid_response",
                            "Free/busy intervals exceed the requested bounds",
                        ));
                    }
                    (
                        true,
                        json!({"busy":intervals.iter().map(|v|json!({"start":v.start.to_rfc3339(),"end":v.end.to_rfc3339()})).collect::<Vec<_>>()}),
                    )
                }
                CalendarBusy::Unavailable(errors) => {
                    if errors.len() > 32 {
                        return Err(err(
                            "invalid_response",
                            "Free/busy error count exceeds bounds",
                        ));
                    }
                    (
                        false,
                        json!({"errors":errors.iter().map(|v|format!("{v:?}")).collect::<Vec<_>>()}),
                    )
                }
            };
            tx.execute("INSERT INTO calendar_busy_snapshots VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(account,calendar_id,from_ms,to_ms) DO UPDATE SET observed_at=excluded.observed_at,permission_epoch=excluded.permission_epoch,json=excluded.json,available=excluded.available",params![snapshot.account.as_str(),calendar,snapshot.from.timestamp_millis(),snapshot.to.timestamp_millis(),observed_at,epoch,serde_json::to_string(&value)?,available])?;
        }
        tx.execute("UPDATE calendar_publication SET revision=revision+1", [])?;
        tx.commit()?;
        Ok(())
    }
    fn calendar_connection(&self, account: &str, kind: &str) -> Result<i64> {
        crate::id(account)?;
        if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Live Calendar sync requires a live profile",
            ));
        }
        let row:Option<(String,i64,String)>=self.db.query_row("SELECT connection_state,connection_generation,scopes_json FROM google_accounts WHERE account=?1",[account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (state, generation, scopes) =
            row.ok_or_else(|| err("not_found", "Google account is not connected"))?;
        let scopes: std::collections::BTreeSet<String> = serde_json::from_str(&scopes)?;
        let scope = match kind {
            "catalog" => "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
            "busy" => "https://www.googleapis.com/auth/calendar.events.freebusy",
            _ => "https://www.googleapis.com/auth/calendar.events",
        };
        if state != "connected" || !scopes.contains(scope) {
            return Err(err(
                "permission_denied",
                "Google Calendar scope or connection is unavailable",
            ));
        }
        Ok(generation)
    }
    fn calendar_run(&self, lease: &CalendarSyncLease) -> Result<Run> {
        self.db.query_row("SELECT kind,mode,state,query_fingerprint,base_revision,permission_epoch,connection_generation,projection_epoch,next_page_token,final_sync_token,identity_complete,errors FROM calendar_sync_runs WHERE id=?1 AND account=?2 AND calendar_id=?3",params![lease.run,lease.account,lease.calendar],|r|Ok(Run{kind:r.get(0)?,mode:r.get(1)?,state:r.get(2)?,query:r.get(3)?,base:r.get(4)?,permission:r.get(5)?,connection:r.get(6)?,projection:r.get(7)?,next:r.get(8)?,final_token:r.get(9)?,identity_complete:r.get(10)?,errors:r.get(11)?})).optional()?.ok_or_else(||err("not_found","Calendar run does not exist"))
    }
    fn check_calendar_run(&self, lease: &CalendarSyncLease, run: &Run) -> Result<()> {
        if run.state != "staging"
            || self.calendar_connection(&lease.account, &run.kind)? != run.connection
            || lease.connection_generation != run.connection
        {
            return Err(err("stale_revision", "Calendar connection or run changed"));
        }
        if run.kind == "events" {
            if self.calendar_membership_denied(&lease.account, &lease.calendar)? {
                return Err(err("stale_revision", "Calendar membership changed"));
            }
            let current:(i64,i64)=self.db.query_row("SELECT revision,permission_epoch FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",params![lease.account,lease.calendar],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if current != (run.base, run.permission) {
                return Err(err(
                    "stale_revision",
                    "Calendar source or permission changed",
                ));
            }
        } else {
            let revision: i64 = self.db.query_row(
                "SELECT revision FROM calendar_catalog_state WHERE account=?1",
                [&lease.account],
                |r| r.get(0),
            )?;
            if revision != run.base {
                return Err(err("stale_revision", "Calendar catalog checkpoint changed"));
            }
        }
        Ok(())
    }
    pub(crate) fn calendar_membership_denied(&self, account: &str, calendar: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT COALESCE(k.deleted,0)=1 OR COALESCE(k.observed_epoch,0)<COALESCE(c.full_generation,0) FROM calendar_catalog_state c LEFT JOIN calendar_catalog k ON k.account=c.account AND k.calendar_id=?2 WHERE c.account=?1",params![account,calendar],|r|r.get(0)).optional()?.unwrap_or(false))
    }
    /// A current explicit provider observation fences earlier event and catalog responses.
    /// The caller supplies the generation captured before that HTTP request.
    pub fn calendar_observe_catalog_entry(
        &mut self,
        account: &str,
        source: &Value,
        connection_generation: i64,
        expected_permission_epoch: i64,
    ) -> Result<i64> {
        if !valid_catalog(source) || serde_json::to_vec(source)?.len() > 2 * 1024 * 1024 {
            return Err(err(
                "invalid_response",
                "Calendar metadata is invalid or exceeds bounds",
            ));
        }
        let calendar = source["id"].as_str().unwrap();
        let checkpoint = self.calendar_checkpoint(account, calendar)?;
        let zone = source["timeZone"]
            .as_str()
            .or(checkpoint["time_zone"].as_str())
            .unwrap();
        let epoch = self.calendar_observe_permission(
            account,
            calendar,
            connection_generation,
            expected_permission_epoch,
            parse_access(role(source))?,
            zone,
        )?;
        // The permission fence above is durable first. A crash before this
        // metadata refresh leaves conservative old display metadata, not grants.
        self.db.execute(
            "UPDATE calendar_catalog SET json=?3,deleted=?4 WHERE account=?1 AND calendar_id=?2",
            params![
                account,
                calendar,
                serde_json::to_string(source)?,
                source["deleted"] == true
            ],
        )?;
        Ok(epoch)
    }

    pub fn calendar_observe_permission(
        &mut self,
        account: &str,
        calendar: &str,
        connection_generation: i64,
        expected_permission_epoch: i64,
        access: Access,
        zone: &str,
    ) -> Result<i64> {
        provider_id(calendar)?;
        // A catalog-entry response also establishes the current role, including
        // accounts that granted catalog/free-busy access without event details.
        let current_connection = self
            .calendar_connection(account, "catalog")
            .or_else(|error| {
                if error.code == "permission_denied" {
                    self.calendar_connection(account, "events")
                } else {
                    Err(error)
                }
            })?;
        if current_connection != connection_generation || !valid_zone(zone) {
            return Err(err(
                "stale_revision",
                "Calendar permission observation is stale or invalid",
            ));
        }
        let tx = self.db.unchecked_transaction()?;
        let (old_access,old_zone,old_revision):(String,String,i64)=tx.query_row(
            "SELECT access,time_zone,revision FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
            params![account,calendar],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        )?;
        let stamp: i64 = tx.query_row("SELECT revision+1 FROM calendar_publication", [], |r| {
            r.get(0)
        })?;
        let access = crate::calendar_store::access_name(access);
        if tx.execute("UPDATE calendar_sync_state SET sync_token=CASE WHEN access<>?3 OR time_zone<>?4 THEN NULL ELSE sync_token END,retained_errors=CASE WHEN access<>?3 OR time_zone<>?4 THEN MAX(retained_errors,1) ELSE retained_errors END,access=?3,time_zone=?4,permission_observation=?5,permission_epoch=permission_epoch+1,revision=revision+1 WHERE account=?1 AND calendar_id=?2 AND permission_epoch=?6",params![account,calendar,access,zone,stamp,expected_permission_epoch])?!=1{return Err(err("stale_revision","Calendar permission observation changed"));}
        if old_access == access && old_zone == zone {
            // The explicit observation still fences older permission replies. An
            // unchanged policy can carry forward staged data without restarting
            // a large calendar on every periodic permission revalidation.
            tx.execute("UPDATE calendar_sync_runs SET base_revision=base_revision+1,permission_epoch=permission_epoch+1 WHERE account=?1 AND calendar_id=?2 AND kind='events' AND state='staging' AND base_revision=?3 AND permission_epoch=?4",params![account,calendar,old_revision,expected_permission_epoch])?;
        }
        let epoch = tx.query_row(
            "SELECT permission_epoch FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
            params![account, calendar],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(epoch)
    }
    fn prepare_calendar_event(
        &self,
        lease: &CalendarSyncLease,
        id: &str,
        observed: &Value,
        zone: &str,
    ) -> Result<Event> {
        let mut source = observed.clone();
        if source["status"] == "cancelled" {
            let old:Option<String>=self.db.query_row("SELECT COALESCE(CASE WHEN pending_generation=?4 THEN pending_json END,json) FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id,lease.run],|r|r.get(0)).optional()?;
            if let Some(old) = old {
                source = serde_json::from_str(&old)?;
                for (k, v) in observed
                    .as_object()
                    .ok_or_else(|| err("invalid_event", "Calendar source must be an object"))?
                {
                    source[k] = v.clone();
                }
            }
        }
        fm_calendar::normalize(&lease.account, &lease.calendar, zone, &source)
            .map_err(calendar_error)
    }
}

fn validate_page(page: &CalendarPage) -> Result<()> {
    if page.items.len() > 250
        || page.next_page_token.is_some() == page.next_sync_token.is_some()
        || serde_json::to_vec(&page.items)?.len() > 16 * 1024 * 1024
    {
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
    Ok(())
}
fn stage_event(
    tx: &rusqlite::Transaction<'_>,
    lease: &CalendarSyncLease,
    run: &Run,
    id: &str,
    observed: &str,
    event: &Event,
) -> Result<()> {
    let existing:Option<i64>=tx.query_row("SELECT source_generation FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id],|r|r.get(0)).optional()?;
    let raw = serde_json::to_string(&event.source)?;
    let full: i64 = if run.mode == "full" {
        lease.run
    } else {
        tx.query_row(
            "SELECT full_generation FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
            params![lease.account, lease.calendar],
            |r| r.get(0),
        )?
    };
    let slot = if existing.is_none() || existing == Some(lease.run) {
        tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json,source_generation) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET json=excluded.json",params![lease.account,lease.calendar,id,raw,lease.run])?;
        tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json,seen_full_generation) VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET observed_json=excluded.observed_json,seen_full_generation=excluded.seen_full_generation",params![lease.account,lease.calendar,id,observed,full])?;
        0
    } else {
        tx.execute("UPDATE calendar_sources SET pending_generation=?4,pending_json=?5 WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,lease.calendar,id,lease.run,raw])?;
        tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json,pending_visible,pending_observed_json,pending_seen_full_generation) SELECT account,calendar_id,event_id,1,json,1,?4,?5 FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3 ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET pending_visible=1,pending_observed_json=excluded.pending_observed_json,pending_seen_full_generation=excluded.pending_seen_full_generation",params![lease.account,lease.calendar,id,observed,full])?;
        1
    };
    index_event(tx, event, slot, lease.run)
}
pub(crate) fn index_event(
    db: &rusqlite::Connection,
    event: &Event,
    slot: i64,
    generation: i64,
) -> Result<()> {
    let bounds = |time: &Option<EventTime>| match time {
        Some(EventTime::Date { date }) => (None, Some(date.to_string())),
        Some(EventTime::Zoned { instant, .. }) => (Some(instant.timestamp_millis()), None),
        None => (None, None),
    };
    let (start_ms, start_date) = bounds(&event.start);
    let (end_ms, end_date) = bounds(&event.end);
    let (original_ms, original_date) = bounds(&event.original_start);
    db.execute("INSERT INTO calendar_event_index VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(account,calendar_id,event_id,slot) DO UPDATE SET generation=excluded.generation,start_ms=excluded.start_ms,end_ms=excluded.end_ms,start_date=excluded.start_date,end_date=excluded.end_date,series_id=excluded.series_id,original_ms=excluded.original_ms,original_date=excluded.original_date,recurring=excluded.recurring,cancelled=excluded.cancelled",params![event.key.account,event.key.calendar,event.key.event,slot,generation,start_ms,end_ms,start_date,end_date,event.series_id,original_ms,original_date,!event.recurrence.is_empty(),event.cancelled])?;
    Ok(())
}
fn valid_catalog(source: &Value) -> bool {
    source.is_object()
        && source["id"]
            .as_str()
            .is_some_and(|v| provider_id(v).is_ok())
        && (source["deleted"] == true || source["timeZone"].as_str().is_some_and(valid_zone))
}
fn valid_zone(zone: &str) -> bool {
    let day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    zone.len() <= 256 && fm_calendar::Window::from_dates(day, day.succ_opt().unwrap(), zone).is_ok()
}
fn role(source: &Value) -> &'static str {
    if source["deleted"] == true {
        return "none";
    }
    match source["accessRole"].as_str() {
        Some("owner") => "owner",
        Some("writer") => "writer",
        Some("writerWithoutPrivateAccess") => "writer_without_private_access",
        Some("reader") => "reader",
        Some("freeBusyReader") => "free_busy",
        _ => "none",
    }
}
fn rank(role: &str) -> u8 {
    match role {
        "owner" => 5,
        "writer" => 4,
        "writer_without_private_access" => 3,
        "reader" => 2,
        "free_busy" => 1,
        _ => 0,
    }
}
fn restrict_catalog_entry(
    tx: &rusqlite::Transaction<'_>,
    lease: &CalendarSyncLease,
    _projection: i64,
    source: &Value,
) -> Result<()> {
    let id = source["id"].as_str().unwrap();
    let new_role = role(source);
    let old: Option<String> = tx
        .query_row(
            "SELECT access FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
            params![lease.account, id],
            |r| r.get(0),
        )
        .optional()?;
    if old.as_deref().is_some_and(|old| rank(new_role) < rank(old)) {
        tx.execute("UPDATE calendar_sync_state SET access=?3,sync_token=NULL,retained_errors=MAX(retained_errors,1),permission_observation=(SELECT revision+1 FROM calendar_publication),permission_epoch=permission_epoch+1,revision=revision+1 WHERE account=?1 AND calendar_id=?2",params![lease.account,id,new_role])?;
    }
    Ok(())
}
fn publish_catalog_entry(
    tx: &rusqlite::Transaction<'_>,
    lease: &CalendarSyncLease,
    projection: i64,
    source: &Value,
) -> Result<()> {
    let id = source["id"].as_str().unwrap();
    let mut new_role = role(source);
    let existing: Option<(String, String, i64)> = tx
        .query_row(
            "SELECT access,time_zone,permission_observation FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",
            params![lease.account, id],
            |r| Ok((r.get(0)?, r.get(1)?,r.get(2)?)),
        )
        .optional()?;
    let mut zone = source["timeZone"]
        .as_str()
        .or(existing.as_ref().map(|v| v.1.as_str()));
    if let Some((old, old_zone, observed)) = &existing {
        if *observed > projection {
            if rank(old) < rank(new_role) {
                new_role = old;
            }
            zone = Some(old_zone);
        }
    }
    // Deleted catalog entries may contain only ID. Preserve existing zone without granting access.
    if let Some(zone) = zone {
        tx.execute("INSERT INTO calendar_sync_state(account,calendar_id,query_fingerprint,time_zone,access,sync_token,revision) VALUES(?1,?2,?3,?4,?5,NULL,0) ON CONFLICT(account,calendar_id) DO UPDATE SET sync_token=CASE WHEN calendar_sync_state.access<>excluded.access OR calendar_sync_state.time_zone<>excluded.time_zone THEN NULL ELSE calendar_sync_state.sync_token END,retained_errors=CASE WHEN calendar_sync_state.access<>excluded.access OR calendar_sync_state.time_zone<>excluded.time_zone THEN MAX(calendar_sync_state.retained_errors,1) ELSE calendar_sync_state.retained_errors END,time_zone=excluded.time_zone,access=excluded.access,permission_epoch=calendar_sync_state.permission_epoch+CASE WHEN calendar_sync_state.access<>excluded.access OR calendar_sync_state.time_zone<>excluded.time_zone THEN 1 ELSE 0 END,revision=calendar_sync_state.revision+CASE WHEN calendar_sync_state.access<>excluded.access OR calendar_sync_state.time_zone<>excluded.time_zone THEN 1 ELSE 0 END",params![lease.account,id,fm_calendar::google_api::EVENT_SYNC_FINGERPRINT,zone,new_role])?;
    }
    tx.execute("INSERT INTO calendar_catalog VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,calendar_id) DO UPDATE SET json=excluded.json,deleted=excluded.deleted,observed_epoch=excluded.observed_epoch",params![lease.account,id,serde_json::to_string(source)?,source["deleted"]==true,lease.run])?;
    tx.execute("UPDATE calendar_publication SET revision=revision+1", [])?;
    Ok(())
}

#[cfg(test)]
#[path = "live_calendar_tests.rs"]
mod tests;
