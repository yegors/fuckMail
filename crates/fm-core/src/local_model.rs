//! Core-owned model settings, one email's model input and its saved results.
mod thread;
use crate::{err, fields, string, Result, Store};
use fm_assist::ollama::{self, Settings};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub use thread::PreparedThread;

/// One received email, captured for the model.
///
/// Built by [`Store::local_model_prepare`] with a snapshot of everything the
/// result depends on; [`Store::local_model_finish`] refuses to save if that
/// snapshot changed while the model ran.
#[derive(Clone)]
pub struct Prepared {
    pub account: String,
    pub conversation: String,
    /// The message ID being analysed.
    pub target: String,
    /// Model settings as read at preparation, used for the whole run.
    pub settings: Settings,
    /// `{from, to, subject, date, text, shortened}`: what `{{messages}}` becomes.
    pub email: Value,
    /// `[{id, name, description}]`: what `{{labels}}` becomes.
    pub labels: Value,
    snapshot: Value,
    /// The body was cut to the model's character budget. Partial runs still
    /// save a summary but never apply a label.
    pub partial: bool,
    /// Set only for mail that arrived while the queue ran; permits label Auto-read.
    pub arrival: bool,
    /// An explicit request replaces saved results; queue work only fills missing ones.
    pub overwrite: bool,
}
impl Prepared {
    /// Returns the label IDs the model may choose from.
    pub fn label_ids(&self) -> Vec<String> {
        ollama::label_ids(&self.labels)
    }
}
/// Mail dated before this cutoff (Unix ms) was already in the account when it
/// was added. The queue leaves it to Catch up, so a first sync does not send a
/// whole mailbox through the model. No cutoff means all synced mail is new.
pub(crate) fn history_cutoff_key(account: &str) -> String {
    format!("local_model_history_cutoff:{account}")
}
/// Catch up's lower bound (Unix ms): older mail down to it joins the queue
/// after new mail.
pub(crate) fn catch_up_key(account: &str) -> String {
    format!("local_model_catch_up_from:{account}")
}
/// Turns analysis of mail that predates the account on or off. Turning it off
/// sets the history cutoff to now, but only if none is set yet, so toggling
/// never moves an existing cutoff forward.
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
    } else {
        db.execute(
            "INSERT OR IGNORE INTO meta(key,value) VALUES(?1,?2)",
            params![
                history_cutoff_key(account),
                chrono::Utc::now().timestamp_millis()
            ],
        )?;
    }
    Ok(())
}
/// Queue attempts for one email before it waits for an explicit Try again.
pub(crate) const MAX_ATTEMPTS: i64 = 2;
fn stale() -> crate::CoreError {
    err(
        "assistance_stale",
        "Message, labels or manual choices changed; run again",
    )
}
/// Snapshots saved by older versions also carried model settings and a version.
/// Settings changes no longer make results out of date, so both are ignored.
fn same_snapshot(stored: &Value, current: &Value) -> bool {
    let strip = |v: &Value| {
        let mut v = v.clone();
        if let Some(fields) = v.as_object_mut() {
            fields.remove("settings");
            fields.remove("version");
        }
        v
    };
    strip(stored) == strip(current)
}
fn same_stored(stored: &str, current: &Value) -> bool {
    serde_json::from_str::<Value>(stored).is_ok_and(|v| same_snapshot(&v, current))
}
/// Copies the People/Automated answer of saved model classifications into
/// `message_sender_kinds` where a message has none: every message, or with
/// `?1`/`?2` set, that account's one message.
const RESTORE_MODEL_KINDS: &str = "INSERT INTO message_sender_kinds(account,message_id,source_hash,kind,provenance) SELECT r.account,r.target,m.source_hash,CASE json_extract(r.result_json,'$.output.sender_kind') WHEN 'human' THEN 'people' ELSE 'automated' END,json_extract(r.result_json,'$.provider') FROM local_model_results r JOIN messages m ON m.account=r.account AND m.id=r.target WHERE r.task='classification' AND (?1 IS NULL OR (r.account=?1 AND r.target=?2)) AND json_extract(r.result_json,'$.output.sender_kind') IN ('human','automated') AND json_extract(r.result_json,'$.provider') IN ('ollama','openai_compatible') AND NOT EXISTS(SELECT 1 FROM message_sender_kinds k WHERE k.account=r.account AND k.message_id=r.target)";

