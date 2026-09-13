//! Deterministic quota admission. The owning worker supplies monotonic time and
//! executes each permit once; cancellation, persistence and retries stay explicit.
use crate::{read_retry_delay, AccountId, GoogleError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Service {
    Gmail,
    Calendar,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkClass {
    UserRead,
    DeltaRead,
    BackfillRead,
    StateMutation,
    DraftMutation,
    Submission,
    CalendarMutation,
}
impl WorkClass {
    pub fn is_read(self) -> bool {
        matches!(self, Self::UserRead | Self::DeltaRead | Self::BackfillRead)
    }
    fn background(self) -> bool {
        self == Self::BackfillRead
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceBudget {
    pub project_units_per_minute: u32,
    pub account_units_per_minute: u32,
}
impl ServiceBudget {
    /// Supply actual Console limits, in that service's units. This reserves 20%
    /// headroom; it is not a claim about a particular project's Google quotas.
    pub fn from_confirmed_limits(project: u32, account: u32) -> Result<Self, SchedulerError> {
        let budget = Self {
            project_units_per_minute: project / 5 * 4,
            account_units_per_minute: account / 5 * 4,
        };
        budget.validate()?;
        Ok(budget)
    }
    fn validate(self) -> Result<(), SchedulerError> {
        if self.project_units_per_minute < 4 || self.account_units_per_minute < 4 {
            Err(SchedulerError::InvalidConfiguration)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub gmail: ServiceBudget,
    pub calendar: ServiceBudget,
    pub max_in_flight: usize,
    pub max_reads_per_account: usize,
    pub max_queued: usize,
    pub max_accounts: usize,
}
impl SchedulerConfig {
    pub fn new(gmail: ServiceBudget, calendar: ServiceBudget) -> Self {
        Self {
            gmail,
            calendar,
            max_in_flight: 4,
            max_reads_per_account: 2,
            max_queued: 1024,
            max_accounts: 128,
        }
    }
    fn budget(&self, service: Service) -> ServiceBudget {
        match service {
            Service::Gmail => self.gmail,
            Service::Calendar => self.calendar,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulerError {
    InvalidConfiguration,
    UnknownAccount,
    Capacity,
    InvalidCost,
    InvalidClass,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkId(u64);
impl WorkId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}
#[derive(Clone)]
pub struct Work {
    pub account: AccountId,
    pub service: Service,
    pub class: WorkClass,
    pub units: u32,
}
impl std::fmt::Debug for Work {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Work")
            .field("service", &self.service)
            .field("class", &self.class)
            .field("units", &self.units)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct Permit {
    owner: u64,
    id: WorkId,
    work: Work,
}
impl Permit {
    pub fn id(&self) -> WorkId {
        self.id
    }
    pub fn work(&self) -> &Work {
        &self.work
    }
}
#[derive(Clone, Debug)]
pub enum CooldownScope {
    Project(Service),
    Account(AccountId, Service),
}

#[derive(Default)]
struct Window {
    charges: VecDeque<(u64, u32)>,
    used: u64,
    // Admitted work may still be awaiting memory or token refresh. Its budget
    // cannot expire until the request actually dispatches (or is cancelled).
    reserved: u64,
}
impl Window {
    fn prune(&mut self, now: u64) {
        while self
            .charges
            .front()
            .is_some_and(|(at, _)| now.saturating_sub(*at) >= 60_000)
        {
            self.used -= u64::from(self.charges.pop_front().unwrap().1);
        }
    }
    fn allows(&mut self, now: u64, units: u32, limit: u32) -> bool {
        self.prune(now);
        self.used + self.reserved + u64::from(units) <= u64::from(limit)
    }
    fn reserve(&mut self, units: u32) {
        self.reserved += u64::from(units);
    }
    fn dispatch(&mut self, now: u64, units: u32) {
        debug_assert!(self.reserved >= u64::from(units));
        self.reserved -= u64::from(units);
        self.prune(now);
        self.charge(now, units);
    }
    fn charge(&mut self, now: u64, units: u32) {
        self.used += u64::from(units);
        if let Some((at, old)) = self.charges.back_mut().filter(|(at, _)| *at == now) {
            let _ = at;
            *old += units;
        } else {
            self.charges.push_back((now, units));
        }
    }
    fn ready_at(&mut self, now: u64, units: u32, limit: u32) -> Option<u64> {
        self.prune(now);
        // Reservations have no timer. Their owner's dispatch/drop will wake
        // the queue; pretending they expire would both overspend and busy-poll.
        if self.reserved + u64::from(units) > u64::from(limit) {
            return None;
        }
        let mut used = self.used + self.reserved;
        if used + u64::from(units) <= u64::from(limit) {
            return Some(now);
        }
        for (at, charge) in &self.charges {
            used -= u64::from(*charge);
            if used + u64::from(units) <= u64::from(limit) {
                return Some(at.saturating_add(60_000));
            }
        }
        None
    }
}
#[derive(Default)]
struct BudgetState {
    total: Window,
    backfill: Window,
    cooldown_until: u64,
}
impl BudgetState {
    fn ready_at(&mut self, now: u64, work: &Work, limit: u32) -> Option<u64> {
        let mut ready = self
            .total
            .ready_at(now, work.units, limit)?
            .max(self.cooldown_until);
        if work.class.background() {
            ready = ready.max(
                self.backfill
                    .ready_at(now, work.units, backfill_limit(limit))?,
            );
        }
        Some(ready)
    }
    fn allows(&mut self, now: u64, work: &Work, limit: u32) -> bool {
        now >= self.cooldown_until
            && self.total.allows(now, work.units, limit)
            && (!work.class.background()
                || self.backfill.allows(now, work.units, backfill_limit(limit)))
    }
    fn reserve(&mut self, work: &Work) {
        self.total.reserve(work.units);
        if work.class.background() {
            self.backfill.reserve(work.units);
        }
    }
    fn dispatch(&mut self, now: u64, work: &Work) {
        self.total.dispatch(now, work.units);
        if work.class.background() {
            self.backfill.dispatch(now, work.units);
        }
    }
}
fn backfill_limit(limit: u32) -> u32 {
    limit - limit.div_ceil(4)
}
#[derive(Default)]
struct Lane {
    urgent: VecDeque<(WorkId, Work)>,
    backfill: VecDeque<(WorkId, Work)>,
    budget: BudgetState,
}
type LaneKey = (AccountId, Service);
static NEXT_SCHEDULER: AtomicU64 = AtomicU64::new(1);

pub struct Scheduler {
    owner: u64,
    config: SchedulerConfig,
    lanes: BTreeMap<LaneKey, Lane>,
    projects: BTreeMap<Service, BudgetState>,
    active: BTreeMap<WorkId, Work>,
    dispatched: BTreeSet<WorkId>,
    last_lane: Option<LaneKey>,
    next_id: u64,
    queued: usize,
    urgent_streak: u8,
    now: u64,
}
impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Result<Self, SchedulerError> {
        config.gmail.validate()?;
        config.calendar.validate()?;
        if config.max_in_flight < 2
            || config.max_in_flight > 32
            || config.max_reads_per_account == 0
            || config.max_reads_per_account > config.max_in_flight
            || config.max_queued == 0
            || config.max_queued > 65_536
            || config.max_accounts == 0
            || config.max_accounts > 1024
        {
            return Err(SchedulerError::InvalidConfiguration);
        }
        Ok(Self {
            owner: NEXT_SCHEDULER.fetch_add(1, Ordering::Relaxed),
            config,
            lanes: BTreeMap::new(),
            projects: [
                (Service::Gmail, BudgetState::default()),
                (Service::Calendar, BudgetState::default()),
            ]
            .into(),
            active: BTreeMap::new(),
            dispatched: BTreeSet::new(),
            last_lane: None,
            next_id: 1,
            queued: 0,
            urgent_streak: 0,
            now: 0,
        })
    }
    pub fn register_account(&mut self, account: AccountId) -> Result<(), SchedulerError> {
        if self.lanes.contains_key(&(account.clone(), Service::Gmail)) {
            return Ok(());
        }
        if self.lanes.len() / 2 >= self.config.max_accounts {
            return Err(SchedulerError::Capacity);
        }
        for service in [Service::Gmail, Service::Calendar] {
            self.lanes
                .insert((account.clone(), service), Lane::default());
        }
        Ok(())
    }
    /// Units include every batch subrequest. Queues contain identifiers only;
    /// payloads and journal records remain with the owning engine worker.
    pub fn enqueue(&mut self, work: Work) -> Result<WorkId, SchedulerError> {
        let limit = self.config.budget(work.service);
        let max = limit
            .project_units_per_minute
            .min(limit.account_units_per_minute);
        let max = if work.class.background() {
            backfill_limit(max)
        } else {
            max
        };
        if work.units == 0 || work.units > max {
            return Err(SchedulerError::InvalidCost);
        }
        if (work.service == Service::Calendar
            && matches!(
                work.class,
                WorkClass::StateMutation | WorkClass::DraftMutation | WorkClass::Submission
            ))
            || (work.service == Service::Gmail && work.class == WorkClass::CalendarMutation)
        {
            return Err(SchedulerError::InvalidClass);
        }
        if self.queued >= self.config.max_queued || self.next_id == u64::MAX {
            return Err(SchedulerError::Capacity);
        }
        let lane = self
            .lanes
            .get_mut(&(work.account.clone(), work.service))
            .ok_or(SchedulerError::UnknownAccount)?;
        let id = WorkId(self.next_id);
        self.next_id += 1;
        if work.class.background() {
            lane.backfill.push_back((id, work));
        } else {
            lane.urgent.push_back((id, work));
        }
        self.queued += 1;
        Ok(id)
    }
    pub fn cancel_queued(&mut self, id: WorkId) -> bool {
        for lane in self.lanes.values_mut() {
            for queue in [&mut lane.urgent, &mut lane.backfill] {
                if let Some(index) = queue.iter().position(|(queued, _)| *queued == id) {
                    queue.remove(index);
                    self.queued -= 1;
                    return true;
                }
            }
        }
        false
    }
    /// Round-robin account/service lanes, preferring urgent work for four
    /// admissions then an eligible backfill. No progress is promised while its
    /// quota is unavailable. At least ceil(overall/4) slots cannot be backfill.
    pub fn next(&mut self, now_ms: u64) -> Option<Permit> {
        self.now = self.now.max(now_ms);
        if self.active.len() >= self.config.max_in_flight {
            return None;
        }
        let mut keys: Vec<_> = self.lanes.keys().cloned().collect();
        if let Some(last) = &self.last_lane {
            let start = keys.partition_point(|key| key <= last);
            keys.rotate_left(start);
        }
        let preference = if self.urgent_streak >= 4 {
            [true, false]
        } else {
            [false, true]
        };
        for background in preference {
            for key in &keys {
                let lane = self.lanes.get(key).unwrap();
                let candidate = if background {
                    lane.backfill.front()
                } else {
                    lane.urgent.front()
                }
                .cloned();
                let Some((id, work)) = candidate else {
                    continue;
                };
                if !self.available(id, &work) {
                    continue;
                }
                let lane = self.lanes.get_mut(key).unwrap();
                if background {
                    lane.backfill.pop_front();
                    self.urgent_streak = 0;
                } else {
                    lane.urgent.pop_front();
                    self.urgent_streak = self.urgent_streak.saturating_add(1);
                }
                lane.budget.reserve(&work);
                self.projects.get_mut(&work.service).unwrap().reserve(&work);
                self.queued -= 1;
                self.last_lane = Some(key.clone());
                self.active.insert(id, work.clone());
                return Some(Permit {
                    owner: self.owner,
                    id,
                    work,
                });
            }
        }
        None
    }
    fn available(&mut self, id: WorkId, work: &Work) -> bool {
        if work.class.background()
            && self
                .active
                .values()
                .filter(|w| w.class.background())
                .count()
                >= self.config.max_in_flight - self.config.max_in_flight.div_ceil(4)
        {
            return false;
        }
        let same_account = || self.active.values().filter(|w| w.account == work.account);
        if work.class.is_read() {
            if same_account().filter(|w| w.class.is_read()).count()
                >= self.config.max_reads_per_account
            {
                return false;
            }
        } else {
            if same_account().any(|w| !w.class.is_read()) {
                return false;
            }
            // A cheaper/later mutation in the other service cannot pass an older
            // account mutation that is queued behind a cooldown or a read.
            if self
                .lanes
                .iter()
                .filter(|((account, _), _)| account == &work.account)
                .any(|(_, lane)| {
                    lane.urgent
                        .iter()
                        .any(|(other, w)| *other < id && !w.class.is_read())
                })
            {
                return false;
            }
        }
        let limit = self.config.budget(work.service);
        self.projects.get_mut(&work.service).unwrap().allows(
            self.now,
            work,
            limit.project_units_per_minute,
        ) && self
            .lanes
            .get_mut(&(work.account.clone(), work.service))
            .unwrap()
            .budget
            .allows(self.now, work, limit.account_units_per_minute)
    }
    /// Convert this exact permit's non-expiring reservation to a rolling-window
    /// charge immediately before HTTP. This never admits a second concurrency
    /// lease and succeeds only once for a live permit owned by this scheduler.
    /// A cooldown learned while waiting also refuses dispatch; the owner must
    /// still finish the permit to release its reservation and concurrency.
    pub fn dispatch(&mut self, permit: &Permit, now_ms: u64) -> bool {
        if permit.owner != self.owner
            || !self.active.contains_key(&permit.id)
            || self.dispatched.contains(&permit.id)
        {
            return false;
        }
        self.now = self.now.max(now_ms);
        if self.dispatch_cooldown_ms(permit, self.now).is_some() {
            return false;
        }
        self.charge_reserved(permit.id);
        true
    }
    /// Remaining cooldown for a valid, still-undispatched permit. Invalid or
    /// reused permits must never be mistaken for recoverable throttling.
    pub fn dispatch_cooldown_ms(&mut self, permit: &Permit, now_ms: u64) -> Option<u64> {
        if permit.owner != self.owner || self.dispatched.contains(&permit.id) {
            return None;
        }
        let work = self.active.get(&permit.id)?;
        self.now = self.now.max(now_ms);
        let until = self
            .projects
            .get(&work.service)
            .unwrap()
            .cooldown_until
            .max(
                self.lanes
                    .get(&(work.account.clone(), work.service))
                    .unwrap()
                    .budget
                    .cooldown_until,
            );
        (until > self.now).then(|| until - self.now)
    }
    fn charge_reserved(&mut self, id: WorkId) {
        let work = self.active.get(&id).unwrap().clone();
        self.lanes
            .get_mut(&(work.account.clone(), work.service))
            .unwrap()
            .budget
            .dispatch(self.now, &work);
        self.projects
            .get_mut(&work.service)
            .unwrap()
            .dispatch(self.now, &work);
        self.dispatched.insert(id);
    }
    /// Release concurrency at a supplied current monotonic time. An undispatched
    /// cancellation is conservatively charged then; an actual HTTP charge is
    /// retained at its original dispatch time, never extended by a slow response.
    pub fn finish_at(&mut self, permit: Permit, now_ms: u64) -> bool {
        if permit.owner != self.owner || !self.active.contains_key(&permit.id) {
            return false;
        }
        self.now = self.now.max(now_ms);
        if !self.dispatched.contains(&permit.id) {
            // Cancellation conservatively charges even during a cooldown;
            // it must not strand a non-expiring reservation after releasing.
            self.charge_reserved(permit.id);
        }
        self.dispatched.remove(&permit.id);
        self.active.remove(&permit.id).is_some()
    }
    /// Compatibility for synchronous deterministic consumers: complete at the
    /// latest time supplied to this scheduler. Async HTTP owners must dispatch
    /// immediately before the request and call finish_at on completion/drop.
    pub fn finish(&mut self, permit: Permit) -> bool {
        self.finish_at(permit, self.now)
    }
    pub fn cooldown(&mut self, scope: CooldownScope, until_ms: u64) -> Result<(), SchedulerError> {
        let state = match scope {
            CooldownScope::Project(service) => self.projects.get_mut(&service).unwrap(),
            CooldownScope::Account(account, service) => {
                &mut self
                    .lanes
                    .get_mut(&(account, service))
                    .ok_or(SchedulerError::UnknownAccount)?
                    .budget
            }
        };
        state.cooldown_until = state.cooldown_until.max(until_ms);
        Ok(())
    }
    pub fn queued(&self) -> usize {
        self.queued
    }
    pub fn in_flight(&self) -> usize {
        self.active.len()
    }
    /// After draining next(), arm at most this one quota/cooldown timer. Also
    /// wake on enqueue, finish or cancellation; concurrency has no timer.
    pub fn next_wake_ms(&mut self, now_ms: u64) -> Option<u64> {
        self.now = self.now.max(now_ms);
        let candidates: Vec<_> = self
            .lanes
            .values()
            .flat_map(|lane| {
                [lane.urgent.front(), lane.backfill.front()]
                    .into_iter()
                    .flatten()
                    .map(|(_, work)| work.clone())
            })
            .collect();
        candidates
            .into_iter()
            .filter_map(|work| {
                let limit = self.config.budget(work.service);
                let project = self.projects.get_mut(&work.service).unwrap().ready_at(
                    self.now,
                    &work,
                    limit.project_units_per_minute,
                )?;
                let account = self
                    .lanes
                    .get_mut(&(work.account.clone(), work.service))
                    .unwrap()
                    .budget
                    .ready_at(self.now, &work, limit.account_units_per_minute)?;
                let ready = project.max(account);
                (ready > self.now).then_some(ready)
            })
            .min()
    }
}

/// A caller must explicitly enqueue a read again. Every mutation, including a
/// draft update or calendar write, requires operation-specific reconciliation.
pub fn retry_delay(
    class: WorkClass,
    error: &GoogleError,
    attempt: u32,
    jitter_ms: u64,
) -> Option<u64> {
    if class.is_read() {
        read_retry_delay(error, attempt, jitter_ms)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account(id: &str) -> AccountId {
        AccountId::new(id).unwrap()
    }
    fn work(id: &str, service: Service, class: WorkClass, units: u32) -> Work {
        Work {
            account: account(id),
            service,
            class,
            units,
        }
    }
    fn scheduler(project: u32, per_account: u32) -> Scheduler {
        let budget = ServiceBudget {
            project_units_per_minute: project,
            account_units_per_minute: per_account,
        };
        let mut scheduler = Scheduler::new(SchedulerConfig::new(budget, budget)).unwrap();
        for id in ["a", "b", "c", "d"] {
            scheduler.register_account(account(id)).unwrap();
        }
        scheduler
    }
    #[test]
    fn delayed_dispatch_keeps_reservation_then_starts_the_actual_rolling_minute() {
        let mut scheduler = scheduler(100, 100);
        scheduler
            .enqueue(work("a", Service::Gmail, WorkClass::UserRead, 80))
            .unwrap();
        let held = scheduler.next(0).unwrap();
        scheduler
            .enqueue(work("b", Service::Gmail, WorkClass::UserRead, 30))
            .unwrap();
        assert!(scheduler.next(61_000).is_none());
        assert_eq!(scheduler.next_wake_ms(61_000), None);
        assert_eq!(scheduler.in_flight(), 1);
        assert!(scheduler.dispatch(&held, 61_000));
        assert!(!scheduler.dispatch(&held, 61_000));
        scheduler
            .cooldown(CooldownScope::Account(account("a"), Service::Gmail), 62_000)
            .unwrap();
        assert_eq!(scheduler.dispatch_cooldown_ms(&held, 61_000), None);
        assert_eq!(scheduler.next_wake_ms(61_000), Some(121_000));
        assert!(scheduler.finish_at(held, 61_001));
        assert!(scheduler.next(120_999).is_none());
        let next = scheduler.next(121_000).unwrap();
        assert_eq!(next.work.account, account("b"));
    }
    #[test]
    fn slow_response_does_not_extend_the_dispatch_charge() {
        let mut scheduler = scheduler(100, 100);
        scheduler
            .enqueue(work("a", Service::Gmail, WorkClass::UserRead, 100))
            .unwrap();
        let held = scheduler.next(0).unwrap();
        assert!(scheduler.dispatch(&held, 1_000));
        scheduler
            .enqueue(work("b", Service::Gmail, WorkClass::UserRead, 100))
            .unwrap();
        assert!(scheduler.finish_at(held, 100_000));
        assert!(scheduler.next(100_000).is_some());
    }
    #[test]
    fn cancelled_reservation_charges_once_and_frees_concurrency_during_cooldown() {
        for scope in [
            CooldownScope::Project(Service::Gmail),
            CooldownScope::Account(account("a"), Service::Gmail),
        ] {
            let mut scheduler = scheduler(100, 100);
            scheduler
                .enqueue(work("a", Service::Gmail, WorkClass::UserRead, 100))
                .unwrap();
            let held = scheduler.next(0).unwrap();
            scheduler.cooldown(scope, 100_000).unwrap();
            assert_eq!(scheduler.dispatch_cooldown_ms(&held, 90_000), Some(10_000));
            assert!(!scheduler.dispatch(&held, 90_000));
            assert!(scheduler.finish_at(held, 90_000));
            assert_eq!(scheduler.in_flight(), 0);
            scheduler
                .enqueue(work("b", Service::Gmail, WorkClass::UserRead, 1))
                .unwrap();
            scheduler
                .enqueue(work("a", Service::Calendar, WorkClass::UserRead, 100))
                .unwrap();
            let calendar = scheduler.next(90_000).unwrap();
            assert_eq!(calendar.work.service, Service::Calendar);
            assert!(scheduler.finish_at(calendar, 90_000));
            assert!(scheduler.next(149_999).is_none());
            assert_eq!(scheduler.next_wake_ms(149_999), Some(150_000));
            assert!(scheduler.next(150_000).is_some());
        }
    }
    #[test]
    fn delayed_backfill_still_reserves_foreground_budget_and_concurrency() {
        let mut scheduler = scheduler(100, 100);
        for id in ["a", "b", "c", "d"] {
            scheduler
                .enqueue(work(id, Service::Gmail, WorkClass::BackfillRead, 25))
                .unwrap();
        }
        let background: Vec<_> = (0..3).map(|_| scheduler.next(0).unwrap()).collect();
        assert!(scheduler.next(61_000).is_none());
        assert_eq!(scheduler.next_wake_ms(61_000), None);
        scheduler
            .enqueue(work("d", Service::Gmail, WorkClass::StateMutation, 25))
            .unwrap();
        let foreground = scheduler.next(61_000).unwrap();
        assert_eq!(foreground.work.class, WorkClass::StateMutation);
        assert!(scheduler.dispatch(&foreground, 61_000));
        assert!(scheduler.finish_at(foreground, 61_000));
        for held in background {
            assert!(scheduler.dispatch(&held, 61_000));
            assert!(scheduler.finish_at(held, 61_000));
        }
        assert!(scheduler.next(120_999).is_none());
        assert!(scheduler.next(121_000).is_some());
    }
    #[test]
    fn rolling_units_account_project_and_services_are_distinct() {
        let mut scheduler = scheduler(100, 60);
        scheduler
            .enqueue(work("a", Service::Gmail, WorkClass::UserRead, 60))
            .unwrap();
        let a = scheduler.next(0).unwrap();
        scheduler.finish(a);
        scheduler
            .enqueue(work("a", Service::Gmail, WorkClass::UserRead, 1))
            .unwrap();
        scheduler
            .enqueue(work("b", Service::Gmail, WorkClass::UserRead, 40))
            .unwrap();
        let b = scheduler.next(0).unwrap();
        assert_eq!(b.work.account, account("b"));
        scheduler.finish(b);
        scheduler
            .enqueue(work("c", Service::Gmail, WorkClass::UserRead, 1))
            .unwrap();
        scheduler
            .enqueue(work("a", Service::Calendar, WorkClass::UserRead, 60))
            .unwrap();
        let calendar = scheduler.next(0).unwrap();
        assert_eq!(calendar.work.service, Service::Calendar);
        scheduler.finish(calendar);
        assert!(scheduler.next(59_999).is_none());
        assert_eq!(scheduler.next_wake_ms(59_999), Some(60_000));
        assert!(scheduler.next(60_000).is_some());
    }
    #[test]
    fn reads_and_manual_mutations_have_bounded_ordered_lanes() {
        let mut scheduler = scheduler(1000, 1000);
        for service in [Service::Gmail, Service::Calendar, Service::Gmail] {
            scheduler
                .enqueue(work("a", service, WorkClass::UserRead, 1))
                .unwrap();
        }
        let first = scheduler.next(0).unwrap();
        let second = scheduler.next(0).unwrap();
        assert!(scheduler.next(0).is_none());
        scheduler.finish(first);
        scheduler.finish(second);
        let remaining = scheduler.next(0).unwrap();
        scheduler.finish(remaining);
        let first_write = scheduler
            .enqueue(work("a", Service::Gmail, WorkClass::Submission, 100))
            .unwrap();
        scheduler
            .enqueue(work("a", Service::Calendar, WorkClass::CalendarMutation, 1))
            .unwrap();
        scheduler
            .cooldown(CooldownScope::Account(account("a"), Service::Gmail), 1000)
            .unwrap();
        assert!(scheduler.next(999).is_none());
        let first = scheduler.next(1000).unwrap();
        assert_eq!(first.id(), first_write);
        assert!(scheduler.next(1000).is_none());
        scheduler.finish(first);
        let second = scheduler.next(1000).unwrap();
        assert_eq!(second.work.class, WorkClass::CalendarMutation);
    }
    #[test]
    fn backfill_reserves_quota_and_one_foreground_slot() {
        let mut scheduler = scheduler(100, 100);
        for id in ["a", "b", "c", "d"] {
            scheduler
                .enqueue(work(id, Service::Gmail, WorkClass::BackfillRead, 25))
                .unwrap();
        }
        let permits: Vec<_> = (0..3).map(|_| scheduler.next(0).unwrap()).collect();
        assert!(scheduler.next(0).is_none());
        scheduler
            .enqueue(work("d", Service::Gmail, WorkClass::StateMutation, 25))
            .unwrap();
        let user = scheduler.next(0).unwrap();
        assert_eq!(user.work.class, WorkClass::StateMutation);
        assert!(scheduler.next(0).is_none());
        for permit in permits.into_iter().chain([user]) {
            scheduler.finish(permit);
        }
        assert!(scheduler.next(59_999).is_none());
        assert!(scheduler.next(60_000).is_some());
    }
    #[test]
    fn continuous_urgent_work_does_not_starve_eligible_backfill_or_lanes() {
        let mut scheduler = scheduler(1000, 1000);
        for _ in 0..12 {
            for (id, service) in [
                ("a", Service::Gmail),
                ("a", Service::Calendar),
                ("b", Service::Gmail),
            ] {
                scheduler
                    .enqueue(work(id, service, WorkClass::UserRead, 1))
                    .unwrap();
            }
        }
        let backfill = scheduler
            .enqueue(work("c", Service::Gmail, WorkClass::BackfillRead, 1))
            .unwrap();
        let mut lanes = Vec::new();
        for _ in 0..4 {
            let p = scheduler.next(0).unwrap();
            lanes.push((p.work.account.clone(), p.work.service));
            scheduler.finish(p);
        }
        assert_eq!(
            lanes[..3],
            [
                (account("a"), Service::Gmail),
                (account("a"), Service::Calendar),
                (account("b"), Service::Gmail)
            ]
        );
        let next = scheduler.next(0).unwrap();
        assert_eq!(next.id(), backfill);
    }
    #[test]
    fn shared_cooldowns_cancel_and_capacity_do_not_leak_work() {
        let mut scheduler = scheduler(100, 100);
        let cancelled = scheduler
            .enqueue(work("a", Service::Gmail, WorkClass::UserRead, 1))
            .unwrap();
        assert!(scheduler.cancel_queued(cancelled));
        assert!(!scheduler.cancel_queued(cancelled));
        scheduler
            .enqueue(work("b", Service::Gmail, WorkClass::UserRead, 1))
            .unwrap();
        scheduler
            .enqueue(work("b", Service::Calendar, WorkClass::UserRead, 1))
            .unwrap();
        scheduler
            .cooldown(CooldownScope::Project(Service::Gmail), 10_000)
            .unwrap();
        scheduler
            .cooldown(CooldownScope::Project(Service::Gmail), 5)
            .unwrap();
        let p = scheduler.next(0).unwrap();
        assert_eq!(p.work.service, Service::Calendar);
        scheduler.finish(p);
        assert!(scheduler.next(9999).is_none());
        assert!(scheduler.next(10_000).is_some());
        assert_eq!(
            scheduler.enqueue(work("unknown", Service::Gmail, WorkClass::UserRead, 1)),
            Err(SchedulerError::UnknownAccount)
        );
        assert_eq!(
            scheduler.enqueue(work("a", Service::Calendar, WorkClass::Submission, 1)),
            Err(SchedulerError::InvalidClass)
        );
        assert_eq!(
            scheduler.enqueue(work("a", Service::Gmail, WorkClass::BackfillRead, 76)),
            Err(SchedulerError::InvalidCost)
        );
    }
    #[test]
    fn retry_policy_never_replays_writes_and_stops_terminal_errors() {
        let quota = GoogleError::RateLimited {
            retry_after_secs: Some(120),
        };
        for class in [
            WorkClass::StateMutation,
            WorkClass::DraftMutation,
            WorkClass::Submission,
            WorkClass::CalendarMutation,
        ] {
            for error in [
                &quota,
                &GoogleError::ServerUnavailable,
                &GoogleError::TransportInterrupted,
            ] {
                assert_eq!(retry_delay(class, error, 0, 5), None);
            }
        }
        assert_eq!(
            retry_delay(WorkClass::DeltaRead, &quota, 0, 999),
            Some(120_000)
        );
        assert_eq!(
            retry_delay(
                WorkClass::UserRead,
                &GoogleError::ServerUnavailable,
                5,
                9999
            ),
            Some(32_999)
        );
        assert_eq!(retry_delay(WorkClass::UserRead, &quota, 6, 0), None);
        for error in [
            GoogleError::PermissionDenied,
            GoogleError::ReauthorizationRequired,
            GoogleError::NotFound,
        ] {
            assert_eq!(retry_delay(WorkClass::UserRead, &error, 0, 0), None);
        }
        assert_eq!(
            ServiceBudget::from_confirmed_limits(1_200_000, 6000).unwrap(),
            ServiceBudget {
                project_units_per_minute: 960_000,
                account_units_per_minute: 4800
            }
        );
    }
}
