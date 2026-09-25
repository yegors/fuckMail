use super::*;

#[derive(Clone)]
pub struct GoogleDraftDeleteScope {
    pub account: String,
    pub connection_generation: i64,
}
pub enum GoogleDraftDeleteStep {
    Reply(Value),
    Prepare(GoogleDraftDeletePlan),
}
pub struct GoogleDraftDeletePlan {
    context: Box<ActionContext>,
    scope: GoogleDraftDeleteScope,
    params: Value,
    operation: Option<Box<DeleteOperation>>,
}
impl GoogleDraftDeletePlan {
    pub fn provider_draft_id(&self) -> &str {
        &self.context.source.provider_draft_id
    }
}
pub struct PreparedGoogleDraftDelete {
    plan: GoogleDraftDeletePlan,
    cancel: CancellationToken,
    changed_source: Option<PreparedGoogleDraftImport>,
}
#[derive(Clone)]
pub struct GoogleDraftDeleteLease {
    profile: PathBuf,
    operation: DeleteOperation,
}
impl GoogleDraftDeleteLease {
    pub fn account(&self) -> &str {
        &self.operation.account
    }
    pub fn draft_id(&self) -> &str {
        &self.operation.draft_id
    }
    pub fn request_id(&self) -> &str {
        &self.operation.request_id
    }
    pub fn provider_draft_id(&self) -> &str {
        &self.operation.source.provider_draft_id
    }
    pub fn revision(&self) -> i64 {
        self.operation.revision
    }
}
pub enum GoogleDraftDeleteOutcome {
    Deleted,
    Absent,
    Rejected { code: String },
    Undispatched { code: String },
    Unknown { code: String },
}
pub struct GoogleDraftDeleteObservationPlan {
    profile: PathBuf,
    blobs: BlobStore,
    scope: GoogleDraftDeleteScope,
    operation: DeleteOperation,
}
impl GoogleDraftDeleteObservationPlan {
    pub fn provider_draft_id(&self) -> &str {
        &self.operation.source.provider_draft_id
    }
}
pub struct PreparedGoogleDraftDeleteObservation {
    plan: GoogleDraftDeleteObservationPlan,
    observation: Option<ObservedTarget>,
    retained: PreparedGoogleDraftImport,
    cancel: CancellationToken,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct DeletionGuard {
    pub request_id: String,
    pub state: String,
    pub blocks_target: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct DeleteOperation {
    kind: String,
    version: u32,
    account: String,
    draft_id: String,
    request_id: String,
    request: Value,
    revision: i64,
    state: String,
    connection_generation: i64,
    source_revision: i64,
    source: ObservedTarget,
    owner_stamp: Value,
    affected_draft_ids: Vec<String>,
    error: Option<String>,
    read_evidence: Option<Value>,
}
fn pending(state: &str) -> bool {
    matches!(state, "prepared" | "in_flight" | "acceptance_unknown")
}
fn code(value: String) -> Result<String> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        Err(invalid())
    } else {
        Ok(value)
    }
}

