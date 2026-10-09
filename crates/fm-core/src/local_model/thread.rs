//! A whole conversation, captured for an on-demand summary, and its one saved
//! result. Unlike email analysis it includes the person's own sent messages,
//! never labels anything and never counts as queue progress.
use super::{same_snapshot, same_stored, stale};
use crate::{err, Result, Store};
use fm_assist::ollama::{self, Settings};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The `task` and `schema` stored with a conversation summary.
pub(crate) const TASK: &str = "thread";
const SCHEMA: i64 = 1;

/// One conversation, captured for the model.
///
/// Built by [`Store::local_model_prepare_thread`] with a snapshot of the
/// conversation's members; [`Store::local_model_finish_thread`] refuses to
/// save if a message arrived, left or changed while the model ran.
#[derive(Clone)]
pub struct PreparedThread {
    pub account: String,
    pub conversation: String,
    /// Model settings as read at preparation, used for the whole run.
    pub settings: Settings,
    /// `{subject, messages: [{from, to, date, text, shortened}], omitted}`,
    /// oldest message first: what the thread prompt's `{{messages}}` becomes.
    pub thread: Value,
    /// Every message sent to the model, oldest first.
    pub source_ids: Vec<String>,
    snapshot: Value,
    /// Older messages were left out or some text was shortened.
    pub partial: bool,
}

/// One message of a conversation, as the summary reads it.
struct Member {
    id: String,
    source_hash: String,
    sent: bool,
    name: String,
    email: String,
    to: String,
    date: String,
    body: String,
}

