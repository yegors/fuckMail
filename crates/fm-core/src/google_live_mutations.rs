//! Durable delivery of explicit local message snapshots. No HTTP runs on Store.
use crate::{
    err,
    live_profile::{ConnectionState, ProfileKind},
    Result, Store,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailMutationAction {
    Modify,
    Trash,
    Restore,
}
impl MailMutationAction {
    fn key(&self) -> &'static str {
        match self {
            Self::Modify => "modify",
            Self::Trash => "trash",
            Self::Restore => "restore",
        }
    }
}
fn is_false(v: &bool) -> bool {
    !*v
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct DesiredField {
    #[serde(default, skip_serializing_if = "is_false")]
    assisted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_categories: Option<BTreeSet<String>>,
    field: String,
    key: String,
    value: bool,
    revision: i64,
    label: String,
    present: bool,
}

#[derive(Clone, Debug)]
pub struct MailMutationWork {
    pub account: String,
    pub request_id: String,
    pub message_id: String,
    pub remote_id: String,
    pub action: MailMutationAction,
    pub connection_generation: i64,
    pub attempt: i64,
    now_ms: u64,
    desired: Vec<DesiredField>,
}
#[derive(Clone, Debug)]
pub struct MailLabelObservation {
    pub remote_id: String,
    pub remote_thread_id: String,
    pub history_id: String,
    pub labels: BTreeSet<String>,
}
#[derive(Clone, Debug)]
pub struct MailMutationAttempt {
    pub work: MailMutationWork,
    pub add_labels: Vec<String>,
    pub remove_labels: Vec<String>,
}
#[derive(Clone, Debug)]
pub enum MailMutationDecision {
    Finished,
    RetryRead,
    Submit(MailMutationAttempt),
}
#[derive(Clone, Debug)]
pub enum MailMutationAttemptOutcome {
    Submitted,
    Reconcile {
        retry_at_ms: u64,
        error_code: &'static str,
    },
    Failed {
        error_code: &'static str,
    },
}

/// Same transaction as the local operation and its exact replay receipt.
pub(crate) fn enqueue(
    db: &Transaction<'_>,
    account: &str,
    request: &str,
    method: &str,
    p: &Value,
) -> Result<()> {
    if !matches!(
        method,
        "flag" | "set_category" | "mail_filing" | "mail_undo"
    ) {
        return Ok(());
    }
    let live: bool = db.query_row(
        "SELECT value=1 FROM meta WHERE key='profile_kind'",
        [],
        |r| r.get(0),
    )?;
    if !live {
        return Ok(());
    }
    if method == "set_category" && !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM google_category_labels WHERE account=?1 AND category_id=?2)",
        params![account,crate::string(p,"category_id")?], |r|r.get::<_,bool>(0),
    )? {
        // Local memberships remain explicit intents until the user links the label.
        return Ok(());
    }
    validate_authoring(db, account)?;
    let action = match p["action"].as_str() {
        Some("trash") => MailMutationAction::Trash,
        Some("restore") => MailMutationAction::Restore,
        _ => MailMutationAction::Modify,
    };
    let keys: Vec<(String, String)> = match method {
        "flag" => ["read", "starred"]
            .into_iter()
            .filter(|field| p.get(*field).is_some())
            .map(|field| (field.into(), String::new()))
            .collect(),
        "set_category" => vec![("category".into(), crate::string(p, "category_id")?.into())],
        "mail_filing" => vec![(
            "label".into(),
            if action == MailMutationAction::Modify {
                "INBOX"
            } else {
                "TRASH"
            }
            .into(),
        )],
        "mail_undo" => vec![],
        _ => unreachable!(),
    };
    let undo_ids;
    let ids = if method == "mail_undo" {
        let receipt: String = db.query_row(
            "SELECT result_json FROM operations WHERE account=?1 AND request_id=?2",
            params![account, crate::string(p, "undo_request_id")?],
            |r| r.get(0),
        )?;
        let receipt: Value = serde_json::from_str(&receipt)?;
        undo_ids = Value::Array(
            receipt["mail_undo"]["messages"]
                .as_array()
                .ok_or_else(|| err("storage_error", "Undo snapshot missing"))?
                .iter()
                .map(|v| v["id"].clone())
                .collect(),
        );
        &undo_ids
    } else {
        &p["message_ids"]
    };
    for value in ids
        .as_array()
        .ok_or_else(|| err("invalid_request", "Supply message IDs"))?
    {
        let message = value
            .as_str()
            .ok_or_else(|| err("invalid_request", "Invalid message ID"))?;
        let remote:String=db.query_row("SELECT m.remote_id FROM messages m JOIN google_message_state s ON s.account=m.account AND s.message_id=m.id WHERE m.account=?1 AND m.id=?2 AND s.deleted=0",params![account,message],|r|r.get(0)).optional()?.ok_or_else(||err("not_found","Provider message is no longer available"))?;
        let mut desired = Vec::new();
        let restored_keys = if method == "mail_undo" {
            // Undo bumps each message revision before recording only the fields it
            // changed, so older human intents are not re-sent as part of it.
            let mut stmt=db.prepare_cached("SELECT field,category_id FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND revision=(SELECT revision FROM messages WHERE account=?1 AND id=?2) AND (field!='category' OR category_id IN (SELECT category_id FROM google_category_labels WHERE account=?1)) ORDER BY field,category_id")?;
            let rows = stmt
                .query_map(params![account, message], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        } else {
            keys.clone()
        };
        for (field, key) in &restored_keys {
            let (value,revision):(bool,i64)=db.query_row("SELECT value,revision FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field=?3 AND category_id=?4",params![account,message,field,key],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let (label, present) = match field.as_str() {
                "read" => ("UNREAD".into(), !value),
                "starred" => ("STARRED".into(), value),
                "label" => (key.clone(), value),
                "category" => {
                    let label:String=db.query_row("SELECT provider_label_id FROM google_category_labels WHERE account=?1 AND category_id=?2",params![account,key],|r|r.get(0)).optional()?.ok_or_else(||err("label_mapping_required","Choose an existing Google label for this category"))?;
                    if crate::google_store::is_system_label(&label) {
                        return Err(err("invalid_request", "Category requires a user label"));
                    }
                    (label, value)
                }
                _ => unreachable!(),
            };
            desired.push(DesiredField {
                assisted: false,
                provider_categories: None,
                field: field.clone(),
                key: key.clone(),
                value,
                revision,
                label,
                present,
            });
        }
        if desired.is_empty() && method == "mail_undo" {
            // This message already matched its snapshot or changed only unlinked labels.
            continue;
        }
        db.execute("INSERT INTO google_mail_mutations(account,request_id,message_id,remote_id,action,desired_json,state) VALUES(?1,?2,?3,?4,?5,?6,'pending')",params![account,request,message,remote,action.key(),serde_json::to_string(&desired)?])?;
    }
    Ok(())
}
/// Persist a source-validated model label through the existing delivery journal.
/// Human choices are durable even after their provider acknowledgement.
pub(crate) fn enqueue_assisted_category(
    db: &Transaction<'_>,
    account: &str,
    message: &str,
    category: &str,
) -> Result<&'static str> {
    let blocked:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM local_model_label_overrides WHERE account=?1 AND message_id=?2) OR EXISTS(SELECT 1 FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field='category')",params![account,message],|r|r.get(0))?;
    if blocked {
        return Ok("manual_override");
    }
    let (sent, trashed, revision): (bool, bool, i64) = db.query_row(
        "SELECT sent,trashed,revision FROM messages WHERE account=?1 AND id=?2",
        params![account, message],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if sent || trashed {
        return Ok("not_incoming");
    }
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM message_categories WHERE account=?1 AND message_id=?2 AND category_id=?3)",params![account,message,category],|r|r.get(0))?;
    if exists {
        return Ok("already_present");
    }
    let live: bool = db.query_row(
        "SELECT value=1 FROM meta WHERE key='profile_kind'",
        [],
        |r| r.get(0),
    )?;
    let mapping:Option<String>=db.query_row("SELECT provider_label_id FROM google_category_labels WHERE account=?1 AND category_id=?2",params![account,category],|r|r.get(0)).optional()?;
    if mapping
        .as_ref()
        .is_some_and(|label| crate::google_store::is_system_label(label))
    {
        return Ok("label_unmapped");
    }
    let deliver = live && mapping.is_some();
    if deliver {
        match validate_authoring(db, account) {
            Err(e) if e.code == "capability_unavailable" || e.code == "not_found" => {
                return Ok("permission_required")
            }
            Err(e) => return Err(e),
            Ok(()) => (),
        }
    }
    let remote:Option<String>=db.query_row("SELECT m.remote_id FROM messages m JOIN google_message_state g ON g.account=m.account AND g.message_id=m.id WHERE m.account=?1 AND m.id=?2 AND g.deleted=0",params![account,message],|r|r.get(0)).optional()?;
    if deliver && remote.is_none() {
        return Ok("source_unavailable");
    }
    let request = format!("assist-{}-{}", message, revision + 1);
    db.execute(
        "INSERT OR IGNORE INTO message_categories(account,message_id,category_id) VALUES(?1,?2,?3)",
        params![account, message, category],
    )?;
    db.execute("INSERT INTO assist_category_intents(account,message_id,category_id,value,revision,request_id) VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(account,message_id,category_id) DO UPDATE SET value=1,revision=excluded.revision,request_id=excluded.request_id",params![account,message,category,revision+1,request])?;
    db.execute(
        "UPDATE messages SET revision=revision+1 WHERE account=?1 AND id=?2",
        params![account, message],
    )?;
    if deliver {
        let labels:String=db.query_row("SELECT provider_labels_json FROM google_message_state WHERE account=?1 AND message_id=?2",params![account,message],|r|r.get(0))?;
        let labels: BTreeSet<String> = serde_json::from_str(&labels)?;
        let categories = mapped_labels(db, account, &labels)?;
        let desired = vec![DesiredField {
            assisted: true,
            provider_categories: Some(categories),
            field: "category".into(),
            key: category.into(),
            value: true,
            revision: revision + 1,
            label: mapping.unwrap(),
            present: true,
        }];
        use sha2::{Digest, Sha256};
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([
                "assist_label_v1",
                account,
                message,
                category,
                revision
            ]))?)
        );
        let receipt = json!({"kind":"assist_label","message_id":message,"category_id":category,"delivery":{"state":"pending","request_id":request}});
        db.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,serde_json::to_string(&receipt)?])?;
        db.execute("INSERT INTO google_mail_mutations(account,request_id,message_id,remote_id,action,desired_json,state) VALUES(?1,?2,?3,?4,'modify',?5,'pending')",params![account,request,message,remote.unwrap(),serde_json::to_string(&desired)?])?;
        Ok("pending")
    } else {
        Ok(if live { "local_only" } else { "applied" })
    }
}

