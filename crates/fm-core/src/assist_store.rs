//! Durable integration for the Phase 0 fake classifier/summarizer only.
//! This module is a Rust-internal API; no IPC method or model/network runner exists.
use crate::{err, id, Result, Store};
use fm_assist::{Boundary, CategoryResult, Mode, Policy, Runner, Source, SourceKind};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_MEMBERS: usize = 200;
const MAX_RETAINED_SOURCE_BYTES: u64 = 16 * 1024 * 1024;

pub enum AssistanceStart {
    Pending(PreparedAssistance),
    Completed(Value),
}

/// Constructed only after Core has resolved the account, policy and retained sources.
pub struct PreparedAssistance {
    account: String,
    conversation: String,
    request_id: String,
    fingerprint: String,
    snapshot: Snapshot,
    job_id: u64,
}

struct Snapshot {
    policy: Policy,
    members: Vec<Member>,
    mappings: Vec<(String, String, Option<String>)>,
}

#[derive(Serialize)]
struct Member {
    id: String,
    source_hash: String,
    revision: u64,
    sent: bool,
    manual: Vec<(String, String, bool, u64)>,
    provider: Option<(String, String, bool)>,
    #[serde(skip)]
    text: String,
}

impl Snapshot {
    fn encoded(&self) -> Result<String> {
        Ok(serde_json::to_string(
            &json!({"policy":{"account":self.policy.account,"revision":self.policy.revision,"mode":mode_name(self.policy.mode),"summaries":self.policy.summaries,"categories":self.policy.categories},"members":self.members,"mappings":self.mappings}),
        )?)
    }
    fn sources(&self) -> Vec<Source> {
        self.members
            .iter()
            .map(|member| Source {
                account: self.policy.account.clone(),
                id: member.id.clone(),
                content_revision: member.revision,
                state_revision: member.revision,
                manual_override_revision: member.manual.iter().map(|row| row.3).max().unwrap_or(0),
                kind: if member.sent {
                    SourceKind::Sent
                } else {
                    SourceKind::Received
                },
                text: member.text.clone(),
            })
            .collect()
    }
}