impl Store {
    /// The conversation's subject and every message, in the reader's order.
    fn thread_members(&self, account: &str, conversation: &str) -> Result<(String, Vec<Member>)> {
        let subject: String = self
            .db
            .query_row(
                "SELECT subject FROM conversations WHERE account=?1 AND id=?2",
                params![account, conversation],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| err("not_found", "Conversation not found in this account"))?;
        let mut stmt = self.db.prepare("SELECT id,source_hash,sent,sender_name,sender_email,recipient,sent_at,body FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY julianday(sent_at),id")?;
        let members = stmt
            .query_map(params![account, conversation], |r| {
                Ok(Member {
                    id: r.get(0)?,
                    source_hash: r.get(1)?,
                    sent: r.get(2)?,
                    name: r.get(3)?,
                    email: r.get(4)?,
                    to: r.get(5)?,
                    date: r.get(6)?,
                    body: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok((subject, members))
    }
    /// What a conversation summary depends on: which messages the
    /// conversation holds, their sources and stored text.
    pub(super) fn local_model_thread_snapshot(
        &self,
        account: &str,
        conversation: &str,
    ) -> Result<Value> {
        let lifecycle = self.require_active_account(account)?;
        let (subject, members) = self.thread_members(account, conversation)?;
        Ok(snapshot(lifecycle, &subject, &members))
    }
    /// Capture a whole conversation for a summary: every message, the
    /// person's own included as "You", oldest first.
    ///
    /// At most the newest [`ollama::THREAD_MESSAGES`] messages are sent, and
    /// their text shares [`ollama::BODY_CHARS`]: older text is shortened
    /// first (see [`ollama::thread_budget`]). Either makes the summary
    /// partial. Needs Summarize on for the account (`assistance_disabled`), a
    /// chosen model (`assistance_unavailable`) and at least two messages
    /// (`invalid_request`).
    pub fn local_model_prepare_thread(
        &self,
        account: &str,
        conversation: &str,
    ) -> Result<PreparedThread> {
        let settings = self.local_model_policy()?;
        let policy = settings.accounts.get(account).cloned().unwrap_or_default();
        if !policy.summaries {
            return Err(err(
                "assistance_disabled",
                "Turn on Summarize for this account",
            ));
        }
        if settings.model.is_none() {
            return Err(err(
                "assistance_unavailable",
                "Choose a model in Settings → Local LLM",
            ));
        }
        let lifecycle = self.require_active_account(account)?;
        let (subject, members) = self.thread_members(account, conversation)?;
        if members.len() < 2 {
            return Err(err(
                "invalid_request",
                "A thread summary needs at least two messages",
            ));
        }
        let snapshot = snapshot(lifecycle, &subject, &members);
        let omitted = members.len().saturating_sub(ollama::THREAD_MESSAGES);
        let sent = &members[omitted..];
        let texts: Vec<&str> = sent.iter().map(|m| m.body.trim()).collect();
        let lengths: Vec<usize> = texts.iter().map(|t| t.chars().count()).collect();
        let kept = ollama::thread_budget(&lengths);
        let mut shortened_any = false;
        let messages: Vec<Value> = sent
            .iter()
            .zip(texts.iter().zip(lengths.iter().zip(&kept)))
            .map(|(m, (text, (&length, &keep)))| {
                let shortened = keep < length;
                shortened_any |= shortened;
                let from = if m.sent {
                    format!("You <{}>", m.email)
                } else if m.name.is_empty() || m.name == m.email {
                    m.email.clone()
                } else {
                    format!("{} <{}>", m.name, m.email)
                };
                let text: String = text.chars().take(keep).collect();
                json!({"from":from,"to":m.to,"date":m.date,"text":text,"shortened":shortened})
            })
            .collect();
        Ok(PreparedThread {
            account: account.into(),
            conversation: conversation.into(),
            settings,
            thread: json!({"subject":subject,"messages":messages,"omitted":omitted}),
            source_ids: sent.iter().map(|m| m.id.clone()).collect(),
            snapshot,
            partial: omitted > 0 || shortened_any,
        })
    }
    /// Fails with `assistance_stale` if the conversation's messages changed
    /// since `p` was prepared. The engine calls it right before inference;
    /// [`Store::local_model_finish_thread`] repeats it on commit.
    pub fn local_model_validate_thread(&self, p: &PreparedThread) -> Result<()> {
        if !same_snapshot(
            &p.snapshot,
            &self.local_model_thread_snapshot(&p.account, &p.conversation)?,
        ) {
            return Err(stale());
        }
        Ok(())
    }
    /// Save a checked conversation summary `{summary}`, replacing any earlier
    /// one, as the conversation's one `thread` result.
    ///
    /// Fails with `invalid_assistance_result` for a summary outside the
    /// bounds and `assistance_stale` if the conversation changed. Labels,
    /// read state, sender kinds, email summaries and queue failures are
    /// untouched.
    pub fn local_model_finish_thread(&self, p: &PreparedThread, output: Value) -> Result<Value> {
        let summary = output["summary"].as_str().unwrap_or_default();
        if !ollama::valid_thread_summary(summary) {
            return Err(err("invalid_assistance_result", "Invalid model summary"));
        }
        // Recheck staleness inside the same transaction as the write.
        let tx = self.db.unchecked_transaction()?;
        self.local_model_validate_thread(p)?;
        let prompt = format!(
            "{:x}",
            Sha256::digest(
                format!("{}\0{}", ollama::THREAD_SYSTEM, ollama::THREAD_USER).as_bytes()
            )
        );
        let result = json!({"task":TASK,"target":p.conversation,"conversation_id":p.conversation,
            "output":{"summary":summary,"source_ids":p.source_ids},"source_ids":p.source_ids,
            "partial":p.partial,"provider":p.settings.provider,"model":p.settings.model,
            "prompt_sha256":prompt,"schema":SCHEMA,"created_at":chrono::Utc::now().to_rfc3339(),"stale":false});
        tx.execute("INSERT INTO local_model_results VALUES(?1,?2,?3,?2,?4,?5) ON CONFLICT(account,conversation_id,task,target) DO UPDATE SET snapshot_json=excluded.snapshot_json,result_json=excluded.result_json",params![p.account,p.conversation,TASK,serde_json::to_string(&p.snapshot)?,serde_json::to_string(&result)?])?;
        tx.execute(
            "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
            params![p.account, p.conversation],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(json!({"task":TASK,"target":p.conversation,"items":[result],"prompt_sha256":prompt}))
    }
    /// Marks a saved conversation summary `stale` when the conversation's
    /// messages changed, and `format_outdated` when its text is outside the
    /// current bounds.
    pub(super) fn local_model_thread_result(
        &self,
        account: &str,
        conversation: &str,
        snapshot: &str,
        result: &mut Value,
    ) {
        result["stale"] = (!self
            .local_model_thread_snapshot(account, conversation)
            .is_ok_and(|current| same_stored(snapshot, &current)))
        .into();
        result["format_outdated"] = (!ollama::valid_thread_summary(
            result["output"]["summary"].as_str().unwrap_or_default(),
        ))
        .into();
    }
}

/// The snapshot of a conversation's members: IDs, sources, stored text and dates.
fn snapshot(lifecycle: i64, subject: &str, members: &[Member]) -> Value {
    let members: Vec<Value> = members
        .iter()
        .map(|m| {
            json!({"id":m.id,"source_hash":m.source_hash,"sent":m.sent,"date":m.date,
                "text_hash":format!("{:x}",Sha256::digest(m.body.as_bytes()))})
        })
        .collect();
    json!({"lifecycle":lifecycle,"subject":subject,"members":members})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_model::tests::configured_store;
    const A: &str = "windscribe";
    /// Three messages; the second is the person's own reply.
    const C: &str = "windscribe-thread-000";
    fn thread_item(s: &Store) -> Value {
        s.local_model_results(A, C).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["task"] == TASK)
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn save(s: &mut Store, change: impl FnOnce(&mut fm_assist::ollama::Account)) {
        let state = s.local_model_settings().unwrap();
        let mut settings: Settings = serde_json::from_value(state["settings"].clone()).unwrap();
        change(settings.accounts.entry(A.into()).or_default());
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":state["revision"],"settings":settings}),
        )
        .unwrap();
    }
    fn add_message(s: &Store, id: &str, sent_at: &str, body: &str, sent: bool) {
        s.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) SELECT account,?2,conversation_id,?2,sender_name,sender_email,recipient,subject,?4,?3,0,0,1,?5,source_hash FROM messages WHERE account=?1 AND id='windscribe-message-000-000'",params![A,id,sent_at,body,sent]).unwrap();
    }
    #[test]
    fn a_thread_is_sent_whole_in_date_order_with_your_own_replies() {
        let (_dir, s) = configured_store();
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        assert!(!plan.partial);
        assert_eq!(
            plan.source_ids,
            [
                "windscribe-message-000-000",
                "windscribe-message-000-001",
                "windscribe-message-000-002"
            ]
        );
        let messages = plan.thread["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["from"], "You <you@windscribe.example>");
        assert!(messages[0]["from"]
            .as_str()
            .unwrap()
            .starts_with("Maya Lindqvist <"));
        assert!(messages
            .windows(2)
            .all(|w| w[0]["date"].as_str() <= w[1]["date"].as_str()));
        assert_eq!(plan.thread["omitted"], 0);
        assert_eq!(plan.thread["subject"], "Q3 infra review: v4 attached");
        // A message dated before the others goes first, whatever its ID.
        add_message(&s, "zz-earliest", "2026-09-01T08:00:00Z", "Kickoff.", false);
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        assert_eq!(plan.source_ids[0], "zz-earliest");
    }
    #[test]
    fn long_threads_shorten_older_text_first_and_leave_out_the_oldest() {
        let (_dir, s) = configured_store();
        let long = "Older detail. ".repeat(1_000);
        s.db.execute(
            "UPDATE messages SET body=?2 WHERE account=?1 AND conversation_id=?3",
            params![A, long, C],
        )
        .unwrap();
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        assert!(plan.partial);
        let messages = plan.thread["messages"].as_array().unwrap();
        let kept: Vec<usize> = messages
            .iter()
            .map(|m| m["text"].as_str().unwrap().chars().count())
            .collect();
        let whole = long.trim().chars().count();
        assert_eq!(kept[2], whole, "the newest message keeps its text");
        assert!(kept[0] < kept[1] && kept[1] <= whole);
        assert!(kept[0] >= ollama::THREAD_FLOOR_CHARS);
        assert!(kept.iter().sum::<usize>() <= ollama::BODY_CHARS);
        assert_eq!(messages[0]["shortened"], true);
        assert_eq!(messages[2]["shortened"], false);
        // More messages than are ever sent: the oldest are left out.
        let (_dir, s) = configured_store();
        for n in 0..ollama::THREAD_MESSAGES {
            add_message(
                &s,
                &format!("extra-{n:02}"),
                &format!("2026-09-12T10:{n:02}:00Z"),
                "Short.",
                false,
            );
        }
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        assert!(plan.partial);
        assert_eq!(plan.thread["omitted"], 3);
        assert_eq!(plan.source_ids.len(), ollama::THREAD_MESSAGES);
        assert_eq!(plan.source_ids[0], "extra-00");
    }
    #[test]
    fn finish_saves_one_thread_result_and_a_new_message_makes_it_stale() {
        let (_dir, s) = configured_store();
        let before: i64 =
            s.db.query_row(
                "SELECT revision FROM conversations WHERE account=?1 AND id=?2",
                params![A, C],
                |r| r.get(0),
            )
            .unwrap();
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        let saved = s
            .local_model_finish_thread(&plan, json!({"summary":"Maya needs your call."}))
            .unwrap();
        assert_eq!(saved["task"], "thread");
        assert_eq!(saved["target"], C);
        let item = thread_item(&s);
        assert_eq!(item["target"], C);
        assert_eq!(item["conversation_id"], C);
        assert_eq!(item["output"]["summary"], "Maya needs your call.");
        assert_eq!(item["output"]["source_ids"], json!(plan.source_ids));
        assert_eq!(item["source_ids"], json!(plan.source_ids));
        assert_eq!(item["schema"], SCHEMA);
        assert_eq!(item["partial"], false);
        assert_eq!(item["stale"], false);
        assert_eq!(item["format_outdated"], false);
        assert_eq!(item["provider"], "ollama");
        assert_eq!(item["model"]["name"], "fixture");
        let after: i64 =
            s.db.query_row(
                "SELECT revision FROM conversations WHERE account=?1 AND id=?2",
                params![A, C],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, before + 1);
        // Running again replaces the one row.
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        s.local_model_finish_thread(&plan, json!({"summary":"Second take."}))
            .unwrap();
        let rows: i64 =
            s.db.query_row(
                "SELECT count(*) FROM local_model_results WHERE account=?1 AND task='thread'",
                [A],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(thread_item(&s)["output"]["summary"], "Second take.");
        // No failures, sender kinds or email summaries come from a thread run.
        let side: i64 = s.db.query_row("SELECT (SELECT count(*) FROM local_model_failures)+(SELECT count(*) FROM message_sender_kinds WHERE provenance IN ('ollama','openai_compatible'))+(SELECT count(*) FROM local_model_results WHERE task<>'thread')",[],|r|r.get(0)).unwrap();
        assert_eq!(side, 0);
        // A reply arriving makes the saved summary stale, not missing.
        add_message(&s, "new-reply", "2026-09-12T08:00:00Z", "One more.", false);
        let item = thread_item(&s);
        assert_eq!(item["stale"], true);
        assert_eq!(item["output"]["summary"], "Second take.");
    }
    #[test]
    fn a_change_between_prepare_and_finish_is_refused() {
        let (_dir, s) = configured_store();
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        s.db.execute(
            "UPDATE messages SET body='Edited.' WHERE account=?1 AND id='windscribe-message-000-001'",
            [A],
        )
        .unwrap();
        assert_eq!(
            s.local_model_finish_thread(&plan, json!({"summary":"Too late."}))
                .unwrap_err()
                .code,
            "assistance_stale"
        );
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        add_message(&s, "arrived", "2026-09-12T08:00:00Z", "New.", false);
        assert_eq!(
            s.local_model_validate_thread(&plan).unwrap_err().code,
            "assistance_stale"
        );
        assert!(thread_item(&s).is_null());
        // A bad summary is refused before anything is saved.
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        for output in [
            json!({"summary":""}),
            json!({"summary":"- list"}),
            json!({}),
        ] {
            assert_eq!(
                s.local_model_finish_thread(&plan, output).unwrap_err().code,
                "invalid_assistance_result"
            );
        }
    }
    #[test]
    fn thread_summaries_need_summaries_on_a_model_and_two_messages() {
        let (_dir, mut s) = configured_store();
        assert_eq!(
            s.local_model_prepare_thread(A, "windscribe-thread-001")
                .err()
                .unwrap()
                .message,
            "A thread summary needs at least two messages"
        );
        assert_eq!(
            s.local_model_prepare_thread(A, "missing")
                .err()
                .unwrap()
                .code,
            "not_found"
        );
        // Labelling alone is not enough.
        save(&mut s, |policy| policy.summaries = false);
        assert_eq!(
            s.local_model_prepare_thread(A, C).err().unwrap().code,
            "assistance_disabled"
        );
        save(&mut s, |policy| policy.summaries = true);
        let state = s.local_model_settings().unwrap();
        let mut settings: Settings = serde_json::from_value(state["settings"].clone()).unwrap();
        settings.model = None;
        s.local_model_handle(
            "assist_settings_save",
            None,
            &json!({"expected_revision":state["revision"],"settings":settings}),
        )
        .unwrap();
        assert_eq!(
            s.local_model_prepare_thread(A, C).err().unwrap().code,
            "assistance_unavailable"
        );
    }
    #[test]
    fn thread_results_leave_queue_progress_and_sent_message_results_alone() {
        let (_dir, s) = configured_store();
        let counts = |s: &Store| {
            let row = s.local_model_progress_within(Some(A), None).unwrap()["accounts"][0].clone();
            assert!(row["done"].is_i64());
            (
                row["done"].clone(),
                row["queued"].clone(),
                row["failed"].clone(),
            )
        };
        let before = counts(&s);
        let plan = s.local_model_prepare_thread(A, C).unwrap();
        s.local_model_finish_thread(&plan, json!({"summary":"Maya needs your call."}))
            .unwrap();
        assert_eq!(counts(&s), before);
        // A result saved for a message that is now marked sent stays hidden.
        let m = "windscribe-message-000-000";
        let p = s.local_model_prepare(A, C, m).unwrap();
        s.local_model_finish(
            &p,
            json!({"summary":"Maya needs your call.","sender_kind":"human","category_ids":[]}),
        )
        .unwrap();
        s.db.execute(
            "UPDATE messages SET sent=1 WHERE account=?1 AND id=?2",
            params![A, m],
        )
        .unwrap();
        let items = s.local_model_results(A, C).unwrap()["items"].clone();
        let tasks: Vec<&str> = items
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["task"].as_str().unwrap())
            .collect();
        assert_eq!(tasks, ["thread"]);
    }
}
