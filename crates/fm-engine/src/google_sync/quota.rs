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
/// The engine-wide Google quota scheduler shared by every account's sync job.
/// It admits work against per-project and per-account per-minute budgets and
/// a concurrency limit; waiters are woken whenever admission state changes.
pub(super) struct QuotaGate {
    state: Mutex<State>,
    changed: Notify,
    /// Origin of the scheduler's millisecond clock.
    clock: Instant,
}
/// An admitted (or still queued) quota reservation for one HTTP request.
/// Dropping it releases the concurrency slot, or removes queued work.
pub(super) struct Lease {
    gate: Arc<QuotaGate>,
    id: WorkId,
}
impl Lease {
    /// Charge at actual HTTP dispatch, after any memory/token wait. The existing
    /// reservation already owns both budget and concurrency; never acquire twice.
    pub(super) fn dispatch(&self, cancel: &CancellationToken) -> Result<(), TransportError> {
        if cancel.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        let mut state = self.gate.state.lock().unwrap();
        if cancel.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        let State {
            scheduler,
            admitted,
        } = &mut *state;
        let permit = admitted
            .get(&self.id)
            .ok_or(TransportError::InvalidRequest)?;
        let now = self.gate.now();
        let result = if let Some(delay) = scheduler.dispatch_cooldown_ms(permit, now) {
            Err(TransportError::Cooldown {
                retry_after_secs: delay.div_ceil(1000),
            })
        } else if scheduler.dispatch(permit, now) {
            Ok(())
        } else {
            Err(TransportError::InvalidRequest)
        };
        drop(state);
        self.gate.changed.notify_waiters();
        result
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.gate.state.lock().unwrap();
        if let Some(permit) = state.admitted.remove(&self.id) {
            state.scheduler.finish_at(permit, self.gate.now());
        } else {
            state.scheduler.cancel_queued(self.id);
        }
        drop(state);
        self.gate.changed.notify_waiters();
    }
}
impl QuotaGate {
    /// Adds the account's Gmail and Calendar lanes. Idempotent; fails once the
    /// scheduler's account limit is reached.
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
    /// Queues `work` and waits until the scheduler admits it.
    ///
    /// Admission reserves the budget and a concurrency slot; the charge
    /// happens later, at [`Lease::dispatch`]. Fails with `Cancelled` when
    /// `cancel` fires, and `InvalidRequest` when the account cannot be
    /// registered or the cost can never fit its budget.
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
            // Register for wake-ups before checking, so a change between the
            // check and the wait is not missed. Each waiter admits every
            // permit that is ready, not only its own.
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
    /// Pauses admission and dispatch for `scope` for at least `seconds` (minimum
    /// one). An existing longer cooldown is kept.
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
    async fn lease_dispatch_is_one_shot_and_cancellation_releases_concurrency() {
        let gate = gate();
        let cancel = CancellationToken::new();
        let held = gate.acquire(work(), &cancel).await.unwrap();
        held.dispatch(&cancel).unwrap();
        gate.cooldown(CooldownScope::Project(Service::Gmail), 30);
        assert!(matches!(
            held.dispatch(&cancel),
            Err(TransportError::InvalidRequest)
        ));
        assert_eq!(gate.state.lock().unwrap().scheduler.in_flight(), 1);
        cancel.cancel();
        assert!(matches!(
            held.dispatch(&cancel),
            Err(TransportError::Cancelled)
        ));
        drop(held);
        let state = gate.state.lock().unwrap();
        assert_eq!(state.scheduler.in_flight(), 0);
        assert!(state.admitted.is_empty());
    }
    #[tokio::test]
    async fn newly_learned_cooldown_is_recoverable_and_keeps_lease_until_drop() {
        let gate = gate();
        let cancel = CancellationToken::new();
        let held = gate.acquire(work(), &cancel).await.unwrap();
        {
            let mut state = gate.state.lock().unwrap();
            assert!(state.scheduler.next(61_000).is_none());
            state
                .scheduler
                .cooldown(CooldownScope::Project(Service::Gmail), 62_001)
                .unwrap();
        }
        assert_eq!(
            held.dispatch(&cancel),
            Err(TransportError::Cooldown {
                retry_after_secs: 2
            })
        );
        assert_eq!(gate.state.lock().unwrap().scheduler.in_flight(), 1);
        drop(held);
        let state = gate.state.lock().unwrap();
        assert_eq!(state.scheduler.in_flight(), 0);
        assert!(state.admitted.is_empty());
    }
    #[tokio::test]
    async fn lease_hook_keeps_delayed_reservation_until_actual_dispatch() {
        let budget = ServiceBudget::from_confirmed_limits(100, 100).unwrap();
        let gate = QuotaGate::new(SchedulerConfig::new(budget, budget)).unwrap();
        let cancel = CancellationToken::new();
        let mut request = work();
        request.units = 80;
        let held = gate.acquire(request, &cancel).await.unwrap();
        let other = AccountId::new("other").unwrap();
        {
            let mut state = gate.state.lock().unwrap();
            state.scheduler.register_account(other.clone()).unwrap();
            state
                .scheduler
                .enqueue(Work {
                    account: other,
                    ..work()
                })
                .unwrap();
            // Advance the deterministic scheduler clock rather than sleeping;
            // Lease dispatch cannot move that clock backwards.
            assert!(state.scheduler.next(61_000).is_none());
            assert_eq!(state.scheduler.next_wake_ms(61_000), None);
        }
        held.dispatch(&cancel).unwrap();
        drop(held);
        let mut state = gate.state.lock().unwrap();
        assert_eq!(state.scheduler.in_flight(), 0);
        assert_eq!(state.scheduler.next_wake_ms(61_000), Some(121_000));
        assert!(state.scheduler.next(120_999).is_none());
        assert!(state.scheduler.next(121_000).is_some());
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
