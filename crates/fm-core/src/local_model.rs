//! Core-owned settings, captured targets and derived Ollama results.
use crate::{err, fields, string, Result, Store};
use fm_assist::ollama::{self, Settings};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct Prepared {
    pub account: String,
    pub conversation: String,
    pub task: String,
    pub target: String,
    pub settings: Settings,
    pub payload: Value,
    pub categories: Vec<String>,
    pub sources: Vec<String>,
    snapshot: String,
    pub partial: bool,
    /// Set only by the engine's new-arrival watcher, never by manual or backlog analysis.
    pub arrival: bool,
}
fn encode(v: &Value) -> Result<String> {
    Ok(serde_json::to_string(v)?)
}
/// Last synced message row the new-mail watcher has queued.
pub(crate) const WATCH_CURSOR: &str = "local_model_watch_cursor";
/// Lower bound the watcher rescans from once, after an account's inputs change.
pub(crate) const WATCH_REQUEUE: &str = "local_model_watch_requeue";
/// Mail dated before this cutoff (Unix ms) was already in the account when it
/// was added. The arrivals watcher leaves it to an explicit Backlog run, so a
/// first sync does not send a whole mailbox through the model. No cutoff means
/// the synced history is processed as it arrives.
pub(crate) fn history_cutoff_key(account: &str) -> String {
    format!("local_model_history_cutoff:{account}")
}
pub(crate) fn set_history_analysis(
    db: &rusqlite::Connection,
    account: &str,
    analyze: bool,
) -> Result<()> {
    if analyze {
        db.execute(
            "DELETE FROM meta WHERE key=?1",
            [history_cutoff_key(account)],
        )?;
        // History already stored before this choice is queued too.
        requeue_after(db, "MIN(rowid)-1", Some(account))
    } else {
        db.execute(
            "INSERT OR IGNORE INTO meta(key,value) VALUES(?1,?2)",
            params![
                history_cutoff_key(account),
                chrono::Utc::now().timestamp_millis()
            ],
        )?;
        Ok(())
    }
}
fn requeue_after(db: &rusqlite::Connection, rowid: &str, account: Option<&str>) -> Result<()> {
    db.execute(&format!("INSERT INTO meta(key,value) SELECT ?1,v FROM (SELECT {rowid} AS v FROM messages WHERE ?2 IS NULL OR account=?2) WHERE v IS NOT NULL ON CONFLICT(key) DO UPDATE SET value=min(value,excluded.value)"),params![WATCH_REQUEUE, account])?;
    Ok(())
}
/// Deleting the newest messages lets SQLite reuse their rowids, so the watcher
/// rescans from the highest remaining row and still sees mail stored under them.
pub(crate) fn local_model_messages_deleted(db: &rusqlite::Connection) -> Result<()> {
    requeue_after(db, "COALESCE(MAX(rowid),0)", None)
}
fn failure() -> crate::CoreError {
    err(
        "assistance_stale",
        "Message, labels, manual choices or model settings changed; run again",
    )
}
impl Store {
    pub fn local_model_settings(&self) -> Result<Value> {
        let row: Option<(i64, String)> = self
            .db
            .query_row(
                "SELECT revision,settings_json FROM local_model_settings WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (revision, settings) = match row {
            Some((r, s)) => (r, serde_json::from_str::<Value>(&s)?),
            None => (0, serde_json::to_value(Settings::default())?),
        };
        Ok(json!({"revision":revision,"settings":settings,"defaults":Settings::default()}))
    }
    pub fn local_model_handle(
        &mut self,
        method: &str,
        account: Option<&str>,
        p: &Value,
    ) -> Result<Value> {
        match method {
            "assist_settings_get" => {
                fields(p, &[])?;
                self.local_model_settings()
            }
            "assist_settings_save" => {
                fields(p, &["expected_revision", "settings"])?;
                let settings: Settings = serde_json::from_value(p["settings"].clone())
                    .map_err(|_| err("invalid_request", "Invalid local model settings"))?;
                settings
                    .validate()
                    .map_err(|s| err("invalid_request", &s))?;
                let tx = self.db.unchecked_transaction()?;
                let old = self.local_model_settings()?;
                if p["expected_revision"].as_i64() != old["revision"].as_i64() {
                    return Err(failure());
                }
                for (account, policy) in &settings.accounts {
                    self.require_active_account(account)?;
                    for category in policy.descriptions.keys().chain(policy.auto_read.keys()) {
                        let exists: bool = tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM categories WHERE account=?1 AND id=?2)",
                            params![account, category],
                            |r| r.get(0),
                        )?;
                        if !exists {
                            return Err(err(
                                "invalid_request",
                                "Description refers to an unknown account label",
                            ));
                        }
                    }
                }
                let revision = old["revision"].as_i64().unwrap() + 1;
                tx.execute("INSERT INTO local_model_settings VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,settings_json=excluded.settings_json",params![revision,serde_json::to_string(&settings)?])?;
                // Summary-only edits must not erase independent sender classifications.
                let previous: Settings = serde_json::from_value(old["settings"].clone())?;
                if previous.provider != settings.provider
                    || previous.endpoint != settings.endpoint
                    || previous.model != settings.model
                    || previous.prompt != settings.prompt
                {
                    tx.execute(
                        "DELETE FROM message_sender_kinds WHERE provenance IN ('ollama','openai_compatible')",
                        [],
                    )?;
                } else {
                    let accounts: std::collections::BTreeSet<_> = previous
                        .accounts
                        .keys()
                        .chain(settings.accounts.keys())
                        .collect();
                    for account in accounts {
                        let before = previous.accounts.get(account).cloned().unwrap_or_default();
                        let after = settings.accounts.get(account).cloned().unwrap_or_default();
                        if before.classification != after.classification
                            || before.descriptions != after.descriptions
                        {
                            tx.execute("DELETE FROM message_sender_kinds WHERE provenance IN ('ollama','openai_compatible') AND account=?1", [account])?;
                        }
                    }
                }
                // Turning an account's assistance on, or resuming it, queues its
                // already-synced mail: the watcher skipped it while off or paused.
                for account in previous.accounts.keys().chain(settings.accounts.keys()) {
                    let on = |s: &Settings| {
                        let policy = s.accounts.get(account).cloned().unwrap_or_default();
                        (policy.summaries || policy.classification) && !policy.paused
                    };
                    if !on(&previous) && on(&settings) {
                        self.local_model_requeue_account(account)?;
                    }
                }
                tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
                tx.commit()?;
                self.local_model_settings()
            }
            "assist_catalog" => {
                fields(p, &[])?;
                let a = account.ok_or_else(|| err("account_required", "Choose an account"))?;
                self.require_active_account(a)?;
                self.categories(a)
            }
            "assist_results" => {
                fields(p, &["conversation_id"])?;
                let a = account.ok_or_else(|| err("account_required", "Choose an account"))?;
                self.require_active_account(a)?;
                self.local_model_results(a, string(p, "conversation_id")?)
            }
            "assist_correct" => {
                fields(
                    p,
                    &["conversation_id", "message_id", "kind", "expected_revision"],
                )?;
                let a = account.ok_or_else(|| err("account_required", "Choose an account"))?;
                self.require_active_account(a)?;
                let c = string(p, "conversation_id")?;
                let m = string(p, "message_id")?;
                let k = string(p, "kind")?;
                if !["human", "automated", "unknown", "reset"].contains(&k) {
                    return Err(err(
                        "invalid_request",
                        "Use human, automated, unknown or reset",
                    ));
                }
                let tx = self.db.unchecked_transaction()?;
                let (hash,rev):(String,i64)=tx.query_row("SELECT source_hash,revision FROM messages WHERE account=?1 AND conversation_id=?2 AND id=?3",params![a,c,m],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||err("not_found","Message not found"))?;
                if p["expected_revision"].as_i64() != Some(rev) {
                    return Err(failure());
                }
                if k == "reset" {
                    tx.execute(
                        "DELETE FROM message_sender_kinds WHERE account=?1 AND message_id=?2",
                        params![a, m],
                    )?;
                } else {
                    tx.execute("INSERT INTO message_sender_kinds VALUES(?1,?2,?3,?4,'manual') ON CONFLICT(account,message_id) DO UPDATE SET source_hash=excluded.source_hash,kind=excluded.kind,provenance='manual'",params![a,m,hash,if k=="human"{"people"}else{k}])?;
                }
                tx.execute(
                    "UPDATE messages SET revision=revision+1 WHERE account=?1 AND id=?2",
                    params![a, m],
                )?;
                tx.execute(
                    "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                    params![a, c],
                )?;
                tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
                tx.commit()?;
                Ok(json!({"corrected":true}))
            }
            "assist_accept" => {
                fields(
                    p,
                    &["conversation_id", "message_id", "category_id", "request_id"],
                )?;
                let a = account.ok_or_else(|| err("account_required", "Choose an account"))?;
                let c = string(p, "conversation_id")?;
                let m = string(p, "message_id")?;
                let category = string(p, "category_id")?;
                let results = self.local_model_results(a, c)?;
                let result = results["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| {
                        r["task"] == "classification" && r["target"] == m && r["stale"] == false
                    })
                    .ok_or_else(failure)?;
                if !result["output"]["category_ids"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == category))
                {
                    return Err(err("invalid_request", "Label is not a current suggestion"));
                }
                let revision: i64 = self.db.query_row(
                    "SELECT revision FROM conversations WHERE account=?1 AND id=?2",
                    params![a, c],
                    |r| r.get(0),
                )?;
                let reply=self.handle("set_category",Some(a),json!({"id":c,"message_ids":[m],"category_id":category,"present":true,"expected_revision":revision,"request_id":string(p,"request_id")?}))?;
                let snapshot = self.local_model_snapshot(a, c, "classification", m)?;
                self.db.execute("UPDATE local_model_results SET snapshot_json=?1 WHERE account=?2 AND conversation_id=?3 AND task='classification' AND target=?4",params![encode(&snapshot)?,a,c,m])?;
                Ok(reply)
            }
            _ => Err(err(
                "unsupported_method",
                "Unknown local assistance operation",
            )),
        }
    }
    pub(super) fn local_model_snapshot(
        &self,
        account: &str,
        conversation: &str,
        task: &str,
        target: &str,
    ) -> Result<Value> {
        let lifecycle = self.require_active_account(account)?;
        let settings = self.local_model_settings()?;
        let mut stmt=self.db.prepare("SELECT m.id,m.source_hash,m.revision,m.sent,m.subject,m.sender_name,m.sender_email,m.recipient,m.sent_at,m.body,COALESCE(k.kind,'unknown'),COALESCE(k.provenance,'') FROM messages m LEFT JOIN message_sender_kinds k ON k.account=m.account AND k.message_id=m.id AND k.source_hash=m.source_hash WHERE m.account=?1 AND m.conversation_id=?2 ORDER BY julianday(m.sent_at),m.id LIMIT 201")?;
        let messages=stmt.query_map(params![account,conversation],|r|{let body:String=r.get(9)?;Ok(json!({"id":r.get::<_,String>(0)?,"source_hash":r.get::<_,String>(1)?,"sent":r.get::<_,bool>(3)?,"subject":r.get::<_,String>(4)?,"sender_name":r.get::<_,String>(5)?,"sender_email":r.get::<_,String>(6)?,"recipient":r.get::<_,String>(7)?,"date":r.get::<_,String>(8)?,"text_hash":format!("{:x}",Sha256::digest(body.as_bytes())),"kind":r.get::<_,String>(10)?,"provenance":r.get::<_,String>(11)?}))})?.collect::<std::result::Result<Vec<_>,_>>()?;
        if messages.is_empty() || messages.len() > 200 {
            return Err(err(
                "assistance_limit",
                "Select a conversation with 1–200 retained messages",
            ));
        }
        let mut selected: Vec<_> = messages
            .iter()
            .filter(|m| m["sent"] == false && (target.is_empty() || m["id"] == target))
            .cloned()
            .collect();
        if selected.is_empty()
            || (task == "classification" && (selected.len() != 1 || selected[0]["sent"] == true))
            || (task == "message" && selected.len() != 1)
        {
            return Err(err(
                "invalid_request",
                "Local analysis is available only for received mail",
            ));
        }
        let mut stmt=self.db.prepare("SELECT c.id,c.name,g.provider_label_id FROM categories c LEFT JOIN google_category_labels g ON g.account=c.account AND g.category_id=c.id WHERE c.account=?1 ORDER BY c.id LIMIT 65")?;
        let labels=stmt.query_map([account],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"provider_label_id":r.get::<_,Option<String>>(2)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        if labels.len() > 64 {
            return Err(err(
                "assistance_limit",
                "At most 64 configured labels are supported",
            ));
        }
        // Include manual/provider category stamps, including changes not reflected in body hashes.
        let mut stmt=self.db.prepare("SELECT message_id,field,category_id,value,revision FROM google_manual_intents WHERE account=?1 AND field='category' AND message_id IN (SELECT id FROM messages WHERE account=?1 AND conversation_id=?2) ORDER BY message_id,field,category_id")?;
        let mut intents = stmt
            .query_map(params![account, conversation], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, i64>(4)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut stmt=self.db.prepare("SELECT m.id,m.trashed,mc.category_id FROM messages m LEFT JOIN message_categories mc ON mc.account=m.account AND mc.message_id=m.id WHERE m.account=?1 AND m.conversation_id=?2 ORDER BY m.id,mc.category_id")?;
        let mut membership = stmt
            .query_map(params![account, conversation], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut stmt=self.db.prepare("SELECT s.message_id,s.deleted,l.value FROM google_message_state s LEFT JOIN json_each(s.provider_labels_json) l ON l.value IN (SELECT provider_label_id FROM google_category_labels WHERE account=?1) WHERE s.account=?1 AND s.message_id IN (SELECT id FROM messages WHERE account=?1 AND conversation_id=?2) ORDER BY s.message_id,l.value")?;
        let mut provider = stmt
            .query_map(params![account, conversation], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !target.is_empty() {
            intents.retain(|row| row[0] == target);
            membership.retain(|row| row[0] == target);
            provider.retain(|row| row[0] == target);
        }
        let configured = &settings["settings"];
        let parsed: Settings = serde_json::from_value(configured.clone())?;
        let policy = parsed.accounts.get(account).cloned().unwrap_or_default();
        let mut relevant_settings = json!({"endpoint":parsed.endpoint,"model":parsed.model,"prompt":parsed.prompt,
            "enabled":if task=="summary" {policy.summaries}else{policy.classification || (task=="message" && policy.summaries)},
            "classification":if task=="summary" {Value::Null}else{json!(policy.classification)},
            "summaries":if task=="message" {json!(policy.summaries)}else{Value::Null},
            "descriptions":if task=="summary" {Value::Null}else{json!(policy.descriptions)}});
        // Keep historical Ollama snapshots byte-equivalent.
        if !parsed.provider.is_ollama() {
            relevant_settings["provider"] = json!(parsed.provider);
        }
        if task == "summary" {
            for member in &mut selected {
                let fields = member.as_object_mut().unwrap();
                fields.remove("kind");
                fields.remove("provenance");
            }
            // Labels and sender annotations are not facts in a source-text summary.
            // Keep exact message/deletion membership while ignoring label-only changes.
            let membership: std::collections::BTreeMap<String, bool> = membership
                .iter()
                .map(|m| (m[0].as_str().unwrap().to_owned(), m[1].as_bool().unwrap()))
                .collect();
            let provider: std::collections::BTreeMap<String, bool> = provider
                .iter()
                .map(|m| (m[0].as_str().unwrap().to_owned(), m[1].as_bool().unwrap()))
                .collect();
            return Ok(
                json!({"version":2,"lifecycle":lifecycle,"settings":relevant_settings,"members":selected,"labels":[],"membership":membership,"provider":provider}),
            );
        }
        Ok(
            json!({"version":2,"lifecycle":lifecycle,"settings":relevant_settings,"members":selected,"labels":labels,"intents":intents,"membership":membership,"provider":provider}),
        )
    }

    pub fn local_model_prepare(
        &self,
        account: &str,
        conversation: &str,
        task: &str,
        target: &str,
    ) -> Result<Prepared> {
        if !["summary", "classification", "message"].contains(&task) {
            return Err(err("invalid_request", "Choose summary or classification"));
        }
        let state = self.local_model_settings()?;
        let settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        if !(if task == "summary" {
            policy.summaries
        } else {
            policy.classification || (task == "message" && policy.summaries)
        }) {
            return Err(err(
                "assistance_disabled",
                "Enable this task for the account in Local model settings",
            ));
        }
        if settings.model(task).is_none() {
            return Err(err(
                "assistance_unavailable",
                "Select a model in Local model settings",
            ));
        }
        let snapshot = self.local_model_snapshot(account, conversation, task, target)?;
        let categories: Vec<String> = snapshot["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_owned())
            .collect();
        let mut labels = snapshot["labels"].clone();
        for label in labels.as_array_mut().unwrap() {
            label["description"] = policy
                .descriptions
                .get(label["id"].as_str().unwrap())
                .cloned()
                .unwrap_or_default()
                .into();
        }
        let mut payload = json!({"labels":labels,"messages":[]});
        let mut sources = Vec::new();
        let mut partial = false;
        // Budget the actual expanded request. Email text is JSON inside a
        // message string, so quotes/newlines can be escaped twice. A raw-byte
        // estimate followed by rejecting the whole message loses usable text.
        const REQUEST_BYTES: usize = ollama::INITIAL_REQUEST_BYTES;
        let model_name = &settings.model(task).unwrap().name;
        let request_size = |payload: &Value, sources: &[String]| -> Result<usize> {
            let request =
                ollama::render_request(settings.prompt(task), payload, model_name, sources)
                    .map_err(|message| err("invalid_request", &message))?;
            Ok(serde_json::to_vec(&request)?.len())
        };
        payload["partial"] = false.into();
        let members = snapshot["members"].as_array().unwrap();
        for member in members.iter().rev().take(128) {
            let id = member["id"].as_str().unwrap();
            let body: String = self.db.query_row(
                "SELECT body FROM messages WHERE account=?1 AND id=?2",
                params![account, id],
                |r| r.get(0),
            )?;
            if member["source_hash"].as_str().unwrap().len() != 64 || body.is_empty() {
                partial = true;
                continue;
            }
            let mut message = json!({"id":id,"from":member["sender_email"],"name":member["sender_name"],"to":member["recipient"],"subject":member["subject"],"date":member["date"],"sent":member["sent"],"text":""});
            let mut candidate_sources = sources.clone();
            candidate_sources.push(id.to_owned());
            let mut low = 1usize;
            let mut high = body.len().min(REQUEST_BYTES);
            let mut end = 0;
            while low <= high {
                let midpoint = low + (high - low) / 2;
                let mut boundary = midpoint;
                while !body.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                message["text"] = body[..boundary].into();
                payload["messages"]
                    .as_array_mut()
                    .unwrap()
                    .push(message.clone());
                let fits = request_size(&payload, &candidate_sources)? <= REQUEST_BYTES;
                payload["messages"].as_array_mut().unwrap().pop();
                if fits {
                    end = boundary;
                    low = midpoint + 1;
                } else {
                    high = midpoint - 1;
                }
            }
            if end == 0 {
                partial = true;
                continue;
            }
            partial |= end < body.len();
            message["text"] = body[..end].into();
            payload["messages"].as_array_mut().unwrap().push(message);
            sources.push(id.to_owned());
        }
        partial |= sources.len() != members.len();
        if sources.is_empty() {
            return Err(err("assistance_limit","No retained text fits the model context; shorten instructions or label descriptions"));
        }
        payload["messages"].as_array_mut().unwrap().reverse();
        sources.reverse();
        payload["partial"] = partial.into();
        Ok(Prepared {
            account: account.into(),
            conversation: conversation.into(),
            task: task.into(),
            target: target.into(),
            settings,
            payload,
            categories,
            sources,
            snapshot: encode(&snapshot)?,
            partial,
            arrival: false,
        })
    }
    pub fn local_model_validate(&self, p: &Prepared) -> Result<()> {
        if encode(&self.local_model_snapshot(&p.account, &p.conversation, &p.task, &p.target)?)?
            != p.snapshot
        {
            return Err(failure());
        }
        Ok(())
    }
    pub fn local_model_finish(&self, p: &Prepared, output: Value) -> Result<Value> {
        ollama::validate(&p.task, &encode(&output)?, &p.categories, &p.sources)
            .map_err(|s| err("invalid_assistance_result", &s))?;
        let tx = self.db.unchecked_transaction()?;
        if encode(&self.local_model_snapshot(&p.account, &p.conversation, &p.task, &p.target)?)?
            != p.snapshot
        {
            return Err(failure());
        }
        let policy = p
            .settings
            .accounts
            .get(&p.account)
            .cloned()
            .unwrap_or_default();
        let sent: bool = if p.target.is_empty() {
            false
        } else {
            tx.query_row(
                "SELECT sent FROM messages WHERE account=?1 AND id=?2",
                params![p.account, p.target],
                |r| r.get(0),
            )?
        };
        let classify =
            (p.task == "classification" || p.task == "message") && policy.classification && !sent;
        let mut label_state = "none";
        if classify {
            let manual:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM message_sender_kinds WHERE account=?1 AND message_id=?2 AND provenance='manual')",params![p.account,p.target],|r|r.get(0))?;
            if !manual {
                let hash: String = tx.query_row(
                    "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
                    params![p.account, p.target],
                    |r| r.get(0),
                )?;
                let kind = output["sender_kind"].as_str().unwrap();
                tx.execute("INSERT INTO message_sender_kinds VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,message_id) DO UPDATE SET source_hash=excluded.source_hash,kind=excluded.kind,provenance=excluded.provenance",params![p.account,p.target,hash,if kind=="human"{"people"}else{kind},p.settings.provider.as_str()])?;
            }
            if !p.partial {
                if let Some(category) = output["category_ids"]
                    .as_array()
                    .and_then(|ids| ids.first())
                    .and_then(Value::as_str)
                {
                    label_state = crate::google_live_mutations::enqueue_assisted_category(
                        &tx, &p.account, &p.target, category,
                    )?;
                    if p.arrival
                        && matches!(
                            label_state,
                            "pending" | "local_only" | "applied" | "already_present"
                        )
                    {
                        let name: Option<String> = tx
                            .query_row(
                                "SELECT name FROM categories WHERE account=?1 AND id=?2",
                                params![p.account, category],
                                |r| r.get(0),
                            )
                            .optional()?;
                        let auto_read =
                            policy.auto_read.get(category).copied().unwrap_or_else(|| {
                                name.as_deref()
                                    .is_some_and(|n| n.eq_ignore_ascii_case("Receipts"))
                            });
                        if auto_read {
                            crate::google_live_mutations::enqueue_assisted_read(
                                &tx, &p.account, &p.target,
                            )?;
                        }
                    }
                }
            } else {
                label_state = "partial_source";
            }
        }
        let mut saved = Vec::new();
        for task in ["classification", "summary"] {
            let persist = if p.task == "message" {
                if task == "summary" {
                    policy.summaries
                } else {
                    classify
                }
            } else {
                p.task == task
            };
            if !persist {
                continue;
            }
            let projected = if task == "summary" {
                json!({"summary":output["summary"],"source_ids":output["source_ids"]})
            } else {
                json!({"sender_kind":output["sender_kind"],"category_ids":output["category_ids"],"category_abstain":output["category_abstain"],"reason":output["reason"]})
            };
            let snapshot =
                self.local_model_snapshot(&p.account, &p.conversation, task, &p.target)?;
            let result = json!({"task":task,"target":p.target,"conversation_id":p.conversation,"output":projected,"source_ids":p.sources,"partial":p.partial,"provider":p.settings.provider,"model":p.settings.model(&p.task),"prompt_sha256":format!("{:x}",Sha256::digest(p.settings.prompt(&p.task).as_bytes())),"schema":2,"label_state":label_state,"created_at":chrono::Utc::now().to_rfc3339(),"stale":false});
            tx.execute("INSERT INTO local_model_results VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account,conversation_id,task,target) DO UPDATE SET snapshot_json=excluded.snapshot_json,result_json=excluded.result_json",params![p.account,p.conversation,task,p.target,encode(&snapshot)?,encode(&result)?])?;
            saved.push(result);
        }
        tx.execute(
            "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
            params![p.account, p.conversation],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(if p.task == "message" {
            json!({"task":p.task,"target":p.target,"items":saved})
        } else {
            saved.remove(0)
        })
    }
    pub(crate) fn local_model_preview(
        &self,
        account: &str,
        conversation: &str,
        target: &str,
    ) -> Result<Option<String>> {
        let row:Option<(String,String)>=self.db.query_row("SELECT snapshot_json,result_json FROM local_model_results WHERE account=?1 AND conversation_id=?2 AND task='summary' AND target=?3",params![account,conversation,target],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((snapshot, result)) = row else {
            return Ok(None);
        };
        let result: Value = serde_json::from_str(&result)?;
        let text = result["output"]["summary"].as_str().unwrap_or_default();
        // A partial summary (long input was shortened) still replaces the
        // preview; the reader labels it Partial.
        if !ollama::valid_summary(text)
            || !self
                .local_model_snapshot(account, conversation, "summary", target)
                .and_then(|v| encode(&v))
                .is_ok_and(|s| s == snapshot)
        {
            return Ok(None);
        }
        Ok(Some(text.into()))
    }
    pub fn local_model_results(&self, account: &str, conversation: &str) -> Result<Value> {
        self.require_active_account(account)?;
        let mut stmt=self.db.prepare("SELECT task,target,snapshot_json,result_json FROM local_model_results WHERE account=?1 AND conversation_id=?2 LIMIT 201")?;
        let rows = stmt
            .query_map(params![account, conversation], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut items = Vec::new();
        for (task, target, snapshot, result) in rows {
            if !target.is_empty()
                && self
                    .db
                    .query_row(
                        "SELECT sent FROM messages WHERE account=?1 AND id=?2",
                        params![account, target],
                        |r| r.get::<_, bool>(0),
                    )
                    .optional()?
                    .unwrap_or(true)
            {
                continue;
            }
            let mut result: Value = serde_json::from_str(&result)?;
            result["stale"] = (!self
                .local_model_snapshot(account, conversation, &task, &target)
                .and_then(|v| encode(&v))
                .is_ok_and(|s| s == snapshot))
            .into();
            if task == "summary" {
                result["format_outdated"] = (!ollama::valid_summary(
                    result["output"]["summary"].as_str().unwrap_or_default(),
                ))
                .into();
            }
            items.push(result);
        }
        Ok(json!({"items":items}))
    }
}

impl Store {
    pub fn local_model_batch(
        &self,
        account: &str,
        conversation: &str,
        task: &str,
        target: &str,
    ) -> Result<Vec<Prepared>> {
        if task == "summary" || !target.is_empty() {
            return Ok(vec![self.local_model_prepare(
                account,
                conversation,
                task,
                target,
            )?]);
        }
        let mut stmt=self.db.prepare("SELECT id FROM messages WHERE account=?1 AND conversation_id=?2 AND sent=0 ORDER BY julianday(sent_at),id LIMIT 129")?;
        let targets = stmt
            .query_map(params![account, conversation], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if targets.is_empty() || targets.len() > 128 {
            return Err(err("assistance_limit", "Select 1–128 received messages"));
        }
        let mut result = Vec::new();
        let mut bytes = 0;
        for target in targets {
            let plan = self.local_model_prepare(account, conversation, task, &target)?;
            bytes += serde_json::to_vec(&plan.payload)?.len();
            if bytes > 128 * 1024 {
                return Err(err(
                    "assistance_limit",
                    "Conversation classification exceeds 128 KiB; select individual messages",
                ));
            }
            result.push(plan);
        }
        Ok(result)
    }
    /// Queue synced mail for the watcher in sync order, including history from a
    /// first sync. The cursor persists, so mail synced while no watcher ran is
    /// still processed. Only mail dated after `since` (the watcher's start) is an
    /// arrival eligible for label Auto-read; history never changes review state.
    pub fn local_model_arrivals(&self, after: Option<i64>, since: &str) -> Result<Value> {
        const SCAN: usize = 32;
        const QUEUE: usize = 4;
        let meta = |key: &str| -> Result<Option<i64>> {
            Ok(self
                .db
                .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
                .optional()?)
        };
        let requeue = meta(WATCH_REQUEUE)?;
        let base = match after {
            Some(after) => after,
            None => meta(WATCH_CURSOR)?.unwrap_or(0),
        };
        let start = requeue.map_or(base, |from| from.min(base));
        if requeue.is_some() {
            self.db
                .execute("DELETE FROM meta WHERE key=?1", [WATCH_REQUEUE])?;
        }
        self.db.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![WATCH_CURSOR, start],
        )?;
        if after.is_none() {
            return Ok(json!({"after":start,"messages":[],"more":true}));
        }
        let state = self.local_model_settings()?;
        let settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let since = chrono::DateTime::parse_from_rfc3339(since).ok();
        let now = chrono::Utc::now();
        let rows = self.db.prepare("SELECT m.rowid,m.account,m.id,m.conversation_id,m.sent_at,m.sent,m.trashed,a.lifecycle_state FROM messages m JOIN accounts a ON a.id=m.account WHERE m.rowid>?1 ORDER BY m.rowid LIMIT ?2")?
            .query_map(params![start, SCAN as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?, r.get::<_, bool>(5)?, r.get::<_, bool>(6)?, r.get::<_, String>(7)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut more = rows.len() == SCAN;
        let mut next = start;
        let mut messages = Vec::new();
        let mut cutoffs = std::collections::BTreeMap::new();
        for (rowid, account, id, conversation, date, sent, trashed, lifecycle) in rows {
            let policy = settings.accounts.get(&account).cloned().unwrap_or_default();
            if !cutoffs.contains_key(&account) {
                let cutoff: Option<i64> = self
                    .db
                    .query_row(
                        "SELECT value FROM meta WHERE key=?1",
                        [history_cutoff_key(&account)],
                        |r| r.get(0),
                    )
                    .optional()?;
                cutoffs.insert(account.clone(), cutoff);
            }
            let history = cutoffs[&account].is_some_and(|cutoff| {
                chrono::DateTime::parse_from_rfc3339(&date)
                    .map_or(true, |date| date.timestamp_millis() < cutoff)
            });
            let enabled = !sent
                && !trashed
                && !history
                && !policy.paused
                && lifecycle == "active"
                && settings.model.is_some()
                && (policy.classification || policy.summaries);
            if enabled && messages.len() == QUEUE {
                more = true;
                break;
            }
            next = rowid;
            // A message that cannot be analysed is skipped, never a watcher failure.
            let task = if enabled
                && self
                    .local_model_backlog_eligible(&account, &id)
                    .unwrap_or(false)
            {
                self.local_model_missing_task(&account, &conversation, &id)
                    .ok()
                    .flatten()
            } else {
                None
            };
            if let Some(task) = task {
                let arrival = since
                    .zip(chrono::DateTime::parse_from_rfc3339(&date).ok())
                    .is_some_and(|(since, date)| {
                        date >= since && date <= now + chrono::Duration::minutes(1)
                    });
                messages.push(json!({"account":account,"id":id,"conversation_id":conversation,"task":task,"arrival":arrival}));
            }
        }
        Ok(json!({"after":next,"messages":messages,"more":more}))
    }
    /// Rescan an account's synced mail for missing results after its inputs change.
    pub fn local_model_requeue_account(&self, account: &str) -> Result<()> {
        requeue_after(&self.db, "MIN(rowid)-1", Some(account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configured_store() -> (tempfile::TempDir, Store) {
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
    #[test]
    fn receipts_auto_read_defaults_on_and_respects_account_label_toggle() {
        let (dir, mut store) = configured_store();
        let account = "windscribe";
        let conversation = "windscribe-thread-001";
        let message = "windscribe-message-001-000";
        store
            .db
            .execute(
                "INSERT INTO categories(account,id,name) VALUES(?1,'receipts','Receipts')",
                [account],
            )
            .unwrap();
        let catalog = store.label_catalog(account).unwrap();
        assert_eq!(
            catalog["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "receipts")
                .unwrap()["auto_read"],
            true
        );
        let result = json!({"summary":"A receipt.","source_ids":[message],"sender_kind":"automated","category_ids":["receipts"],"category_abstain":false,"reason":"Purchase receipt"});
        let mut plan = store
            .local_model_prepare(account, conversation, "message", message)
            .unwrap();
        plan.arrival = true;
        store.local_model_finish(&plan, result.clone()).unwrap();
        let unread: bool = store
            .db
            .query_row(
                "SELECT unread FROM messages WHERE account=?1 AND id=?2",
                params![account, message],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!unread);
        store
            .db
            .execute(
                "UPDATE messages SET unread=1,revision=revision+1 WHERE account=?1 AND id=?2",
                params![account, message],
            )
            .unwrap();
        let manual = store
            .local_model_prepare(account, conversation, "message", message)
            .unwrap();
        store.local_model_finish(&manual, result).unwrap();
        let still_unread: bool = store
            .db
            .query_row(
                "SELECT unread FROM messages WHERE account=?1 AND id=?2",
                params![account, message],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            still_unread,
            "manual or backlog classification cannot silently review mail"
        );
        let model_kinds = |store: &Store| -> i64 {
            store.db.query_row("SELECT count(*) FROM message_sender_kinds WHERE account=?1 AND provenance IN ('ollama','openai_compatible')",[account],|r|r.get(0)).unwrap()
        };
        let kinds_before = model_kinds(&store);
        assert!(kinds_before > 0);
        store.label_edit(account,"label_auto_read",&json!({"request_id":"disable-receipts","id":"receipts","expected_auto_read":true,"auto_read":false})).unwrap();
        assert_eq!(
            model_kinds(&store),
            kinds_before,
            "Auto-read does not invalidate People/Automated results"
        );
        assert_eq!(
            store.label_catalog(account).unwrap()["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "receipts")
                .unwrap()["auto_read"],
            false
        );
        let other: String = store
            .db
            .query_row(
                "SELECT id FROM accounts WHERE id<>?1 LIMIT 1",
                [account],
                |r| r.get(0),
            )
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO categories(account,id,name) VALUES(?1,'receipts','Receipts')",
                [&other],
            )
            .unwrap();
        drop(store);
        let store = Store::open_synthetic(dir.path()).unwrap();
        let choice = |account: &str| {
            store.label_catalog(account).unwrap()["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "receipts")
                .unwrap()["auto_read"]
                .as_bool()
                .unwrap()
        };
        assert!(!choice(account), "the explicit opt-out survives restart");
        assert!(choice(&other), "another account keeps the Receipts default");
    }
    #[test]
    fn local_model_escaped_text_is_truncated_to_the_expanded_request_budget() {
        let (_dir, s) = configured_store();
        let (account, conversation, message) = (
            "windscribe",
            "windscribe-thread-001",
            "windscribe-message-001-000",
        );
        for text in [
            "\"quoted\"\\path\n".repeat(4000),
            "日本語 😀\n".repeat(4000),
        ] {
            s.db.execute(
                "UPDATE messages SET body=?3 WHERE account=?1 AND id=?2",
                params![account, message, text],
            )
            .unwrap();
            let plan = s
                .local_model_prepare(account, conversation, "message", message)
                .unwrap();
            assert!(plan.partial);
            let retained = plan.payload["messages"][0]["text"].as_str().unwrap();
            assert!(!retained.is_empty() && text.starts_with(retained));
            let request = ollama::render_request(
                &plan.settings.prompt,
                &plan.payload,
                "fixture",
                &plan.sources,
            )
            .unwrap();
            assert!(serde_json::to_vec(&request).unwrap().len() <= ollama::INITIAL_REQUEST_BYTES);
            assert_eq!(plan.sources, vec![message]);
        }
    }
    #[test]
    fn local_model_sent_messages_are_excluded_and_reader_reports_direction() {
        let (_dir, s) = configured_store();
        let (a, c, m) = (
            "windscribe",
            "windscribe-thread-001",
            "windscribe-message-001-000",
        );
        let p = s.local_model_prepare(a, c, "message", m).unwrap();
        let out = json!({"summary":"The design is approved.","source_ids":[m],"sender_kind":"human","category_ids":[],"category_abstain":false,"reason":"Personal reply"});
        s.local_model_finish(&p, out).unwrap();
        assert_eq!(
            s.conversation(a, c).unwrap()["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == m)
                .unwrap()["sent"],
            false
        );
        s.db.execute(
            "UPDATE messages SET sent=1 WHERE account=?1 AND id=?2",
            params![a, m],
        )
        .unwrap();
        assert_eq!(
            s.conversation(a, c).unwrap()["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == m)
                .unwrap()["sent"],
            true
        );
        for task in ["message", "summary", "classification"] {
            assert!(s.local_model_prepare(a, c, task, m).is_err());
        }
        assert!(s.local_model_results(a, c).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(s.local_model_preview(a, c, m).unwrap().is_none());
        let stored: i64 =
            s.db.query_row("SELECT COUNT(*) FROM local_model_results", [], |r| r.get(0))
                .unwrap();
        assert_eq!(stored, 2, "hide old results without deleting history");
    }
    #[test]
    fn combined_message_summary_preview_and_manual_label_precedence() {
        let (_dir, mut s) = configured_store();
        let a = "windscribe";
        let c = "windscribe-thread-001";
        let m = "windscribe-message-001-000";
        s.db.execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'llm-test','Test')",
            [a],
        )
        .unwrap();
        let before: (bool, bool) =
            s.db.query_row(
                "SELECT unread,starred FROM messages WHERE account=?1 AND id=?2",
                params![a, m],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let p = s.local_model_prepare(a, c, "message", m).unwrap();
        let out = json!({"summary":"The team requests feedback on the design by Friday.","source_ids":[m],"sender_kind":"human","category_ids":["llm-test"],"category_abstain":false,"reason":"Personal request"});
        s.local_model_finish(&p, out.clone()).unwrap();
        let results = s.local_model_results(a, c).unwrap();
        assert_eq!(results["items"].as_array().unwrap().len(), 2);
        assert!(results["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["stale"] == false));
        assert_eq!(
            s.local_model_preview(a, c, m).unwrap(),
            Some(out["summary"].as_str().unwrap().into())
        );
        let after: (bool, bool) =
            s.db.query_row(
                "SELECT unread,starred FROM messages WHERE account=?1 AND id=?2",
                params![a, m],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(before, after);
        let thread = s.conversation(a, c).unwrap();
        s.handle("set_category",Some(a),json!({"id":c,"message_ids":[m],"category_id":"llm-test","present":false,"expected_revision":thread["revision"],"request_id":"manual-label-removal"})).unwrap();
        // A completed human label receipt still prevents future model relabeling.
        s.db.execute("DELETE FROM google_manual_intents WHERE account=?1", [a])
            .unwrap();
        let p = s.local_model_prepare(a, c, "message", m).unwrap();
        s.local_model_finish(&p, out).unwrap();
        let count:i64=s.db.query_row("SELECT count(*) FROM message_categories WHERE account=?1 AND message_id=?2 AND category_id='llm-test'",params![a,m],|r|r.get(0)).unwrap();
        assert_eq!(count, 0);
        s.db.execute(
            "UPDATE messages SET body='New source' WHERE account=?1 AND id=?2",
            params![a, m],
        )
        .unwrap();
        assert!(s.local_model_preview(a, c, m).unwrap().is_none());
    }

    #[test]
    fn settings_restart_results_manual_precedence_and_stale_guards() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open_synthetic(dir.path()).unwrap();
        let mut settings = Settings::default();
        settings.accounts.insert(
            "windscribe".into(),
            ollama::Account {
                classification: true,
                summaries: true,
                ..Default::default()
            },
        );
        settings.model = Some(ollama::Model {
            name: "fixture".into(),
            digest: "a".repeat(64),
        });
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":0,"settings":settings}),
        )
        .unwrap();
        assert!(s
            .local_model_handle(
                "assist_settings_save",
                None,
                &json!({"expected_revision":0,"settings":settings})
            )
            .is_err());
        let c = "windscribe-thread-001";
        let m = "windscribe-message-001-000";
        let p = s
            .local_model_prepare("windscribe", c, "classification", m)
            .unwrap();
        let before: (bool, bool) =
            s.db.query_row(
                "SELECT unread,starred FROM messages WHERE account='windscribe' AND id=?1",
                [m],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        s.local_model_finish(&p,json!({"sender_kind":"human","category_ids":[],"category_abstain":false,"reason":"Personal reply"})).unwrap();
        let after: (bool, bool) =
            s.db.query_row(
                "SELECT unread,starred FROM messages WHERE account='windscribe' AND id=?1",
                [m],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(before, after);
        let thread = s.conversation("windscribe", c).unwrap();
        s.handle("flag",Some("windscribe"),json!({"id":c,"message_ids":[m],"read":true,"expected_revision":thread["revision"],"request_id":"read-after-model"})).unwrap();
        assert_eq!(
            s.local_model_results("windscribe", c).unwrap()["items"][0]["stale"],
            false
        );
        let summary = s
            .local_model_prepare("windscribe", c, "summary", "")
            .unwrap();
        let summary_output =
            json!({"summary":"A design is discussed.","source_ids":summary.sources});
        s.local_model_finish(&summary, summary_output).unwrap();
        let pending = s
            .local_model_prepare("windscribe", c, "classification", m)
            .unwrap();
        let revision: i64 =
            s.db.query_row(
                "SELECT revision FROM messages WHERE account='windscribe' AND id=?1",
                [m],
                |r| r.get(0),
            )
            .unwrap();
        s.local_model_handle("assist_correct",Some("windscribe"),&json!({"conversation_id":c,"message_id":m,"kind":"automated","expected_revision":revision})).unwrap();
        assert!(s.local_model_finish(&pending,json!({"sender_kind":"human","category_ids":[],"category_abstain":false,"reason":"x"})).is_err());
        assert_eq!(
            s.local_model_results("windscribe", c).unwrap()["items"][0]["stale"],
            true
        );
        let results = s.local_model_results("windscribe", c).unwrap();
        assert_eq!(
            results["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["task"] == "summary")
                .unwrap()["stale"],
            false
        );
        settings.prompt = settings.prompt.replace(
            "Read this email for me.",
            "Use strict label matches. Read this email for me.",
        );
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":1,"settings":settings}),
        )
        .unwrap();
        let results = s.local_model_results("windscribe", c).unwrap();
        assert_eq!(
            results["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["task"] == "summary")
                .unwrap()["stale"],
            true
        );
        settings.prompt = settings.prompt.replace(
            "Read this email for me.",
            "Use shorter sentences. Read this email for me.",
        );
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":2,"settings":settings}),
        )
        .unwrap();
        let results = s.local_model_results("windscribe", c).unwrap();
        assert_eq!(
            results["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["task"] == "summary")
                .unwrap()["stale"],
            true
        );
        drop(s);
        let s = Store::open_synthetic(dir.path()).unwrap();
        assert_eq!(s.local_model_settings().unwrap()["revision"], 3);
    }

    fn drain_arrivals(s: &Store, since: &str) -> (i64, Vec<Value>) {
        let mut after = s.local_model_arrivals(None, since).unwrap()["after"]
            .as_i64()
            .unwrap();
        let mut queued = Vec::new();
        loop {
            let page = s.local_model_arrivals(Some(after), since).unwrap();
            assert!(page["messages"].as_array().unwrap().len() <= 4);
            queued.extend(page["messages"].as_array().unwrap().iter().cloned());
            after = page["after"].as_i64().unwrap();
            if page["more"] == false {
                // Record the end, as the idle watcher does on its next call.
                s.local_model_arrivals(Some(after), since).unwrap();
                return (after, queued);
            }
        }
    }
    fn eligible(s: &Store, account: &str) -> Vec<String> {
        let ids: Vec<String> =
            s.db.prepare("SELECT id FROM messages WHERE account=?1 ORDER BY rowid")
                .unwrap()
                .query_map([account], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
        ids.into_iter()
            .filter(|id| s.local_model_backlog_eligible(account, id).unwrap())
            .collect()
    }
    #[test]
    fn a_paused_account_is_skipped_then_its_mail_is_queued_again_on_resume() {
        let (_dir, mut s) = configured_store();
        let account = "windscribe";
        let history = "2999-01-01T00:00:00Z";
        let save = |s: &mut Store, paused: bool| {
            let state = s.local_model_settings().unwrap();
            let mut settings: Settings = serde_json::from_value(state["settings"].clone()).unwrap();
            settings.accounts.entry(account.into()).or_default().paused = paused;
            s.local_model_handle(
                "assist_settings_save",
                None,
                &json!({"expected_revision":state["revision"],"settings":settings}),
            )
            .unwrap();
        };
        save(&mut s, true);
        let progress = s.local_model_progress(Some(account)).unwrap()["accounts"][0].clone();
        assert_eq!(progress["paused"], true);
        assert_eq!(progress["queued"], 0, "a paused account has nothing queued");
        assert_eq!(progress["unprocessed"], progress["eligible"]);
        let (_, queued) = drain_arrivals(&s, history);
        assert!(queued.iter().all(|m| m["account"] != account));
        save(&mut s, false);
        let progress = s.local_model_progress(Some(account)).unwrap()["accounts"][0].clone();
        assert_eq!(progress["queued"], progress["eligible"], "resuming queues every skipped message");
        let (_, queued) = drain_arrivals(&s, history);
        assert_eq!(
            queued.iter().filter(|m| m["account"] == account).count(),
            eligible(&s, account).len()
        );
    }
    #[test]
    fn progress_splits_history_before_the_cutoff_from_queued_mail() {
        let (_dir, s) = configured_store();
        let account = "windscribe";
        let before = s.local_model_progress(Some(account)).unwrap()["accounts"][0].clone();
        assert_eq!(before["done"], 0);
        assert!(before["queued"].as_i64().unwrap() > 0);
        set_history_analysis(&s.db, account, false).unwrap();
        let after = s.local_model_progress(Some(account)).unwrap()["accounts"][0].clone();
        assert_eq!(after["queued"], 0, "fixture mail predates the cutoff");
        assert_eq!(after["unprocessed"], after["eligible"]);
        assert!(after["history_cutoff"].is_i64());
        assert!(after["local"].as_i64().unwrap() >= after["eligible"].as_i64().unwrap());
    }
    #[test]
    fn history_cutoff_leaves_existing_mail_to_the_backlog_until_opted_in() {
        let (_dir, s) = configured_store();
        let account = "windscribe";
        let history = "2999-01-01T00:00:00Z";
        set_history_analysis(&s.db, account, false).unwrap();
        let (_, queued) = drain_arrivals(&s, history);
        assert!(
            queued.iter().all(|m| m["account"] != account),
            "mail dated before the account was added is not analyzed automatically"
        );
        assert!(
            queued.iter().any(|m| m["account"] != account),
            "other accounts keep their own policy"
        );
        // A second "no" keeps the original cutoff instead of moving it later.
        let key = history_cutoff_key(account);
        let cutoff = |s: &Store| -> i64 {
            s.db.query_row("SELECT value FROM meta WHERE key=?1", [&key], |r| r.get(0))
                .unwrap()
        };
        let first = cutoff(&s);
        set_history_analysis(&s.db, account, false).unwrap();
        assert_eq!(cutoff(&s), first);
        // Opting in removes the cutoff and queues the already-synced history.
        set_history_analysis(&s.db, account, true).unwrap();
        let (_, queued) = drain_arrivals(&s, history);
        assert_eq!(
            queued.iter().filter(|m| m["account"] == account).count(),
            eligible(&s, account).len()
        );
    }
    #[test]
    fn watcher_queues_synced_history_once_without_auto_read_and_resumes_after_restart() {
        let (dir, s) = configured_store();
        let account = "windscribe";
        let history = "2999-01-01T00:00:00Z";
        let (end, queued) = drain_arrivals(&s, history);
        let expected = eligible(&s, account);
        let mine: Vec<_> = queued.iter().filter(|m| m["account"] == account).collect();
        assert!(
            expected.len() > 4,
            "the fixture must span several watcher pages"
        );
        assert_eq!(
            mine.len(),
            expected.len(),
            "every synced message is queued once"
        );
        assert!(queued
            .iter()
            .all(|m| m["arrival"] == false && m["task"] == "message"));
        drop(s);
        let mut s = Store::open_synthetic(dir.path()).unwrap();
        assert_eq!(
            s.local_model_arrivals(None, history).unwrap()["after"],
            end,
            "the cursor survives restart"
        );
        assert!(drain_arrivals(&s, history).1.is_empty());

        // Turning an account's assistance back on queues its synced mail again.
        let mut settings: Settings =
            serde_json::from_value(s.local_model_settings().unwrap()["settings"].clone()).unwrap();
        settings.accounts.insert(
            account.into(),
            ollama::Account {
                summaries: false,
                classification: false,
                ..Default::default()
            },
        );
        for on in [false, true] {
            settings.accounts.get_mut(account).unwrap().summaries = on;
            let revision = s.local_model_settings().unwrap()["revision"].clone();
            s.local_model_handle(
                "assist_settings_save",
                None,
                &json!({"expected_revision":revision,"settings":settings}),
            )
            .unwrap();
            let requeued = drain_arrivals(&s, history).1;
            let mine: Vec<_> = requeued
                .iter()
                .filter(|m| m["account"] == account)
                .collect();
            assert_eq!(mine.len(), if on { expected.len() } else { 0 });
            assert!(mine.iter().all(|m| m["task"] == "summary"));
        }

        // Only mail dated after the watcher starts is an arrival for label Auto-read.
        let since = chrono::Utc::now() - chrono::Duration::hours(2);
        let (recent, future) = (&expected[0], &expected[1]);
        for (id, at) in [
            (recent, chrono::Utc::now() - chrono::Duration::hours(1)),
            (future, chrono::Utc::now() + chrono::Duration::days(1)),
        ] {
            s.db.execute(
                "UPDATE messages SET sent_at=?3 WHERE account=?1 AND id=?2",
                params![account, id, at.to_rfc3339()],
            )
            .unwrap();
        }
        s.local_model_requeue_account(account).unwrap();
        let queued = drain_arrivals(&s, &since.to_rfc3339()).1;
        let arrival = |id: &str| queued.iter().find(|m| m["id"] == id).unwrap()["arrival"].clone();
        assert_eq!(arrival(recent), true);
        assert_eq!(arrival(future), false);
        assert_eq!(
            queued.iter().filter(|m| m["arrival"] == true).count(),
            1,
            "older synced history never auto-reads"
        );
    }
    #[test]
    fn watcher_scans_new_mail_stored_under_a_reused_rowid() {
        let (_dir, s) = configured_store();
        let history = "2999-01-01T00:00:00Z";
        let copy = |id: &str| {
            s.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) SELECT account,?1,conversation_id,?1,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash FROM messages WHERE account='windscribe' AND id='windscribe-message-001-000'",[id]).unwrap();
            s.db.last_insert_rowid()
        };
        let ids = |queued: &[Value]| queued.iter().map(|m| m["id"].clone()).collect::<Vec<_>>();
        drain_arrivals(&s, history);
        let reused = copy("watch-new-1");
        let (end, queued) = drain_arrivals(&s, history);
        assert_eq!(ids(&queued), vec![json!("watch-new-1")]);
        s.db.execute("DELETE FROM messages WHERE id='watch-new-1'", [])
            .unwrap();
        local_model_messages_deleted(&s.db).unwrap();
        assert_eq!(
            copy("watch-new-2"),
            reused,
            "SQLite reuses the deleted rowid"
        );
        let page = s.local_model_arrivals(Some(end), history).unwrap();
        assert_eq!(
            ids(page["messages"].as_array().unwrap()),
            vec![json!("watch-new-2")]
        );
    }
}