pub fn prepare_google_draft_delete(
    plan: GoogleDraftDeletePlan,
    remote: GoogleDraftActionRemote,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftDelete> {
    cancelled(cancel)?;
    local_content::read_blob(
        &plan.context.blobs,
        &plan.context.source.sha256,
        plan.context.source.size,
    )?;
    let changed_source = match authoring::verify_remote(&plan.context.source, &remote) {
        Ok(()) => None,
        Err(e) if e.code == "stale_source_revision" => Some(prepare_action_source(
            &plan.context,
            plan.scope.connection_generation,
            Some(remote),
            cancel,
        )?),
        Err(e) => return Err(e),
    };
    cancelled(cancel)?;
    Ok(PreparedGoogleDraftDelete {
        plan,
        cancel: cancel.clone(),
        changed_source,
    })
}
pub fn prepare_google_draft_delete_observation(
    plan: GoogleDraftDeleteObservationPlan,
    remote: Option<GoogleDraftActionRemote>,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftDeleteObservation> {
    cancelled(cancel)?;
    let observation = remote
        .as_ref()
        .map(|remote| -> Result<_> {
            identifier(&remote.draft_id)?;
            identifier(&remote.message_id)?;
            identifier(&remote.thread_id)?;
            if remote.draft_id != plan.operation.source.provider_draft_id {
                return Err(err(
                    "invalid_google_response",
                    "Draft reconciliation returned another container",
                ));
            }
            if remote.raw.is_empty() || remote.raw.len() > MAX_IMPORT_RAW_BYTES {
                return Err(err("limit_exceeded", "Remote draft source exceeds 40 MiB"));
            }
            let size = remote.raw.len() as u64;
            let sha256 = plan.blobs.stage_blob(&remote.raw)?;
            Ok(ObservedTarget {
                provider_draft_id: remote.draft_id.clone(),
                message_id: remote.message_id.clone(),
                thread_id: remote.thread_id.clone(),
                sha256,
                size,
            })
        })
        .transpose()?;
    let import = GoogleDraftImportPlan {
        scope: GoogleDraftReadScope {
            account: plan.scope.account.clone(),
            connection_generation: plan.scope.connection_generation,
        },
        blobs: plan.blobs.clone(),
        generation: 0,
        page_revision: 0,
        item: GoogleDraftListedId {
            draft_id: plan.operation.source.provider_draft_id.clone(),
            message_id: plan.operation.source.message_id.clone(),
        },
        fence: None,
    };
    let outcome = remote
        .map(|r| GoogleDraftImportOutcome::Available {
            draft_id: r.draft_id,
            message_id: r.message_id,
            thread_id: r.thread_id,
            raw: r.raw,
        })
        .unwrap_or(GoogleDraftImportOutcome::Missing);
    let retained = prepare_google_draft_import(import, outcome, cancel)?;
    cancelled(cancel)?;
    Ok(PreparedGoogleDraftDeleteObservation {
        plan,
        observation,
        retained,
        cancel: cancel.clone(),
    })
}
impl Store {
    fn delete_scope(&self, scope: &GoogleDraftDeleteScope) -> Result<()> {
        self.draft_connection(&scope.account, scope.connection_generation)
    }
    fn delete_operation(&self, account: &str, request: &str) -> Result<Option<DeleteOperation>> {
        identifier(account)?;
        identifier(request)?;
        google_drafts::check_google_operation_kind(&self.db, account, request, "draft_delete")?;
        let row:Option<(String,String)>=self.db.query_row("SELECT json,blob_hash FROM capture_operations WHERE account=?1 AND id=?2 AND execution_kind='google' AND json_extract(json,'$.kind')='draft_delete'",params![account,request],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        row.map(|(raw, hash)| {
            let op: DeleteOperation = serde_json::from_str(&raw)?;
            if op.kind != "draft_delete"
                || op.version != 1
                || op.account != account
                || op.request_id != request
                || op.source.sha256 != hash
                || op.revision < 1
                || op.source.size > MAX_IMPORT_RAW_BYTES as u64
            {
                return Err(invalid());
            }
            Ok(op)
        })
        .transpose()
    }
    pub fn google_draft_delete_begin(
        &self,
        scope: &GoogleDraftDeleteScope,
        p: &Value,
    ) -> Result<GoogleDraftDeleteStep> {
        source_params(p, &["request_id"])?;
        let operation = self.delete_operation(&scope.account, request_id(p)?)?;
        if let Some(op) = &operation {
            if op.request != *p {
                return Err(err(
                    "request_id_conflict",
                    "Deletion request freezes another source",
                ));
            }
            if op.state != "prepared"
                || self.account_lifecycle(&scope.account)?["state"] == "removed"
            {
                return Ok(GoogleDraftDeleteStep::Reply(self.delete_reply(op)?));
            }
        }
        self.delete_scope(scope)?;
        let source = self.action_source(&scope.account, p)?;
        if !source.is_current {
            return Err(err(
                "stale_source_revision",
                "Inspect the current source before deleting",
            ));
        }
        self.check_action_target_available(&scope.account, &source)?;
        self.check_action_work(&scope.account, &source, Some(request_id(p)?))?;
        let context = ActionContext {
            lifecycle_revision: self.require_active_account(&scope.account)?,
            blobs: self.google_blob_store(),
            account: scope.account.clone(),
            selected: crate::string(p, "draft_id")?.into(),
            owner_stamp: self.action_owner_stamp(&scope.account, &source)?,
            identity: self.action_identity(&scope.account)?,
            source,
        };
        if operation.as_ref().is_some_and(|op| {
            op.owner_stamp != context.owner_stamp || op.source != context.source.target()
        }) {
            return Err(err(
                "stale_source_revision",
                "The prepared deletion no longer matches the retained source or local editor",
            ));
        }
        Ok(GoogleDraftDeleteStep::Prepare(GoogleDraftDeletePlan {
            context: Box::new(context),
            scope: scope.clone(),
            params: p.clone(),
            operation: operation.map(Box::new),
        }))
    }
    pub fn google_draft_delete_finish(
        &mut self,
        proof: PreparedGoogleDraftDelete,
    ) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let p = proof.plan;
        let request = request_id(&p.params)?;
        let existing = self.delete_operation(&p.scope.account, request)?;
        if let Some(op) = &existing {
            if op.request != p.params {
                return Err(err(
                    "request_id_conflict",
                    "Deletion request freezes another source",
                ));
            }
            if op.state != "prepared"
                || self.account_lifecycle(&p.scope.account)?["state"] == "removed"
            {
                return self.delete_reply(op);
            }
            if p.operation.as_ref().map(|v| v.revision) != Some(op.revision) {
                return Err(err(
                    "stale_google_work",
                    "Deletion changed during verification",
                ));
            }
        }
        self.delete_scope(&p.scope)?;
        self.check_action_context(&p.context, true, Some(request))?;
        if let Some(source) = proof.changed_source {
            self.retain_action_source(source)?;
            return Err(err(
                "stale_source_revision",
                "Remote source changed; both retained versions are available in Saved sources",
            ));
        }
        let affected = p
            .context
            .owner_stamp
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["draft_id"].as_str().map(str::to_owned))
            .collect();
        let mut op = existing.unwrap_or(DeleteOperation {
            kind: "draft_delete".into(),
            version: 1,
            account: p.scope.account,
            draft_id: p.context.selected,
            request_id: request.into(),
            request: p.params.clone(),
            revision: 0,
            state: "prepared".into(),
            connection_generation: p.scope.connection_generation,
            source_revision: p.context.source.revision,
            source: p.context.source.target(),
            owner_stamp: p.context.owner_stamp,
            affected_draft_ids: affected,
            error: None,
            read_evidence: None,
        });
        let insert = op.revision == 0;
        op.revision += 1;
        op.connection_generation = p.scope.connection_generation;
        self.persist_delete(&op, insert)?;
        self.delete_reply(&op)
    }
    pub fn google_draft_delete_claim(
        &mut self,
        scope: &GoogleDraftDeleteScope,
        request: &str,
        revision: i64,
    ) -> Result<GoogleDraftDeleteLease> {
        self.delete_scope(scope)?;
        self.check_transfer_cleanup_authority(&scope.account, request)?;
        let mut op = self
            .delete_operation(&scope.account, request)?
            .ok_or_else(|| err("not_found", "Deletion request is not recorded"))?;
        if op.state != "prepared"
            || op.revision != revision
            || op.connection_generation != scope.connection_generation
        {
            return Err(err(
                "stale_google_work",
                "Only the verified known-unsent deletion can be claimed",
            ));
        }
        let source = self.action_source(&op.account, &op.request)?;
        if !source.is_current
            || source.target() != op.source
            || self.action_owner_stamp(&op.account, &source)? != op.owner_stamp
        {
            return Err(err(
                "stale_source_revision",
                "Source or local editor changed before deletion",
            ));
        }
        self.check_action_work(&op.account, &source, Some(request))?;
        op.revision += 1;
        op.state = "in_flight".into();
        op.error = None;
        self.persist_delete(&op, false)?;
        Ok(GoogleDraftDeleteLease {
            profile: self.profile.clone(),
            operation: op,
        })
    }
    pub fn google_draft_delete_outcome(
        &mut self,
        lease: &GoogleDraftDeleteLease,
        outcome: GoogleDraftDeleteOutcome,
    ) -> Result<Value> {
        if lease.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Deletion lease belongs to another profile",
            ));
        }
        let mut op = self
            .delete_operation(lease.account(), lease.request_id())?
            .ok_or_else(invalid)?;
        let (state, error, evidence) = match outcome {
            GoogleDraftDeleteOutcome::Deleted => ("deleted", None, None),
            GoogleDraftDeleteOutcome::Absent => (
                "rejected",
                Some("remote_absent".into()),
                Some(
                    json!({"draft_exists":false,"deletion_proven":false,"source":"delete_response"}),
                ),
            ),
            GoogleDraftDeleteOutcome::Rejected { code: c } => ("rejected", Some(code(c)?), None),
            GoogleDraftDeleteOutcome::Undispatched { code: c } => {
                ("prepared", Some(code(c)?), None)
            }
            GoogleDraftDeleteOutcome::Unknown { code: c } => {
                ("acceptance_unknown", Some(code(c)?), None)
            }
        };
        if op.revision != lease.revision() || op.state != "in_flight" {
            if op.revision == lease.revision() + 1
                && op.state == state
                && op.error == error
                && op.read_evidence == evidence
            {
                return self.delete_reply(&op);
            }
            return Err(err(
                "stale_google_work",
                "Deletion outcome does not identify the active attempt",
            ));
        }
        op.revision += 1;
        op.state = state.into();
        op.error = error;
        op.read_evidence = evidence;
        self.persist_delete(&op, false)?;
        self.delete_reply(&op)
    }
    pub fn google_draft_delete_status(&self, account: &str, request: &str) -> Result<Value> {
        self.delete_reply(
            &self
                .delete_operation(account, request)?
                .ok_or_else(|| err("not_found", "Deletion request is not recorded"))?,
        )
    }
    pub fn google_draft_delete_abandon(&mut self, account: &str, p: &Value) -> Result<Value> {
        self.require_active_account(account)?;
        crate::fields(p, &["request_id", "expected_revision"])?;
        let mut op = self
            .delete_operation(account, request_id(p)?)?
            .ok_or_else(|| err("not_found", "Deletion request is not recorded"))?;
        if op.state != "prepared"
            || op.revision != page_number(p, "expected_revision", 0, 1, i64::MAX)?
        {
            return Err(err(
                "stale_google_work",
                "Only this proven-undispatched deletion can be abandoned",
            ));
        }
        op.state = "abandoned".into();
        op.revision += 1;
        self.persist_delete(&op, false)?;
        self.delete_reply(&op)
    }
    pub fn google_draft_delete_reconcile_begin(
        &self,
        scope: &GoogleDraftDeleteScope,
        request: &str,
    ) -> Result<GoogleDraftDeleteObservationPlan> {
        self.inventory_scope(&GoogleDraftReadScope {
            account: scope.account.clone(),
            connection_generation: scope.connection_generation,
        })?;
        let op = self
            .delete_operation(&scope.account, request)?
            .ok_or_else(|| err("not_found", "Deletion request is not recorded"))?;
        if op.state == "in_flight" {
            return Err(err(
                "google_draft_pending",
                "The active deletion must return before read-only inspection",
            ));
        }
        Ok(GoogleDraftDeleteObservationPlan {
            profile: self.profile.clone(),
            blobs: self.google_blob_store(),
            scope: scope.clone(),
            operation: op,
        })
    }
    pub fn google_draft_delete_reconcile_finish(
        &mut self,
        proof: PreparedGoogleDraftDeleteObservation,
    ) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let p = proof.plan;
        if p.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Deletion observation belongs to another profile",
            ));
        }
        self.inventory_scope(&GoogleDraftReadScope {
            account: p.scope.account.clone(),
            connection_generation: p.scope.connection_generation,
        })?;
        let mut op = self
            .delete_operation(&p.scope.account, &p.operation.request_id)?
            .ok_or_else(invalid)?;
        if op.revision != p.operation.revision {
            return Err(err(
                "stale_google_work",
                "Deletion changed during read-only observation",
            ));
        }
        self.retain_action_source(proof.retained)?;
        let evidence = json!({"draft_exists":proof.observation.is_some(),"deletion_proven":false,"source":"draft_get","observed":proof.observation});
        if op.read_evidence.as_ref() != Some(&evidence) {
            op.read_evidence = Some(evidence);
            op.revision += 1;
            self.persist_delete(&op, false)?;
        }
        self.delete_reply(&op)
    }
    pub(crate) fn check_google_draft_deletion_target(
        &self,
        account: &str,
        target: &str,
        except: Option<&str>,
    ) -> Result<()> {
        let blocked:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM capture_operations WHERE account=?1 AND execution_kind='google' AND json_extract(json,'$.kind')='draft_delete' AND json_extract(json,'$.source.provider_draft_id')=?2 AND json_extract(json,'$.state') IN ('prepared','in_flight','acceptance_unknown') AND (?3 IS NULL OR id!=?3))",params![account,target,except],|r|r.get(0))?;
        if blocked {
            Err(err(
                "google_draft_delete_pending",
                "Deletion owns or consumed this exact target; inspect its receipt",
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn check_google_draft_deleted_target(
        &self,
        account: &str,
        target: &str,
    ) -> Result<()> {
        let deleted:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM capture_operations WHERE account=?1 AND execution_kind='google' AND json_extract(json,'$.kind')='draft_delete' AND json_extract(json,'$.source.provider_draft_id')=?2 AND json_extract(json,'$.state')='deleted')",params![account,target],|r|r.get(0))?;
        if deleted {
            Err(err(
                "google_draft_deleted",
                "This exact target was deleted; a separate copy remains available",
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn check_google_draft_deletion_write(
        &self,
        account: &str,
        draft: &str,
    ) -> Result<()> {
        let guard:Option<String>=self.db.query_row("SELECT json_extract(google_json,'$.deletion') FROM drafts WHERE account=?1 AND draft_id=?2",params![account,draft],|r|r.get(0)).optional()?.flatten();
        if guard
            .map(|v| serde_json::from_str::<DeletionGuard>(&v))
            .transpose()?
            .is_some_and(|g| g.blocks_target)
        {
            return Err(err(
                "google_draft_delete_pending",
                "Deletion owns or consumed this target",
            ));
        }
        Ok(())
    }
    fn persist_delete(&mut self, op: &DeleteOperation, insert: bool) -> Result<()> {
        let raw = encode(op)?;
        let mut updates = Vec::new();
        for draft in &op.affected_draft_ids {
            let current:Option<String>=self.db.query_row("SELECT json_extract(google_json,'$.deletion.request_id') FROM drafts WHERE account=?1 AND draft_id=?2",params![op.account,draft],|r|r.get(0)).optional()?.flatten();
            if insert || current.as_deref() == Some(&op.request_id) {
                updates.push((
                    draft,
                    self.google_delete_twin_raw(
                        &op.account,
                        draft,
                        DeletionGuard {
                            request_id: op.request_id.clone(),
                            state: op.state.clone(),
                            blocks_target: pending(&op.state) || op.state == "deleted",
                        },
                    )?,
                ));
            }
        }
        let tx = self.db.transaction()?;
        if insert {tx.execute("INSERT INTO capture_operations(account,id,json,blob_hash,execution_kind) VALUES(?1,?2,?3,?4,'google')",params![op.account,op.request_id,raw,op.source.sha256])?;}
        else if tx.execute("UPDATE capture_operations SET json=?3 WHERE account=?1 AND id=?2 AND execution_kind='google' AND json_extract(json,'$.kind')='draft_delete'",params![op.account,op.request_id,raw])?!=1 {return Err(invalid());}
        for (draft, raw) in updates {
            tx.execute(
                "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
                params![op.account, draft, raw],
            )?;
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }
    fn delete_reply(&self, op: &DeleteOperation) -> Result<Value> {
        let (google,current):(i64,Option<String>)=self.db.query_row("SELECT COALESCE(json_extract(google_json,'$.revision'),0),json_extract(google_json,'$.deletion.request_id') FROM drafts WHERE account=?1 AND draft_id=?2",params![op.account,op.draft_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok(
            json!({"account":op.account,"draft_id":op.draft_id,"request_id":op.request_id,"request":op.request,"revision":op.revision,"current_google_revision":google,"operation_is_current":current.as_deref()==Some(&op.request_id),"state":op.state,"connection_generation":op.connection_generation,"related_draft_ids":op.affected_draft_ids,"source_revision":op.source_revision,"source_sha256":op.source.sha256,"provider_draft_id":op.source.provider_draft_id,"message_id":op.source.message_id,"thread_id":op.source.thread_id,"size":op.source.size,"error":op.error,"read_evidence":op.read_evidence}),
        )
    }
    pub(crate) fn google_delete_for_draft(&self, account: &str, draft: &str) -> Result<Value> {
        let request:Option<String>=self.db.query_row("SELECT json_extract(google_json,'$.deletion.request_id') FROM drafts WHERE account=?1 AND draft_id=?2",params![account,draft],|r|r.get(0)).optional()?.flatten();
        request
            .map(|r| self.google_draft_delete_status(account, &r))
            .transpose()
            .map(|v| v.unwrap_or(Value::Null))
    }
    pub(crate) fn google_delete_history(
        &self,
        account: &str,
        draft: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Value> {
        let selection="SELECT DISTINCT o.id,o.json FROM capture_operations o JOIN google_draft_sources s ON s.account=o.account AND s.provider_draft_id=json_extract(o.json,'$.source.provider_draft_id') WHERE o.account=?1 AND o.execution_kind='google' AND json_extract(o.json,'$.kind')='draft_delete' AND (s.draft_id=?2 OR s.linked_local_draft_id=?2)";
        let total: i64 = self.db.query_row(
            &format!("SELECT COUNT(*) FROM ({selection})"),
            params![account, draft],
            |r| r.get(0),
        )?;
        let mut stmt = self
            .db
            .prepare(&format!("{selection} ORDER BY o.id LIMIT ?3 OFFSET ?4"))?;
        let rows = stmt
            .query_map(params![account, draft, limit, offset], |r| {
                r.get::<_, String>(1)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let refs=rows.into_iter().map(|raw|->Result<Value>{let op:DeleteOperation=serde_json::from_str(&raw)?;Ok(json!({"request_id":op.request_id,"revision":op.revision,"state":op.state,"source_revision":op.source_revision}))}).collect::<Result<Vec<_>>>()?;
        let next = offset + refs.len() as i64;
        Ok(
            json!({"delete_operations":refs,"delete_total":total,"delete_next_offset":if next<total{Some(next)}else{None}}),
        )
    }
    pub(crate) fn recover_google_draft_deletions(&mut self) -> Result<()> {
        let ids = {
            let mut stmt=self.db.prepare("SELECT account,id FROM capture_operations WHERE execution_kind='google' AND json_extract(json,'$.kind')='draft_delete' AND json_extract(json,'$.state')='in_flight'")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for (account, id) in ids {
            let mut op = self.delete_operation(&account, &id)?.ok_or_else(invalid)?;
            op.state = "acceptance_unknown".into();
            op.error = Some("engine_interrupted".into());
            op.revision += 1;
            self.persist_delete(&op, false)?;
        }
        Ok(())
    }
}