/// Auto-read is admitted only for an incoming, still-unread message whose
/// selected model label is enabled. A pending human read choice wins.
pub(crate) fn enqueue_assisted_read(
    db: &Transaction<'_>,
    account: &str,
    message: &str,
) -> Result<bool> {
    let row: Option<(bool,bool,bool,i64,String)> = db.query_row(
        "SELECT unread,sent,trashed,revision,remote_id FROM messages WHERE account=?1 AND id=?2",
        params![account,message], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
    ).optional()?;
    let Some((unread, sent, trashed, revision, remote)) = row else {
        return Ok(false);
    };
    if !unread || sent || trashed || db.query_row(
        "SELECT EXISTS(SELECT 1 FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field='read')",
        params![account,message], |r|r.get::<_,bool>(0)
    )? { return Ok(false); }
    let live: bool = db.query_row(
        "SELECT value=1 FROM meta WHERE key='profile_kind'",
        [],
        |r| r.get(0),
    )?;
    if live {
        if validate_authoring(db, account).is_err() {
            return Ok(false);
        }
        let available:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM google_message_state WHERE account=?1 AND message_id=?2 AND deleted=0)",params![account,message],|r|r.get(0))?;
        if !available {
            return Ok(false);
        }
    }
    let next = revision + 1;
    db.execute(
        "UPDATE messages SET unread=0,revision=?3 WHERE account=?1 AND id=?2",
        params![account, message, next],
    )?;
    if live {
        let request = format!("assist-read-{message}-{next}");
        use sha2::{Digest, Sha256};
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([
                "assist_read_v1",
                account,
                message,
                next
            ]))?)
        );
        let receipt = json!({"kind":"assist_read","message_id":message,"delivery":{"state":"pending","request_id":request}});
        db.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,serde_json::to_string(&receipt)?])?;
        db.execute("INSERT INTO google_manual_intents(account,message_id,field,category_id,value,revision) VALUES(?1,?2,'read','',1,?3)",params![account,message,next])?;
        let desired = vec![DesiredField {
            assisted: false,
            provider_categories: None,
            field: "read".into(),
            key: String::new(),
            value: true,
            revision: next,
            label: "UNREAD".into(),
            present: false,
        }];
        db.execute("INSERT INTO google_mail_mutations(account,request_id,message_id,remote_id,action,desired_json,state) VALUES(?1,?2,?3,?4,'modify',?5,'pending')",params![account,request,message,remote,serde_json::to_string(&desired)?])?;
    }
    Ok(true)
}
fn mapped_labels(
    db: &Connection,
    account: &str,
    labels: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let mut stmt =
        db.prepare("SELECT provider_label_id FROM google_category_labels WHERE account=?1")?;
    let mapped = stmt
        .query_map([account], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    Ok(labels.intersection(&mapped).cloned().collect())
}

/// Admit previously local label choices after an explicit label link/sync.
pub(crate) fn enqueue_bound_categories(db: &Transaction<'_>, account: &str) -> Result<usize> {
    validate_authoring(db, account)?;
    let rows=db.prepare("WITH intents AS (
        SELECT account,message_id,category_id,value,revision,0 AS assisted FROM google_manual_intents WHERE account=?1 AND field='category'
        UNION ALL SELECT account,message_id,category_id,value,revision,1 FROM assist_category_intents WHERE account=?1
        AND NOT EXISTS(SELECT 1 FROM local_model_label_overrides o WHERE o.account=assist_category_intents.account AND o.message_id=assist_category_intents.message_id)
    ) SELECT i.message_id,i.category_id,i.value,i.revision,i.assisted,m.remote_id,g.provider_labels_json,c.provider_label_id
    FROM intents i JOIN messages m ON m.account=i.account AND m.id=i.message_id
    JOIN google_message_state g ON g.account=i.account AND g.message_id=i.message_id AND g.deleted=0
    JOIN google_category_labels c ON c.account=i.account AND c.category_id=i.category_id
    WHERE NOT EXISTS(SELECT 1 FROM google_mail_mutations j,json_each(j.desired_json) d WHERE j.account=i.account AND j.message_id=i.message_id AND json_extract(d.value,'$.field')='category' AND json_extract(d.value,'$.key')=i.category_id AND json_extract(d.value,'$.revision')=i.revision AND json_extract(d.value,'$.label')=c.provider_label_id AND COALESCE(json_extract(d.value,'$.assisted'),0)=i.assisted)
    ORDER BY i.message_id,i.category_id LIMIT 64")?.query_map([account],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,i64>(3)?,r.get::<_,bool>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (message, category, value, revision, assisted, remote, labels, label) in &rows {
        if crate::google_store::is_system_label(label) {
            return Err(err(
                "invalid_request",
                "Category requires a Google user label",
            ));
        }
        let desired = vec![DesiredField {
            assisted: *assisted,
            provider_categories: if *assisted {
                Some(mapped_labels(db, account, &serde_json::from_str(labels)?)?)
            } else {
                None
            },
            field: "category".into(),
            key: category.clone(),
            value: *value,
            revision: *revision,
            label: label.clone(),
            present: *value,
        }];
        use sha2::{Digest, Sha256};
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([
                "linked_category_v1",
                account,
                message,
                category,
                value,
                revision,
                assisted,
                label
            ]))?)
        );
        let request = format!("label-link-{fingerprint}");
        let receipt = json!({"kind":"linked_category","message_id":message,"category_id":category,"assisted":assisted,"delivery":{"state":"pending","request_id":request}});
        db.execute("INSERT OR IGNORE INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,serde_json::to_string(&receipt)?])?;
        db.execute("INSERT OR IGNORE INTO google_mail_mutations(account,request_id,message_id,remote_id,action,desired_json,state) VALUES(?1,?2,?3,?4,'modify',?5,'pending')",params![account,request,message,remote,serde_json::to_string(&desired)?])?;
    }
    Ok(rows.len())
}
fn validate_authoring(db: &Connection, account: &str) -> Result<()> {
    let scopes: Option<String> = db
        .query_row(
            "SELECT scopes_json FROM google_accounts WHERE account=?1",
            [account],
            |r| r.get(0),
        )
        .optional()?;
    let scopes: BTreeSet<String> = serde_json::from_str(
        &scopes.ok_or_else(|| err("not_found", "Google account is unavailable"))?,
    )?;
    if !fm_google::auth::ScopeSet::new(&scopes).mail_write()
        || !crate::preferences::google_write_enabled(db, account, "mail")?
    {
        return Err(err(
            "capability_unavailable",
            "Google mail modification permission is required",
        ));
    }
    Ok(())
}
impl Store {
    pub fn google_mail_mutation_next(
        &self,
        account: &str,
        generation: i64,
        now_ms: u64,
    ) -> Result<Option<MailMutationWork>> {
        self.mutation_connection(account, generation)?;
        type Row = (String, String, String, String, String, i64);
        let row:Option<Row>=self.db.query_row("SELECT request_id,message_id,remote_id,action,desired_json,attempt FROM google_mail_mutations WHERE account=?1 AND state IN ('pending','in_flight','reconcile') AND retry_at_ms<=?2 ORDER BY rowid LIMIT 1",params![account,integer_time(now_ms)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        row.map(
            |(request_id, message_id, remote_id, action, desired, attempt)| {
                Ok(MailMutationWork {
                    account: account.into(),
                    request_id,
                    message_id,
                    remote_id,
                    action: serde_json::from_value(json!(action))?,
                    desired: serde_json::from_str(&desired)?,
                    attempt,
                    connection_generation: generation,
                    now_ms,
                })
            },
        )
        .transpose()
    }
    pub fn google_mail_mutation_observe(
        &mut self,
        work: &MailMutationWork,
        observation: Option<MailLabelObservation>,
    ) -> Result<MailMutationDecision> {
        self.mutation_connection(&work.account, work.connection_generation)?;
        let tx = self.db.unchecked_transaction()?;
        verify_work(&tx, work)?;
        let local: Option<(String, String)> = tx
            .query_row(
                "SELECT conversation_id,remote_id FROM messages WHERE account=?1 AND id=?2",
                params![work.account, work.message_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((conversation, remote)) = local else {
            finish(&tx, work, "failed", Some("local_message_missing"))?;
            tx.commit()?;
            return Ok(MailMutationDecision::Finished);
        };
        if remote != work.remote_id {
            return Err(err(
                "stale_google_work",
                "Captured message identity changed",
            ));
        }
        let Some(observed) = observation else {
            finish(&tx, work, "failed", Some("provider_message_missing"))?;
            tx.commit()?;
            return Ok(MailMutationDecision::Finished);
        };
        validate_observation(work, &observed)?;
        let history: u128 = observed.history_id.parse().map_err(|_| {
            err(
                "invalid_google_history",
                "Invalid Google history identifier",
            )
        })?;
        let previous:Option<(String,String)>=tx.query_row("SELECT history_id,remote_thread_id FROM google_message_state WHERE account=?1 AND message_id=?2",params![work.account,work.message_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if previous
            .as_ref()
            .is_some_and(|(_, thread)| thread != &observed.remote_thread_id)
        {
            return Err(err(
                "invalid_google_response",
                "Google message thread changed",
            ));
        }
        let coverage: Option<String> = tx
            .query_row(
                "SELECT history_id FROM google_mail_coverage WHERE account=?1 AND remote_id=?2",
                params![work.account, work.remote_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if previous
            .as_ref()
            .and_then(|(v, _)| v.parse::<u128>().ok())
            .into_iter()
            .chain(coverage.as_ref().and_then(|v| v.parse::<u128>().ok()))
            .any(|v| v > history)
        {
            tx.execute("UPDATE google_mail_mutations SET state='reconcile',retry_at_ms=?4,error_code='stale_readback' WHERE account=?1 AND request_id=?2 AND message_id=?3",params![work.account,work.request_id,work.message_id,integer_time(work.now_ms.saturating_add(1000))])?;
            tx.commit()?;
            return Ok(MailMutationDecision::RetryRead);
        }
        let mut active = Vec::new();
        for desired in &work.desired {
            let current: Option<(bool, i64)> = if desired.assisted {
                let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM local_model_label_overrides WHERE account=?1 AND message_id=?2) OR EXISTS(SELECT 1 FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field='category')",params![work.account,work.message_id],|r|r.get(0))?;
                let provider_changed = work.attempt == 0
                    && !observed.labels.contains(&desired.label)
                    && desired
                        .provider_categories
                        .as_ref()
                        .is_some_and(|baseline| {
                            mapped_labels(&tx, &work.account, &observed.labels)
                                .is_ok_and(|current| &current != baseline)
                        });
                if blocked || provider_changed {
                    tx.execute("DELETE FROM assist_category_intents WHERE account=?1 AND message_id=?2 AND category_id=?3",params![work.account,work.message_id,desired.key])?;
                    None
                } else {
                    tx.query_row("SELECT value,revision FROM assist_category_intents WHERE account=?1 AND message_id=?2 AND category_id=?3",params![work.account,work.message_id,desired.key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?
                }
            } else {
                tx.query_row("SELECT value,revision FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field=?3 AND category_id=?4",params![work.account,work.message_id,desired.field,desired.key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?
            };
            if current != Some((desired.value, desired.revision)) {
                continue;
            }
            if desired.field == "category" {
                let mapping:Option<String>=tx.query_row("SELECT provider_label_id FROM google_category_labels WHERE account=?1 AND category_id=?2",params![work.account,desired.key],|r|r.get(0)).optional()?;
                if mapping.as_deref() != Some(&desired.label) {
                    finish(&tx, work, "failed", Some("label_mapping_changed"))?;
                    tx.commit()?;
                    return Ok(MailMutationDecision::Finished);
                }
            }
            active.push(desired.clone());
        }
        let was_active = !active.is_empty();
        let mut remaining = Vec::new();
        for desired in active {
            if observed.labels.contains(&desired.label) == desired.present {
                if desired.assisted {
                    tx.execute("DELETE FROM assist_category_intents WHERE account=?1 AND message_id=?2 AND category_id=?3 AND revision=?4",params![work.account,work.message_id,desired.key,desired.revision])?;
                } else {
                    tx.execute("DELETE FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field=?3 AND category_id=?4 AND value=?5 AND revision=?6",params![work.account,work.message_id,desired.field,desired.key,desired.value,desired.revision])?;
                }
            } else {
                remaining.push(desired);
            }
        }
        tx.execute("UPDATE google_message_state SET provider_labels_json=?3,history_id=?4,deleted=0 WHERE account=?1 AND message_id=?2",params![work.account,work.message_id,serde_json::to_string(&observed.labels)?,observed.history_id])?;
        // Keep the existing acquisition fence current without changing source,
        // index, coverage metrics or any lane's independent continuation.
        tx.execute(
            "UPDATE google_mail_coverage SET history_id=?3 WHERE account=?1 AND remote_id=?2",
            params![work.account, work.remote_id, observed.history_id],
        )?;
        let before = tx.total_changes();
        project_labels(&tx, &work.account, &work.message_id, &observed.labels)?;
        if tx.total_changes() != before {
            tx.execute(
                "UPDATE messages SET revision=revision+1 WHERE account=?1 AND id=?2",
                params![work.account, work.message_id],
            )?;
            tx.execute(
                "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                params![work.account, conversation],
            )?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        if remaining.is_empty() {
            finish(
                &tx,
                work,
                if was_active {
                    "confirmed"
                } else {
                    "superseded"
                },
                None,
            )?;
            tx.commit()?;
            return Ok(MailMutationDecision::Finished);
        }
        let attempt = work
            .attempt
            .checked_add(1)
            .ok_or_else(|| err("storage_error", "Mutation attempt sequence exhausted"))?;
        tx.execute("UPDATE google_mail_mutations SET state='in_flight',attempt=?4,connection_generation=?5,retry_at_ms=0,error_code=NULL WHERE account=?1 AND request_id=?2 AND message_id=?3",params![work.account,work.request_id,work.message_id,attempt,work.connection_generation])?;
        let mut claimed = work.clone();
        claimed.attempt = attempt;
        // Trash/restore can change other labels. Reconcile it first, then use
        // modify for remaining fields in the same durable Undo operation.
        claimed.action = remaining
            .iter()
            .find(|v| v.label == "TRASH")
            .map(|v| {
                if v.present {
                    MailMutationAction::Trash
                } else {
                    MailMutationAction::Restore
                }
            })
            .unwrap_or(MailMutationAction::Modify);
        let add_labels = remaining
            .iter()
            .filter(|v| v.present)
            .map(|v| v.label.clone())
            .collect();
        let remove_labels = remaining
            .iter()
            .filter(|v| !v.present)
            .map(|v| v.label.clone())
            .collect();
        tx.commit()?;
        Ok(MailMutationDecision::Submit(MailMutationAttempt {
            work: claimed,
            add_labels,
            remove_labels,
        }))
    }
    pub fn google_mail_mutation_attempt_result(
        &mut self,
        attempt: &MailMutationAttempt,
        outcome: MailMutationAttemptOutcome,
    ) -> Result<()> {
        let work = &attempt.work;
        // An already admitted HTTP attempt may return after disconnect. Only
        // its exact durable claim can accept this status; no labels or pending
        // human intent are published here, and a newer claim fences it out.
        let tx = self.db.unchecked_transaction()?;
        verify_work(&tx, work)?;
        let (state, retry, error) = match outcome {
            MailMutationAttemptOutcome::Submitted => {
                // A successful HTTP response is still not a label receipt.
                // Repeated mismatches (including provider propagation delay)
                // are read back with bounded cadence instead of a hot POST loop.
                let delay = if work.attempt <= 1 {
                    0
                } else if work.attempt >= 6 {
                    300_000
                } else {
                    1000_u64 << (work.attempt - 2)
                };
                (
                    "reconcile",
                    integer_time(work.now_ms.saturating_add(delay)),
                    None,
                )
            }
            MailMutationAttemptOutcome::Reconcile {
                retry_at_ms,
                error_code,
            } => ("reconcile", integer_time(retry_at_ms), Some(error_code)),
            MailMutationAttemptOutcome::Failed { error_code } => ("failed", 0, Some(error_code)),
        };
        let changed=tx.execute("UPDATE google_mail_mutations SET state=?4,retry_at_ms=?5,error_code=?6 WHERE account=?1 AND request_id=?2 AND message_id=?3 AND state='in_flight' AND connection_generation=?7",params![work.account,work.request_id,work.message_id,state,retry,error,work.connection_generation])?;
        if changed != 1 {
            return Err(err(
                "stale_google_work",
                "Mutation attempt is no longer current",
            ));
        }
        tx.commit()?;
        Ok(())
    }
    pub fn google_mail_mutation_status(&self, account: &str) -> Result<Value> {
        let mut statement = self.db.prepare(
            "SELECT state,count(*) FROM google_mail_mutations WHERE account=?1 GROUP BY state",
        )?;
        let counts = statement
            .query_map([account], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
            })?
            .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()?;
        let mut statement=self.db.prepare("SELECT request_id,message_id,error_code FROM google_mail_mutations WHERE account=?1 AND state='failed' ORDER BY rowid DESC LIMIT 20")?;
        let failures=statement.query_map([account],|r|Ok(json!({"request_id":r.get::<_,String>(0)?,"message_id":r.get::<_,String>(1)?,"error_code":r.get::<_,Option<String>>(2)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let mut statement=self.db.prepare("SELECT request_id,message_id,state,error_code,retry_at_ms FROM google_mail_mutations WHERE account=?1 AND state IN ('pending','in_flight','reconcile') ORDER BY rowid LIMIT 20")?;
        let pending=statement.query_map([account],|r|Ok(json!({"request_id":r.get::<_,String>(0)?,"message_id":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"error_code":r.get::<_,Option<String>>(3)?,"retry_at_ms":r.get::<_,i64>(4)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        Ok(json!({"counts":counts,"failures":failures,"pending":pending}))
    }
    /// Exact delivery state is derived from every row in this immutable request,
    /// never from the separately bounded account pending/failure previews.
    pub fn google_mail_mutation_request_status(
        &self,
        account: &str,
        request_id: &str,
    ) -> Result<Value> {
        crate::id(account)?;
        crate::id(request_id)?;
        let mut statement = self.db.prepare(
            "SELECT state,count(*) FROM google_mail_mutations WHERE account=?1 AND request_id=?2 GROUP BY state",
        )?;
        let counts = statement
            .query_map(params![account, request_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })?
            .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()?;
        let total: u64 = counts.values().sum();
        if total == 0 {
            return Err(err(
                "not_found",
                "No mail delivery journal exists for this request",
            ));
        }
        let count = |state: &str| counts.get(state).copied().unwrap_or(0);
        let state = if count("confirmed") == total {
            "confirmed"
        } else if count("in_flight") + count("reconcile") > 0 {
            "unknown"
        } else if count("failed") > 0 {
            "failed"
        } else if count("pending") > 0 {
            "pending"
        } else {
            "superseded"
        };
        Ok(json!({"request_id":request_id,"state":state,"counts":counts,"total":total}))
    }
    fn mutation_connection(&self, account: &str, generation: i64) -> Result<()> {
        self.require_active_account(account)?;
        if self.profile_kind()? != ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Live delivery requires a live profile",
            ));
        }
        if !self.google_account_bindings()?.iter().any(|binding| {
            binding.account == account
                && binding.connection_generation == generation
                && binding.state == ConnectionState::Connected
        }) {
            return Err(err("stale_google_work", "Google connection changed"));
        }
        validate_authoring(&self.db, account)
    }
}
fn integer_time(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}
fn verify_work(db: &Connection, work: &MailMutationWork) -> Result<()> {
    let current:Option<(String,i64,String)>=db.query_row("SELECT state,attempt,remote_id FROM google_mail_mutations WHERE account=?1 AND request_id=?2 AND message_id=?3",params![work.account,work.request_id,work.message_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    if !current.is_some_and(|(state, attempt, remote)| {
        matches!(state.as_str(), "pending" | "in_flight" | "reconcile")
            && attempt == work.attempt
            && remote == work.remote_id
    }) {
        return Err(err(
            "stale_google_work",
            "Mutation work is no longer current",
        ));
    }
    Ok(())
}
fn finish(
    db: &Connection,
    work: &MailMutationWork,
    state: &str,
    error: Option<&str>,
) -> Result<()> {
    db.execute("UPDATE google_mail_mutations SET state=?4,error_code=?5,retry_at_ms=0 WHERE account=?1 AND request_id=?2 AND message_id=?3",params![work.account,work.request_id,work.message_id,state,error])?;
    Ok(())
}
fn validate_observation(work: &MailMutationWork, observation: &MailLabelObservation) -> Result<()> {
    if observation.remote_id != work.remote_id
        || observation.remote_thread_id.is_empty()
        || observation.remote_thread_id.len() > 256
        || observation.history_id.is_empty()
        || observation.history_id.len() > 40
        || !observation.history_id.bytes().all(|b| b.is_ascii_digit())
        || observation.labels.len() > 1000
        || observation
            .labels
            .iter()
            .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err(err(
            "invalid_google_response",
            "Invalid Google message state",
        ));
    }
    Ok(())
}
/// Shared by full-source publication and lightweight mutation observations.
pub(crate) fn project_labels(
    db: &Connection,
    account: &str,
    message: &str,
    labels: &BTreeSet<String>,
) -> Result<()> {
    let pending = |field: &str, key: &str| -> Result<Option<bool>> {
        Ok(db.query_row("SELECT value FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field=?3 AND category_id=?4",params![account,message,field,key],|r|r.get(0)).optional()?)
    };
    let unread = pending("read", "")?
        .map(|v| !v)
        .unwrap_or(labels.contains("UNREAD"));
    let starred = pending("starred", "")?.unwrap_or(labels.contains("STARRED"));
    let inbox = pending("label", "INBOX")?.unwrap_or(labels.contains("INBOX"));
    let trash = pending("label", "TRASH")?.unwrap_or(labels.contains("TRASH"));
    db.execute("UPDATE messages SET unread=?3,starred=?4,inbox=?5,sent=?6,trashed=?7 WHERE account=?1 AND id=?2 AND (unread!=?3 OR starred!=?4 OR inbox!=?5 OR sent!=?6 OR trashed!=?7)",params![account,message,unread,starred,inbox,labels.contains("SENT"),trash])?;
    let mut statement = db.prepare(
        "SELECT category_id,provider_label_id FROM google_category_labels WHERE account=?1",
    )?;
    let mappings = statement
        .query_map([account], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (category, label) in mappings {
        if crate::google_store::is_system_label(&label) {
            return Err(err(
                "invalid_google_response",
                "Category targets a system label",
            ));
        }
        let assisted:Option<bool>=db.query_row("SELECT value FROM assist_category_intents WHERE account=?1 AND message_id=?2 AND category_id=?3",params![account,message,category],|r|r.get(0)).optional()?;
        if pending("category", &category)?
            .or(assisted)
            .unwrap_or(labels.contains(&label))
        {
            db.execute("INSERT OR IGNORE INTO message_categories(account,message_id,category_id) VALUES(?1,?2,?3)",params![account,message,category])?;
        } else {
            db.execute("DELETE FROM message_categories WHERE account=?1 AND message_id=?2 AND category_id=?3",params![account,message,category])?;
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "google_live_mutations/tests.rs"]
mod tests;
