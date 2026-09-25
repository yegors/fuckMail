//! The local model's one queue. Nothing is stored for it: the queue is every
//! eligible received email without a saved result, in two classes, newest
//! first within each. **New** is mail dated from the account's history cutoff
//! (all mail when there is none). **Catch up** is older mail down to the
//! account's Catch up bound. New mail therefore goes ahead of a running Catch
//! up. Explicit requests are handled by the engine before either class.
use crate::local_model::{catch_up_key, history_cutoff_key, MAX_ATTEMPTS};
use crate::{err, Result, Store};
use fm_assist::ollama::Account;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

/// Received mail the local model can analyse.
const ELIGIBLE: &str = "m.account=?1 AND NOT m.sent AND NOT m.trashed AND length(m.source_hash)=64 AND length(m.body)>0
    AND NOT EXISTS(SELECT 1 FROM google_message_state g WHERE g.account=m.account AND g.message_id=m.id
        AND (g.deleted=1 OR EXISTS(SELECT 1 FROM json_each(g.provider_labels_json) l WHERE l.value='DRAFT')))";
const DATE_MS: &str = "((julianday(m.sent_at)-2440587.5)*86400000.0)";
fn exhausted() -> String {
    format!("EXISTS(SELECT 1 FROM local_model_failures f WHERE f.account=m.account AND f.message_id=m.id AND f.attempts>={MAX_ATTEMPTS})")
}
/// A message is done when every enabled result is saved. With both off, any
/// saved result counts, so progress still shows past work.
fn done_rule(policy: &Account) -> String {
    let result = |task: &str| {
        format!("EXISTS(SELECT 1 FROM local_model_results r WHERE r.account=m.account AND r.conversation_id=m.conversation_id AND r.target=m.id AND r.task='{task}')")
    };
    let mut tasks = Vec::new();
    if policy.summaries {
        tasks.push(result("summary"));
    }
    if policy.classification {
        tasks.push(result("classification"));
    }
    if tasks.is_empty() {
        "EXISTS(SELECT 1 FROM local_model_results r WHERE r.account=m.account AND r.target=m.id)"
            .into()
    } else {
        tasks.join(" AND ")
    }
}
struct Bounds {
    cutoff: Option<i64>,
    catch_up: Option<i64>,
}
impl Bounds {
    fn new_class(&self) -> String {
        self.cutoff
            .map_or("1".into(), |ms| format!("{DATE_MS}>={ms}"))
    }
    /// None when this account has nothing in the Catch up class.
    fn catch_up_class(&self) -> Option<String> {
        let (cutoff, from) = (self.cutoff?, self.catch_up?);
        (from < cutoff).then(|| format!("{DATE_MS}<{cutoff} AND {DATE_MS}>={from}"))
    }
    /// Everything the queue reaches for this account.
    fn queued(&self) -> String {
        match (self.cutoff, self.catch_up) {
            (None, _) => "1".into(),
            (Some(cutoff), Some(from)) => format!("{DATE_MS}>={}", from.min(cutoff)),
            (Some(cutoff), None) => format!("{DATE_MS}>={cutoff}"),
        }
    }
}
impl Store {
    fn meta_value(&self, key: &str) -> Result<Option<i64>> {
        Ok(self
            .db
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .optional()?)
    }
    fn bounds(&self, account: &str) -> Result<Bounds> {
        Ok(Bounds {
            cutoff: self.meta_value(&history_cutoff_key(account))?,
            catch_up: self.meta_value(&catch_up_key(account))?,
        })
    }
    /// The next emails for the model, up to `limit`, all from the first
    /// account and class with work (empty when the queue is empty). Accounts in
    /// `held` (paused in this session) are skipped; within each class accounts
    /// take turns, starting after `after`. `cursors` holds where earlier batches
    /// stopped (`{account, class, date, id}`), so mail already passed is not
    /// scanned again.
    pub fn local_model_next(
        &self,
        held: &[String],
        after: Option<&str>,
        limit: usize,
        cursors: &[Value],
    ) -> Result<Vec<Value>> {
        let settings = self.local_model_policy()?;
        if settings.model.is_none() {
            return Ok(vec![]);
        }
        let mut accounts: Vec<String> = self
            .db
            .prepare("SELECT id FROM accounts WHERE lifecycle_state='active' ORDER BY id")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        accounts.retain(|a| {
            let policy = settings.accounts.get(a).cloned().unwrap_or_default();
            (policy.summaries || policy.classification) && !policy.paused && !held.contains(a)
        });
        if let Some(pos) = after.and_then(|a| accounts.iter().position(|x| x == a)) {
            accounts.rotate_left(pos + 1);
        }
        let mut classes = Vec::new();
        for account in &accounts {
            classes.push(("new", account.clone(), self.bounds(account)?.new_class()));
        }
        for account in &accounts {
            if let Some(rule) = self.bounds(account)?.catch_up_class() {
                classes.push(("catch_up", account.clone(), rule));
            }
        }
        for (class, account, rule) in classes {
            let policy = settings.accounts.get(&account).cloned().unwrap_or_default();
            let cursor = cursors
                .iter()
                .find(|c| c["class"] == class && c["account"] == account.as_str());
            let (after_date, after_id) = (
                cursor.and_then(|c| c["date"].as_str()),
                cursor.and_then(|c| c["id"].as_str()),
            );
            let sql = format!("SELECT m.id,m.conversation_id,m.sent_at FROM messages m WHERE {ELIGIBLE} AND NOT ({}) AND NOT {} AND {rule}
                AND (?2 IS NULL OR m.sent_at<?2 OR (m.sent_at=?2 AND m.id<?3)) ORDER BY m.sent_at DESC,m.id DESC LIMIT ?4", done_rule(&policy), exhausted());
            let rows = self
                .db
                .prepare(&sql)?
                .query_map(params![account, after_date, after_id, limit as i64], |r| {
                    Ok(json!({"account":account,"id":r.get::<_,String>(0)?,"conversation_id":r.get::<_,String>(1)?,"date":r.get::<_,String>(2)?,"class":class}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if !rows.is_empty() {
                return Ok(rows);
            }
        }
        Ok(vec![])
    }
    /// Whether one email still needs the model: eligible, enabled, without its
    /// results and not failed out. Checked just before inference.
    pub fn local_model_pending(&self, account: &str, message: &str) -> Result<bool> {
        let settings = self.local_model_policy()?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        if settings.model.is_none() || (!policy.summaries && !policy.classification) {
            return Ok(false);
        }
        Ok(self.db.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM messages m WHERE {ELIGIBLE} AND m.id=?2 AND NOT ({}) AND NOT {})", done_rule(&policy), exhausted()),
            params![account, message],
            |r| r.get(0),
        )?)
    }
    /// The newest stored message row: it grows when new mail is stored, which
    /// sends the queue back to its top.
    pub fn local_model_newest_row(&self) -> Result<i64> {
        Ok(self
            .db
            .query_row("SELECT COALESCE(MAX(rowid),0) FROM messages", [], |r| r.get(0))?)
    }
    /// Per-account counts for the Accounts view and account tabs, one pass
    /// over the account's mail each.
    pub fn local_model_progress(&self, account: Option<&str>) -> Result<Value> {
        let settings = self.local_model_policy()?;
        let accounts: Vec<String> = self
            .db
            .prepare("SELECT id FROM accounts WHERE lifecycle_state='active' AND (?1 IS NULL OR id=?1) ORDER BY id")?
            .query_map([account], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut rows = Vec::new();
        for id in accounts {
            let policy = settings.accounts.get(&id).cloned().unwrap_or_default();
            let bounds = self.bounds(&id)?;
            let enabled = settings.model.is_some() && (policy.summaries || policy.classification);
            let local: i64 = self.db.query_row(
                "SELECT count(*) FROM messages m WHERE m.account=?1 AND NOT EXISTS(SELECT 1 FROM google_message_state g WHERE g.account=m.account AND g.message_id=m.id AND g.deleted=1)",
                [&id],
                |r| r.get(0),
            )?;
            let catch_up = bounds.catch_up_class().unwrap_or_else(|| "0".into());
            let (eligible, done, failed, queued, catching_up): (i64, i64, i64, i64, i64) = self.db.query_row(
                &format!("SELECT count(*),COALESCE(SUM(done),0),COALESCE(SUM(NOT done AND failed),0),COALESCE(SUM(NOT done AND NOT failed AND queued),0),COALESCE(SUM(NOT done AND NOT failed AND catch_up),0)
                    FROM (SELECT {} AS done,{} AS failed,{} AS queued,{catch_up} AS catch_up FROM messages m WHERE {ELIGIBLE})",
                    done_rule(&policy), exhausted(), bounds.queued()),
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            let queued = if enabled { queued } else { 0 };
            rows.push(json!({"account":id,"local":local,"eligible":eligible,"done":done,"queued":queued,
                "failed":failed,"unprocessed":(eligible-done-queued-failed).max(0),
                "model_configured":settings.model.is_some(),"enabled":enabled,
                "summaries":policy.summaries,"classification":policy.classification,
                "paused":policy.paused,"history_cutoff":bounds.cutoff,"catch_up_from":bounds.catch_up,
                "catching_up":enabled && catching_up > 0}));
        }
        Ok(json!({"accounts":rows}))
    }
    /// How many unprocessed emails each Catch up range would add.
    pub fn local_model_catch_up_counts(&self, account: &str) -> Result<Value> {
        self.require_active_account(account)?;
        let settings = self.local_model_policy()?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        let enabled = settings.model.is_some() && (policy.summaries || policy.classification);
        let bounds = self.bounds(account)?;
        let now = chrono::Utc::now().timestamp_millis();
        let day = 86_400_000i64;
        // Mail already in the queue is not counted again.
        let outside = format!("NOT ({})", bounds.queued());
        let (week, month, year, all): (i64, i64, i64, i64) = self.db.query_row(
            &format!("SELECT COALESCE(SUM(d>=?2),0),COALESCE(SUM(d>=?3),0),COALESCE(SUM(d>=?4),0),count(*) FROM (SELECT {DATE_MS} AS d FROM messages m
                WHERE {ELIGIBLE} AND NOT ({}) AND NOT {} AND {outside} AND {DATE_MS}<=?5)", done_rule(&policy), exhausted()),
            params![account, now - 7 * day, now - 30 * day, now - 365 * day, now + 60_000],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        Ok(json!({"account":account,"enabled":enabled,"ranges":{"week":week,"month":month,"year":year,"all":all}}))
    }
    /// Widen the queue to older mail: `week`, `month`, `year` or `all`.
    /// The bound only moves further back; `stop` removes it.
    pub fn local_model_catch_up(&self, account: &str, range: &str) -> Result<Value> {
        self.require_active_account(account)?;
        let day = 86_400_000i64;
        let now = chrono::Utc::now().timestamp_millis();
        let from = match range {
            "week" => now - 7 * day,
            "month" => now - 30 * day,
            "year" => now - 365 * day,
            // Year 1: every readable date.
            "all" => -62_135_596_800_000,
            "stop" => {
                self.db
                    .execute("DELETE FROM meta WHERE key=?1", [catch_up_key(account)])?;
                return self.local_model_progress(Some(account));
            }
            _ => return Err(err("invalid_request", "Choose week, month, year, all or stop")),
        };
        self.db.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=min(value,excluded.value)",
            params![catch_up_key(account), from],
        )?;
        self.local_model_progress(Some(account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_model::set_history_analysis;
    use fm_assist::ollama::{self, Settings};
    const ACCOUNT: &str = "windscribe";
    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open_synthetic(dir.path()).unwrap();
        let mut settings = Settings {
            model: Some(ollama::Model {
                name: "fixture".into(),
                digest: "a".repeat(64),
            }),
            ..Default::default()
        };
        // Only the one account under test; others would share the queue.
        let others: Vec<String> = s
            .db
            .prepare("SELECT id FROM accounts WHERE id<>?1")
            .unwrap()
            .query_map([ACCOUNT], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for other in others {
            settings.accounts.insert(
                other,
                Account {
                    summaries: false,
                    classification: false,
                    ..Default::default()
                },
            );
        }
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":0,"settings":settings}),
        )
        .unwrap();
        (dir, s)
    }
    fn finish(s: &Store, next: &Value) {
        let plan = s
            .local_model_prepare(
                ACCOUNT,
                next["conversation_id"].as_str().unwrap(),
                next["id"].as_str().unwrap(),
            )
            .unwrap();
        s.local_model_finish(
            &plan,
            json!({"summary":"You are asked to review.","sender_kind":"human","category_ids":[]}),
        )
        .unwrap();
    }
    fn set_date(s: &Store, id: &str, at: chrono::DateTime<chrono::Utc>) {
        s.db.execute(
            "UPDATE messages SET sent_at=?3 WHERE account=?1 AND id=?2",
            params![ACCOUNT, id, at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()],
        )
        .unwrap();
    }
    fn progress(s: &Store) -> Value {
        s.local_model_progress(Some(ACCOUNT)).unwrap()["accounts"][0].clone()
    }
    #[test]
    fn the_queue_runs_newest_first_and_empties_once_everything_is_saved() {
        let (_dir, s) = store();
        let start = progress(&s);
        assert!(start["queued"].as_i64().unwrap() > 4);
        assert_eq!(start["queued"], start["eligible"]);
        let mut last: Option<String> = None;
        let mut seen = 0;
        while let Some(next) = s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next() {
            let date = next["date"].as_str().unwrap().to_owned();
            assert!(last.as_ref().is_none_or(|l| *l >= date), "newest first");
            assert_eq!(next["class"], "new");
            finish(&s, &next);
            last = Some(date);
            seen += 1;
            assert!(seen <= 1000);
        }
        let end = progress(&s);
        assert_eq!(end["queued"], 0);
        assert_eq!(end["done"], end["eligible"]);
        assert_eq!(seen, start["queued"].as_i64().unwrap());
    }
    #[test]
    fn new_mail_goes_ahead_of_catch_up_and_catch_up_only_widens() {
        let (_dir, s) = store();
        set_history_analysis(&s.db, ACCOUNT, false).unwrap();
        let before = progress(&s);
        assert_eq!(before["queued"], 0, "fixture mail predates the cutoff");
        assert_eq!(before["unprocessed"], before["eligible"]);
        assert!(s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().is_none());
        let ids: Vec<String> = s
            .db
            .prepare("SELECT id FROM messages WHERE account=?1 AND NOT sent ORDER BY rowid")
            .unwrap()
            .query_map([ACCOUNT], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let now = chrono::Utc::now();
        set_date(&s, &ids[0], now - chrono::Duration::days(3));
        let counts = s.local_model_catch_up_counts(ACCOUNT).unwrap();
        assert_eq!(counts["ranges"]["week"], 1);
        assert_eq!(counts["ranges"]["all"], before["eligible"]);
        s.local_model_catch_up(ACCOUNT, "all").unwrap();
        let first = s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().unwrap();
        assert_eq!(first["id"], ids[0], "newest catch-up mail first");
        assert_eq!(first["class"], "catch_up");
        assert_eq!(progress(&s)["catching_up"], true);
        // Mail arriving now overtakes the running catch-up.
        set_date(&s, &ids[1], now + chrono::Duration::seconds(5));
        let next = s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().unwrap();
        assert_eq!(next["id"], ids[1]);
        assert_eq!(next["class"], "new");
        // A narrower range never shrinks the queue.
        s.local_model_catch_up(ACCOUNT, "week").unwrap();
        assert_eq!(progress(&s)["unprocessed"], 0);
        assert_eq!(s.local_model_catch_up_counts(ACCOUNT).unwrap()["ranges"]["all"], 0);
        s.local_model_catch_up(ACCOUNT, "stop").unwrap();
        assert_eq!(progress(&s)["queued"], 1, "only the new message remains");
        assert!(s.local_model_catch_up(ACCOUNT, "decade").is_err());
    }
    #[test]
    fn paused_held_and_failing_mail_leave_the_queue() {
        let (_dir, mut s) = store();
        let first = s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().unwrap();
        assert!(s.local_model_next(&[ACCOUNT.into()], None, 1, &[]).unwrap().into_iter().next().is_none());
        let id = first["id"].as_str().unwrap();
        assert_eq!(s.local_model_fail(ACCOUNT, id, "invalid_assistance_result", "Bad JSON").unwrap(), 1);
        assert_eq!(s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().unwrap()["id"], id, "one retry");
        s.local_model_fail(ACCOUNT, id, "invalid_assistance_result", "Bad JSON").unwrap();
        assert_ne!(s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().unwrap()["id"], id);
        assert_eq!(progress(&s)["failed"], 1);
        // An explicit request that succeeds clears the failure.
        finish(&s, &first);
        assert_eq!(progress(&s)["failed"], 0);
        let state = s.local_model_settings().unwrap();
        let mut settings: Settings = serde_json::from_value(state["settings"].clone()).unwrap();
        settings.accounts.entry(ACCOUNT.into()).or_default().paused = true;
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":state["revision"],"settings":settings}),
        )
        .unwrap();
        assert!(s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().is_none());
        let paused = progress(&s);
        assert_eq!(paused["paused"], true);
        assert!(paused["queued"].as_i64().unwrap() > 0, "paused mail stays queued");
    }
    #[test]
    fn accounts_take_turns_within_a_class() {
        let (_dir, mut s) = store();
        let state = s.local_model_settings().unwrap();
        let mut settings: Settings = serde_json::from_value(state["settings"].clone()).unwrap();
        settings.accounts.clear();
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":state["revision"],"settings":settings}),
        )
        .unwrap();
        let first = s.local_model_next(&[], None, 1, &[]).unwrap().into_iter().next().unwrap();
        let account = first["account"].as_str().unwrap();
        let second = s.local_model_next(&[], Some(account), 1, &[]).unwrap().into_iter().next().unwrap();
        assert_ne!(second["account"], account);
    }
    #[test]
    fn batches_continue_where_they_stopped_and_pending_is_checked_per_email() {
        let (_dir, s) = store();
        let first = s.local_model_next(&[], None, 3, &[]).unwrap();
        assert_eq!(first.len(), 3);
        let more = s.local_model_next(&[], None, 3, &[first[2].clone()]).unwrap();
        assert_eq!(more.len(), 3);
        let all = s.local_model_next(&[], None, 6, &[]).unwrap();
        let ids = |rows: &[Value]| rows.iter().map(|r| r["id"].clone()).collect::<Vec<_>>();
        assert_eq!(ids(&all), [ids(&first), ids(&more)].concat(), "a continued batch is the same order");
        let id = first[0]["id"].as_str().unwrap();
        assert!(s.local_model_pending(ACCOUNT, id).unwrap());
        finish(&s, &first[0]);
        assert!(!s.local_model_pending(ACCOUNT, id).unwrap());
        let before = s.local_model_newest_row().unwrap();
        s.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) SELECT account,'queue-new',conversation_id,'queue-new',sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash FROM messages WHERE account=?1 AND id=?2",params![ACCOUNT,id]).unwrap();
        assert!(s.local_model_newest_row().unwrap() > before);
    }
}