impl Store {
    /// Returns `{revision, settings, defaults}` for the local model; a profile
    /// that never saved settings reports revision 0 with the defaults.
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
            Some((r, s)) => (r, serde_json::from_str::<Settings>(&s)?),
            None => (0, Settings::default()),
        };
        Ok(json!({"revision":revision,"settings":settings,"defaults":Settings::default()}))
    }
    /// Changes whenever mail, results or settings change; the queue worker
    /// uses it to skip rescanning an empty queue.
    pub fn local_model_revision(&self) -> Result<i64> {
        self.revision()
    }
    pub(crate) fn local_model_policy(&self) -> Result<Settings> {
        Ok(serde_json::from_value(
            self.local_model_settings()?["settings"].clone(),
        )?)
    }
    /// Dispatches the `assist_*` IPC methods: settings get and save, the label
    /// catalog, saved results, sender-kind corrections, and accepting a
    /// suggested label.
    ///
    /// Settings saves are revision-checked (`assistance_stale` on conflict)
    /// and may only reference active accounts and their existing labels.
    /// Accepting a label applies it through the ordinary `set_category` path,
    /// so it behaves exactly like a manual choice.
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
                    return Err(err(
                        "assistance_stale",
                        "Local model settings changed elsewhere; reload them",
                    ));
                }
                for (account, policy) in &settings.accounts {
                    self.require_active_account(account)?;
                    for category in policy
                        .descriptions
                        .keys()
                        .chain(policy.auto_read.keys())
                        .chain(policy.auto_spam.keys())
                    {
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
                // Saved results stay: a prompt, model or policy change applies to
                // mail analysed from now on, and Resummarize or Catch up redoes the rest.
                let revision = old["revision"].as_i64().unwrap() + 1;
                tx.execute("INSERT INTO local_model_settings VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,settings_json=excluded.settings_json",params![revision,serde_json::to_string(&settings)?])?;
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
                    return Err(stale());
                }
                // A manual kind is pinned to this source hash and survives
                // later model runs; reset returns the model's saved answer.
                if k == "reset" {
                    tx.execute(
                        "DELETE FROM message_sender_kinds WHERE account=?1 AND message_id=?2",
                        params![a, m],
                    )?;
                    tx.execute(RESTORE_MODEL_KINDS, params![a, m])?;
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
                    .ok_or_else(stale)?;
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
                // Accepting changes the label state the classification depends
                // on; re-stamp it so the accepted suggestion does not read as stale.
                let snapshot = self.local_model_snapshot(a, c, "classification", m)?;
                self.db.execute("UPDATE local_model_results SET snapshot_json=?1 WHERE account=?2 AND conversation_id=?3 AND task='classification' AND target=?4",params![serde_json::to_string(&snapshot)?,a,c,m])?;
                Ok(reply)
            }
            _ => Err(err(
                "unsupported_method",
                "Unknown local assistance operation",
            )),
        }
    }
    /// What a saved result depends on: the email's source, and for
    /// classification its labels, sender correction and label choices.
    pub(super) fn local_model_snapshot(
        &self,
        account: &str,
        conversation: &str,
        task: &str,
        target: &str,
    ) -> Result<Value> {
        let lifecycle = self.require_active_account(account)?;
        let member=self.db.query_row("SELECT m.id,m.source_hash,m.sent,m.subject,m.sender_name,m.sender_email,m.recipient,m.sent_at,m.body,COALESCE(k.kind,'unknown'),COALESCE(k.provenance,'') FROM messages m LEFT JOIN message_sender_kinds k ON k.account=m.account AND k.message_id=m.id AND k.source_hash=m.source_hash WHERE m.account=?1 AND m.conversation_id=?2 AND m.id=?3",params![account,conversation,target],|r|{let body:String=r.get(8)?;Ok(json!({"id":r.get::<_,String>(0)?,"source_hash":r.get::<_,String>(1)?,"sent":r.get::<_,bool>(2)?,"subject":r.get::<_,String>(3)?,"sender_name":r.get::<_,String>(4)?,"sender_email":r.get::<_,String>(5)?,"recipient":r.get::<_,String>(6)?,"date":r.get::<_,String>(7)?,"text_hash":format!("{:x}",Sha256::digest(body.as_bytes())),"kind":r.get::<_,String>(9)?,"provenance":r.get::<_,String>(10)?}))}).optional()?.ok_or_else(||err("not_found","Message not found"))?;
        if member["sent"] == true {
            return Err(err(
                "invalid_request",
                "Local analysis is available only for received mail",
            ));
        }
        let mut members = vec![member];
        let mut stmt=self.db.prepare("SELECT m.id,m.trashed,mc.category_id FROM messages m LEFT JOIN message_categories mc ON mc.account=m.account AND mc.message_id=m.id WHERE m.account=?1 AND m.id=?2 ORDER BY mc.category_id")?;
        let membership = stmt
            .query_map(params![account, target], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut stmt=self.db.prepare("SELECT s.message_id,s.deleted,l.value FROM google_message_state s LEFT JOIN json_each(s.provider_labels_json) l ON l.value IN (SELECT provider_label_id FROM google_category_labels WHERE account=?1) WHERE s.account=?1 AND s.message_id=?2 ORDER BY l.value")?;
        let provider = stmt
            .query_map(params![account, target], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if task == "summary" {
            // Labels and sender annotations are not facts in a source-text summary.
            // Keep exact message/deletion membership while ignoring label-only changes.
            let fields = members[0].as_object_mut().unwrap();
            fields.remove("kind");
            fields.remove("provenance");
            let membership: std::collections::BTreeMap<String, bool> = membership
                .iter()
                .map(|m| (m[0].as_str().unwrap().to_owned(), m[1].as_bool().unwrap()))
                .collect();
            let provider: std::collections::BTreeMap<String, bool> = provider
                .iter()
                .map(|m| (m[0].as_str().unwrap().to_owned(), m[1].as_bool().unwrap()))
                .collect();
            return Ok(
                json!({"lifecycle":lifecycle,"members":members,"labels":[],"membership":membership,"provider":provider}),
            );
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
        let mut stmt=self.db.prepare("SELECT message_id,field,category_id,value,revision FROM google_manual_intents WHERE account=?1 AND field='category' AND message_id=?2 ORDER BY category_id")?;
        let intents = stmt
            .query_map(params![account, target], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, i64>(4)?
                ]))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(
            json!({"lifecycle":lifecycle,"members":members,"labels":labels,"intents":intents,"membership":membership,"provider":provider}),
        )
    }
    /// Capture one received email for the model: its stored text (reply
    /// history already removed at sync), headers and the account's labels.
    /// An email with no text is read from its subject; one with neither
    /// fails with `assistance_limit`.
    pub fn local_model_prepare(
        &self,
        account: &str,
        conversation: &str,
        target: &str,
    ) -> Result<Prepared> {
        let settings = self.local_model_policy()?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        if !policy.summaries && !policy.classification {
            return Err(err(
                "assistance_disabled",
                "Turn on Summarize or Label for this account",
            ));
        }
        if settings.model.is_none() {
            return Err(err(
                "assistance_unavailable",
                "Choose a model in Settings → Local LLM",
            ));
        }
        let snapshot = self.local_model_snapshot(account, conversation, "message", target)?;
        let (name, email, to, subject, date, body): (String, String, String, String, String, String) = self.db.query_row(
            "SELECT sender_name,sender_email,recipient,subject,sent_at,body FROM messages WHERE account=?1 AND id=?2",
            params![account, target],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?;
        // Sync stores "(no subject)" for a missing subject.
        if body.trim().is_empty() && (subject.trim().is_empty() || subject == "(no subject)") {
            return Err(err(
                "assistance_limit",
                "This message has no subject or text to summarize",
            ));
        }
        let partial = body.chars().count() > ollama::BODY_CHARS;
        let text: String = body.trim_start().chars().take(ollama::BODY_CHARS).collect();
        let from = if name.is_empty() || name == email {
            email
        } else {
            format!("{name} <{email}>")
        };
        let mut labels = snapshot["labels"].clone();
        for label in labels.as_array_mut().unwrap() {
            let id = label["id"].as_str().unwrap().to_owned();
            *label = json!({"id":id,"name":label["name"],
                "description":policy.descriptions.get(&id).cloned().unwrap_or_default()});
        }
        Ok(Prepared {
            account: account.into(),
            conversation: conversation.into(),
            target: target.into(),
            settings,
            email: json!({"from":from,"to":to,"subject":subject,"date":date,"text":text,"shortened":partial}),
            labels,
            snapshot,
            partial,
            arrival: false,
            overwrite: false,
        })
    }
    /// Fails with `assistance_stale` if the email, its labels or manual
    /// choices changed since `p` was prepared. The engine calls it right
    /// before inference; [`Store::local_model_finish`] repeats it on commit.
    pub fn local_model_validate(&self, p: &Prepared) -> Result<()> {
        if !same_snapshot(
            &p.snapshot,
            &self.local_model_snapshot(&p.account, &p.conversation, "message", &p.target)?,
        ) {
            return Err(stale());
        }
        Ok(())
    }
    /// Save a checked reply: `{summary, sender_kind, category_ids}`.
    ///
    /// The output must have a valid summary, a `human` or `automated` sender
    /// kind and at most one label from `p`'s label list
    /// (`invalid_assistance_result` otherwise). Only enabled tasks are saved,
    /// and queue work only fills missing results unless `p.overwrite` is set.
    /// A chosen label is queued as a Gmail label change, followed by Spam or
    /// Auto-read when the account's policy asks for it. A manual sender-kind
    /// correction is never overwritten. Clears the email's failure count.
    pub fn local_model_finish(&self, p: &Prepared, output: Value) -> Result<Value> {
        let summary = output["summary"].as_str().unwrap_or_default();
        let kind = output["sender_kind"].as_str().unwrap_or_default();
        let ids = p.label_ids();
        if !ollama::valid_summary(summary)
            || !["human", "automated"].contains(&kind)
            || !output["category_ids"].as_array().is_some_and(|c| {
                c.len() <= 1
                    && c.iter()
                        .all(|id| id.as_str().is_some_and(|id| ids.iter().any(|i| i == id)))
            })
        {
            return Err(err("invalid_assistance_result", "Invalid model analysis"));
        }
        // Recheck staleness inside the same transaction as the writes.
        let tx = self.db.unchecked_transaction()?;
        self.local_model_validate(p)?;
        let policy = p
            .settings
            .accounts
            .get(&p.account)
            .cloned()
            .unwrap_or_default();
        let saved = |task: &str| -> Result<bool> {
            Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM local_model_results WHERE account=?1 AND conversation_id=?2 AND task=?3 AND target=?4)",params![p.account,p.conversation,task,p.target],|r|r.get(0))?)
        };
        let summarize = policy.summaries && (p.overwrite || !saved("summary")?);
        let classify = policy.classification && (p.overwrite || !saved("classification")?);
        let mut label_state = "none";
        if classify {
            let manual:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM message_sender_kinds WHERE account=?1 AND message_id=?2 AND provenance='manual')",params![p.account,p.target],|r|r.get(0))?;
            if !manual {
                let hash: String = tx.query_row(
                    "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
                    params![p.account, p.target],
                    |r| r.get(0),
                )?;
                tx.execute("INSERT INTO message_sender_kinds VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,message_id) DO UPDATE SET source_hash=excluded.source_hash,kind=excluded.kind,provenance=excluded.provenance",params![p.account,p.target,hash,if kind=="human"{"people"}else{kind},p.settings.provider.as_str()])?;
            }
            // A label chosen from a truncated body is not trusted enough to apply.
            if p.partial {
                label_state = "partial_source";
            } else if let Some(category) = output["category_ids"]
                .as_array()
                .and_then(|ids| ids.first())
                .and_then(Value::as_str)
            {
                label_state = crate::google_live_mutations::enqueue_assisted_category(
                    &tx, &p.account, &p.target, category,
                )?;
                let applied = matches!(
                    label_state,
                    "pending" | "local_only" | "applied" | "already_present"
                );
                // A label with Spam on sends the message to Spam whenever the
                // model applies it; that is enough, so Auto-read is skipped.
                if applied && policy.auto_spam.get(category).copied().unwrap_or(false) {
                    crate::google_live_mutations::enqueue_assisted_spam(
                        &tx, &p.account, &p.target,
                    )?;
                } else if p.arrival && applied {
                    let name: Option<String> = tx
                        .query_row(
                            "SELECT name FROM categories WHERE account=?1 AND id=?2",
                            params![p.account, category],
                            |r| r.get(0),
                        )
                        .optional()?;
                    let auto_read = policy.auto_read.get(category).copied().unwrap_or_else(|| {
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
        }
        let prompt = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{}", p.settings.system, p.settings.user).as_bytes())
        );
        let mut items = Vec::new();
        for (task, persist) in [("classification", classify), ("summary", summarize)] {
            if !persist {
                continue;
            }
            let projected = if task == "summary" {
                json!({"summary":summary,"source_ids":[p.target]})
            } else {
                json!({"sender_kind":kind,"category_ids":output["category_ids"]})
            };
            let snapshot =
                self.local_model_snapshot(&p.account, &p.conversation, task, &p.target)?;
            let result = json!({"task":task,"target":p.target,"conversation_id":p.conversation,"output":projected,"source_ids":[p.target],"partial":p.partial,"provider":p.settings.provider,"model":p.settings.model,"prompt_sha256":prompt,"schema":3,"label_state":label_state,"created_at":chrono::Utc::now().to_rfc3339(),"stale":false});
            tx.execute("INSERT INTO local_model_results VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account,conversation_id,task,target) DO UPDATE SET snapshot_json=excluded.snapshot_json,result_json=excluded.result_json",params![p.account,p.conversation,task,p.target,serde_json::to_string(&snapshot)?,serde_json::to_string(&result)?])?;
            items.push(result);
        }
        tx.execute(
            "DELETE FROM local_model_failures WHERE account=?1 AND message_id=?2",
            params![p.account, p.target],
        )?;
        tx.execute(
            "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
            params![p.account, p.conversation],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(
            json!({"task":"message","target":p.target,"items":items,"label_state":label_state,"prompt_sha256":prompt}),
        )
    }
    /// Count a failed attempt on one email. After `MAX_ATTEMPTS` the queue
    /// skips it until an explicit Try again succeeds.
    pub fn local_model_fail(
        &self,
        account: &str,
        message: &str,
        code: &str,
        text: &str,
    ) -> Result<i64> {
        self.db.execute("INSERT INTO local_model_failures(account,message_id,attempts,code,message,updated_at) SELECT ?1,?2,1,?3,?4,?5 WHERE EXISTS(SELECT 1 FROM messages WHERE account=?1 AND id=?2) ON CONFLICT(account,message_id) DO UPDATE SET attempts=attempts+1,code=excluded.code,message=excluded.message,updated_at=excluded.updated_at",
            params![account, message, code, text, chrono::Utc::now().to_rfc3339()])?;
        Ok(self
            .db
            .query_row(
                "SELECT attempts FROM local_model_failures WHERE account=?1 AND message_id=?2",
                params![account, message],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    /// Restores the People/Automated answer of every saved model
    /// classification whose answer is missing. Label edits by earlier builds
    /// deleted these answers but kept the results, so the queue never asked
    /// again.
    pub(crate) fn recover_model_sender_kinds(&self) -> Result<()> {
        let restored = self
            .db
            .execute(RESTORE_MODEL_KINDS, params![None::<&str>, None::<&str>])?;
        if restored > 0 {
            self.db
                .execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        Ok(())
    }
    /// Classifications made before an account's labels existed chose nothing.
    /// Forget them so the queue labels that mail again; summaries stay.
    pub(crate) fn local_model_forget_unlabeled(&self, account: &str) -> Result<()> {
        self.db.execute("DELETE FROM local_model_results WHERE account=?1 AND task='classification' AND json_array_length(result_json,'$.output.category_ids')=0",[account])?;
        Ok(())
    }
    /// Returns the saved summary to show as a message preview, or `None` when
    /// there is none, it is in an outdated format, or the message changed
    /// since it was written.
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
                .is_ok_and(|current| same_stored(&snapshot, &current))
        {
            return Ok(None);
        }
        Ok(Some(text.into()))
    }
    /// Returns `{items, failures}` for a conversation: the saved results of
    /// its received messages and its conversation summary (`task` `thread`),
    /// each marked `stale` when its inputs changed (and, for summaries,
    /// `format_outdated`), plus queue failures so the reader can offer Try
    /// again.
    pub fn local_model_results(&self, account: &str, conversation: &str) -> Result<Value> {
        self.require_active_account(account)?;
        // A conversation summary targets the conversation itself; every other
        // result needs its message, still received.
        let mut stmt=self.db.prepare("SELECT r.task,r.target,r.snapshot_json,r.result_json FROM local_model_results r LEFT JOIN messages m ON m.account=r.account AND m.id=r.target WHERE r.account=?1 AND r.conversation_id=?2 AND (CASE WHEN r.task='thread' THEN r.target=r.conversation_id ELSE m.id IS NOT NULL AND NOT m.sent END) LIMIT 401")?;
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
            if task == thread::TASK {
                self.local_model_thread_result(account, conversation, &snapshot, &mut result);
                items.push(result);
                continue;
            }
            result["stale"] = (!self
                .local_model_snapshot(account, conversation, &task, &target)
                .is_ok_and(|current| same_stored(&snapshot, &current)))
            .into();
            if task == "summary" {
                result["format_outdated"] = (!ollama::valid_summary(
                    result["output"]["summary"].as_str().unwrap_or_default(),
                ))
                .into();
            }
            items.push(result);
        }
        // Queue failures on messages without results, so the reader can offer Try again.
        let mut stmt=self.db.prepare("SELECT f.message_id,f.code,f.message,f.attempts FROM local_model_failures f JOIN messages m ON m.account=f.account AND m.id=f.message_id WHERE f.account=?1 AND m.conversation_id=?2")?;
        let failures = stmt
            .query_map(params![account, conversation], |r| {
                Ok(json!({"target":r.get::<_,String>(0)?,"code":r.get::<_,String>(1)?,"message":r.get::<_,String>(2)?,"attempts":r.get::<_,i64>(3)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({"items":items,"failures":failures}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "windscribe";
    const C: &str = "windscribe-thread-001";
    const M: &str = "windscribe-message-001-000";
    pub(super) fn configured_store() -> (tempfile::TempDir, Store) {
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
    fn output(summary: &str, labels: Value) -> Value {
        json!({"summary":summary,"sender_kind":"human","category_ids":labels})
    }
    fn save(s: &mut Store, change: impl FnOnce(&mut Settings)) {
        let state = s.local_model_settings().unwrap();
        let mut settings: Settings = serde_json::from_value(state["settings"].clone()).unwrap();
        change(&mut settings);
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":state["revision"],"settings":settings}),
        )
        .unwrap();
    }
    fn summary(s: &Store) -> Value {
        s.local_model_results(A, C).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["task"] == "summary" && r["target"] == M)
            .cloned()
            .unwrap_or(Value::Null)
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
        let result =
            json!({"summary":"A receipt.","sender_kind":"automated","category_ids":["receipts"]});
        let mut plan = store
            .local_model_prepare(account, conversation, message)
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
        let mut manual = store
            .local_model_prepare(account, conversation, message)
            .unwrap();
        manual.overwrite = true;
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
        store.label_edit(account,"label_describe",&json!({"request_id":"describe-receipts","id":"receipts","expected_description":"","description":"Order and payment receipts"})).unwrap();
        assert_eq!(
            model_kinds(&store),
            kinds_before,
            "label instructions do not invalidate People/Automated results"
        );
        // Answers an earlier build deleted come back from the saved results.
        store.db.execute("DELETE FROM message_sender_kinds WHERE account=?1 AND provenance IN ('ollama','openai_compatible')",[account]).unwrap();
        store.recover_model_sender_kinds().unwrap();
        assert_eq!(model_kinds(&store), kinds_before);
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
    fn label_spam_is_per_account_and_moves_labelled_mail_instead_of_auto_read() {
        let (_dir, mut store) = configured_store();
        let other: String = store
            .db
            .query_row("SELECT id FROM accounts WHERE id<>?1 LIMIT 1", [A], |r| {
                r.get(0)
            })
            .unwrap();
        for account in [A, other.as_str()] {
            store
                .db
                .execute(
                    "INSERT INTO categories(account,id,name) VALUES(?1,'receipts','Receipts')",
                    [account],
                )
                .unwrap();
        }
        let choice = |store: &Store, account: &str| {
            store.label_catalog(account).unwrap()["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "receipts")
                .unwrap()["auto_spam"]
                .as_bool()
                .unwrap()
        };
        assert!(!choice(&store, A), "Spam is off until chosen");
        store.label_edit(A,"label_auto_spam",&json!({"request_id":"receipts-spam","id":"receipts","expected_auto_spam":false,"auto_spam":true})).unwrap();
        assert!(choice(&store, A));
        assert!(
            !choice(&store, &other),
            "the choice belongs to one account and label"
        );
        let flags = |store: &Store| -> (bool, bool) {
            store
                .db
                .query_row(
                    "SELECT unread,inbox FROM messages WHERE account=?1 AND id=?2",
                    params![A, M],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        store
            .db
            .execute(
                "UPDATE messages SET unread=1,inbox=1 WHERE account=?1 AND id=?2",
                params![A, M],
            )
            .unwrap();
        // Receipts also has Auto-read on by default; Spam is enough.
        let mut plan = store.local_model_prepare(A, C, M).unwrap();
        plan.arrival = true;
        let result =
            json!({"summary":"A receipt.","sender_kind":"automated","category_ids":["receipts"]});
        store.local_model_finish(&plan, result.clone()).unwrap();
        assert_eq!(
            flags(&store),
            (true, false),
            "out of the Inbox, still unread"
        );
        let receipts: i64 = store.db.query_row("SELECT count(*) FROM operations WHERE account=?1 AND json_extract(result_json,'$.kind')='assist_spam' AND json_extract(result_json,'$.message_id')=?2",params![A,M],|r|r.get(0)).unwrap();
        assert_eq!(receipts, 1);
        // Put back by hand, an explicit rerun does not move it again.
        store
            .db
            .execute(
                "UPDATE messages SET inbox=1,revision=revision+1 WHERE account=?1 AND id=?2",
                params![A, M],
            )
            .unwrap();
        let mut again = store.local_model_prepare(A, C, M).unwrap();
        again.overwrite = true;
        store.local_model_finish(&again, result).unwrap();
        assert_eq!(
            flags(&store),
            (true, true),
            "the model moves a message at most once"
        );
        // Removing the label forgets its Spam choice.
        store.label_edit(A,"label_remove",&json!({"request_id":"remove-receipts","id":"receipts","expected_name":"Receipts","expected_provider_label_id":null})).unwrap();
        let settings = store.local_model_settings().unwrap();
        assert!(settings["settings"]["accounts"][A]
            .get("auto_spam")
            .is_none_or(|choices| choices.get("receipts").is_none()));
    }
    #[test]
    fn one_email_is_sent_with_its_labels_and_long_text_is_shortened() {
        let (_dir, mut s) = configured_store();
        s.db.execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'receipts','Receipts')",
            [A],
        )
        .unwrap();
        save(&mut s, |settings| {
            settings.accounts.entry(A.into()).or_default().descriptions =
                [("receipts".to_owned(), "Purchase confirmations".to_owned())].into();
        });
        let plan = s.local_model_prepare(A, C, M).unwrap();
        assert!(!plan.partial);
        assert_eq!(plan.email["shortened"], false);
        assert!(plan.email["from"].as_str().unwrap().contains('@'));
        let receipts = plan
            .labels
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == "receipts")
            .unwrap();
        assert_eq!(receipts["description"], "Purchase confirmations");
        let text = "\"quoted\"\\path 日本語 😀\n".repeat(4000);
        s.db.execute(
            "UPDATE messages SET body=?3 WHERE account=?1 AND id=?2",
            params![A, M, text],
        )
        .unwrap();
        let plan = s.local_model_prepare(A, C, M).unwrap();
        assert!(plan.partial);
        let kept = plan.email["text"].as_str().unwrap();
        assert_eq!(kept.chars().count(), ollama::BODY_CHARS);
        assert!(text.starts_with(kept));
        // No text: the subject is the message, as in a reminder to oneself.
        s.db.execute(
            "UPDATE messages SET body=' ',subject='Dont forget to milk the cow' WHERE account=?1 AND id=?2",
            params![A, M],
        )
        .unwrap();
        let plan = s.local_model_prepare(A, C, M).unwrap();
        assert_eq!(plan.email["subject"], "Dont forget to milk the cow");
        assert_eq!(plan.email["text"], "");
        assert!(!plan.partial);
        for subject in ["(no subject)", "  "] {
            s.db.execute(
                "UPDATE messages SET subject=?3 WHERE account=?1 AND id=?2",
                params![A, M, subject],
            )
            .unwrap();
            assert_eq!(
                s.local_model_prepare(A, C, M).err().unwrap().code,
                "assistance_limit"
            );
        }
    }
    #[test]
    fn sent_messages_are_excluded_and_old_results_hidden() {
        let (_dir, s) = configured_store();
        let p = s.local_model_prepare(A, C, M).unwrap();
        s.local_model_finish(&p, output("The design is approved.", json!([])))
            .unwrap();
        s.db.execute(
            "UPDATE messages SET sent=1 WHERE account=?1 AND id=?2",
            params![A, M],
        )
        .unwrap();
        assert!(s.local_model_prepare(A, C, M).is_err());
        assert!(s.local_model_results(A, C).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(s.local_model_preview(A, C, M).unwrap().is_none());
        let stored: i64 =
            s.db.query_row("SELECT COUNT(*) FROM local_model_results", [], |r| r.get(0))
                .unwrap();
        assert_eq!(stored, 2, "hide old results without deleting history");
    }
    #[test]
    fn summary_preview_review_state_and_manual_label_precedence() {
        let (_dir, mut s) = configured_store();
        s.db.execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'llm-test','Test')",
            [A],
        )
        .unwrap();
        let flags = |s: &Store| -> (bool, bool) {
            s.db.query_row(
                "SELECT unread,starred FROM messages WHERE account=?1 AND id=?2",
                params![A, M],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        let before = flags(&s);
        let p = s.local_model_prepare(A, C, M).unwrap();
        assert!(p.label_ids().contains(&"llm-test".to_owned()));
        let out = output("The team wants feedback by Friday.", json!(["llm-test"]));
        s.local_model_finish(&p, out.clone()).unwrap();
        let results = s.local_model_results(A, C).unwrap();
        assert_eq!(results["items"].as_array().unwrap().len(), 2);
        assert!(results["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["stale"] == false));
        assert_eq!(
            s.local_model_preview(A, C, M).unwrap().as_deref(),
            Some("The team wants feedback by Friday.")
        );
        assert_eq!(before, flags(&s));
        let thread = s.conversation(A, C).unwrap();
        s.handle("set_category",Some(A),json!({"id":C,"message_ids":[M],"category_id":"llm-test","present":false,"expected_revision":thread["revision"],"request_id":"manual-label-removal"})).unwrap();
        // A completed human label receipt still prevents future model relabeling.
        s.db.execute("DELETE FROM google_manual_intents WHERE account=?1", [A])
            .unwrap();
        let mut p = s.local_model_prepare(A, C, M).unwrap();
        p.overwrite = true;
        s.local_model_finish(&p, out).unwrap();
        let count:i64=s.db.query_row("SELECT count(*) FROM message_categories WHERE account=?1 AND message_id=?2 AND category_id='llm-test'",params![A,M],|r|r.get(0)).unwrap();
        assert_eq!(count, 0);
        s.db.execute(
            "UPDATE messages SET body='New source' WHERE account=?1 AND id=?2",
            params![A, M],
        )
        .unwrap();
        assert!(s.local_model_preview(A, C, M).unwrap().is_none());
        // A label the account does not have is refused before anything is saved.
        let p = s.local_model_prepare(A, C, M).unwrap();
        assert!(s
            .local_model_finish(&p, output("x y", json!(["invented"])))
            .is_err());
    }
    #[test]
    fn settings_changes_keep_saved_results_and_queue_work_fills_only_gaps() {
        let (_dir, mut s) = configured_store();
        save(&mut s, |settings| {
            settings
                .accounts
                .entry(A.into())
                .or_default()
                .classification = false;
        });
        let p = s.local_model_prepare(A, C, M).unwrap();
        s.local_model_finish(&p, output("First summary.", json!([])))
            .unwrap();
        assert_eq!(summary(&s)["output"]["summary"], "First summary.");
        // Prompt and model changes leave the saved summary current.
        save(&mut s, |settings| {
            settings.system = "Summarize tersely.".into();
            settings.model.as_mut().unwrap().name = "other".into();
            settings
                .accounts
                .entry(A.into())
                .or_default()
                .classification = true;
        });
        assert_eq!(summary(&s)["stale"], false);
        assert_eq!(
            s.local_model_preview(A, C, M).unwrap().as_deref(),
            Some("First summary.")
        );
        // Queue work adds the missing classification without replacing the summary.
        let p = s.local_model_prepare(A, C, M).unwrap();
        let saved = s
            .local_model_finish(&p, output("Second summary.", json!([])))
            .unwrap();
        assert_eq!(saved["items"].as_array().unwrap().len(), 1);
        assert_eq!(saved["items"][0]["task"], "classification");
        assert_eq!(summary(&s)["output"]["summary"], "First summary.");
        // An explicit request (Resummarize) replaces it, with the new prompt's hash.
        let mut p = s.local_model_prepare(A, C, M).unwrap();
        p.overwrite = true;
        s.local_model_finish(&p, output("Third summary.", json!([])))
            .unwrap();
        let now = summary(&s);
        assert_eq!(now["output"]["summary"], "Third summary.");
        assert_eq!(now["model"]["name"], "other");
        assert_eq!(now["schema"], 3);
    }
    #[test]
    fn a_correction_between_prepare_and_finish_is_refused() {
        let (_dir, mut s) = configured_store();
        let pending = s.local_model_prepare(A, C, M).unwrap();
        let revision: i64 =
            s.db.query_row(
                "SELECT revision FROM messages WHERE account=?1 AND id=?2",
                params![A, M],
                |r| r.get(0),
            )
            .unwrap();
        s.local_model_handle("assist_correct",Some(A),&json!({"conversation_id":C,"message_id":M,"kind":"automated","expected_revision":revision})).unwrap();
        assert_eq!(
            s.local_model_finish(&pending, output("x y", json!([])))
                .unwrap_err()
                .code,
            "assistance_stale"
        );
        let settings_conflict = s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":0,"settings":Settings::default()}),
        );
        assert_eq!(settings_conflict.unwrap_err().code, "assistance_stale");
    }
    #[test]
    fn results_saved_before_d111_stay_current() {
        let (_dir, s) = configured_store();
        let p = s.local_model_prepare(A, C, M).unwrap();
        s.local_model_finish(&p, output("Saved earlier.", json!([])))
            .unwrap();
        // Older snapshots also held the model settings and a version number.
        s.db.execute("UPDATE local_model_results SET snapshot_json=json_set(snapshot_json,'$.version',2,'$.settings',json('{\"prompt\":\"old\"}')) WHERE account=?1",[A]).unwrap();
        assert_eq!(summary(&s)["stale"], false);
        assert_eq!(
            s.local_model_preview(A, C, M).unwrap().as_deref(),
            Some("Saved earlier.")
        );
    }
    #[test]
    fn unlabeled_classifications_are_forgotten_after_a_label_import() {
        let (_dir, s) = configured_store();
        let p = s.local_model_prepare(A, C, M).unwrap();
        s.local_model_finish(&p, output("Saved earlier.", json!([])))
            .unwrap();
        s.local_model_forget_unlabeled(A).unwrap();
        let tasks: Vec<String> = s.local_model_results(A, C).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["target"] == M)
            .map(|r| r["task"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(tasks, vec!["summary"]);
    }
}
