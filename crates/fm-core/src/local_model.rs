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
}
fn encode(v: &Value) -> Result<String> {
    Ok(serde_json::to_string(v)?)
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
                    for category in policy.descriptions.keys() {
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
                if previous.endpoint != settings.endpoint
                    || previous.classification_model != settings.classification_model
                    || previous.classification_prompt != settings.classification_prompt
                {
                    tx.execute(
                        "DELETE FROM message_sender_kinds WHERE provenance='ollama'",
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
                            tx.execute("DELETE FROM message_sender_kinds WHERE provenance='ollama' AND account=?1", [account])?;
                        }
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
    fn local_model_snapshot(
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
            .filter(|m| target.is_empty() || m["id"] == target)
            .cloned()
            .collect();
        if selected.is_empty()
            || (task == "classification" && (selected.len() != 1 || selected[0]["sent"] == true))
        {
            return Err(err(
                "invalid_request",
                "Classification targets one received message",
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
        let intents = stmt
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
        let membership = stmt
            .query_map(params![account, conversation], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut stmt=self.db.prepare("SELECT s.message_id,s.deleted,l.value FROM google_message_state s LEFT JOIN json_each(s.provider_labels_json) l ON l.value IN (SELECT provider_label_id FROM google_category_labels WHERE account=?1) WHERE s.account=?1 AND s.message_id IN (SELECT id FROM messages WHERE account=?1 AND conversation_id=?2) ORDER BY s.message_id,l.value")?;
        let provider = stmt
            .query_map(params![account, conversation], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let configured = &settings["settings"];
        let account_settings = &configured["accounts"][account];
        let relevant_settings = if task == "summary" {
            json!({"endpoint":configured["endpoint"],"model":configured["summary_model"],"prompt":configured["summary_prompt"],"enabled":account_settings["summaries"]})
        } else {
            json!({"endpoint":configured["endpoint"],"model":configured["classification_model"],"prompt":configured["classification_prompt"],"enabled":account_settings["classification"],"descriptions":account_settings["descriptions"]})
        };
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
        if !["summary", "classification"].contains(&task) {
            return Err(err("invalid_request", "Choose summary or classification"));
        }
        let state = self.local_model_settings()?;
        let settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        if !(if task == "summary" {
            policy.summaries
        } else {
            policy.classification
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
        // A conservative byte ceiling leaves room for the schema, envelope and output.
        let budget = 12000usize.saturating_sub(
            settings.prompt(task).len()
                + ollama::ENVELOPE.len()
                + serde_json::to_vec(&labels)?.len()
                + 2000,
        );
        let mut remaining = budget;
        let members = snapshot["members"].as_array().unwrap();
        for member in members.iter().rev().take(128) {
            let id = member["id"].as_str().unwrap();
            let body: String = self.db.query_row(
                "SELECT body FROM messages WHERE account=?1 AND id=?2",
                params![account, id],
                |r| r.get(0),
            )?;
            if member["source_hash"].as_str().unwrap().len() != 64 || remaining < 512 {
                partial = true;
                continue;
            }
            let mut message = json!({"id":id,"from":member["sender_email"],"name":member["sender_name"],"to":member["recipient"],"subject":member["subject"],"date":member["date"],"sent":member["sent"],"text":""});
            let overhead = serde_json::to_vec(&message)?.len();
            let max = remaining.saturating_sub(overhead + 32);
            let mut end = body.len().min(max);
            while !body.is_char_boundary(end) {
                end -= 1;
            }
            if end == 0 {
                partial = true;
                continue;
            }
            partial |= end < body.len();
            message["text"] = body[..end].into();
            let size = serde_json::to_vec(&message)?.len();
            if size > remaining {
                partial = true;
                continue;
            }
            remaining -= size;
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
        if p.task == "classification" {
            let manual:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM message_sender_kinds WHERE account=?1 AND message_id=?2 AND provenance='manual')",params![p.account,p.target],|r|r.get(0))?;
            if !manual {
                let hash: String = tx.query_row(
                    "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
                    params![p.account, p.target],
                    |r| r.get(0),
                )?;
                let kind = output["sender_kind"].as_str().unwrap();
                tx.execute("INSERT INTO message_sender_kinds VALUES(?1,?2,?3,?4,'ollama') ON CONFLICT(account,message_id) DO UPDATE SET source_hash=excluded.source_hash,kind=excluded.kind,provenance='ollama'",params![p.account,p.target,hash,if kind=="human"{"people"}else{kind}])?;
            }
        }
        let snapshot =
            self.local_model_snapshot(&p.account, &p.conversation, &p.task, &p.target)?;
        let result = json!({"task":p.task,"target":p.target,"conversation_id":p.conversation,"output":output,"source_ids":p.sources,"partial":p.partial,"model":p.settings.model(&p.task),"prompt_sha256":format!("{:x}",Sha256::digest(p.settings.prompt(&p.task).as_bytes())),"schema":1,"created_at":chrono::Utc::now().to_rfc3339(),"stale":false});
        tx.execute("INSERT INTO local_model_results VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account,conversation_id,task,target) DO UPDATE SET snapshot_json=excluded.snapshot_json,result_json=excluded.result_json",params![p.account,p.conversation,p.task,p.target,encode(&snapshot)?,encode(&result)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
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
            let mut result: Value = serde_json::from_str(&result)?;
            result["stale"] = (!self
                .local_model_snapshot(account, conversation, &task, &target)
                .and_then(|v| encode(&v))
                .is_ok_and(|s| s == snapshot))
            .into();
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
    pub fn local_model_arrivals(&self, after: Option<i64>, since: &str) -> Result<Value> {
        let max: i64 =
            self.db
                .query_row("SELECT COALESCE(MAX(rowid),0) FROM messages", [], |r| {
                    r.get(0)
                })?;
        let Some(after) = after else {
            return Ok(json!({"after":max,"messages":[]}));
        };
        let state = self.local_model_settings()?;
        let settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let mut stmt=self.db.prepare("SELECT m.rowid,m.account,m.id,m.conversation_id,m.sent_at,m.sent,m.trashed,a.lifecycle_state FROM messages m JOIN accounts a ON a.id=m.account WHERE m.rowid>?1 ORDER BY m.rowid LIMIT 1000")?;
        let mut rows = stmt.query([after])?;
        let mut next = after;
        let mut messages = Vec::new();
        while let Some(row) = rows.next()? {
            let account: String = row.get(1)?;
            let date: String = row.get(4)?;
            let eligible = !row.get::<_, bool>(5)?
                && !row.get::<_, bool>(6)?
                && row.get::<_, String>(7)? == "active"
                && settings
                    .accounts
                    .get(&account)
                    .is_some_and(|a| a.classification)
                && chrono::DateTime::parse_from_rfc3339(&date)
                    .ok()
                    .zip(chrono::DateTime::parse_from_rfc3339(since).ok())
                    .is_some_and(|(d, s)| {
                        d >= s && d <= chrono::Utc::now() + chrono::Duration::minutes(1)
                    });
            if eligible && messages.len() == 16 {
                break;
            }
            next = row.get(0)?;
            if eligible {
                messages.push(json!({"account":account,"id":row.get::<_,String>(2)?,"conversation_id":row.get::<_,String>(3)?}));
            }
        }
        Ok(json!({"after":next,"messages":messages}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        settings.classification_model = Some(ollama::Model {
            name: "fixture".into(),
            digest: "a".repeat(64),
        });
        settings.summary_model = settings.classification_model.clone();
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
        settings
            .classification_prompt
            .push_str("\nUse strict label matches.");
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
            false
        );
        settings.summary_prompt.push_str("\nUse shorter sentences.");
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
}
