//! Explicit source conversion, observed adoption and permanent draft deletion.
//! The existing draft store, source history and journals retain their identities.
use super::*;
use crate::{
    composition,
    google_drafts::{self, GoogleDraftScope},
    local_content,
};

mod authoring;
mod deletion;
pub use authoring::prepare_google_draft_authoring;
pub use deletion::*;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod tests;

pub struct GoogleDraftActionRemote {
    pub draft_id: String,
    pub message_id: String,
    pub thread_id: String,
    pub raw: Vec<u8>,
}
pub enum GoogleDraftAuthoringStep {
    Reply(Value),
    Prepare(GoogleDraftAuthoringPlan),
}
pub struct GoogleDraftAuthoringPlan {
    context: Box<ActionContext>,
    method: String,
    params: Value,
    scope: Option<GoogleDraftScope>,
}
impl GoogleDraftAuthoringPlan {
    pub fn provider_draft_id(&self) -> &str {
        &self.context.source.provider_draft_id
    }
}
pub struct PreparedGoogleDraftAuthoring {
    plan: GoogleDraftAuthoringPlan,
    assessment: Value,
    authored: Option<Authored>,
    cancel: CancellationToken,
    changed_source: Option<PreparedGoogleDraftImport>,
}
struct Authored {
    routing: Value,
    body: String,
    from_name: String,
    parts: Vec<Part>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ObservedTarget {
    pub provider_draft_id: String,
    pub message_id: String,
    pub thread_id: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Adoption {
    pub source_draft_id: String,
    pub source_revision: i64,
    pub draft_revision: i64,
    pub target: ObservedTarget,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Derivation {
    pub source_draft_id: String,
    pub source_revision: i64,
    pub source_sha256: String,
    pub policies: Value,
}
#[derive(Clone)]
struct SourceStamp {
    provider_draft_id: String,
    message_id: String,
    thread_id: String,
    sha256: String,
    size: u64,
    revision: i64,
    original_draft_id: String,
    linked_local_draft_id: Option<String>,
    is_current: bool,
}
impl SourceStamp {
    fn target(&self) -> ObservedTarget {
        ObservedTarget {
            provider_draft_id: self.provider_draft_id.clone(),
            message_id: self.message_id.clone(),
            thread_id: self.thread_id.clone(),
            sha256: self.sha256.clone(),
            size: self.size,
        }
    }
}
struct ActionContext {
    lifecycle_revision: i64,
    blobs: BlobStore,
    account: String,
    selected: String,
    source: SourceStamp,
    owner_stamp: Value,
    identity: Value,
}
fn request_id(p: &Value) -> Result<&str> {
    let id = crate::string(p, "request_id")?;
    identifier(id)?;
    if id.len() > 128 {
        return Err(invalid());
    }
    Ok(id)
}
fn source_params(p: &Value, extra: &[&str]) -> Result<()> {
    let mut fields = vec!["draft_id", "source_revision"];
    fields.extend_from_slice(extra);
    crate::fields(p, &fields)?;
    identifier(crate::string(p, "draft_id")?)?;
    page_number(p, "source_revision", 0, 1, i64::MAX)?;
    if extra.contains(&"request_id") {
        request_id(p)?;
    }
    Ok(())
}

impl Store {
    pub(crate) fn retain_action_source(&mut self, proof: PreparedGoogleDraftImport) -> Result<()> {
        cancelled(&proof.cancel)?;
        if proof.plan.blobs.profile != self.profile {
            return Err(stale());
        }
        self.inventory_scope(&proof.plan.scope)?;
        let account = &proof.plan.scope.account;
        let remote = &proof.plan.item.draft_id;
        let mut state = self.inventory(account)?.ok_or_else(stale)?;
        let seen:Option<i64>=self.db.query_row("SELECT seen_generation FROM google_draft_sources WHERE account=?1 AND provider_draft_id=?2 AND is_current=1",params![account,remote],|r|r.get(0)).optional()?;
        let scan_count = state.observed_this_scan;
        let tx = self.db.unchecked_transaction()?;
        self.publish_draft_source(
            account,
            remote,
            proof.source,
            proof.preview,
            proof.parts,
            &mut state,
        )?;
        // An explicit GET is a fresh observation, not a claim that a list scan saw it.
        self.db.execute("UPDATE google_draft_sources SET seen_generation=?3 WHERE account=?1 AND provider_draft_id=?2 AND is_current=1",params![account,remote,seen.unwrap_or(0)])?;
        state.observed_this_scan = scan_count;
        state.revision += 1;
        self.save_inventory(account, &state)?;
        tx.commit()?;
        Ok(())
    }
    fn action_source(&self, account: &str, p: &Value) -> Result<SourceStamp> {
        identifier(account)?;
        let selected = crate::string(p, "draft_id")?;
        let revision = page_number(p, "source_revision", 0, 1, i64::MAX)?;
        let row:Option<(String,Option<String>,bool,String)>=self.db.query_row("SELECT draft_id,linked_local_draft_id,is_current,metadata_json FROM google_draft_sources WHERE account=?1 AND source_revision=?2 AND (draft_id=?3 OR linked_local_draft_id=?3)",params![account,revision,selected],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let (original, linked, is_current, raw) =
            row.ok_or_else(|| err("not_found", "Source is outside this account and draft"))?;
        let source: Source = serde_json::from_str(&raw)?;
        if source.source_state != "available" {
            return Err(err(
                "content_unavailable",
                "A complete retained RAW source is required",
            ));
        }
        Ok(SourceStamp {
            provider_draft_id: source.provider_draft_id,
            message_id: source.message_id.ok_or_else(invalid)?,
            thread_id: source.thread_id.ok_or_else(invalid)?,
            sha256: source.sha256.ok_or_else(invalid)?,
            size: source.size.ok_or_else(invalid)?,
            revision,
            original_draft_id: original,
            linked_local_draft_id: linked,
            is_current,
        })
    }
    pub(crate) fn linked_draft_delete_request(
        &self,
        account: &str,
        draft: &str,
        expected: &Value,
        request: &str,
    ) -> Result<Value> {
        let revision:Option<i64>=self.db.query_row("SELECT source_revision FROM google_draft_sources WHERE account=?1 AND is_current=1 AND (draft_id=?2 OR linked_local_draft_id=?2) ORDER BY source_revision DESC LIMIT 1",params![account,draft],|r|r.get(0)).optional()?;
        let revision = revision.ok_or_else(|| {
            err(
                "content_unavailable",
                "Wait for the linked Google draft to be read before removing it",
            )
        })?;
        let p = json!({"draft_id":draft,"source_revision":revision,"request_id":request});
        let source = self.action_source(account, &p)?;
        if source.provider_draft_id != expected["provider_draft_id"].as_str().unwrap_or("")
            || source.message_id != expected["message_id"].as_str().unwrap_or("")
            || source.sha256 != expected["sha256"].as_str().unwrap_or("")
            || Some(source.size) != expected["size"].as_u64()
        {
            return Err(err(
                "stale_source_revision",
                "The linked Google draft changed; its newer version is preserved",
            ));
        }
        Ok(p)
    }
    fn action_owner_stamp(&self, account: &str, source: &SourceStamp) -> Result<Value> {
        let (owner, ambiguous) = self.known_draft_owner(account, &source.provider_draft_id)?;
        if ambiguous {
            return Err(err(
                "google_draft_conflict",
                "The remote target has ambiguous local ownership",
            ));
        }
        let mut ids = BTreeSet::from([source.original_draft_id.clone()]);
        ids.extend(source.linked_local_draft_id.clone());
        ids.extend(owner);
        let mut stamps = Vec::new();
        for id in ids {
            let row:(i64,String,Option<String>,bool)=self.db.query_row("SELECT revision,origin,google_json,discarded FROM drafts WHERE account=?1 AND draft_id=?2",params![account,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
            let twin: Value = row
                .2
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or(Value::Null);
            // An operation's own deletion guard/revision must not invalidate its
            // source proof. All independent authored and provider state remains fenced.
            let semantic = json!({"state":twin["state"].as_str().unwrap_or("local"),"operation_id":twin["operation_id"],"baseline":twin["baseline"],"conflict":twin["conflict"],"submission":twin["submission"],"adoption":twin["adoption"],"derivation":twin["derivation"]});
            stamps.push(json!({"draft_id":id,"revision":row.0,"origin":row.1,"google":semantic,"discarded":row.3}));
        }
        Ok(json!(stamps))
    }
    fn check_action_work(
        &self,
        account: &str,
        source: &SourceStamp,
        except_delete: Option<&str>,
    ) -> Result<()> {
        let stamps = self.action_owner_stamp(account, source)?;
        for stamp in stamps.as_array().unwrap() {
            let twin = &stamp["google"];
            if matches!(
                twin["state"].as_str(),
                Some("prepared" | "in_flight" | "acceptance_unknown" | "acknowledged")
            ) || matches!(
                twin["submission"]["state"].as_str(),
                Some("prepared" | "in_flight" | "acceptance_unknown")
            ) {
                return Err(err(
                    "google_draft_pending",
                    "Inspect the related upload or submission before acting on its source",
                ));
            }
        }
        self.check_google_draft_deletion_target(account, &source.provider_draft_id, except_delete)
    }
    fn action_context(&self, account: &str, p: &Value, current: bool) -> Result<ActionContext> {
        self.require_active_account(account)?;
        if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Remote source actions require a live profile",
            ));
        }
        let source = self.action_source(account, p)?;
        if current && !source.is_current {
            return Err(err(
                "stale_source_revision",
                "Inspect the latest retained source before acting",
            ));
        }
        if current {
            self.check_action_target_available(account, &source)?;
        }
        self.check_action_work(account, &source, None)?;
        let identity = self.action_identity(account)?;
        let owner_stamp = self.action_owner_stamp(account, &source)?;
        Ok(ActionContext {
            lifecycle_revision: self.require_active_account(account)?,
            blobs: self.google_blob_store(),
            account: account.into(),
            selected: crate::string(p, "draft_id")?.into(),
            source,
            owner_stamp,
            identity,
        })
    }
    fn check_action_target_available(&self, account: &str, source: &SourceStamp) -> Result<()> {
        let stamps = self.action_owner_stamp(account, source)?;
        if stamps
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["google"]["submission"]["last_accepted_draft_revision"].is_i64())
        {
            return Err(err(
                "google_send_consumed",
                "This target was consumed by an accepted submission",
            ));
        }
        self.check_google_draft_deleted_target(account, &source.provider_draft_id)
    }
    fn action_identity(&self, account: &str) -> Result<Value> {
        Ok(self.db.query_row("SELECT a.email,a.name,m.revision FROM accounts a JOIN account_metadata m ON m.account=a.id WHERE a.id=?1",[account],|r|Ok(json!({"email":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"revision":r.get::<_,i64>(2)?})))?)
    }
    fn check_action_context(
        &self,
        c: &ActionContext,
        current: bool,
        except_delete: Option<&str>,
    ) -> Result<()> {
        self.check_active_account_revision(&c.account, c.lifecycle_revision)?;
        if c.blobs.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Source proof belongs to another profile",
            ));
        }
        let source = self.action_source(
            &c.account,
            &json!({"draft_id":c.selected,"source_revision":c.source.revision}),
        )?;
        if source.sha256 != c.source.sha256
            || source.linked_local_draft_id != c.source.linked_local_draft_id
            || current && !source.is_current
            || self.action_owner_stamp(&c.account, &source)? != c.owner_stamp
            || self.action_identity(&c.account)? != c.identity
        {
            return Err(err(
                "stale_source_revision",
                "Source, local editor or account identity changed during preparation",
            ));
        }
        self.check_action_work(&c.account, &source, except_delete)
    }
    fn source_action_receipt(
        &self,
        account: &str,
        method: &str,
        p: &Value,
    ) -> Result<Option<Value>> {
        let fingerprint = crate::mutations::fingerprint(method, p)?;
        let row: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request_id(p)?],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(old, reply)| {
            if old != fingerprint {
                Err(err(
                    "request_id_conflict",
                    "Request ID identifies another source action",
                ))
            } else {
                Ok(serde_json::from_str(&reply)?)
            }
        })
        .transpose()
    }
    pub fn google_draft_authoring_begin(
        &self,
        account: &str,
        p: &Value,
    ) -> Result<GoogleDraftAuthoringStep> {
        source_params(p, &[])?;
        Ok(GoogleDraftAuthoringStep::Prepare(
            GoogleDraftAuthoringPlan {
                context: Box::new(self.action_context(account, p, false)?),
                method: "google_draft_authoring".into(),
                params: p.clone(),
                scope: None,
            },
        ))
    }
    pub fn google_draft_copy_begin(
        &self,
        account: &str,
        p: &Value,
    ) -> Result<GoogleDraftAuthoringStep> {
        source_params(
            p,
            &[
                "request_id",
                "from_policy",
                "content_policy",
                "inline_policy",
                "thread_policy",
            ],
        )?;
        for (key, value) in [
            ("from_policy", "primary"),
            ("content_policy", "plain_text"),
            ("inline_policy", "attachments"),
            ("thread_policy", "omit"),
        ] {
            if p[key] != value {
                return Err(err("invalid_request","Copy requires explicit primary From, plain text, ordinary files and omitted thread policies"));
            }
        }
        if let Some(reply) = self.source_action_receipt(account, "google_draft_copy", p)? {
            return Ok(GoogleDraftAuthoringStep::Reply(reply));
        }
        Ok(GoogleDraftAuthoringStep::Prepare(
            GoogleDraftAuthoringPlan {
                context: Box::new(self.action_context(account, p, false)?),
                method: "google_draft_copy".into(),
                params: p.clone(),
                scope: None,
            },
        ))
    }
    pub fn google_draft_adopt_begin(
        &self,
        scope: &GoogleDraftScope,
        p: &Value,
    ) -> Result<GoogleDraftAuthoringStep> {
        source_params(p, &["request_id"])?;
        if let Some(reply) = self.source_action_receipt(&scope.account, "google_draft_adopt", p)? {
            return Ok(GoogleDraftAuthoringStep::Reply(reply));
        }
        self.draft_scope(scope)?;
        Ok(GoogleDraftAuthoringStep::Prepare(
            GoogleDraftAuthoringPlan {
                context: Box::new(self.action_context(&scope.account, p, true)?),
                method: "google_draft_adopt".into(),
                params: p.clone(),
                scope: Some(scope.clone()),
            },
        ))
    }
    pub fn google_draft_authoring_finish(
        &mut self,
        proof: PreparedGoogleDraftAuthoring,
    ) -> Result<Value> {
        let mut commit_attempted = false;
        self.finish_draft_authoring(proof, &mut commit_attempted)
            .map_err(|mut error| {
                if !commit_attempted {
                    error.details["draft_authoring_invocation_finished_without_commit"] =
                        json!(true);
                }
                error
            })
    }
    fn finish_draft_authoring(
        &mut self,
        proof: PreparedGoogleDraftAuthoring,
        commit_attempted: &mut bool,
    ) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let p = proof.plan;
        if p.method != "google_draft_authoring" {
            if let Some(reply) =
                self.source_action_receipt(&p.context.account, &p.method, &p.params)?
            {
                return Ok(reply);
            }
        }
        if let Some(scope) = &p.scope {
            self.draft_scope(scope)?;
        }
        self.check_action_context(&p.context, p.method == "google_draft_adopt", None)?;
        if let Some(source) = proof.changed_source {
            self.retain_action_source(source)?;
            return Err(err(
                "stale_source_revision",
                "Remote source changed; both retained versions are available in Saved sources",
            ));
        }
        if p.method == "google_draft_authoring" {
            return Ok(proof.assessment);
        }
        let adopt = p.method == "google_draft_adopt";
        let eligible = if adopt { "adopt" } else { "copy" };
        if proof.assessment[eligible]["allowed"] != true {
            let mut failure = err(
                "draft_source_unsupported",
                "The complete source cannot be represented by this operation",
            );
            failure.details = proof.assessment;
            return Err(failure);
        }
        let authored = proof.authored.ok_or_else(invalid)?;
        let c = p.context;
        let account = &c.account;
        let existing = if adopt {
            self.known_draft_owner(account, &c.source.provider_draft_id)?
                .0
                .or(c.source.linked_local_draft_id.clone())
        } else {
            None
        };
        let tx = self.db.unchecked_transaction()?;
        let draft_id = if let Some(existing) = existing {
            if !self.compatible_google_adoption(account, &existing, &c.source.target())? {
                return Err(err("google_draft_conflict","The existing local editor cannot be overwritten by this source; inspect or copy it"));
            }
            existing
        } else {
            let id = format!(
                "draft-{}",
                hash(format!("{account}\0{}", request_id(&p.params)?).as_bytes())
            );
            for part in &authored.parts {
                tx.execute("INSERT OR IGNORE INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,NULL,?3,?4,?5,'available',?6)",params![account,part.id,part.name,part.mime_type,part.size,part.sha256])?;
            }
            let ids: Vec<_> = authored.parts.iter().map(|part| &part.id).collect();
            let twin = self.google_source_twin_raw(
                if adopt {
                    Some(Adoption {
                        source_draft_id: c.source.original_draft_id.clone(),
                        source_revision: c.source.revision,
                        draft_revision: 1,
                        target: c.source.target(),
                    })
                } else {
                    None
                },
                if adopt {
                    None
                } else {
                    Some(Derivation {
                        source_draft_id: c.source.original_draft_id.clone(),
                        source_revision: c.source.revision,
                        source_sha256: c.source.sha256.clone(),
                        policies: p.params.clone(),
                    })
                },
            )?;
            tx.execute("INSERT INTO drafts(account,draft_id,mode,routing_json,body,revision,attachments_json,signature_text,from_name,identity_revision,google_json,created_at,updated_at) VALUES(?1,?2,'new',?3,?4,1,?5,'',?6,?7,?8,?9,?9)",params![account,id,serde_json::to_string(&authored.routing)?,authored.body,serde_json::to_string(&ids)?,authored.from_name,c.identity["revision"].as_i64(),twin,now()?/1000])?;
            if adopt {
                tx.execute("UPDATE google_draft_sources SET linked_local_draft_id=?3 WHERE account=?1 AND provider_draft_id=?2",params![account,c.source.provider_draft_id,id])?;
            }
            id
        };
        let reply = self.composition(account, &json!({"draft_id":draft_id}))?;
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request_id(&p.params)?,crate::mutations::fingerprint(&p.method,&p.params)?,serde_json::to_string(&reply)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        *commit_attempted = true;
        tx.commit()?;
        Ok(reply)
    }
}
pub(crate) fn prepare_adopted_source(
    blobs: &BlobStore,
    scope: &GoogleDraftScope,
    target: &str,
    message: &str,
    remote: Option<google_drafts::GoogleDraftRemote>,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftImport> {
    let plan = GoogleDraftImportPlan {
        scope: GoogleDraftReadScope {
            account: scope.account.clone(),
            connection_generation: scope.connection_generation,
        },
        blobs: blobs.clone(),
        generation: 0,
        page_revision: 0,
        item: GoogleDraftListedId {
            draft_id: target.into(),
            message_id: message.into(),
        },
    };
    let outcome = remote
        .map(|r| GoogleDraftImportOutcome::Available {
            draft_id: r.draft_id,
            message_id: r.message_id,
            thread_id: r.thread_id,
            raw: r.raw,
        })
        .unwrap_or(GoogleDraftImportOutcome::Missing);
    prepare_google_draft_import(plan, outcome, cancel)
}
fn prepare_action_source(
    c: &ActionContext,
    generation: i64,
    remote: Option<GoogleDraftActionRemote>,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftImport> {
    let plan = GoogleDraftImportPlan {
        scope: GoogleDraftReadScope {
            account: c.account.clone(),
            connection_generation: generation,
        },
        blobs: c.blobs.clone(),
        generation: 0,
        page_revision: 0,
        item: GoogleDraftListedId {
            draft_id: c.source.provider_draft_id.clone(),
            message_id: c.source.message_id.clone(),
        },
    };
    let outcome = remote
        .map(|r| GoogleDraftImportOutcome::Available {
            draft_id: r.draft_id,
            message_id: r.message_id,
            thread_id: r.thread_id,
            raw: r.raw,
        })
        .unwrap_or(GoogleDraftImportOutcome::Missing);
    prepare_google_draft_import(plan, outcome, cancel)
}
