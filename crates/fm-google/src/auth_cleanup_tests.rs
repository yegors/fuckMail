use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct HeldVault {
    token: Mutex<Option<StoredToken>>,
    removals: AtomicUsize,
    entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl CredentialVault for HeldVault {
    fn load(&self, _: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        Ok(self.token.lock().unwrap().clone())
    }
    fn store(&self, _: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        *self.token.lock().unwrap() = Some(token);
        Ok(())
    }
    fn remove(&self, _: &AccountId) -> Result<(), GoogleError> {
        self.removals.fetch_add(1, Ordering::SeqCst);
        if let Some(entered) = self.entered.lock().unwrap().take() {
            let _ = entered.send(());
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| GoogleError::StoreUnavailable)?;
        }
        self.token.lock().unwrap().take();
        Ok(())
    }
}
struct Release(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(send) = self.0.take() {
            let _ = send.send(());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_cleanup_waiter_keeps_exact_account_locked_until_native_delete_finishes() {
    let (entered, wait) = tokio::sync::oneshot::channel();
    let (release, receive) = std::sync::mpsc::channel();
    let release = Release(Some(release));
    let vault = Arc::new(HeldVault {
        token: Mutex::new(Some(StoredToken {
            access_token: "synthetic-access".into(),
            refresh_token: "synthetic-refresh".into(),
            expires_at_secs: 9999,
        })),
        removals: AtomicUsize::new(0),
        entered: Mutex::new(Some(entered)),
        release: Mutex::new(receive),
    });
    let coordinator = Arc::new(TokenCoordinator::default());
    let account = AccountId::new("cleanup-account").unwrap();
    let guard = coordinator
        .account_guard(&account, &CancellationToken::new())
        .await
        .unwrap();
    let saved = vault.clone();
    let cleanup = tokio::spawn(async move { guard.remove(saved).await });
    tokio::time::timeout(Duration::from_secs(2), wait)
        .await
        .unwrap()
        .unwrap();
    cleanup.abort();
    assert!(cleanup.await.unwrap_err().is_cancelled());
    let next_coordinator = coordinator.clone();
    let next_account = account.clone();
    let mut next = tokio::spawn(async move {
        next_coordinator
            .account_guard(&next_account, &CancellationToken::new())
            .await
    });
    assert!(tokio::time::timeout(Duration::from_millis(50), &mut next)
        .await
        .is_err());
    assert_eq!(vault.removals.load(Ordering::SeqCst), 1);
    // Another account remains independent, while cancellation of a queued
    // same-account operation cannot reach the vault.
    let other = AccountId::new("other-account").unwrap();
    let other_guard = tokio::time::timeout(
        Duration::from_secs(1),
        coordinator.account_guard(&other, &CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    drop(other_guard);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        coordinator.account_guard(&account, &cancelled).await,
        Err(GoogleError::TransportInterrupted)
    ));
    drop(release);
    let next = tokio::time::timeout(Duration::from_secs(2), next)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(vault.load(&account).unwrap().is_none());
    // Exact-entry absence is idempotent, with no credential recreation.
    next.remove(vault.clone()).await.unwrap();
    assert_eq!(vault.removals.load(Ordering::SeqCst), 2);
    assert!(vault.load(&account).unwrap().is_none());
}
