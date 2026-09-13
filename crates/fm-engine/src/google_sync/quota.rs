//! One engine-wide admission queue. HTTP never holds the Core writer.
use fm_google::{
    scheduler::{CooldownScope, Permit, Scheduler, SchedulerConfig, Work, WorkId},
    transport::{CancellationToken, TransportError},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

struct State {
    scheduler: Scheduler,
    admitted: BTreeMap<WorkId, Permit>,
}
pub(super) struct QuotaGate {
    state: Mutex<State>,
    changed: Notify,
    clock: Instant,
}
pub(super) struct Lease {
    gate: Arc<QuotaGate>,
    id: WorkId,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.gate.state.lock().unwrap();
        if let Some(permit) = state.admitted.remove(&self.id) {
            state.scheduler.finish(permit);
        } else {
            state.scheduler.cancel_queued(self.id);
        }
        drop(state);
        self.gate.changed.notify_waiters();
    }
}
impl QuotaGate {
    pub(super) fn register(&self, account: fm_google::AccountId) -> Result<(), TransportError> {
        self.state
            .lock()
            .unwrap()
            .scheduler
            .register_account(account)
            .map_err(|_| TransportError::InvalidRequest)
    }
    pub(super) fn new(config: SchedulerConfig) -> Result<Arc<Self>, TransportError> {
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                scheduler: Scheduler::new(config).map_err(|_| TransportError::InvalidRequest)?,
                admitted: BTreeMap::new(),
            }),
            changed: Notify::new(),
            clock: Instant::now(),
        }))
    }
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
    pub(super) async fn acquire(
        self: &Arc<Self>,
        work: Work,
        cancel: &CancellationToken,
    ) -> Result<Lease, TransportError> {
        if cancel.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        let id = {
            let mut state = self.state.lock().unwrap();
            state
                .scheduler
                .register_account(work.account.clone())
                .map_err(|_| TransportError::InvalidRequest)?;
            state
                .scheduler
                .enqueue(work)
                .map_err(|_| TransportError::InvalidRequest)?
        };
        // This guard also removes queued/admitted work when its future is aborted.
        let lease = Lease {
            gate: self.clone(),
            id,
        };
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (ready, wait) = {
                let mut state = self.state.lock().unwrap();
                let now = self.now();
                while let Some(permit) = state.scheduler.next(now) {
                    state.admitted.insert(permit.id(), permit);
                    self.changed.notify_waiters();
                }
                (
                    state.admitted.contains_key(&id),
                    state
                        .scheduler
                        .next_wake_ms(now)
                        .map(|at| Duration::from_millis(at.saturating_sub(now).max(1))),
                )
            };
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            if ready {
                return Ok(lease);
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err(TransportError::Cancelled),
                _ = &mut notified => {},
                _ = async { match wait { Some(delay) => tokio::time::sleep(delay.min(Duration::from_secs(60))).await, None => std::future::pending::<()>().await } } => {},
            }
        }
    }
    pub(super) fn cooldown(&self, scope: CooldownScope, seconds: u64) {
        let until = self
            .now()
            .saturating_add(seconds.max(1).saturating_mul(1000));
        let _ = self.state.lock().unwrap().scheduler.cooldown(scope, until);
        self.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_google::{
        scheduler::{Service, ServiceBudget, WorkClass},
        AccountId,
    };
    fn gate() -> Arc<QuotaGate> {
        let budget = ServiceBudget::from_confirmed_limits(1000, 1000).unwrap();
        let mut config = SchedulerConfig::new(budget, budget);
        config.max_in_flight = 2;
        config.max_reads_per_account = 1;
        QuotaGate::new(config).unwrap()
    }
    fn work() -> Work {
        Work {
            account: AccountId::new("test").unwrap(),
            service: Service::Gmail,
            class: WorkClass::UserRead,
            units: 1,
        }
    }
    #[tokio::test]
    async fn aborted_waiter_and_lease_release_admission() {
        let gate = gate();
        let cancel = CancellationToken::new();
        let active = gate.acquire(work(), &cancel).await.unwrap();
        let child_gate = gate.clone();
        let blocked =
            tokio::spawn(
                async move { child_gate.acquire(work(), &CancellationToken::new()).await },
            );
        tokio::task::yield_now().await;
        blocked.abort();
        let _ = blocked.await;
        assert_eq!(gate.state.lock().unwrap().scheduler.queued(), 0);
        drop(active);
        let next = tokio::time::timeout(Duration::from_secs(1), gate.acquire(work(), &cancel))
            .await
            .unwrap()
            .unwrap();
        drop(next);
        assert_eq!(gate.state.lock().unwrap().scheduler.in_flight(), 0);
    }
    #[tokio::test]
    async fn cancellation_wakes_quota_wait_without_spending_another_request() {
        let gate = gate();
        let cancel = CancellationToken::new();
        let active = gate.acquire(work(), &cancel).await.unwrap();
        let other = CancellationToken::new();
        let child = other.clone();
        let child_gate = gate.clone();
        let blocked = tokio::spawn(async move { child_gate.acquire(work(), &child).await });
        tokio::task::yield_now().await;
        other.cancel();
        assert!(matches!(
            blocked.await.unwrap(),
            Err(TransportError::Cancelled)
        ));
        assert_eq!(gate.state.lock().unwrap().scheduler.queued(), 0);
        drop(active);
    }
}
