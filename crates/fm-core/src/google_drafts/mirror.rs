//! Drafts are Gmail drafts (D121). A draft seen in Gmail becomes an ordinary
//! editable draft here; a change made in Gmail replaces an unchanged local
//! copy; a draft gone from Gmail leaves; a draft discarded here is deleted in
//! Gmail once its Undo has passed. Local edits always win: they upload over
//! whatever Gmail holds.
use super::*;
use crate::google_draft_actions::{Adoption, ObservedTarget};

/// Discarded drafts wait this long before their Gmail copy is deleted, so the
/// six-second Undo can still bring them back unchanged.
pub const DISCARD_DELETE_DELAY_MS: i64 = 10_000;

/// A Gmail draft as observed by the inventory.
pub(crate) struct RemoteDraft<'a> {
    pub provider_draft_id: &'a str,
    pub message_id: &'a str,
    pub thread_id: &'a str,
    pub sha256: &'a str,
    pub size: u64,
    pub source_draft_id: &'a str,
    pub source_revision: i64,
    pub routing: &'a Value,
    pub body: &'a str,
    pub html: Option<&'a str>,
    pub from_name: &'a str,
    pub attachment_ids: &'a [String],
}

impl RemoteDraft<'_> {
    fn adoption(&self, draft_revision: i64) -> Adoption {
        Adoption {
            source_draft_id: self.source_draft_id.into(),
            source_revision: self.source_revision,
            draft_revision,
            target: ObservedTarget {
                provider_draft_id: self.provider_draft_id.into(),
                message_id: self.message_id.into(),
                thread_id: self.thread_id.into(),
                sha256: self.sha256.into(),
                size: self.size,
            },
        }
    }
}

/// True when nothing local is waiting to upload.
fn clean(revision: i64, twin: &Twin) -> bool {
    match twin.state.as_str() {
        "synced" => twin
            .baseline
            .as_ref()
            .is_some_and(|b| b.draft_revision == revision),
        "adopted" => twin
            .adoption
            .as_ref()
            .is_some_and(|a| a.draft_revision == revision),
        _ => false,
    }
}

fn settled(twin: &Twin) -> bool {
    twin.submission.is_none() && twin.deletion.is_none() && !twin.remote_deleted
}

