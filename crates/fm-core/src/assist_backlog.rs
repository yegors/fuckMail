//! Bounded, read-only pages for explicit historical mail analysis.
use crate::{err, Result, Store};
use fm_assist::ollama::{self, Settings};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

/// Received mail the local model can analyse (same rule as the Backlog scan).
const ELIGIBLE: &str = "m.account=?1 AND NOT m.sent AND NOT m.trashed AND length(m.source_hash)=64 AND length(m.body)>0
    AND NOT EXISTS(SELECT 1 FROM google_message_state g WHERE g.account=m.account AND g.message_id=m.id
        AND (g.deleted=1 OR EXISTS(SELECT 1 FROM json_each(g.provider_labels_json) l WHERE l.value='DRAFT')))";

impl Store {
    /// Cheap counts for the Accounts view and account tabs, safe to poll.
    /// `done` has saved results for every enabled task (not re-validated
    /// against later label or prompt changes, which the watcher requeues);
    /// `queued` is what the arrivals watcher still has ahead of it; the rest
    /// is `unprocessed`, reachable through an explicit Backlog run.
    pub fn local_model_progress(&self, account: Option<&str>) -> Result<Value> {
        let settings: Settings =
            serde_json::from_value(self.local_model_settings()?["settings"].clone())?;
        let accounts: Vec<String> = self
            .db
            .prepare("SELECT id FROM accounts WHERE lifecycle_state='active' AND (?1 IS NULL OR id=?1) ORDER BY id")?
            .query_map([account], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let meta = |key: &str| -> Result<Option<i64>> {
            Ok(self
                .db
                .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
                .optional()?)
        };
        let cursor = meta(crate::local_model::WATCH_CURSOR)?.unwrap_or(0);
        let start = meta(crate::local_model::WATCH_REQUEUE)?.map_or(cursor, |v| v.min(cursor));
        let mut rows = Vec::new();
        for id in accounts {
            let policy = settings.accounts.get(&id).cloned().unwrap_or_default();
            let cutoff = meta(&crate::local_model::history_cutoff_key(&id))?;
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
            let done_rule = if tasks.is_empty() {
                "EXISTS(SELECT 1 FROM local_model_results r WHERE r.account=m.account AND r.target=m.id)".to_owned()
            } else {
                tasks.join(" AND ")
            };
            let count = |rule: &str| -> Result<i64> {
                Ok(self.db.query_row(
                    &format!("SELECT count(*) FROM messages m WHERE {ELIGIBLE} AND {rule}"),
                    params![id],
                    |r| r.get(0),
                )?)
            };
            let local: i64 = self.db.query_row(
                "SELECT count(*) FROM messages m WHERE m.account=?1 AND NOT EXISTS(SELECT 1 FROM google_message_state g WHERE g.account=m.account AND g.message_id=m.id AND g.deleted=1)",
                [&id],
                |r| r.get(0),
            )?;
            let eligible = count("1")?;
            let done = count(&done_rule)?;
            let enabled =
                settings.model.is_some() && (policy.summaries || policy.classification);
            let queued = if enabled && !policy.paused {
                let history = cutoff.map_or(String::new(), |ms| {
                    format!(" AND (julianday(m.sent_at)-2440587.5)*86400000.0>={ms}")
                });
                count(&format!("m.rowid>{start} AND NOT ({done_rule}){history}"))?
            } else {
                0
            };
            rows.push(json!({"account":id,"local":local,"eligible":eligible,"done":done,"queued":queued,
                "unprocessed":(eligible-done-queued).max(0),"model_configured":settings.model.is_some(),
                "enabled":enabled,"summaries":policy.summaries,"classification":policy.classification,
                "paused":policy.paused,"history_cutoff":cutoff}));
        }
        Ok(json!({"accounts":rows}))
    }
    pub fn local_model_backlog_eligible(&self, account: &str, message: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM messages m WHERE m.account=?1 AND m.id=?2
            AND NOT m.sent AND NOT m.trashed AND length(m.source_hash)=64 AND length(m.body)>0
            AND NOT EXISTS(SELECT 1 FROM google_message_state g WHERE g.account=m.account AND g.message_id=m.id
                AND (g.deleted=1 OR EXISTS(SELECT 1 FROM json_each(g.provider_labels_json) l WHERE l.value='DRAFT'))))",
            params![account,message], |r| r.get(0))?)
    }
    pub fn local_model_backlog_context(&self, account: &str) -> Result<Value> {
        let lifecycle = self.require_active_account(account)?;
        let state = self.local_model_settings()?;
        let settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        let through: i64 = self.db.query_row(
            "SELECT COALESCE(MAX(rowid),0) FROM messages WHERE account=?1",
            [account],
            |r| r.get(0),
        )?;
        Ok(
            json!({"account":account,"lifecycle":lifecycle,"settings_revision":state["revision"],
            "through":through,"until":chrono::Utc::now().to_rfc3339(),
            "enabled":settings.model.is_some() && (policy.summaries || policy.classification),
            "summaries":policy.summaries,"classification":policy.classification}),
        )
    }

    pub fn local_model_backlog_validate(&self, context: &Value) -> Result<()> {
        let account = context["account"].as_str().unwrap_or_default();
        if self.require_active_account(account)? != context["lifecycle"]
            || self.local_model_settings()?["revision"] != context["settings_revision"]
        {
            return Err(err(
                "assistance_stale",
                "Account or local model settings changed; start again",
            ));
        }
        Ok(())
    }

    /// Returns at most 32 metadata rows; never prepares an entire mailbox in memory.
    pub fn local_model_backlog_page(&self, context: &Value, after: i64) -> Result<Value> {
        self.local_model_backlog_validate(context)?;
        let account = context["account"].as_str().unwrap_or_default();
        let mut stmt = self.db.prepare("SELECT m.rowid,m.id,m.conversation_id,m.sent_at,
            NOT m.sent AND NOT m.trashed AND length(m.source_hash)=64 AND length(m.body)>0
            AND NOT EXISTS(SELECT 1 FROM google_message_state g,json_each(g.provider_labels_json) l
                WHERE g.account=m.account AND g.message_id=m.id AND l.value='DRAFT') AS eligible
            FROM messages m WHERE m.account=?1 AND m.rowid>?2 AND m.rowid<=?3
            AND NOT EXISTS(SELECT 1 FROM google_message_state g WHERE g.account=m.account AND g.message_id=m.id AND g.deleted=1)
            ORDER BY m.rowid LIMIT 32")?;
        let rows = stmt
            .query_map(
                params![account, after, context["through"].as_i64().unwrap_or(0)],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, bool>(4)?,
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut items = Vec::new();
        let mut next = after;
        for (row, id, conversation, date, eligible) in &rows {
            next = *row;
            let task = if *eligible {
                self.local_model_missing_task(account, conversation, id)?
            } else {
                None
            };
            items.push(json!({"id":id,"conversation_id":conversation,"date":date,"eligible":eligible,"task":task}));
        }
        Ok(json!({"after":next,"done":rows.len()<32,"items":items}))
    }

    /// Select only missing projections so a valid summary is never overwritten by a backlog run.
    pub fn local_model_missing_task(
        &self,
        account: &str,
        conversation: &str,
        target: &str,
    ) -> Result<Option<String>> {
        let settings: Settings =
            serde_json::from_value(self.local_model_settings()?["settings"].clone())?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        let mut missing = Vec::new();
        for (task, enabled) in [
            ("summary", policy.summaries),
            ("classification", policy.classification),
        ] {
            if !enabled {
                continue;
            }
            let saved: Option<(String,String)> = self.db.query_row(
                "SELECT snapshot_json,result_json FROM local_model_results WHERE account=?1 AND conversation_id=?2 AND target=?3 AND task=?4",
                params![account,conversation,target,task], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
            let current = saved.is_some_and(|(snapshot, result)| {
                let valid = serde_json::from_str::<Value>(&result).is_ok_and(|v| {
                    task != "summary"
                        || ollama::valid_summary(
                            v["output"]["summary"].as_str().unwrap_or_default(),
                        )
                });
                valid
                    && self
                        .local_model_snapshot(account, conversation, task, target)
                        .and_then(|v| Ok(serde_json::to_string(&v)?))
                        .is_ok_and(|s| s == snapshot)
            });
            if !current {
                missing.push(task);
            }
        }
        Ok(match missing.as_slice() {
            [] => None,
            [task] => Some((*task).into()),
            _ => Some("message".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ACCOUNT: &str = "windscribe";
    const CONVERSATION: &str = "windscribe-thread-001";
    const MESSAGE: &str = "windscribe-message-001-000";
    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open_synthetic(dir.path()).unwrap();
        let settings = Settings {
            model: Some(ollama::Model {
                name: "fixture".into(),
                digest: "a".repeat(64),
            }),
            ..Default::default()
        };
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":0,"settings":settings}),
        )
        .unwrap();
        (dir, s)
    }
    fn output() -> Value {
        json!({"summary":"You need to review the design by Friday.","source_ids":[MESSAGE],"sender_kind":"human","category_ids":[],"category_abstain":true,"reason":"No matching label"})
    }
    #[test]
    fn backlog_preserves_current_summary_and_review_state() {
        let (_dir, s) = store();
        let flags = || {
            s.db.query_row(
                "SELECT unread,starred FROM messages WHERE account=?1 AND id=?2",
                params![ACCOUNT, MESSAGE],
                |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?)),
            )
            .unwrap()
        };
        let before = flags();
        assert_eq!(
            s.local_model_missing_task(ACCOUNT, CONVERSATION, MESSAGE)
                .unwrap()
                .as_deref(),
            Some("message")
        );
        let summary = s
            .local_model_prepare(ACCOUNT, CONVERSATION, "summary", MESSAGE)
            .unwrap();
        s.local_model_finish(
            &summary,
            json!({"summary":output()["summary"],"source_ids":[MESSAGE]}),
        )
        .unwrap();
        let saved = || {
            s.db.query_row("SELECT result_json FROM local_model_results WHERE account=?1 AND target=?2 AND task='summary'",params![ACCOUNT,MESSAGE],|r|r.get::<_,String>(0)).unwrap()
        };
        let original = saved();
        assert_eq!(
            s.local_model_missing_task(ACCOUNT, CONVERSATION, MESSAGE)
                .unwrap()
                .as_deref(),
            Some("classification")
        );
        let plan = s
            .local_model_prepare(ACCOUNT, CONVERSATION, "classification", MESSAGE)
            .unwrap();
        let mut classification = output();
        classification.as_object_mut().unwrap().remove("summary");
        classification.as_object_mut().unwrap().remove("source_ids");
        s.local_model_finish(&plan, classification).unwrap();
        assert_eq!(saved(), original);
        assert!(s
            .local_model_missing_task(ACCOUNT, CONVERSATION, MESSAGE)
            .unwrap()
            .is_none());
        assert_eq!(flags(), before);
        s.db.execute(
            "UPDATE messages SET body='Changed source' WHERE account=?1 AND id=?2",
            params![ACCOUNT, MESSAGE],
        )
        .unwrap();
        assert_eq!(
            s.local_model_missing_task(ACCOUNT, CONVERSATION, MESSAGE)
                .unwrap()
                .as_deref(),
            Some("message")
        );
    }
    #[test]
    fn backlog_pages_are_bounded_scoped_and_read_only() {
        let (_dir, s) = store();
        let context = s.local_model_backlog_context(ACCOUNT).unwrap();
        let revision = s.revision().unwrap();
        let mut after = 0;
        let mut count = 0;
        loop {
            let page = s.local_model_backlog_page(&context, after).unwrap();
            assert!(page["items"].as_array().unwrap().len() <= 32);
            for row in page["items"].as_array().unwrap() {
                assert!(row["id"].as_str().unwrap().starts_with("windscribe-"));
                count += 1;
            }
            if page["done"] == true {
                break;
            }
            assert!(page["after"].as_i64().unwrap() > after);
            after = page["after"].as_i64().unwrap();
        }
        assert!(count > 32);
        assert_eq!(s.revision().unwrap(), revision);
        assert!(s.local_model_backlog_context("missing").is_err());
        assert!(s.local_model_backlog_eligible(ACCOUNT, MESSAGE).unwrap());
        for field in ["sent", "trashed"] {
            s.db.execute(
                &format!("UPDATE messages SET {field}=1 WHERE account=?1 AND id=?2"),
                params![ACCOUNT, MESSAGE],
            )
            .unwrap();
            assert!(!s.local_model_backlog_eligible(ACCOUNT, MESSAGE).unwrap());
            s.db.execute(
                &format!("UPDATE messages SET {field}=0 WHERE account=?1 AND id=?2"),
                params![ACCOUNT, MESSAGE],
            )
            .unwrap();
        }
        s.db.execute(
            "UPDATE messages SET body='' WHERE account=?1 AND id=?2",
            params![ACCOUNT, MESSAGE],
        )
        .unwrap();
        assert!(!s.local_model_backlog_eligible(ACCOUNT, MESSAGE).unwrap());
    }
    #[test]
    fn backlog_settings_lifecycle_and_opt_out_fences() {
        let (_dir, mut s) = store();
        let context = s.local_model_backlog_context(ACCOUNT).unwrap();
        let mut settings = s.local_model_settings().unwrap()["settings"].clone();
        settings["accounts"] =
            json!({ACCOUNT:{"classification":false,"summaries":false,"descriptions":{}}});
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":1,"settings":settings}),
        )
        .unwrap();
        assert_eq!(
            s.local_model_backlog_page(&context, 0).unwrap_err().code,
            "assistance_stale"
        );
        let context = s.local_model_backlog_context(ACCOUNT).unwrap();
        assert_eq!(context["enabled"], false);
        assert!(s
            .local_model_missing_task(ACCOUNT, CONVERSATION, MESSAGE)
            .unwrap()
            .is_none());
        s.db.execute(
            "UPDATE accounts SET lifecycle_revision=lifecycle_revision+1 WHERE id=?1",
            [ACCOUNT],
        )
        .unwrap();
        assert!(s.local_model_backlog_validate(&context).is_err());
    }
}