impl Store {
    pub fn finish_fake_assistance(
        &self,
        pending: &PreparedAssistance,
        boundary: &mut Boundary,
        raw: &[u8],
    ) -> Result<Value> {
        if raw.len() > fm_assist::MAX_RESULT_BYTES {
            return Err(err("assistance_limit", "Assistance result exceeds 16 KiB"));
        }
        let output_hash = digest(raw);
        // All reads of current stamps and all permitted writes share this transaction.
        // No runner or network operation occurs while it is held.
        let tx = self.db.unchecked_transaction()?;
        let previous:Option<(String,String,String)>=tx.query_row("SELECT request_fingerprint,output_sha256,result_json FROM assist_results WHERE account=?1 AND request_id=?2",params![pending.account,pending.request_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        if let Some((fingerprint, previous_hash, result)) = previous {
            if fingerprint != pending.fingerprint || previous_hash != output_hash {
                return Err(err(
                    "request_id_conflict",
                    "Assistance request ID already identifies another result",
                ));
            }
            let _ = boundary.finish(
                pending.job_id,
                &pending.snapshot.policy,
                &pending.snapshot.sources(),
                raw,
            );
            return Ok(serde_json::from_str(&result)?);
        }
        let current = self.assistance_snapshot(&pending.account, &pending.conversation, false)?;
        let accepted = boundary
            .finish(pending.job_id, &current.policy, &current.sources(), raw)
            .map_err(assist_error)?;
        let snapshot = current.encoded()?;
        if snapshot != pending.snapshot.encoded()? {
            return Err(err(
                "assistance_stale",
                "Retained source, provider, manual choice or mapping changed",
            ));
        }
        let (category_mode, categories) = match &accepted.categories {
            CategoryResult::Suggested(categories) => ("suggested", categories.clone()),
            CategoryResult::ApplyConfigured(categories) => ("applied", categories.clone()),
            CategoryResult::None => ("none", vec![]),
        };
        let mut applied = Vec::new();
        let mut skipped_manual = Vec::new();
        if matches!(accepted.categories, CategoryResult::ApplyConfigured(_)) {
            for included in &accepted.coverage.included {
                let source = current
                    .members
                    .iter()
                    .find(|source| source.id == included.id)
                    .ok_or_else(|| err("assistance_stale", "Included source disappeared"))?;
                if source.sent || source.provider.as_ref().is_some_and(|provider| provider.2) {
                    continue;
                }
                let mut changed = false;
                for category in &categories {
                    if source
                        .manual
                        .iter()
                        .any(|entry| entry.0 == "category" && entry.1 == *category)
                    {
                        skipped_manual.push(json!({"message_id":source.id,"category_id":category}));
                        continue;
                    }
                    let revision = source
                        .revision
                        .checked_add(1)
                        .ok_or_else(|| err("storage_error", "Message revision overflow"))?;
                    tx.execute("INSERT OR IGNORE INTO message_categories(account,message_id,category_id) VALUES(?1,?2,?3)",params![pending.account,source.id,category])?;
                    tx.execute("INSERT INTO assist_category_intents(account,message_id,category_id,value,revision,request_id) VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(account,message_id,category_id) DO UPDATE SET value=excluded.value,revision=excluded.revision,request_id=excluded.request_id",params![pending.account,source.id,category,revision,pending.request_id])?;
                    applied.push(json!({"message_id":source.id,"category_id":category}));
                    changed = true;
                }
                if changed {
                    tx.execute(
                        "UPDATE messages SET revision=revision+1 WHERE account=?1 AND id=?2",
                        params![pending.account, source.id],
                    )?;
                }
            }
        }
        if !applied.is_empty() {
            tx.execute(
                "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                params![pending.account, pending.conversation],
            )?;
        }
        let applied_snapshot_hash = digest(
            self.assistance_snapshot(&pending.account, &pending.conversation, false)?
                .encoded()?
                .as_bytes(),
        );
        let result = json!({"synthetic":true,"account":pending.account,"request_id":pending.request_id,"conversation_id":pending.conversation,
            "category_mode":category_mode,"categories":categories,"applied":applied,"skipped_manual":skipped_manual,"summary":accepted.summary,
            "coverage":{"source_count":accepted.coverage.source_count,"partial":accepted.coverage.partial,"included_message_ids":accepted.coverage.included.iter().map(|source|source.id.clone()).collect::<Vec<_>>()},
            "provenance":{"runner":"fake-phase0","prompt_schema":1,"policy_revision":current.policy.revision,"applied_snapshot_sha256":applied_snapshot_hash}});
        tx.execute("INSERT INTO assist_results(account,request_id,conversation_id,request_fingerprint,snapshot_json,output_sha256,result_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![pending.account,pending.request_id,pending.conversation,pending.fingerprint,snapshot,output_hash,serde_json::to_string(&result)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }

    /// Stored receipts are replayable; stale marks current source/selection coverage.
    pub fn fake_assistance_result(&self, account: &str, request_id: &str) -> Result<Value> {
        id(account)?;
        id(request_id)?;
        let row:Option<(String,String,String)>=self.db.query_row("SELECT conversation_id,snapshot_json,result_json FROM assist_results WHERE account=?1 AND request_id=?2",[account,request_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        let (conversation, snapshot, result) =
            row.ok_or_else(|| err("not_found", "Assistance result is not in this account"))?;
        let mut result: Value = serde_json::from_str(&result)?;
        let expected = result["provenance"]["applied_snapshot_sha256"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| digest(snapshot.as_bytes()));
        result["stale"] = json!(self
            .assistance_snapshot(account, &conversation, false)
            .and_then(|current| current.encoded())
            .map_or(true, |current| digest(current.as_bytes()) != expected));
        Ok(result)
    }

    /// Diagnostic policy configuration, not a settings/IPC surface or runner installer.
    pub fn configure_fake_assistance(&self, policy: &Policy, expected_revision: u64) -> Result<()> {
        id(&policy.account)?;
        if policy.categories.len() > 64
            || policy.revision
                != expected_revision
                    .checked_add(1)
                    .ok_or_else(|| err("invalid_request", "Policy revision overflow"))?
        {
            return Err(err("invalid_request", "Invalid assistance policy bounds"));
        }
        for category in &policy.categories {
            id(category)?;
            if [
                "SPAM", "TRASH", "INBOX", "UNREAD", "STARRED", "SENT", "DRAFT",
            ]
            .contains(&category.to_uppercase().as_str())
            {
                return Err(err(
                    "invalid_request",
                    "System categories cannot be model destinations",
                ));
            }
        }
        let tx = self.db.unchecked_transaction()?;
        let current = load_policy(&tx, &policy.account)?;
        if current.revision != expected_revision {
            return Err(err("stale_revision", "Assistance policy changed"));
        }
        configured_mappings(&tx, policy)?;
        tx.execute("INSERT INTO assist_policies(account,revision,mode,summaries,categories_json) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account) DO UPDATE SET revision=excluded.revision,mode=excluded.mode,summaries=excluded.summaries,categories_json=excluded.categories_json",params![policy.account,policy.revision,mode_name(policy.mode),policy.summaries,serde_json::to_string(&policy.categories)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn start_fake_assistance(
        &self,
        account: &str,
        conversation: &str,
        request_id: &str,
        owner: &str,
        boundary: &mut Boundary,
        runner: &mut impl Runner,
    ) -> Result<AssistanceStart> {
        for value in [account, conversation, request_id, owner] {
            id(value)?;
            if value.len() > 128 {
                return Err(err(
                    "invalid_request",
                    "Assistance identifier exceeds 128 bytes",
                ));
            }
        }
        // Owner controls cancellation lifetime, not immutable request identity: a
        // replacement client can replay a completed request after restart.
        let fingerprint = digest(&serde_json::to_vec(&json!([account, conversation]))?);
        if let Some((previous,result)) = self.db.query_row("SELECT request_fingerprint,result_json FROM assist_results WHERE account=?1 AND request_id=?2",[account,request_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional()? {
            if previous != fingerprint { return Err(err("request_id_conflict","Assistance request ID already identifies another scope")); }
            return Ok(AssistanceStart::Completed(serde_json::from_str(&result)?));
        }
        let policy = load_policy(&self.db, account)?;
        if policy.mode == Mode::Off && !policy.summaries {
            return Err(err("assistance_disabled", "Local assistance is off"));
        }
        let snapshot = self.assistance_snapshot(account, conversation, true)?;
        let job_id = boundary
            .start(owner, &snapshot.policy, &snapshot.sources(), runner)
            .map_err(assist_error)?;
        Ok(AssistanceStart::Pending(PreparedAssistance {
            account: account.into(),
            conversation: conversation.into(),
            request_id: request_id.into(),
            fingerprint,
            snapshot,
            job_id,
        }))
    }

    fn assistance_snapshot(
        &self,
        account: &str,
        conversation: &str,
        include_text: bool,
    ) -> Result<Snapshot> {
        let policy = load_policy(&self.db, account)?;
        let mappings = configured_mappings(&self.db, &policy)?;
        let rows = {
            let mut statement = self.db.prepare("SELECT id,source_hash,revision,sent,sender_name,sender_email,subject FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY sent_at,id LIMIT 201")?;
            let rows = statement
                .query_map([account, conversation], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        if rows.is_empty() {
            return Err(err(
                "not_found",
                "Assistance conversation is not in this account",
            ));
        }
        if rows.len() > MAX_MEMBERS {
            return Err(err(
                "assistance_limit",
                "Assistance conversation exceeds 200 members",
            ));
        }
        let mut source_bytes = 0u64;
        let mut members = Vec::new();
        for (message, hash, revision, sent, sender, email, subject) in rows {
            if !self.blob_present(&hash) {
                return Err(err(
                    "content_unavailable",
                    "Assistance source is not retained",
                ));
            }
            let size = std::fs::symlink_metadata(self.profile.join("blobs").join(&hash))?.len();
            source_bytes = source_bytes
                .checked_add(size)
                .filter(|bytes| *bytes <= MAX_RETAINED_SOURCE_BYTES)
                .ok_or_else(|| {
                    err(
                        "assistance_limit",
                        "Retained assistance sources exceed the 16 MiB diagnostic bound",
                    )
                })?;
            let path = self.verified_blob(&hash, size)?;
            let text = if include_text {
                let raw = std::fs::read(path)?;
                if raw.len() as u64 != size || digest(&raw) != hash {
                    return Err(err(
                        "content_unavailable",
                        "Assistance source changed during verification",
                    ));
                }
                let body = fm_mail::extract_incoming_text(&raw).map_err(|_| {
                    err(
                        "content_unavailable",
                        "Assistance source could not be decoded",
                    )
                })?;
                format!("From: {sender} <{email}>\nSubject: {subject}\n\n{body}")
            } else {
                String::new()
            };
            let manual = {
                let mut statement=self.db.prepare("SELECT field,category_id,value,revision FROM google_manual_intents WHERE account=?1 AND message_id=?2 ORDER BY field,category_id")?;
                let found = statement
                    .query_map([account, &message], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, bool>(2)?,
                            row.get::<_, u64>(3)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                found
            };
            let provider = self.db.query_row("SELECT history_id,provider_labels_json,deleted FROM google_message_state WHERE account=?1 AND message_id=?2",[account,&message],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,bool>(2)?))).optional()?;
            members.push(Member {
                id: message,
                source_hash: hash,
                revision,
                sent,
                manual,
                provider,
                text,
            });
        }
        Ok(Snapshot {
            policy,
            members,
            mappings,
        })
    }
}

fn load_policy(db: &Connection, account: &str) -> Result<Policy> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
        [account],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(err("not_found", "Assistance account does not exist"));
    }
    let row: Option<(u64, String, bool, String)> = db
        .query_row(
            "SELECT revision,mode,summaries,categories_json FROM assist_policies WHERE account=?1",
            [account],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((revision, mode, summaries, categories)) = row else {
        return Ok(Policy {
            account: account.into(),
            revision: 0,
            mode: Mode::Off,
            categories: BTreeSet::new(),
            summaries: false,
        });
    };
    let mode = match mode.as_str() {
        "off" => Mode::Off,
        "suggest" => Mode::Suggest,
        "auto_apply" => Mode::AutoApply,
        _ => return Err(err("storage_error", "Stored assistance mode is invalid")),
    };
    Ok(Policy {
        account: account.into(),
        revision,
        mode,
        summaries,
        categories: serde_json::from_str(&categories)?,
    })
}

fn configured_mappings(
    db: &Connection,
    policy: &Policy,
) -> Result<Vec<(String, String, Option<String>)>> {
    policy.categories.iter().map(|category| {
        let name:String=db.query_row("SELECT name FROM categories WHERE account=?1 AND id=?2",params![policy.account,category],|row|row.get(0)).optional()?.ok_or_else(||err("invalid_request","Assistance category is not configured for this account"))?;
        let mapping:Option<String>=db.query_row("SELECT provider_label_id FROM google_category_labels WHERE account=?1 AND category_id=?2",params![policy.account,category],|row|row.get(0)).optional()?;
        if mapping.as_ref().is_some_and(|label|crate::google_store::is_system_label(label)) {
            return Err(err("invalid_request","Assistance mappings cannot target provider system labels"));
        }
        if policy.mode==Mode::AutoApply && mapping.is_none() {return Err(err("invalid_request","Auto-applied categories require an explicit Google label mapping"));}
        Ok((category.clone(),name,mapping))
    }).collect()
}
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "off",
        Mode::Suggest => "suggest",
        Mode::AutoApply => "auto_apply",
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn assist_error(error: fm_assist::Error) -> crate::CoreError {
    let (code, message) = match error {
        fm_assist::Error::Disabled => ("assistance_disabled", "Local assistance is off"),
        fm_assist::Error::Unavailable => (
            "assistance_unavailable",
            "No local fake runner is available",
        ),
        fm_assist::Error::Stale => ("assistance_stale", "Assistance source or policy changed"),
        fm_assist::Error::Canceled => (
            "assistance_canceled",
            "Assistance job was canceled or consumed",
        ),
        fm_assist::Error::TooLarge | fm_assist::Error::Busy => {
            ("assistance_limit", "Assistance request exceeds its bound")
        }
        _ => (
            "invalid_assistance_result",
            "Assistance input or result failed validation",
        ),
    };
    err(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    const ACCOUNT: &str = "windscribe";
    const THREAD: &str = "windscribe-thread-001";
    const VALID: &[u8] =
        br#"{"categories":["finance"],"abstain":false,"summary":"A human request for review."}"#;

    #[derive(Default)]
    struct Fake {
        absent: bool,
        probes: Cell<usize>,
        started: Vec<fm_assist::Job>,
    }
    impl Runner for Fake {
        fn available(&self) -> bool {
            self.probes.set(self.probes.get() + 1);
            !self.absent
        }
        fn start(&mut self, job: &fm_assist::Job) -> std::result::Result<(), fm_assist::Error> {
            self.started.push(job.clone());
            Ok(())
        }
        fn cancel(&mut self, _: u64) {}
    }
    fn setup(store: &mut Store, mode: Mode) {
        store
            .google_map_category(ACCOUNT, "finance", "Label_finance")
            .unwrap();
        store
            .google_map_category(ACCOUNT, "noise", "Label_noise")
            .unwrap();
        let revision = load_policy(&store.db, ACCOUNT).unwrap().revision;
        store
            .configure_fake_assistance(
                &Policy {
                    account: ACCOUNT.into(),
                    revision: revision + 1,
                    mode,
                    summaries: true,
                    categories: ["finance".into(), "noise".into()].into(),
                },
                revision,
            )
            .unwrap();
    }
    fn start(
        store: &Store,
        id: &str,
        boundary: &mut Boundary,
        fake: &mut Fake,
    ) -> PreparedAssistance {
        match store
            .start_fake_assistance(ACCOUNT, THREAD, id, "fixture-owner", boundary, fake)
            .unwrap()
        {
            AssistanceStart::Pending(pending) => pending,
            AssistanceStart::Completed(_) => panic!("unexpected replay"),
        }
    }
    fn protected(store: &Store) -> Value {
        let mut statement=store.db.prepare("SELECT id,unread,starred,inbox,sent,body,source_hash FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY id").unwrap();
        let messages = statement
            .query_map([ACCOUNT, THREAD], |row| {
                Ok(json!([
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?
                ]))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        json!({"messages":messages,"draft":store.draft(ACCOUNT,THREAD).unwrap()})
    }

    #[test]
    fn disabled_or_absent_runner_never_starts_and_suggestions_exclude_drafts() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("profile")).unwrap();
        let mut boundary = Boundary::default();
        let mut fake = Fake::default();
        assert_eq!(
            store
                .start_fake_assistance(
                    ACCOUNT,
                    THREAD,
                    "off",
                    "fixture-owner",
                    &mut boundary,
                    &mut fake
                )
                .err()
                .unwrap()
                .code,
            "assistance_disabled"
        );
        assert_eq!(fake.probes.get(), 0);
        assert!(fake.started.is_empty());
        setup(&mut store, Mode::Suggest);
        fake.absent = true;
        assert_eq!(
            store
                .start_fake_assistance(
                    ACCOUNT,
                    THREAD,
                    "absent",
                    "fixture-owner",
                    &mut boundary,
                    &mut fake
                )
                .err()
                .unwrap()
                .code,
            "assistance_unavailable"
        );
        assert!(fake.started.is_empty());
        fake.absent = false;
        store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"UNPUBLISHED-REPLY-SECRET","expected_revision":0,"request_id":"draft"})).unwrap();
        let before = protected(&store);
        let pending = start(&store, "suggestion", &mut boundary, &mut fake);
        assert_eq!(fake.started[0].context.len(), 1);
        assert!(fake.started[0].context[0]
            .1
            .contains("Revised paywall copy is ready"));
        assert!(!fake.started[0]
            .context
            .iter()
            .any(|(_, text)| text.contains("UNPUBLISHED-REPLY-SECRET")));
        let result = store
            .finish_fake_assistance(&pending, &mut boundary, VALID)
            .unwrap();
        assert_eq!(result["category_mode"], "suggested");
        assert_eq!(result["applied"], json!([]));
        assert_eq!(result["summary"], "A human request for review.");
        assert_eq!(protected(&store), before);
        assert_eq!(
            store.fake_assistance_result(ACCOUNT, "suggestion").unwrap()["stale"],
            false
        );
        assert_eq!(
            store
                .fake_assistance_result("personal", "suggestion")
                .unwrap_err()
                .code,
            "not_found"
        );
    }

    #[test]
    fn permitted_auto_apply_and_summary_commit_once_and_replay_after_restart() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        let mut store = Store::open(&profile).unwrap();
        setup(&mut store, Mode::AutoApply);
        let before = protected(&store);
        let mut boundary = Boundary::default();
        let mut fake = Fake::default();
        let pending = start(&store, "apply-once", &mut boundary, &mut fake);
        let result = store
            .finish_fake_assistance(&pending, &mut boundary, VALID)
            .unwrap();
        assert_eq!(result["applied"].as_array().unwrap().len(), 1);
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM assist_category_intents", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM google_manual_intents", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        assert_eq!(protected(&store), before);
        assert_eq!(
            store.fake_assistance_result(ACCOUNT, "apply-once").unwrap()["stale"],
            false
        );
        drop(store);
        let reopened = Store::open(&profile).unwrap();
        assert_eq!(
            reopened
                .finish_fake_assistance(&pending, &mut Boundary::default(), VALID)
                .unwrap(),
            result
        );
        match reopened
            .start_fake_assistance(
                ACCOUNT,
                THREAD,
                "apply-once",
                "restarted-owner",
                &mut boundary,
                &mut fake,
            )
            .unwrap()
        {
            AssistanceStart::Completed(replay) => assert_eq!(replay, result),
            AssistanceStart::Pending(_) => panic!("replayed work started again"),
        }
        assert_eq!(fake.started.len(), 1);
        assert_eq!(protected(&reopened), before);
        assert_eq!(
            reopened
                .finish_fake_assistance(
                    &pending,
                    &mut boundary,
                    br#"{"categories":["noise"],"abstain":false,"summary":null}"#
                )
                .unwrap_err()
                .code,
            "request_id_conflict"
        );
    }

    #[test]
    fn manual_correction_defeats_late_output_and_future_routine_reclassification() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("profile")).unwrap();
        setup(&mut store, Mode::AutoApply);
        let before = protected(&store);
        let mut boundary = Boundary::default();
        let mut fake = Fake::default();
        let pending = start(&store, "before-correction", &mut boundary, &mut fake);
        let conversation = store.conversation(ACCOUNT, THREAD).unwrap();
        store.handle("set_category",Some(ACCOUNT),json!({"id":THREAD,"message_ids":conversation["message_ids"],"category_id":"finance","present":false,"expected_revision":conversation["revision"],"request_id":"manual-remove-finance"})).unwrap();
        assert_eq!(
            store
                .finish_fake_assistance(&pending, &mut boundary, VALID)
                .unwrap_err()
                .code,
            "assistance_stale"
        );
        let fresh = start(&store, "after-correction", &mut boundary, &mut fake);
        let result = store
            .finish_fake_assistance(&fresh, &mut boundary, VALID)
            .unwrap();
        assert_eq!(result["applied"], json!([]));
        assert_eq!(result["skipped_manual"].as_array().unwrap().len(), 1);
        assert_eq!(store.db.query_row("SELECT COUNT(*) FROM message_categories WHERE account=?1 AND category_id='finance' AND message_id IN (SELECT id FROM messages WHERE account=?1 AND conversation_id=?2)",[ACCOUNT,THREAD],|row|row.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM assist_category_intents", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(protected(&store), before);
    }

    #[test]
    fn policy_provider_content_mapping_or_new_member_changes_reject_stale_results() {
        for change in 0..5 {
            let temporary = tempfile::tempdir().unwrap();
            let mut store = Store::open(&temporary.path().join("profile")).unwrap();
            setup(&mut store, Mode::AutoApply);
            let mut boundary = Boundary::default();
            let mut fake = Fake::default();
            let pending = start(&store, "delayed-result", &mut boundary, &mut fake);
            let message = &pending.snapshot.members[0].id;
            match change {
                0 => {
                    let mut policy = load_policy(&store.db, ACCOUNT).unwrap();
                    let previous = policy.revision;
                    policy.revision += 1;
                    store.configure_fake_assistance(&policy, previous).unwrap();
                }
                1 => {
                    store.db.execute("INSERT INTO google_message_state(account,message_id,remote_thread_id,history_id,provider_labels_json,deleted) VALUES(?1,?2,'remote-thread','2','[]',0)",params![ACCOUNT,message]).unwrap();
                }
                2 => {
                    let hash=store.stage_blob(b"From: sender@example.test\r\nSubject: Changed retained source\r\n\r\nNew provider wording").unwrap();
                    store
                        .db
                        .execute(
                            "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id=?3",
                            params![hash, ACCOUNT, message],
                        )
                        .unwrap();
                }
                3 => {
                    store
                        .google_map_category(ACCOUNT, "finance", "Label_finance_changed")
                        .unwrap();
                }
                _ => {
                    store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) SELECT account,'new-assist-member',conversation_id,'new-assist-remote',sender_name,sender_email,recipient,subject,body,'2026-09-11T20:00:00Z',1,0,1,0,source_hash,1 FROM messages WHERE account=?1 AND id=?2",params![ACCOUNT,message]).unwrap();
                }
            }
            let state = protected(&store);
            assert_eq!(
                store
                    .finish_fake_assistance(&pending, &mut boundary, VALID)
                    .unwrap_err()
                    .code,
                "assistance_stale",
                "change {change}"
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT COUNT(*) FROM assist_results", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                store
                    .db
                    .query_row("SELECT COUNT(*) FROM assist_category_intents", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );
            assert_eq!(protected(&store), state);
        }
    }

    #[test]
    fn forbidden_output_and_failed_result_commit_have_no_partial_category_effect() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("profile")).unwrap();
        setup(&mut store, Mode::AutoApply);
        let before = protected(&store);
        let mut boundary = Boundary::default();
        let mut fake = Fake::default();
        for (index,raw) in [
            br#"{"categories":["finance"],"abstain":false,"summary":null,"read":true}"#.as_slice(),
            br#"{"categories":["SPAM"],"abstain":false,"summary":null}"#,
            br#"{"categories":["finance"],"abstain":false,"summary":null,"draft":"replace authored words"}"#,
        ].into_iter().enumerate() {
            let pending=start(&store,&format!("bad-output-{index}"),&mut boundary,&mut fake);
            assert_eq!(store.finish_fake_assistance(&pending,&mut boundary,raw).unwrap_err().code,"invalid_assistance_result");
        }
        let pending = start(&store, "commit-failure", &mut boundary, &mut fake);
        store.db.execute_batch("CREATE TRIGGER fail_assist_commit BEFORE INSERT ON assist_results BEGIN SELECT RAISE(ABORT,'fixture result commit interruption'); END;").unwrap();
        assert_eq!(
            store
                .finish_fake_assistance(&pending, &mut boundary, VALID)
                .unwrap_err()
                .code,
            "storage_error"
        );
        assert_eq!(protected(&store), before);
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM assist_results", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM assist_category_intents", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(store.db.query_row("SELECT COUNT(*) FROM message_categories WHERE account=?1 AND category_id='finance' AND message_id IN (SELECT id FROM messages WHERE account=?1 AND conversation_id=?2)",[ACCOUNT,THREAD],|row|row.get::<_,i64>(0)).unwrap(),0);
    }

    #[test]
    fn independent_summary_switch_and_source_count_bound_are_enforced() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("profile")).unwrap();
        setup(&mut store, Mode::Suggest);
        let before = protected(&store);
        let mut boundary = Boundary::default();
        let mut fake = Fake::default();
        let mut policy = load_policy(&store.db, ACCOUNT).unwrap();
        policy.revision = 2;
        policy.summaries = false;
        store.configure_fake_assistance(&policy, 1).unwrap();
        let pending = start(&store, "summary-disabled", &mut boundary, &mut fake);
        assert_eq!(
            store
                .finish_fake_assistance(&pending, &mut boundary, VALID)
                .unwrap_err()
                .code,
            "invalid_assistance_result"
        );
        policy.revision = 3;
        policy.mode = Mode::Off;
        policy.summaries = true;
        store.configure_fake_assistance(&policy, 2).unwrap();
        let pending = start(&store, "summary-only", &mut boundary, &mut fake);
        let result=store.finish_fake_assistance(&pending,&mut boundary,br#"{"categories":[],"abstain":false,"summary":"Only an explicitly enabled summary."}"#).unwrap();
        assert_eq!(result["category_mode"], "none");
        assert_eq!(result["applied"], json!([]));
        assert_eq!(result["summary"], "Only an explicitly enabled summary.");
        assert_eq!(protected(&store), before);
        store.db.execute("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<200) INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) SELECT m.account,'bounded-'||n.x,m.conversation_id,'bounded-remote-'||n.x,m.sender_name,m.sender_email,m.recipient,m.subject,m.body,m.sent_at,1,0,1,0,m.source_hash,1 FROM messages m CROSS JOIN n WHERE m.account=?1 AND m.id='windscribe-message-001-000'",[ACCOUNT]).unwrap();
        let calls = fake.started.len();
        assert_eq!(
            store
                .start_fake_assistance(
                    ACCOUNT,
                    THREAD,
                    "over-member-bound",
                    "fixture-owner",
                    &mut boundary,
                    &mut fake
                )
                .err()
                .unwrap()
                .code,
            "assistance_limit"
        );
        assert_eq!(fake.started.len(), calls);
    }
}