impl Store {
    /// Mirrors one observed Gmail draft into the local store. `local` is the
    /// local draft already tied to it, if any. Returns the draft it updated or
    /// created, or None when a pending local change wins.
    pub(crate) fn mirror_remote_draft(
        &self,
        account: &str,
        remote: &RemoteDraft<'_>,
        local: Option<&str>,
        new_id: &str,
    ) -> Result<Option<String>> {
        let now = unix_ms()? / 1000;
        if let Some(local) = local {
            let row: Option<(i64, Option<String>, bool)> = self
                .db
                .query_row(
                    "SELECT revision,google_json,discarded FROM drafts WHERE account=?1 AND draft_id=?2 AND origin='local'",
                    params![account, local],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((revision, raw, discarded)) = row else {
                return Ok(None);
            };
            let twin: Twin = raw.as_deref().map(decode).transpose()?.unwrap_or_default();
            // Our own upload, a draft being sent or deleted, or local edits.
            if discarded
                || !settled(&twin)
                || !clean(revision, &twin)
                || effective_observation(&twin).is_some_and(|o| o.sha256 == remote.sha256)
            {
                return Ok(None);
            }
            let next = revision + 1;
            let twin = Twin {
                version: twin.version,
                revision: twin.revision + 1,
                state: "adopted".into(),
                adoption: Some(remote.adoption(next)),
                ..Twin::default()
            };
            self.db.execute(
                "UPDATE drafts SET routing_json=?3,body=?4,body_html=?5,from_name=?6,attachments_json=?7,revision=?8,google_json=?9,updated_at=?10 WHERE account=?1 AND draft_id=?2",
                params![account, local, serde_json::to_string(remote.routing)?, remote.body, remote.html,
                    remote.from_name, serde_json::to_string(remote.attachment_ids)?, next, encode(&twin)?, now],
            )?;
            return Ok(Some(local.to_owned()));
        }
        // A new Gmail draft. A reply made in a thread stored here opens with it.
        let (name, _, identity) = self.draft_identity(account)?;
        let from_name = if remote.from_name.is_empty() {
            name
        } else {
            remote.from_name.to_owned()
        };
        let replies = remote.routing["reply_to_message_id"]
            .as_str()
            .is_some_and(|s| !s.is_empty());
        let source: Option<(String, String, String)> = if replies && !remote.thread_id.is_empty() {
            self.db.query_row(
                "SELECT m.conversation_id,m.id,m.source_hash FROM google_message_state g JOIN messages m ON m.account=g.account AND m.id=g.message_id WHERE g.account=?1 AND g.remote_thread_id=?2 AND g.deleted=0 ORDER BY m.sent=0 DESC,julianday(m.sent_at) DESC,m.id DESC LIMIT 1",
                params![account, remote.thread_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).optional()?
        } else {
            None
        };
        // An earlier version's read-only copy of this Gmail draft becomes the
        // mirror in place: its source history refers to the row.
        let legacy: Option<i64> = self
            .db
            .query_row(
                "SELECT revision FROM drafts WHERE account=?1 AND draft_id=?2 AND origin='google_snapshot'",
                params![account, new_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(revision) = legacy {
            let next = revision + 1;
            let twin = Twin {
                state: "adopted".into(),
                revision: 1,
                adoption: Some(remote.adoption(next)),
                ..Twin::default()
            };
            self.db.execute(
                "UPDATE drafts SET conversation_id=?3,legacy_reply=0,mode=?4,source_message_id=?5,source_hash=?6,routing_json=?7,body=?8,body_html=?9,revision=?10,attachments_json=?11,discarded=0,signature_text='',from_name=?12,identity_revision=?13,google_json=?14,origin='local',recipient_inputs_json='{}',updated_at=?15 WHERE account=?1 AND draft_id=?2 AND origin='google_snapshot'",
                params![account, new_id, source.as_ref().map(|s| &s.0), if source.is_some() { "reply" } else { "new" },
                    source.as_ref().map(|s| &s.1), source.as_ref().map(|s| &s.2), serde_json::to_string(remote.routing)?,
                    remote.body, remote.html, next, serde_json::to_string(remote.attachment_ids)?, from_name, identity, encode(&twin)?, now],
            )?;
            return Ok(Some(new_id.to_owned()));
        }
        let twin = Twin {
            state: "adopted".into(),
            revision: 1,
            adoption: Some(remote.adoption(1)),
            ..Twin::default()
        };
        self.db.execute(
            "INSERT INTO drafts(account,draft_id,conversation_id,mode,source_message_id,source_hash,routing_json,body,body_html,revision,attachments_json,signature_text,from_name,identity_revision,google_json,origin,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10,'',?11,?12,?13,'local',?14,?14)",
            params![account, new_id, source.as_ref().map(|s| &s.0), if source.is_some() { "reply" } else { "new" },
                source.as_ref().map(|s| &s.1), source.as_ref().map(|s| &s.2), serde_json::to_string(remote.routing)?,
                remote.body, remote.html, serde_json::to_string(remote.attachment_ids)?, from_name, identity, encode(&twin)?, now],
        )?;
        Ok(Some(new_id.to_owned()))
    }

    /// After a complete Gmail draft listing: drafts tied to a Gmail draft that
    /// was not listed are gone there (deleted or sent). An unchanged local copy
    /// leaves too; one with local edits loses its target and uploads anew.
    pub(crate) fn reconcile_absent_drafts(
        &self,
        account: &str,
        generation: i64,
        started_ms: i64,
    ) -> Result<usize> {
        let rows: Vec<(String, i64, Option<String>, i64, String)> = {
            let mut stmt = self.db.prepare(
                "SELECT d.draft_id,d.revision,d.google_json,d.updated_at,s.provider_draft_id FROM drafts d JOIN google_draft_sources s ON s.account=d.account AND s.is_current=1 AND (s.draft_id=d.draft_id OR s.linked_local_draft_id=d.draft_id) WHERE d.account=?1 AND d.discarded=0 AND d.origin='local' AND s.seen_generation<?2",
            )?;
            let rows = stmt
                .query_map(params![account, generation], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let mut changed = 0;
        // Read-only copies from an earlier version whose Gmail draft is gone.
        let legacy: Vec<String> = {
            let mut stmt = self.db.prepare(
                "SELECT d.draft_id FROM drafts d WHERE d.account=?1 AND d.origin='google_snapshot' AND d.discarded=0 AND NOT EXISTS(SELECT 1 FROM google_draft_sources s WHERE s.account=d.account AND s.draft_id=d.draft_id AND s.is_current=1 AND s.seen_generation>=?2)",
            )?;
            let rows = stmt
                .query_map(params![account, generation], |r| r.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for draft in legacy {
            if self.retire_legacy_snapshot(account, &draft)? {
                changed += 1;
            }
        }
        for (draft, revision, raw, updated_at, provider) in rows {
            if updated_at * 1000 < started_ms
                && self.retire_remote_draft(account, &draft, revision, raw, &provider)?
            {
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Gmail answered that this listed draft no longer exists.
    pub(crate) fn mirror_missing_draft(
        &self,
        account: &str,
        local: &str,
        provider: &str,
    ) -> Result<bool> {
        let row: Option<(i64, Option<String>)> = self
            .db
            .query_row(
                "SELECT revision,google_json FROM drafts WHERE account=?1 AND draft_id=?2 AND origin='local' AND discarded=0",
                params![account, local],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((revision, raw)) = row else {
            return Ok(false);
        };
        self.retire_remote_draft(account, local, revision, raw, provider)
    }

    /// A draft whose Gmail copy is gone: an unchanged one leaves, one with
    /// local edits loses its target and uploads anew.
    fn retire_remote_draft(
        &self,
        account: &str,
        draft: &str,
        revision: i64,
        raw: Option<String>,
        provider: &str,
    ) -> Result<bool> {
        {
            let twin: Twin = raw.as_deref().map(decode).transpose()?.unwrap_or_default();
            if !settled(&twin)
                || effective_observation(&twin).is_none_or(|o| o.draft_id != provider)
            {
                return Ok(false);
            }
            if clean(revision, &twin) {
                let twin = Twin {
                    remote_deleted: true,
                    ..twin
                };
                self.db.execute(
                    "UPDATE drafts SET discarded=1,google_json=?3 WHERE account=?1 AND draft_id=?2",
                    params![account, draft, encode(&twin)?],
                )?;
            } else if matches!(twin.state.as_str(), "synced" | "adopted" | "local") {
                let twin = Twin {
                    version: twin.version,
                    revision: twin.revision + 1,
                    ..Twin::default()
                };
                self.db.execute(
                    "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
                    params![account, draft, encode(&twin)?],
                )?;
            } else {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The next discarded draft whose Gmail copy should now be deleted:
    /// (draft id, Gmail draft id).
    pub fn google_draft_discard_due(&self, account: &str) -> Result<Option<(String, String)>> {
        let cutoff = unix_ms()? - DISCARD_DELETE_DELAY_MS;
        let mut stmt = self.db.prepare(
            "SELECT draft_id,google_json FROM drafts WHERE account=?1 AND discarded=1 AND origin='local' AND google_json IS NOT NULL AND updated_at*1000<=?2 AND COALESCE(json_extract(google_json,'$.remote_deleted'),0)=0 AND json_extract(google_json,'$.submission') IS NULL AND json_extract(google_json,'$.deletion') IS NULL AND json_extract(google_json,'$.state') IN ('synced','adopted','conflict','failed','local') ORDER BY updated_at,draft_id LIMIT 8",
        )?;
        let rows = stmt
            .query_map(params![account, cutoff], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (draft, raw) in rows {
            let twin: Twin = decode(&raw)?;
            if let Some(target) = effective_observation(&twin) {
                return Ok(Some((draft, target.draft_id)));
            }
            // Never reached Gmail: nothing to delete there.
            self.mark_remote_deleted(account, &draft, twin)?;
        }
        Ok(None)
    }

    /// Gmail no longer has this discarded draft (deleted now, or already gone).
    pub fn google_draft_remote_deleted(
        &self,
        account: &str,
        draft: &str,
        provider: &str,
    ) -> Result<()> {
        let (_, twin) = self.google_twin(account, draft)?;
        if effective_observation(&twin).is_none_or(|o| o.draft_id != provider) {
            return Ok(());
        }
        self.mark_remote_deleted(account, draft, twin)
    }

    fn mark_remote_deleted(&self, account: &str, draft: &str, twin: Twin) -> Result<()> {
        let twin = Twin {
            remote_deleted: true,
            ..twin
        };
        self.db.execute(
            "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2 AND discarded=1",
            params![account, draft, encode(&twin)?],
        )?;
        self.db
            .execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        Ok(())
    }

    /// A discarded draft brought back after its Gmail copy was deleted starts
    /// over and uploads as a new Gmail draft.
    pub(crate) fn revive_deleted_draft(&self, account: &str, draft: &str) -> Result<()> {
        let (_, twin) = self.google_twin(account, draft)?;
        if !twin.remote_deleted {
            return Ok(());
        }
        let twin = Twin {
            version: twin.version,
            revision: twin.revision + 1,
            ..Twin::default()
        };
        self.db.execute(
            "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
            params![account, draft, encode(&twin)?],
        )?;
        Ok(())
    }

    /// The next draft in conflict with its Gmail copy: {draft_id, revision,
    /// google_revision, observed}. Local edits win; a draft missing from Gmail
    /// uploads anew (see `google_draft_forget_target`). An unknown create is
    /// left alone: retrying it could make a second Gmail draft.
    pub fn google_draft_conflicted(&self, account: &str) -> Result<Option<Value>> {
        let row: Option<(String, i64, String)> = self
            .db
            .query_row(
                "SELECT draft_id,revision,google_json FROM drafts WHERE account=?1 AND discarded=0 AND origin='local' AND json_extract(google_json,'$.state')='conflict' AND (json_extract(google_json,'$.conflict.observed') IS NOT NULL OR json_extract(google_json,'$.conflict.reason')='missing') AND json_extract(google_json,'$.submission') IS NULL AND json_extract(google_json,'$.deletion') IS NULL ORDER BY updated_at,draft_id LIMIT 1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((draft, revision, raw)) = row else {
            return Ok(None);
        };
        let twin: Twin = decode(&raw)?;
        let observed = twin
            .conflict
            .as_ref()
            .and_then(|c| c.observed.is_some().then_some(true));
        Ok(Some(
            json!({"draft_id":draft,"revision":revision,"google_revision":twin.revision,
            "reason":twin.conflict.as_ref().map(|c|c.reason.clone()),"observed":observed.unwrap_or(false)}),
        ))
    }

    /// Drops a conflicted draft's Gmail target so it uploads as a new draft.
    pub fn google_draft_forget_target(
        &self,
        account: &str,
        draft: &str,
        google_revision: i64,
    ) -> Result<()> {
        let (_, twin) = self.google_twin(account, draft)?;
        if twin.state != "conflict" || twin.revision != google_revision {
            return Err(err("stale_google_revision", "Google draft state changed"));
        }
        let twin = Twin {
            version: twin.version,
            revision: twin.revision + 1,
            ..Twin::default()
        };
        self.db.execute(
            "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
            params![account, draft, encode(&twin)?],
        )?;
        self.db
            .execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        Ok(())
    }
}
