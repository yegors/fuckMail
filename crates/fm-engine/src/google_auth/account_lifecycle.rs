use super::*;
use fm_core::account_lifecycle::{AccountCleanupLease, AccountCleanupOutcome, AccountRemovalStep};
use fm_google::desktop_auth::VerifiedGrant;

impl Runtime {
    pub(super) async fn remove_account(
        &self,
        account: &str,
        params: Value,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let selected = account.to_owned();
        let step = self
            .inner
            .core
            .call(move |store| store.account_remove_begin(&selected, &params))
            .await?;
        let lease = match step {
            AccountRemovalStep::Reply(reply) => return Ok(reply),
            AccountRemovalStep::Cleanup(lease) => lease,
        };
        // Historical completed replay never reaches this cancellation or vault
        // path: it cannot disturb a later restored account or its new grant.
        {
            let state = self.inner.state.lock().await;
            for attempt in &state.attempts {
                if attempt.account.as_deref() == Some(account) {
                    attempt.cancel.cancel();
                }
            }
            if let Some(work) = state.accounts.get(account) {
                work.cancel.cancel();
            }
        }
        self.cleanup_account(lease, cancel).await
    }

    pub(super) async fn cleanup_account(
        &self,
        lease: AccountCleanupLease,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let account_id = AccountId::new(lease.account().to_owned()).map_err(google_error)?;
        let guard = match self
            .inner
            .coordinator
            .account_guard(&account_id, cancel)
            .await
        {
            Ok(guard) => guard,
            Err(_) if cancel.is_cancelled() || self.inner.cancel.is_cancelled() => {
                let account = lease.account().to_owned();
                let request = lease.request_id().to_owned();
                return self
                    .inner
                    .core
                    .call(move |store| store.account_remove_status(&account, &request))
                    .await;
            }
            Err(failure) => return Err(google_error(failure)),
        };
        // A duplicate queued before restoration must recheck the durable lease
        // AFTER it owns the same lock used by refresh and verified-grant stores.
        let saved = lease.clone();
        let claim_cancel = cancel.clone();
        let engine_cancel = self.inner.cancel.clone();
        let claimed = self
            .inner
            .core
            .call(move |store| {
                if claim_cancel.is_cancelled() || engine_cancel.is_cancelled() {
                    return Err(cancelled());
                }
                store.account_cleanup_claim(&saved)
            })
            .await?;
        let outcome = match guard.remove(self.inner.vault.clone()).await {
            Ok(()) => AccountCleanupOutcome::Removed,
            Err(failure) => AccountCleanupOutcome::Failed {
                code: google_error(failure).code,
            },
        };
        // The admitted native operation and exact durable outcome finish even
        // after its session closes. No provider endpoint is involved.
        self.inner
            .core
            .call(move |store| store.account_cleanup_finish(&claimed, outcome))
            .await
    }

    pub(super) async fn complete_restore(
        &self,
        id: &str,
        client: Arc<LoadedClient>,
        account: &str,
        revision: i64,
        grant: VerifiedGrant,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let selected = account.to_owned();
        let account_id = AccountId::new(selected.clone()).map_err(google_error)?;
        let subject = grant.identity.sub.clone();
        let email = grant.identity.email.clone();
        let scopes = grant.granted_scopes.clone();
        let client_id = client.oauth.client_id().to_owned();
        client
            .oauth
            .bind_account(account_id.clone(), subject.clone())
            .map_err(google_error)?;
        let guard = self
            .inner
            .coordinator
            .account_guard(&account_id, cancel)
            .await
            .map_err(google_error)?;
        let before_store = cancel.clone();
        let engine_cancel = self.inner.cancel.clone();
        let lease = self
            .inner
            .core
            .call(move |store| {
                if before_store.is_cancelled() || engine_cancel.is_cancelled() {
                    return Err(cancelled());
                }
                store.google_restore_admit(
                    &selected, revision, &subject, &email, &scopes, &client_id,
                )
            })
            .await?;
        self.phase(id, "persisting").await;
        let persisted = if cancel.is_cancelled() || self.inner.cancel.is_cancelled() {
            drop(guard);
            Err(cancelled())
        } else {
            guard
                .persist_retaining(grant, self.inner.vault.clone())
                .await
                .map_err(google_error)
        };
        let result = match persisted {
            Ok(_credential_guard) => {
                let saved = lease.clone();
                let path = client.path.clone();
                let commit_cancel = cancel.clone();
                let engine_cancel = self.inner.cancel.clone();
                self.inner
                    .core
                    .call(move |store| {
                        if commit_cancel.is_cancelled() || engine_cancel.is_cancelled() {
                            return Err(cancelled());
                        }
                        store.google_restore_bind(&saved, &path)
                    })
                    .await
            }
            Err(failure) => Err(failure),
        };
        match result {
            Ok(account) => {
                let mut state = self.inner.state.lock().await;
                state.client = Some(client);
                state.accounts.remove(&account);
                Ok(account)
            }
            Err(failure) => {
                let code = failure.code.clone();
                // Failure may follow a changed native credential. Keep the
                // archived account's cleanup state truthful, including on close.
                self.inner
                    .core
                    .call(move |store| store.google_restore_failed(&lease, &code))
                    .await?;
                Err(failure)
            }
        }
    }
}
